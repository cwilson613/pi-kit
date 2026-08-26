//! Validated, read-only access to schema-v1 session projections.

use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::{
    session_projection_cursor::{
        ProjectionCursorStore, ProjectionDisposition, ProjectorIdentity,
        ValidatedProjectionFrontier,
    },
    session_replay::{ReplayEnd, SessionReplay},
    session_shadow_projection::ShadowProjector,
    surfaces::session::{
        MAX_CHUNK_BYTES, PROJECTION_SCHEMA_VERSION, PROJECTOR_VERSION, ProjectionChunkV1,
        ProjectionEnvelopeV1, ProjectionExactnessV1, ProjectionLineageV1, ProjectionPayloadV1,
        ProjectorIdV1,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ValidatedProjectionV1 {
    pub(crate) envelope: ProjectionEnvelopeV1,
    pub(crate) chunks: Vec<ProjectionChunkV1>,
    pub(crate) frontier: ValidatedProjectionFrontier,
}

#[derive(Debug, Clone)]
pub(crate) enum ProjectionReadV1 {
    ExactFull(ValidatedProjectionV1),
    ExactSuffix(ValidatedProjectionV1),
    LegacyUnavailable(ValidatedProjectionV1),
    SessionlessUnavailable,
    Stale {
        projection: ValidatedProjectionV1,
        lag_events: u64,
    },
    Corrupt {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ExactProjectionReadError {
    #[error("projection is stale by {lag_events} authority events")]
    Stale { lag_events: u64 },
    #[error("projection is unavailable for legacy lineage")]
    LegacyUnavailable,
    #[error("sessionless execution has no semantic projection lineage")]
    SessionlessUnavailable,
    #[error("projection validation failed: {0}")]
    Corrupt(String),
}

#[derive(Debug, Clone)]
pub(crate) struct SessionProjectionReader {
    session_snapshot: PathBuf,
    projection_root: PathBuf,
    session_id: String,
    stream_id: Uuid,
}

impl SessionProjectionReader {
    pub(crate) fn adjacent_to(
        session_snapshot: &Path,
        session_id: impl Into<String>,
        stream_id: Uuid,
    ) -> Result<Self, ExactProjectionReadError> {
        let parent = session_snapshot.parent().ok_or_else(|| {
            ExactProjectionReadError::Corrupt("session snapshot has no parent".into())
        })?;
        let stem = session_snapshot
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                ExactProjectionReadError::Corrupt("session snapshot has no UTF-8 stem".into())
            })?;
        Ok(Self {
            session_snapshot: session_snapshot.to_path_buf(),
            projection_root: parent.join(format!("{stem}.projections")),
            session_id: session_id.into(),
            stream_id,
        })
    }

    pub(crate) fn read(
        &self,
        projector: ShadowProjector,
        intended_end: ReplayEnd,
    ) -> ProjectionReadV1 {
        let replay = match SessionReplay::replay_prefix(
            &self.session_snapshot,
            &self.session_id,
            self.stream_id,
            intended_end,
        ) {
            Ok(replay) => replay,
            Err(error) => return corrupt(error),
        };
        let identity = match ProjectorIdentity::new(
            projector.id(),
            u32::from(PROJECTOR_VERSION),
            u32::from(PROJECTION_SCHEMA_VERSION),
        ) {
            Ok(identity) => identity,
            Err(error) => return corrupt(error),
        };
        let store = match ProjectionCursorStore::open_existing(&self.projection_root, identity) {
            Ok(store) => store,
            Err(error) => return corrupt(error),
        };
        let disposition = match store.validate(&replay) {
            Ok(disposition) => disposition,
            Err(error) => return corrupt(error),
        };
        let (frontier, output, lag_events) = match disposition {
            ProjectionDisposition::Resume { frontier, output } => (frontier, output, 0),
            ProjectionDisposition::ReplayTail {
                frontier,
                output,
                through,
            } => {
                let lag = through
                    .sequence()
                    .saturating_sub(frontier.authority.sequence());
                (frontier, output, lag)
            }
            ProjectionDisposition::Rebuild { reason } => {
                return ProjectionReadV1::Corrupt {
                    reason: format!("projection cursor requires rebuild: {reason:?}"),
                };
            }
        };
        let projection = match self.validate_projection(projector, frontier, output) {
            Ok(projection) => projection,
            Err(reason) => return ProjectionReadV1::Corrupt { reason },
        };
        if lag_events != 0 {
            return ProjectionReadV1::Stale {
                projection,
                lag_events,
            };
        }
        match projection.envelope.exactness {
            ProjectionExactnessV1::ExactFull => ProjectionReadV1::ExactFull(projection),
            ProjectionExactnessV1::ExactSuffix => ProjectionReadV1::ExactSuffix(projection),
            ProjectionExactnessV1::None => ProjectionReadV1::LegacyUnavailable(projection),
        }
    }

    pub(crate) fn read_exact(
        &self,
        projector: ShadowProjector,
        intended_end: ReplayEnd,
    ) -> Result<ValidatedProjectionV1, ExactProjectionReadError> {
        match self.read(projector, intended_end) {
            ProjectionReadV1::ExactFull(value) | ProjectionReadV1::ExactSuffix(value) => Ok(value),
            ProjectionReadV1::LegacyUnavailable(_) => {
                Err(ExactProjectionReadError::LegacyUnavailable)
            }
            ProjectionReadV1::SessionlessUnavailable => {
                Err(ExactProjectionReadError::SessionlessUnavailable)
            }
            ProjectionReadV1::Stale { lag_events, .. } => {
                Err(ExactProjectionReadError::Stale { lag_events })
            }
            ProjectionReadV1::Corrupt { reason } => Err(ExactProjectionReadError::Corrupt(reason)),
        }
    }

    fn validate_projection(
        &self,
        projector: ShadowProjector,
        frontier: ValidatedProjectionFrontier,
        output: Vec<u8>,
    ) -> Result<ValidatedProjectionV1, String> {
        let envelope: ProjectionEnvelopeV1 = strict_json(&output)?;
        envelope.validate().map_err(|error| error.to_string())?;
        if envelope
            .canonical_bytes()
            .map_err(|error| error.to_string())?
            != output
        {
            return Err("projection envelope is not canonical JSON".into());
        }
        let expected_frontier = crate::surfaces::session::SourceEventV1 {
            sequence: frontier.authority.sequence(),
            event_id: frontier.authority.event_id(),
        };
        if envelope.projector_id != projector.dto_id()
            || envelope.session_id != self.session_id
            || envelope.stream_id != Some(self.stream_id)
            || (envelope.lineage_level != ProjectionLineageV1::Legacy
                && envelope.source_frontier.as_ref() != Some(&expected_frontier))
        {
            return Err("projection envelope identity or frontier disagrees with cursor".into());
        }

        let chunks = match &envelope.payload {
            ProjectionPayloadV1::ChunkManifest { manifest } => {
                if manifest.projector_id != projector.dto_id()
                    || manifest.session_id != self.session_id
                    || manifest.stream_id != self.stream_id
                    || manifest.source_frontier != expected_frontier
                {
                    return Err(
                        "chunk manifest identity or frontier disagrees with envelope".into(),
                    );
                }
                let mut chunks = Vec::with_capacity(manifest.chunks.len());
                let mut validated = Vec::with_capacity(manifest.chunks.len());
                for entry in &manifest.chunks {
                    let path = self
                        .projection_root
                        .join(projector.id())
                        .join("chunks")
                        .join("sha256")
                        .join(format!("{}.json", entry.digest));
                    let bytes = read_regular_bounded(&path, MAX_CHUNK_BYTES as u64)?;
                    let chunk: ProjectionChunkV1 = strict_json(&bytes)?;
                    validated.push((chunk.clone(), bytes));
                    chunks.push(chunk);
                }
                manifest
                    .validate_chunks(&validated)
                    .map_err(|error| error.to_string())?;
                chunks
            }
            ProjectionPayloadV1::None => Vec::new(),
            ProjectionPayloadV1::FrontendSnapshot { .. }
                if projector.dto_id() == ProjectorIdV1::FrontendSnapshot =>
            {
                Vec::new()
            }
            ProjectionPayloadV1::CompactionCheckpoint { .. }
                if projector.dto_id() == ProjectorIdV1::CompactionCheckpoint =>
            {
                Vec::new()
            }
            _ => return Err("projection payload does not belong to projector".into()),
        };
        Ok(ValidatedProjectionV1 {
            envelope,
            chunks,
            frontier,
        })
    }
}

fn strict_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = T::deserialize(&mut deserializer).map_err(|error| error.to_string())?;
    deserializer.end().map_err(|error| error.to_string())?;
    Ok(value)
}

fn read_regular_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > maximum {
        return Err(format!(
            "projection chunk is not a bounded regular file: {}",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(format!(
                "projection chunk permissions are not restrictive: {}",
                path.display()
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true).custom_flags(libc::O_NOFOLLOW);
        read_bounded(
            options.open(path).map_err(|error| error.to_string())?,
            maximum,
        )
    }
    #[cfg(not(unix))]
    {
        let mut options = OpenOptions::new();
        options.read(true);
        read_bounded(
            options.open(path).map_err(|error| error.to_string())?,
            maximum,
        )
    }
}

fn read_bounded(file: fs::File, maximum: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > maximum {
        return Err("projection chunk grew while reading".into());
    }
    Ok(bytes)
}

fn corrupt(error: impl std::fmt::Display) -> ProjectionReadV1 {
    ProjectionReadV1::Corrupt {
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::session_shadow_projection::{ALL_SHADOW_PROJECTORS, SessionProjectionCoordinator};

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/session-semantic-v1"
    );
    const SESSION_ID: &str = "fixture-session";
    const STREAM_ID: Uuid = Uuid::from_u128(0x10000000_0000_4000_8000_000000000001);

    fn fixture(
        name: &str,
        publish_end: ReplayEnd,
    ) -> (tempfile::TempDir, PathBuf, SessionProjectionReader) {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = directory.path().join("session.json");
        fs::write(
            directory.path().join("session.authority.jsonl"),
            fs::read(Path::new(FIXTURES).join(name)).unwrap(),
        )
        .unwrap();
        let replay =
            SessionReplay::replay_prefix(&snapshot, SESSION_ID, STREAM_ID, publish_end).unwrap();
        let root = directory.path().join("session.projections");
        SessionProjectionCoordinator::open(&root)
            .unwrap()
            .publish(&replay, &ALL_SHADOW_PROJECTORS);
        let reader =
            SessionProjectionReader::adjacent_to(&snapshot, SESSION_ID, STREAM_ID).unwrap();
        (directory, root, reader)
    }

    #[test]
    fn reads_exact_full_suffix_and_legacy_with_verified_chunks() {
        let (_full_dir, _full_root, full) = fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::EndOfStream,
        );
        let ProjectionReadV1::ExactFull(transcript) =
            full.read(ShadowProjector::Transcript, ReplayEnd::EndOfStream)
        else {
            panic!("full transcript was not exact");
        };
        assert_eq!(transcript.chunks.len(), 1);

        let (_mixed_dir, _mixed_root, mixed) =
            fixture("mixed-legacy-full.authority.jsonl", ReplayEnd::EndOfStream);
        assert!(matches!(
            mixed.read(ShadowProjector::FrontendSnapshot, ReplayEnd::EndOfStream),
            ProjectionReadV1::ExactSuffix(_)
        ));

        let (_legacy_dir, _legacy_root, legacy) =
            fixture("slice-1-closed.authority.jsonl", ReplayEnd::EndOfStream);
        assert!(matches!(
            legacy.read(ShadowProjector::Transcript, ReplayEnd::EndOfStream),
            ProjectionReadV1::LegacyUnavailable(_)
        ));
    }

    #[test]
    fn stale_projection_is_disclosed_to_ui_and_rejected_by_exact_reader() {
        let (_directory, _root, reader) = fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::Sequence(3),
        );
        assert!(matches!(
            reader.read(ShadowProjector::FrontendSnapshot, ReplayEnd::EndOfStream),
            ProjectionReadV1::Stale { lag_events: 1, .. }
        ));
        assert_eq!(
            reader
                .read_exact(ShadowProjector::FrontendSnapshot, ReplayEnd::EndOfStream)
                .unwrap_err(),
            ExactProjectionReadError::Stale { lag_events: 1 }
        );
    }

    #[test]
    fn cursor_output_and_chunk_corruption_fail_closed() {
        let (_directory, root, reader) = fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::EndOfStream,
        );
        let output = root.join("session.frontend-snapshot/output.bin");
        fs::write(&output, b"tampered").unwrap();
        assert!(matches!(
            reader.read(ShadowProjector::FrontendSnapshot, ReplayEnd::EndOfStream),
            ProjectionReadV1::Corrupt { .. }
        ));

        let ProjectionReadV1::ExactFull(transcript) =
            reader.read(ShadowProjector::Transcript, ReplayEnd::EndOfStream)
        else {
            panic!("transcript should initially validate");
        };
        let ProjectionPayloadV1::ChunkManifest { manifest } = &transcript.envelope.payload else {
            panic!("transcript should be chunked");
        };
        let chunk = root
            .join("session.transcript/chunks/sha256")
            .join(format!("{}.json", manifest.chunks[0].digest));
        fs::write(chunk, b"{}").unwrap();
        assert!(matches!(
            reader.read(ShadowProjector::Transcript, ReplayEnd::EndOfStream),
            ProjectionReadV1::Corrupt { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn chunk_symlinks_are_rejected() {
        use std::os::unix::fs::symlink;

        let (directory, root, reader) = fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::EndOfStream,
        );
        let ProjectionReadV1::ExactFull(transcript) =
            reader.read(ShadowProjector::Transcript, ReplayEnd::EndOfStream)
        else {
            panic!("transcript should initially validate");
        };
        let ProjectionPayloadV1::ChunkManifest { manifest } = &transcript.envelope.payload else {
            panic!("transcript should be chunked");
        };
        let chunk = root
            .join("session.transcript/chunks/sha256")
            .join(format!("{}.json", manifest.chunks[0].digest));
        let outside = directory.path().join("outside.json");
        fs::write(&outside, b"{}").unwrap();
        fs::remove_file(&chunk).unwrap();
        symlink(outside, chunk).unwrap();
        assert!(matches!(
            reader.read(ShadowProjector::Transcript, ReplayEnd::EndOfStream),
            ProjectionReadV1::Corrupt { .. }
        ));
    }

    #[test]
    fn compatibility_consumers_have_no_direct_projection_storage_reads() {
        for (name, source) in [
            ("provider dispatch", include_str!("providers.rs")),
            ("ConversationState", include_str!("conversation.rs")),
            ("session", include_str!("session.rs")),
            ("commands", include_str!("session_commands.rs")),
            ("compaction", include_str!("features/auto_compact.rs")),
            ("TUI", include_str!("tui/mod.rs")),
            ("TUI projection", include_str!("tui/session_projection.rs")),
            ("ACP", include_str!("acp.rs")),
            ("ACP worker", include_str!("acp_worker.rs")),
            ("control runtime", include_str!("control_runtime.rs")),
            ("Web", include_str!("web/mod.rs")),
            ("Web API", include_str!("web/api.rs")),
            ("Web surfaces", include_str!("web/surfaces.rs")),
            ("IPC", include_str!("ipc/snapshot.rs")),
            ("telemetry", include_str!("checkpoint.rs")),
            ("audit", include_str!("features/audit_log.rs")),
            ("journal", include_str!("features/session_log.rs")),
        ] {
            assert!(
                !source.contains("output.bin")
                    && !source.contains("chunks/sha256")
                    && !source.contains("ProjectionCursorStore"),
                "{name} must use SessionProjectionReader rather than projection storage"
            );
        }
    }
}

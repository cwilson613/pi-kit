use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    session_blob_store::{ContentRef, ProjectionClass},
    session_replay::{AuthorityFrontier, SessionReplay},
};

const CURSOR_VERSION: u16 = 1;
const MAX_CURSOR_BYTES: u64 = 64 * 1024;
const MAX_PROJECTION_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ProjectionCursorError {
    #[error("projection cursor I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("projection cursor JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("projection cursor is invalid: {0}")]
    Invalid(String),
    #[error("projection publication conflicts with deterministic identity: {0}")]
    Corruption(String),
}

type Result<T> = std::result::Result<T, ProjectionCursorError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectorIdentity {
    projector_id: String,
    projector_version: u32,
    projection_schema_version: u32,
}

impl ProjectorIdentity {
    pub(crate) fn new(
        projector_id: impl Into<String>,
        projector_version: u32,
        projection_schema_version: u32,
    ) -> Result<Self> {
        let projector_id = projector_id.into();
        validate_projector_id(&projector_id)?;
        if projector_version == 0 || projection_schema_version == 0 {
            return Err(ProjectionCursorError::Invalid(
                "projector and projection schema versions must be non-zero".into(),
            ));
        }
        Ok(Self {
            projector_id,
            projector_version,
            projection_schema_version,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DeterministicProjectionBytes {
    bytes: Vec<u8>,
}

impl DeterministicProjectionBytes {
    pub(crate) fn new(bytes: Vec<u8>, source_refs: &[&ContentRef]) -> Result<Self> {
        if bytes.len() as u64 > MAX_PROJECTION_BYTES {
            return Err(ProjectionCursorError::Invalid(
                "projection output exceeds 16 MiB".into(),
            ));
        }
        if source_refs
            .iter()
            .any(|content_ref| content_ref.projection_class() != ProjectionClass::Default)
        {
            return Err(ProjectionCursorError::Invalid(
                "restricted continuity references cannot enter default projection publication"
                    .into(),
            ));
        }
        Ok(Self { bytes })
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OutputDigestAlgorithm {
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectionCursorV1 {
    cursor_version: u16,
    projector_id: String,
    projector_version: u32,
    projection_schema_version: u32,
    session_id: String,
    stream_id: Uuid,
    last_sequence: u64,
    last_event_id: Uuid,
    output_revision: u64,
    output_digest_algorithm: OutputDigestAlgorithm,
    output_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedProjectionFrontier {
    pub(crate) authority: AuthorityFrontier,
    pub(crate) output_revision: u64,
    pub(crate) output_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RebuildReason {
    MissingDerivedState,
    CorruptCursor,
    CorruptOutput,
    WrongProjector,
    WrongSchema,
    WrongSession,
    WrongStream,
    CursorAhead,
    WrongEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectionDisposition {
    Resume {
        frontier: ValidatedProjectionFrontier,
        output: Vec<u8>,
    },
    ReplayTail {
        frontier: ValidatedProjectionFrontier,
        output: Vec<u8>,
        through: AuthorityFrontier,
    },
    Rebuild {
        reason: RebuildReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicationOutcome {
    pub(crate) frontier: ValidatedProjectionFrontier,
    pub(crate) idempotent: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectionCursorStore {
    directory: PathBuf,
    identity: ProjectorIdentity,
}

impl ProjectionCursorStore {
    pub(crate) fn open(root: &Path, identity: ProjectorIdentity) -> Result<Self> {
        ensure_projection_root(root)?;
        let directory = root.join(&identity.projector_id);
        ensure_restricted_directory(&directory)?;
        if !directory.starts_with(root) {
            return Err(ProjectionCursorError::Invalid(
                "projector directory escapes projection root".into(),
            ));
        }
        Ok(Self {
            directory,
            identity,
        })
    }

    /// Opens already-published state without creating reader-visible artifacts.
    pub(crate) fn open_existing(root: &Path, identity: ProjectorIdentity) -> Result<Self> {
        if !root.is_absolute() {
            return Err(ProjectionCursorError::Invalid(
                "projection root must be absolute".into(),
            ));
        }
        validate_existing_directory(root)?;
        let directory = root.join(&identity.projector_id);
        if !directory.starts_with(root) {
            return Err(ProjectionCursorError::Invalid(
                "projector directory escapes projection root".into(),
            ));
        }
        validate_existing_directory(&directory)?;
        Ok(Self {
            directory,
            identity,
        })
    }

    pub(crate) fn output_path(&self) -> PathBuf {
        self.directory.join("output.bin")
    }

    pub(crate) fn cursor_path(&self) -> PathBuf {
        self.directory.join("cursor.json")
    }

    pub(crate) fn validate(&self, replay: &SessionReplay) -> Result<ProjectionDisposition> {
        let _lock = ProjectionLock::acquire(&self.directory)?;
        self.validate_locked(replay)
    }

    pub(crate) fn publish(
        &self,
        replay: &SessionReplay,
        output: &DeterministicProjectionBytes,
    ) -> Result<PublicationOutcome> {
        let _lock = ProjectionLock::acquire(&self.directory)?;
        self.publish_locked(replay, output, &RealPublicationIo)
    }

    pub(crate) fn with_publication_lock<T>(&self, operation: impl FnOnce() -> T) -> Result<T> {
        let _lock = ProjectionLock::acquire(&self.directory)?;
        Ok(operation())
    }

    fn validate_locked(&self, replay: &SessionReplay) -> Result<ProjectionDisposition> {
        let cursor_path = self.cursor_path();
        let output_path = self.output_path();
        let cursor_exists = safe_file_exists(&cursor_path)?;
        let output_exists = safe_file_exists(&output_path)?;
        if !cursor_exists || !output_exists {
            return Ok(ProjectionDisposition::Rebuild {
                reason: RebuildReason::MissingDerivedState,
            });
        }

        let cursor = match read_cursor(&cursor_path) {
            Ok(cursor) if validate_cursor_shape(&cursor).is_ok() => cursor,
            Ok(_)
            | Err(ProjectionCursorError::Json(_))
            | Err(ProjectionCursorError::Invalid(_)) => {
                return Ok(ProjectionDisposition::Rebuild {
                    reason: RebuildReason::CorruptCursor,
                });
            }
            Err(error) => return Err(error),
        };
        if cursor.projector_id != self.identity.projector_id
            || cursor.projector_version != self.identity.projector_version
        {
            return Ok(ProjectionDisposition::Rebuild {
                reason: RebuildReason::WrongProjector,
            });
        }
        if cursor.projection_schema_version != self.identity.projection_schema_version {
            return Ok(ProjectionDisposition::Rebuild {
                reason: RebuildReason::WrongSchema,
            });
        }
        if cursor.session_id != replay.frontier().session_id() {
            return Ok(ProjectionDisposition::Rebuild {
                reason: RebuildReason::WrongSession,
            });
        }
        if cursor.stream_id != replay.frontier().stream_id() {
            return Ok(ProjectionDisposition::Rebuild {
                reason: RebuildReason::WrongStream,
            });
        }
        if cursor.last_sequence > replay.frontier().sequence() {
            return Ok(ProjectionDisposition::Rebuild {
                reason: RebuildReason::CursorAhead,
            });
        }
        let record = replay
            .records()
            .get(cursor.last_sequence.saturating_sub(1) as usize);
        if record.map(|record| record.frontier().event_id()) != Some(cursor.last_event_id) {
            return Ok(ProjectionDisposition::Rebuild {
                reason: RebuildReason::WrongEvent,
            });
        }
        let output = match read_regular_bounded(&output_path, MAX_PROJECTION_BYTES) {
            Ok(output) => output,
            Err(ProjectionCursorError::Invalid(_)) => {
                return Ok(ProjectionDisposition::Rebuild {
                    reason: RebuildReason::CorruptOutput,
                });
            }
            Err(error) => return Err(error),
        };
        if sha256(&output) != cursor.output_digest {
            return Ok(ProjectionDisposition::Rebuild {
                reason: RebuildReason::CorruptOutput,
            });
        }
        let frontier = ValidatedProjectionFrontier {
            authority: record
                .expect("validated non-zero cursor")
                .frontier()
                .clone(),
            output_revision: cursor.output_revision,
            output_digest: cursor.output_digest,
        };
        if frontier.authority.sequence() == replay.frontier().sequence() {
            Ok(ProjectionDisposition::Resume { frontier, output })
        } else {
            Ok(ProjectionDisposition::ReplayTail {
                frontier,
                output,
                through: replay.frontier().clone(),
            })
        }
    }

    fn publish_locked(
        &self,
        replay: &SessionReplay,
        output: &DeterministicProjectionBytes,
        io: &dyn PublicationIo,
    ) -> Result<PublicationOutcome> {
        let digest = sha256(output.as_bytes());
        let disposition = self.validate_locked(replay)?;
        let revision = match disposition {
            ProjectionDisposition::Resume { ref frontier, .. } => {
                if frontier.output_digest == digest {
                    return Ok(PublicationOutcome {
                        frontier: frontier.clone(),
                        idempotent: true,
                    });
                }
                return Err(ProjectionCursorError::Corruption(
                    "the same projector/source identity produced different bytes".into(),
                ));
            }
            ProjectionDisposition::ReplayTail { ref frontier, .. } => frontier
                .output_revision
                .checked_add(1)
                .ok_or_else(|| ProjectionCursorError::Invalid("output revision overflow".into()))?,
            ProjectionDisposition::Rebuild { .. } => 1,
        };
        let cursor = ProjectionCursorV1 {
            cursor_version: CURSOR_VERSION,
            projector_id: self.identity.projector_id.clone(),
            projector_version: self.identity.projector_version,
            projection_schema_version: self.identity.projection_schema_version,
            session_id: replay.frontier().session_id().to_owned(),
            stream_id: replay.frontier().stream_id(),
            last_sequence: replay.frontier().sequence(),
            last_event_id: replay.frontier().event_id(),
            output_revision: revision,
            output_digest_algorithm: OutputDigestAlgorithm::Sha256,
            output_digest: digest.clone(),
        };
        let mut cursor_bytes = serde_json::to_vec(&cursor)?;
        cursor_bytes.push(b'\n');

        replace_bytes_atomically_with_io(
            &self.directory,
            &self.output_path(),
            output.as_bytes(),
            PublishKind::Output,
            io,
        )?;
        replace_bytes_atomically_with_io(
            &self.directory,
            &self.cursor_path(),
            &cursor_bytes,
            PublishKind::Cursor,
            io,
        )?;
        Ok(PublicationOutcome {
            frontier: ValidatedProjectionFrontier {
                authority: replay.frontier().clone(),
                output_revision: revision,
                output_digest: digest,
            },
            idempotent: false,
        })
    }
}

fn validate_existing_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(ProjectionCursorError::Invalid(format!(
            "projection path is not a real directory: {}",
            path.display()
        )));
    }
    validate_restrictive_mode(&metadata, path)
}

fn validate_cursor_shape(cursor: &ProjectionCursorV1) -> Result<()> {
    if cursor.cursor_version != CURSOR_VERSION {
        return Err(ProjectionCursorError::Invalid(
            "unsupported projection cursor version".into(),
        ));
    }
    validate_projector_id(&cursor.projector_id)?;
    if cursor.projector_version == 0
        || cursor.projection_schema_version == 0
        || cursor.last_sequence == 0
        || cursor.output_revision == 0
        || cursor.last_event_id.is_nil()
        || !is_sha256(&cursor.output_digest)
    {
        return Err(ProjectionCursorError::Invalid(
            "projection cursor contains an invalid required field".into(),
        ));
    }
    Ok(())
}

fn validate_projector_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(ProjectionCursorError::Invalid(
            "projector ID must be a safe ASCII path component".into(),
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_cursor(path: &Path) -> Result<ProjectionCursorV1> {
    let bytes = read_regular_bounded(path, MAX_CURSOR_BYTES)?;
    let mut deserializer = serde_json::Deserializer::from_slice(&bytes);
    let cursor = ProjectionCursorV1::deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(cursor)
}

fn read_regular_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let file = open_regular_file(path, false)?;
    if file.metadata()?.len() > maximum {
        return Err(ProjectionCursorError::Invalid(format!(
            "projection file exceeds {} bytes",
            maximum
        )));
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(ProjectionCursorError::Invalid(
            "projection file grew while reading".into(),
        ));
    }
    Ok(bytes)
}

fn ensure_projection_root(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(ProjectionCursorError::Invalid(
            "projection root must be absolute".into(),
        ));
    }
    ensure_restricted_directory(path)
}

fn ensure_restricted_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => validate_restrictive_mode(&metadata, path),
        Ok(_) => Err(ProjectionCursorError::Invalid(format!(
            "projection path is not a real directory: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_restricted_directory(path)?;
            sync_directory(path.parent().ok_or_else(|| {
                ProjectionCursorError::Invalid("projection directory has no parent".into())
            })?)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn validate_restrictive_mode(metadata: &fs::Metadata, path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ProjectionCursorError::Invalid(format!(
            "projection directory permissions are not restrictive: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_restrictive_mode(_metadata: &fs::Metadata, _path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn create_restricted_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_restricted_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path)
}

fn safe_file_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(ProjectionCursorError::Invalid(format!(
            "projection entry is not a regular file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn open_regular_file(path: &Path, write: bool) -> Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(ProjectionCursorError::Invalid(
            "projection entry is not a regular file".into(),
        ));
    }
    let mut options = OpenOptions::new();
    options.read(!write).write(write);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(ProjectionCursorError::Invalid(
            "projection entry changed type while opening".into(),
        ));
    }
    Ok(file)
}

fn create_restricted_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

struct TemporaryFile {
    path: PathBuf,
    file: Option<File>,
}

impl TemporaryFile {
    fn create(parent: &Path) -> Result<Self> {
        for _ in 0..32 {
            let path = parent.join(format!(".projection-tmp-{}", Uuid::new_v4()));
            match create_restricted_file(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(ProjectionCursorError::Invalid(
            "could not allocate an exclusive projection temporary file".into(),
        ))
    }

    fn write_and_sync(&mut self, bytes: &[u8]) -> Result<()> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| ProjectionCursorError::Invalid("temporary file is closed".into()))?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }

    fn close(&mut self) {
        self.file.take();
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        self.close();
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishKind {
    Output,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishStage {
    OutputFileSynced,
    OutputRenamed,
    OutputParentSynced,
    CursorFileSynced,
    CursorRenamed,
    CursorParentSynced,
}

trait PublicationIo {
    fn after(&self, stage: PublishStage) -> Result<()>;
}

struct RealPublicationIo;

impl PublicationIo for RealPublicationIo {
    fn after(&self, _stage: PublishStage) -> Result<()> {
        Ok(())
    }
}

fn replace_bytes_atomically(parent: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    replace_bytes_atomically_with_io(
        parent,
        destination,
        bytes,
        PublishKind::Output,
        &RealPublicationIo,
    )
}

fn replace_bytes_atomically_with_io(
    parent: &Path,
    destination: &Path,
    bytes: &[u8],
    kind: PublishKind,
    io: &dyn PublicationIo,
) -> Result<()> {
    if destination.parent() != Some(parent) {
        return Err(ProjectionCursorError::Invalid(
            "projection destination escapes its directory".into(),
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(destination)
        && !metadata.file_type().is_file()
    {
        return Err(ProjectionCursorError::Invalid(
            "projection destination is not a regular file".into(),
        ));
    }
    let mut temporary = TemporaryFile::create(parent)?;
    temporary.write_and_sync(bytes)?;
    io.after(match kind {
        PublishKind::Output => PublishStage::OutputFileSynced,
        PublishKind::Cursor => PublishStage::CursorFileSynced,
    })?;
    temporary.close();
    fs::rename(&temporary.path, destination)?;
    io.after(match kind {
        PublishKind::Output => PublishStage::OutputRenamed,
        PublishKind::Cursor => PublishStage::CursorRenamed,
    })?;
    sync_directory(parent)?;
    io.after(match kind {
        PublishKind::Output => PublishStage::OutputParentSynced,
        PublishKind::Cursor => PublishStage::CursorParentSynced,
    })?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

struct ProjectionLock {
    file: File,
}

impl ProjectionLock {
    fn acquire(directory: &Path) -> Result<Self> {
        let path = directory.join("publication.lock");
        let file = match safe_file_exists(&path)? {
            true => open_regular_file(&path, true)?,
            false => match create_restricted_file(&path) {
                Ok(file) => {
                    sync_directory(directory)?;
                    file
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    open_regular_file(&path, true)?
                }
                Err(error) => return Err(error.into()),
            },
        };
        lock_exclusive(&file)?;
        Ok(Self { file })
    }
}

impl Drop for ProjectionLock {
    fn drop(&mut self) {
        let _ = unlock(&self.file);
    }
}

#[cfg(unix)]
fn lock_exclusive(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn unlock(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn lock_exclusive(_file: &File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn unlock(_file: &File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Barrier, Mutex},
        thread,
    };

    use serde_json::{Value, json};

    use super::*;
    use crate::{session_authority::AuthorityLineageLevel, session_replay::ReplayEnd};

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/session-semantic-v1"
    );
    const SESSION_ID: &str = "fixture-session";
    const STREAM_ID: Uuid = Uuid::from_u128(0x10000000_0000_4000_8000_000000000001);

    fn replay_fixture(fixture: &str, end: ReplayEnd) -> (tempfile::TempDir, SessionReplay) {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = directory.path().join("session.json");
        fs::copy(
            Path::new(FIXTURES).join(fixture),
            directory.path().join("session.authority.jsonl"),
        )
        .unwrap();
        let replay = SessionReplay::replay_prefix(&snapshot, SESSION_ID, STREAM_ID, end).unwrap();
        (directory, replay)
    }

    fn store(directory: &tempfile::TempDir) -> ProjectionCursorStore {
        ProjectionCursorStore::open(
            &directory.path().join("projections"),
            ProjectorIdentity::new("test-projector", 1, 1).unwrap(),
        )
        .unwrap()
    }

    fn output(bytes: &[u8]) -> DeterministicProjectionBytes {
        DeterministicProjectionBytes::new(bytes.to_vec(), &[]).unwrap()
    }

    fn published(
        fixture: &str,
        end: ReplayEnd,
        bytes: &[u8],
    ) -> (tempfile::TempDir, SessionReplay, ProjectionCursorStore) {
        let (directory, replay) = replay_fixture(fixture, end);
        let store = store(&directory);
        store.publish(&replay, &output(bytes)).unwrap();
        (directory, replay, store)
    }

    fn mutate_cursor(store: &ProjectionCursorStore, field: &str, value: Value) {
        let mut cursor: Value =
            serde_json::from_slice(&fs::read(store.cursor_path()).unwrap()).unwrap();
        cursor[field] = value;
        fs::write(store.cursor_path(), serde_json::to_vec(&cursor).unwrap()).unwrap();
    }

    fn rebuild_reason(disposition: ProjectionDisposition) -> RebuildReason {
        match disposition {
            ProjectionDisposition::Rebuild { reason } => reason,
            other => panic!("expected rebuild, got {other:?}"),
        }
    }

    #[test]
    fn cursor_schema_is_strict_and_deterministic() {
        let (_directory, replay, store) = published(
            "slice-1-closed.authority.jsonl",
            ReplayEnd::EndOfStream,
            b"deterministic",
        );
        let first = fs::read(store.cursor_path()).unwrap();
        let outcome = store.publish(&replay, &output(b"deterministic")).unwrap();
        assert!(outcome.idempotent);
        assert_eq!(first, fs::read(store.cursor_path()).unwrap());
        assert_eq!(outcome.frontier.output_digest, sha256(b"deterministic"));

        let mut cursor: Value = serde_json::from_slice(&first).unwrap();
        cursor["unexpected"] = json!(true);
        fs::write(store.cursor_path(), serde_json::to_vec(&cursor).unwrap()).unwrap();
        assert_eq!(
            rebuild_reason(store.validate(&replay).unwrap()),
            RebuildReason::CorruptCursor
        );
    }

    #[test]
    fn exact_resume_stale_cursor_and_authority_extension_are_typed() {
        let (directory, prefix) =
            replay_fixture("slice-1-closed.authority.jsonl", ReplayEnd::Sequence(2));
        let store = store(&directory);
        let first = store.publish(&prefix, &output(b"prefix")).unwrap();
        assert_eq!(first.frontier.output_revision, 1);
        assert!(matches!(
            store.validate(&prefix).unwrap(),
            ProjectionDisposition::Resume { .. }
        ));

        let full = SessionReplay::replay_prefix(
            &directory.path().join("session.json"),
            SESSION_ID,
            STREAM_ID,
            ReplayEnd::EndOfStream,
        )
        .unwrap();
        match store.validate(&full).unwrap() {
            ProjectionDisposition::ReplayTail {
                frontier,
                output,
                through,
            } => {
                assert_eq!(frontier.authority.sequence(), 2);
                assert_eq!(output, b"prefix");
                assert_eq!(through.sequence(), 4);
            }
            other => panic!("expected replay tail, got {other:?}"),
        }
        let second = store.publish(&full, &output(b"full")).unwrap();
        assert_eq!(second.frontier.output_revision, 2);
    }

    #[test]
    fn every_cursor_identity_mismatch_rebuilds() {
        let cases = [
            ("cursor_version", json!(2), RebuildReason::CorruptCursor),
            (
                "projector_id",
                json!("other"),
                RebuildReason::WrongProjector,
            ),
            ("projector_version", json!(2), RebuildReason::WrongProjector),
            (
                "projection_schema_version",
                json!(2),
                RebuildReason::WrongSchema,
            ),
            ("session_id", json!("other"), RebuildReason::WrongSession),
            (
                "stream_id",
                json!(Uuid::new_v4()),
                RebuildReason::WrongStream,
            ),
            ("last_sequence", json!(99), RebuildReason::CursorAhead),
            (
                "last_event_id",
                json!(Uuid::new_v4()),
                RebuildReason::WrongEvent,
            ),
            ("output_revision", json!(0), RebuildReason::CorruptCursor),
            (
                "output_digest_algorithm",
                json!("sha512"),
                RebuildReason::CorruptCursor,
            ),
            ("output_digest", json!("bad"), RebuildReason::CorruptCursor),
            (
                "output_digest",
                json!("11".repeat(32)),
                RebuildReason::CorruptOutput,
            ),
        ];
        for (field, value, expected) in cases {
            let (_directory, replay, store) = published(
                "slice-1-closed.authority.jsonl",
                ReplayEnd::EndOfStream,
                b"output",
            );
            mutate_cursor(&store, field, value);
            assert_eq!(rebuild_reason(store.validate(&replay).unwrap()), expected);
        }
    }

    #[test]
    fn missing_corrupt_and_mismatched_derived_files_rebuild() {
        let (directory, replay) =
            replay_fixture("slice-1-closed.authority.jsonl", ReplayEnd::EndOfStream);
        let store = store(&directory);
        assert_eq!(
            rebuild_reason(store.validate(&replay).unwrap()),
            RebuildReason::MissingDerivedState
        );
        store.publish(&replay, &output(b"output")).unwrap();
        fs::write(store.cursor_path(), b"not-json").unwrap();
        assert_eq!(
            rebuild_reason(store.validate(&replay).unwrap()),
            RebuildReason::CorruptCursor
        );
        store.publish(&replay, &output(b"output")).unwrap();
        fs::write(store.output_path(), b"tampered").unwrap();
        assert_eq!(
            rebuild_reason(store.validate(&replay).unwrap()),
            RebuildReason::CorruptOutput
        );
        fs::remove_file(store.output_path()).unwrap();
        assert_eq!(
            rebuild_reason(store.validate(&replay).unwrap()),
            RebuildReason::MissingDerivedState
        );
    }

    #[test]
    fn same_identity_with_different_bytes_is_corruption() {
        let (_directory, replay, store) = published(
            "slice-1-closed.authority.jsonl",
            ReplayEnd::EndOfStream,
            b"first",
        );
        assert!(matches!(
            store.publish(&replay, &output(b"second")),
            Err(ProjectionCursorError::Corruption(_))
        ));
        assert_eq!(fs::read(store.output_path()).unwrap(), b"first");
    }

    #[test]
    fn publication_never_mutates_authority() {
        let (directory, replay) =
            replay_fixture("mixed-legacy-full.authority.jsonl", ReplayEnd::EndOfStream);
        let authority = directory.path().join("session.authority.jsonl");
        let before = fs::read(&authority).unwrap();
        store(&directory)
            .publish(&replay, &output(b"projection"))
            .unwrap();
        assert_eq!(fs::read(authority).unwrap(), before);
    }

    struct FailingIo {
        fail_after: PublishStage,
        order: Mutex<Vec<PublishStage>>,
    }

    impl PublicationIo for FailingIo {
        fn after(&self, stage: PublishStage) -> Result<()> {
            self.order.lock().unwrap().push(stage);
            if stage == self.fail_after {
                return Err(std::io::Error::other("injected publication failure").into());
            }
            Ok(())
        }
    }

    #[test]
    fn every_output_and_cursor_crash_boundary_is_safe() {
        let stages = [
            PublishStage::OutputFileSynced,
            PublishStage::OutputRenamed,
            PublishStage::OutputParentSynced,
            PublishStage::CursorFileSynced,
            PublishStage::CursorRenamed,
            PublishStage::CursorParentSynced,
        ];
        for fail_after in stages {
            let (directory, replay) =
                replay_fixture("slice-1-closed.authority.jsonl", ReplayEnd::EndOfStream);
            let store = store(&directory);
            let io = FailingIo {
                fail_after,
                order: Mutex::new(Vec::new()),
            };
            let _lock = ProjectionLock::acquire(&store.directory).unwrap();
            assert!(store.publish_locked(&replay, &output(b"new"), &io).is_err());
            let order = io.order.into_inner().unwrap();
            assert_eq!(order.last(), Some(&fail_after));
            match fail_after {
                PublishStage::OutputFileSynced => {
                    assert!(!store.output_path().exists());
                    assert!(!store.cursor_path().exists());
                }
                PublishStage::OutputRenamed
                | PublishStage::OutputParentSynced
                | PublishStage::CursorFileSynced => {
                    assert_eq!(fs::read(store.output_path()).unwrap(), b"new");
                    assert!(!store.cursor_path().exists());
                    assert_eq!(
                        rebuild_reason(store.validate_locked(&replay).unwrap()),
                        RebuildReason::MissingDerivedState
                    );
                }
                PublishStage::CursorRenamed | PublishStage::CursorParentSynced => {
                    assert_eq!(fs::read(store.output_path()).unwrap(), b"new");
                    assert!(matches!(
                        store.validate_locked(&replay).unwrap(),
                        ProjectionDisposition::Resume { .. }
                    ));
                }
            }
            if matches!(fail_after, PublishStage::OutputFileSynced) {
                assert!(!store.cursor_path().exists());
            }
        }
    }

    #[test]
    fn output_before_cursor_crash_rebuilds_and_cursor_before_output_is_unservable() {
        let (_directory, replay, store) = published(
            "slice-1-closed.authority.jsonl",
            ReplayEnd::Sequence(2),
            b"old",
        );
        replace_bytes_atomically(&store.directory, &store.output_path(), b"new").unwrap();
        assert_eq!(
            rebuild_reason(store.validate(&replay).unwrap()),
            RebuildReason::CorruptOutput
        );

        fs::remove_file(store.output_path()).unwrap();
        assert_eq!(
            rebuild_reason(store.validate(&replay).unwrap()),
            RebuildReason::MissingDerivedState
        );
    }

    #[test]
    fn restricted_references_are_rejected_before_publication() {
        let restricted: ContentRef = serde_json::from_value(json!({
            "digest_algorithm": "sha256",
            "digest": "00".repeat(32),
            "media_type": "application/octet-stream",
            "byte_length": 1,
            "storage_class": "session_blob_v1",
            "projection_class": "restricted_continuity"
        }))
        .unwrap();
        assert!(
            DeterministicProjectionBytes::new(b"metadata or bytes".to_vec(), &[&restricted])
                .is_err()
        );
    }

    #[test]
    fn traversal_symlinks_and_unsafe_modes_are_rejected() {
        assert!(ProjectorIdentity::new("../escape", 1, 1).is_err());
        assert!(ProjectorIdentity::new("a/b", 1, 1).is_err());
        assert!(ProjectorIdentity::new("valid", 0, 1).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::{PermissionsExt, symlink};

            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join("target");
            fs::create_dir(&target).unwrap();
            let linked_root = directory.path().join("linked");
            symlink(&target, &linked_root).unwrap();
            assert!(
                ProjectionCursorStore::open(
                    &linked_root,
                    ProjectorIdentity::new("safe", 1, 1).unwrap()
                )
                .is_err()
            );

            let broad = directory.path().join("broad");
            fs::create_dir(&broad).unwrap();
            fs::set_permissions(&broad, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                ProjectionCursorStore::open(&broad, ProjectorIdentity::new("safe", 1, 1).unwrap())
                    .is_err()
            );

            let (fixture_dir, replay) =
                replay_fixture("slice-1-closed.authority.jsonl", ReplayEnd::EndOfStream);
            let store = store(&fixture_dir);
            symlink(fixture_dir.path().join("outside"), store.output_path()).unwrap();
            assert!(store.publish(&replay, &output(b"blocked")).is_err());
        }
    }

    #[test]
    #[cfg(unix)]
    fn directories_files_and_temporaries_are_restrictive() {
        use std::os::unix::fs::PermissionsExt;

        let (directory, replay) =
            replay_fixture("slice-1-closed.authority.jsonl", ReplayEnd::EndOfStream);
        let store = store(&directory);
        store.publish(&replay, &output(b"mode")).unwrap();
        assert_eq!(
            fs::metadata(&store.directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for path in [
            store.output_path(),
            store.cursor_path(),
            store.directory.join("publication.lock"),
        ] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(fs::read_dir(&store.directory).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".projection-tmp-")
        }));
    }

    #[test]
    fn concurrent_exact_publication_is_single_revision_and_idempotent() {
        let (directory, replay) =
            replay_fixture("slice-1-closed.authority.jsonl", ReplayEnd::EndOfStream);
        let store = Arc::new(store(&directory));
        let replay = Arc::new(replay);
        let barrier = Arc::new(Barrier::new(8));
        let handles = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let replay = Arc::clone(&replay);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    store.publish(&replay, &output(b"same")).unwrap()
                })
            })
            .collect::<Vec<_>>();
        let outcomes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| !outcome.idempotent)
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .map(|outcome| outcome.frontier.output_revision)
                .collect::<HashSet<_>>(),
            HashSet::from([1])
        );
    }

    #[test]
    fn concurrent_different_bytes_detect_determinism_corruption() {
        let (directory, replay) =
            replay_fixture("slice-1-closed.authority.jsonl", ReplayEnd::EndOfStream);
        let store = Arc::new(store(&directory));
        let replay = Arc::new(replay);
        let barrier = Arc::new(Barrier::new(2));
        let handles = [b"first".as_slice(), b"second".as_slice()]
            .into_iter()
            .map(|bytes| {
                let store = Arc::clone(&store);
                let replay = Arc::clone(&replay);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    store.publish(&replay, &output(bytes))
                })
            })
            .collect::<Vec<_>>();
        let outcomes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, Err(ProjectionCursorError::Corruption(_))))
                .count(),
            1
        );
        assert!(matches!(
            store.validate(&replay).unwrap(),
            ProjectionDisposition::Resume { .. }
        ));
    }

    #[test]
    fn legacy_mixed_and_full_replay_frontiers_are_supported_without_policy() {
        let fixtures = [
            (
                "slice-1-closed.authority.jsonl",
                AuthorityLineageLevel::LegacyOnly,
            ),
            (
                "mixed-legacy-full.authority.jsonl",
                AuthorityLineageLevel::Mixed,
            ),
            (
                "full-spine-crash-prefix.authority.jsonl",
                AuthorityLineageLevel::FullSpine,
            ),
        ];
        for (fixture, lineage) in fixtures {
            let (directory, replay) = replay_fixture(fixture, ReplayEnd::EndOfStream);
            assert_eq!(replay.lineage_level(), lineage);
            let store = store(&directory);
            let outcome = store.publish(&replay, &output(fixture.as_bytes())).unwrap();
            assert_eq!(outcome.frontier.authority, *replay.frontier());
        }
    }
}

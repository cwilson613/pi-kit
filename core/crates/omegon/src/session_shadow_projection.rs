//! Shadow-only publication for the four frozen schema-v1 session projections.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    session_projection_cursor::{
        DeterministicProjectionBytes, ProjectionCursorError, ProjectionCursorStore,
        ProjectorIdentity, PublicationOutcome,
    },
    session_projection_model::SessionProjectionModel,
    session_replay::SessionReplay,
    surfaces::session::{
        PROJECTION_SCHEMA_VERSION, PROJECTOR_VERSION, ProjectionEnvelopeV1, ProjectionLineageV1,
        ProjectionPayloadV1, ProjectionValidationError, ProjectorIdV1,
    },
};

pub(crate) const ALL_SHADOW_PROJECTORS: [ShadowProjector; 4] = [
    ShadowProjector::ProviderHistory,
    ShadowProjector::Transcript,
    ShadowProjector::FrontendSnapshot,
    ShadowProjector::CompactionCheckpoint,
];

const COALESCE_DELAY: Duration = Duration::from_millis(50);
const MAX_PUBLICATION_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub(crate) struct SessionProjectionWorkerDescriptor {
    pub(crate) session_snapshot: PathBuf,
    pub(crate) session_id: String,
    pub(crate) stream_id: Uuid,
}

impl SessionProjectionWorkerDescriptor {
    fn root(&self) -> Result<PathBuf, SessionProjectionWorkerError> {
        let parent = self.session_snapshot.parent().ok_or_else(|| {
            SessionProjectionWorkerError::Configuration(
                "session snapshot has no projection parent".into(),
            )
        })?;
        let stem = self
            .session_snapshot
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                SessionProjectionWorkerError::Configuration(
                    "session snapshot has no UTF-8 projection stem".into(),
                )
            })?;
        Ok(parent.join(format!("{stem}.projections")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SessionProjectionWorkerError {
    #[error("projection worker configuration failed: {0}")]
    Configuration(String),
    #[error("projection worker replay failed: {0}")]
    Replay(String),
    #[error("projection worker coordinator failed: {0}")]
    Coordinator(String),
    #[error("projection worker thread failed: {0}")]
    Thread(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionProjectionWorkerSnapshot {
    pub(crate) runs: u64,
    pub(crate) last_frontier_sequence: Option<u64>,
    pub(crate) failed_projectors: Vec<(ShadowProjector, ProjectorFailureStage, String)>,
    pub(crate) worker_error: Option<SessionProjectionWorkerError>,
    pub(crate) stopped: bool,
}

#[derive(Debug)]
struct ProjectionWakeState {
    dirty: AtomicBool,
    immediate: AtomicBool,
    stopping: AtomicBool,
    wake: SyncSender<()>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionProjectionWakeHandle(Arc<ProjectionWakeState>);

impl SessionProjectionWakeHandle {
    pub(crate) fn hint(&self, immediate: bool) {
        self.0.dirty.store(true, Ordering::Release);
        if immediate {
            self.0.immediate.store(true, Ordering::Release);
        }
        let _ = self.0.wake.try_send(());
    }
}

#[derive(Debug)]
pub(crate) struct SessionProjectionWorker {
    wake: SessionProjectionWakeHandle,
    snapshot: Arc<Mutex<SessionProjectionWorkerSnapshot>>,
    thread: Option<JoinHandle<()>>,
}

impl SessionProjectionWorker {
    pub(crate) fn start(
        descriptor: SessionProjectionWorkerDescriptor,
    ) -> Result<Self, SessionProjectionWorkerError> {
        let (wake_tx, wake_rx) = mpsc::sync_channel(1);
        let state = Arc::new(ProjectionWakeState {
            dirty: AtomicBool::new(true),
            immediate: AtomicBool::new(true),
            stopping: AtomicBool::new(false),
            wake: wake_tx,
        });
        let wake = SessionProjectionWakeHandle(Arc::clone(&state));
        let snapshot = Arc::new(Mutex::new(SessionProjectionWorkerSnapshot::default()));
        let worker_snapshot = Arc::clone(&snapshot);
        let thread = thread::Builder::new()
            .name(format!("session-projection-{}", descriptor.session_id))
            .spawn(move || projection_worker_loop(descriptor, state, wake_rx, worker_snapshot))
            .map_err(|error| SessionProjectionWorkerError::Thread(error.to_string()))?;
        let _ = wake.0.wake.try_send(());
        Ok(Self {
            wake,
            snapshot,
            thread: Some(thread),
        })
    }

    pub(crate) fn wake_handle(&self) -> SessionProjectionWakeHandle {
        self.wake.clone()
    }

    pub(crate) fn snapshot(&self) -> SessionProjectionWorkerSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn snapshot_state(&self) -> Arc<Mutex<SessionProjectionWorkerSnapshot>> {
        Arc::clone(&self.snapshot)
    }

    pub(crate) fn flush(&self) {
        self.wake.hint(true);
    }

    pub(crate) fn shutdown(&mut self) {
        self.request_shutdown();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            let mut snapshot = self
                .snapshot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            snapshot.worker_error = Some(SessionProjectionWorkerError::Thread(
                "projection worker panicked".into(),
            ));
            snapshot.stopped = true;
        }
    }

    pub(crate) fn request_shutdown(&self) {
        self.wake.0.dirty.store(true, Ordering::Release);
        self.wake.0.immediate.store(true, Ordering::Release);
        self.wake.0.stopping.store(true, Ordering::Release);
        let _ = self.wake.0.wake.try_send(());
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for SessionProjectionWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn projection_worker_loop(
    descriptor: SessionProjectionWorkerDescriptor,
    state: Arc<ProjectionWakeState>,
    wake_rx: mpsc::Receiver<()>,
    snapshot: Arc<Mutex<SessionProjectionWorkerSnapshot>>,
) {
    while wake_rx.recv().is_ok() {
        let started = Instant::now();
        while !state.immediate.load(Ordering::Acquire) && !state.stopping.load(Ordering::Acquire) {
            let remaining = MAX_PUBLICATION_DELAY.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            match wake_rx.recv_timeout(COALESCE_DELAY.min(remaining)) {
                Ok(()) => {}
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        state.immediate.store(false, Ordering::Release);
        if state.dirty.swap(false, Ordering::AcqRel) {
            publish_latest(&descriptor, &snapshot);
        }
        if state.stopping.load(Ordering::Acquire) {
            if state.dirty.swap(false, Ordering::AcqRel) {
                publish_latest(&descriptor, &snapshot);
            }
            break;
        }
        if state.dirty.load(Ordering::Acquire) {
            let _ = state.wake.try_send(());
        }
    }
    snapshot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .stopped = true;
}

fn publish_latest(
    descriptor: &SessionProjectionWorkerDescriptor,
    snapshot: &Arc<Mutex<SessionProjectionWorkerSnapshot>>,
) {
    let result = descriptor.root().and_then(|root| {
        let replay = SessionReplay::replay_prefix(
            &descriptor.session_snapshot,
            &descriptor.session_id,
            descriptor.stream_id,
            crate::session_replay::ReplayEnd::EndOfStream,
        )
        .map_err(|error| SessionProjectionWorkerError::Replay(error.to_string()))?;
        let coordinator = SessionProjectionCoordinator::open(&root)
            .map_err(|error| SessionProjectionWorkerError::Coordinator(error.to_string()))?;
        Ok((replay, coordinator))
    });
    let mut observed = snapshot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match result {
        Ok((replay, coordinator)) => {
            let reports = coordinator.publish(&replay, &ALL_SHADOW_PROJECTORS);
            observed.runs = observed.runs.saturating_add(1);
            observed.last_frontier_sequence = Some(replay.frontier().sequence());
            observed.failed_projectors = reports
                .into_iter()
                .filter_map(|report| match report.status {
                    ProjectorPublicationStatus::Published(_) => None,
                    ProjectorPublicationStatus::Failed { stage, error } => {
                        Some((report.projector, stage, error.to_string()))
                    }
                })
                .collect();
            observed.worker_error = None;
            for (projector, stage, error) in &observed.failed_projectors {
                tracing::warn!(projector = projector.id(), ?stage, %error, "shadow session projector failed");
            }
        }
        Err(error) => {
            tracing::warn!(%error, "shadow session projection worker failed");
            observed.worker_error = Some(error);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShadowProjector {
    ProviderHistory,
    Transcript,
    FrontendSnapshot,
    CompactionCheckpoint,
}

impl ShadowProjector {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::ProviderHistory => "session.provider-history",
            Self::Transcript => "session.transcript",
            Self::FrontendSnapshot => "session.frontend-snapshot",
            Self::CompactionCheckpoint => "session.compaction-checkpoint",
        }
    }

    pub(crate) const fn dto_id(self) -> ProjectorIdV1 {
        match self {
            Self::ProviderHistory => ProjectorIdV1::ProviderHistory,
            Self::Transcript => ProjectorIdV1::Transcript,
            Self::FrontendSnapshot => ProjectorIdV1::FrontendSnapshot,
            Self::CompactionCheckpoint => ProjectorIdV1::CompactionCheckpoint,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ShadowProjectionError {
    #[error("projection derivation failed: {0}")]
    Derivation(String),
    #[error("projection cursor failed: {0}")]
    Cursor(#[from] ProjectionCursorError),
    #[error("projection chunk storage failed: {0}")]
    Chunk(String),
}

impl From<ProjectionValidationError> for ShadowProjectionError {
    fn from(error: ProjectionValidationError) -> Self {
        Self::Derivation(error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectorFailureStage {
    Derivation,
    ChunkStorage,
    Publication,
}

#[derive(Debug)]
pub(crate) enum ProjectorPublicationStatus {
    Published(PublicationOutcome),
    Failed {
        stage: ProjectorFailureStage,
        error: ShadowProjectionError,
    },
}

#[derive(Debug)]
pub(crate) struct ProjectorPublicationReport {
    pub(crate) projector: ShadowProjector,
    pub(crate) status: ProjectorPublicationStatus,
}

#[derive(Debug)]
struct BuiltProjection {
    envelope: Vec<u8>,
    chunks: Vec<(String, Vec<u8>)>,
}

/// Publishes requested projectors independently from one immutable replay frontier.
#[derive(Debug)]
pub(crate) struct SessionProjectionCoordinator {
    root: PathBuf,
}

impl SessionProjectionCoordinator {
    pub(crate) fn open(root: &Path) -> Result<Self, ShadowProjectionError> {
        if !root.is_absolute() {
            return Err(ShadowProjectionError::Chunk(
                "shadow projection root must be absolute".into(),
            ));
        }
        ensure_restricted_directory(root)?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub(crate) fn publish(
        &self,
        replay: &SessionReplay,
        requested: &[ShadowProjector],
    ) -> Vec<ProjectorPublicationReport> {
        // SessionReplay is immutable and already bounded to a stable authority prefix.
        let model = match SessionProjectionModel::from_replay(replay) {
            Ok(model) => model,
            Err(error) => {
                let message = error.to_string();
                return requested
                    .iter()
                    .copied()
                    .map(|projector| ProjectorPublicationReport {
                        projector,
                        status: ProjectorPublicationStatus::Failed {
                            stage: ProjectorFailureStage::Derivation,
                            error: ShadowProjectionError::Derivation(message.clone()),
                        },
                    })
                    .collect();
            }
        };

        requested
            .iter()
            .copied()
            .map(|projector| self.publish_one(replay, &model, projector))
            .collect()
    }

    fn publish_one(
        &self,
        replay: &SessionReplay,
        model: &SessionProjectionModel,
        projector: ShadowProjector,
    ) -> ProjectorPublicationReport {
        let built = match build_projection(model, projector) {
            Ok(built) => built,
            Err(error) => {
                return ProjectorPublicationReport {
                    projector,
                    status: ProjectorPublicationStatus::Failed {
                        stage: ProjectorFailureStage::Derivation,
                        error,
                    },
                };
            }
        };
        let cursor = match projector_store(&self.root, projector) {
            Ok(cursor) => cursor,
            Err(error) => {
                return ProjectorPublicationReport {
                    projector,
                    status: ProjectorPublicationStatus::Failed {
                        stage: ProjectorFailureStage::Publication,
                        error,
                    },
                };
            }
        };
        let chunks = match ImmutableChunkStore::open(&self.root, projector) {
            Ok(chunks) => chunks,
            Err(error) => {
                return ProjectorPublicationReport {
                    projector,
                    status: ProjectorPublicationStatus::Failed {
                        stage: ProjectorFailureStage::ChunkStorage,
                        error,
                    },
                };
            }
        };
        let chunk_publication = cursor
            .with_publication_lock(|| {
                built
                    .chunks
                    .iter()
                    .try_for_each(|(digest, bytes)| chunks.put_repairing(digest, bytes))
            })
            .map_err(ShadowProjectionError::from)
            .and_then(|result| result);
        if let Err(error) = chunk_publication {
            return ProjectorPublicationReport {
                projector,
                status: ProjectorPublicationStatus::Failed {
                    stage: ProjectorFailureStage::ChunkStorage,
                    error,
                },
            };
        }
        let output = match DeterministicProjectionBytes::new(built.envelope, &[]) {
            Ok(output) => output,
            Err(error) => {
                return ProjectorPublicationReport {
                    projector,
                    status: ProjectorPublicationStatus::Failed {
                        stage: ProjectorFailureStage::Publication,
                        error: error.into(),
                    },
                };
            }
        };
        let status = match cursor.publish(replay, &output) {
            Ok(outcome) => ProjectorPublicationStatus::Published(outcome),
            Err(error) => ProjectorPublicationStatus::Failed {
                stage: ProjectorFailureStage::Publication,
                error: error.into(),
            },
        };
        ProjectorPublicationReport { projector, status }
    }
}

fn projector_store(
    root: &Path,
    projector: ShadowProjector,
) -> Result<ProjectionCursorStore, ShadowProjectionError> {
    Ok(ProjectionCursorStore::open(
        root,
        ProjectorIdentity::new(
            projector.id(),
            u32::from(PROJECTOR_VERSION),
            u32::from(PROJECTION_SCHEMA_VERSION),
        )?,
    )?)
}

fn build_projection(
    model: &SessionProjectionModel,
    projector: ShadowProjector,
) -> Result<BuiltProjection, ShadowProjectionError> {
    let unavailable = model.lineage() == ProjectionLineageV1::Legacy;
    let (payload, chunks) = if unavailable {
        (ProjectionPayloadV1::None, Vec::new())
    } else {
        match projector {
            ShadowProjector::ProviderHistory => {
                let (manifest, chunks) = model.provider_history_chunks()?;
                (
                    ProjectionPayloadV1::ChunkManifest { manifest },
                    chunks
                        .into_iter()
                        .map(|(_, bytes)| (sha256(&bytes), bytes))
                        .collect(),
                )
            }
            ShadowProjector::Transcript => {
                let (manifest, chunks) = model.transcript_chunks()?;
                (
                    ProjectionPayloadV1::ChunkManifest { manifest },
                    chunks
                        .into_iter()
                        .map(|(_, bytes)| (sha256(&bytes), bytes))
                        .collect(),
                )
            }
            ShadowProjector::FrontendSnapshot => (
                ProjectionPayloadV1::FrontendSnapshot {
                    snapshot: model.frontend_snapshot().clone(),
                },
                Vec::new(),
            ),
            ShadowProjector::CompactionCheckpoint => (
                ProjectionPayloadV1::CompactionCheckpoint {
                    checkpoint: model.compaction_checkpoint().clone(),
                },
                Vec::new(),
            ),
        }
    };
    let envelope: ProjectionEnvelopeV1 = model.envelope(projector.dto_id(), payload)?;
    Ok(BuiltProjection {
        envelope: envelope.canonical_bytes()?,
        chunks,
    })
}

#[derive(Debug)]
struct ImmutableChunkStore {
    directory: PathBuf,
}

impl ImmutableChunkStore {
    fn open(root: &Path, projector: ShadowProjector) -> Result<Self, ShadowProjectionError> {
        let projector_directory = root.join(projector.id());
        let chunks = projector_directory.join("chunks");
        let directory = chunks.join("sha256");
        if !directory.starts_with(root) {
            return Err(ShadowProjectionError::Chunk(
                "chunk directory escapes projection root".into(),
            ));
        }
        ensure_restricted_directory(&projector_directory)?;
        ensure_restricted_directory(&chunks)?;
        ensure_restricted_directory(&directory)?;
        Ok(Self { directory })
    }

    fn path(&self, digest: &str) -> Result<PathBuf, ShadowProjectionError> {
        if !is_sha256(digest) {
            return Err(ShadowProjectionError::Chunk(
                "chunk digest is not lowercase SHA-256".into(),
            ));
        }
        let path = self.directory.join(format!("{digest}.json"));
        if path.parent() != Some(self.directory.as_path()) {
            return Err(ShadowProjectionError::Chunk(
                "chunk path escapes projector storage".into(),
            ));
        }
        Ok(path)
    }

    fn put(&self, digest: &str, bytes: &[u8]) -> Result<(), ShadowProjectionError> {
        self.put_inner(digest, bytes, false)
    }

    fn put_repairing(&self, digest: &str, bytes: &[u8]) -> Result<(), ShadowProjectionError> {
        self.put_inner(digest, bytes, true)
    }

    fn put_inner(
        &self,
        digest: &str,
        bytes: &[u8],
        repair_corrupt: bool,
    ) -> Result<(), ShadowProjectionError> {
        if sha256(bytes) != digest {
            return Err(ShadowProjectionError::Chunk(
                "chunk bytes disagree with digest identity".into(),
            ));
        }
        let destination = self.path(digest)?;
        if path_exists_without_following(&destination)? {
            match verify_chunk(&destination, digest, bytes) {
                Ok(()) => return Ok(()),
                Err(error) if !repair_corrupt => return Err(error),
                Err(_) => self.quarantine_corrupt(&destination, digest)?,
            }
        }
        let mut temporary = ChunkTemporary::create(&self.directory)?;
        temporary.write_and_sync(bytes)?;
        temporary.close();
        match fs::hard_link(&temporary.path, &destination) {
            Ok(()) => {
                sync_directory(&self.directory)?;
                verify_chunk(&destination, digest, bytes)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                verify_chunk(&destination, digest, bytes)
            }
            Err(error) => Err(chunk_io(error)),
        }
    }

    fn quarantine_corrupt(
        &self,
        destination: &Path,
        digest: &str,
    ) -> Result<(), ShadowProjectionError> {
        let mut quarantine = None;
        for ordinal in 1..=32 {
            let candidate = self.directory.join(if ordinal == 1 {
                format!("{digest}.corrupt")
            } else {
                format!("{digest}.corrupt.{ordinal}")
            });
            if !path_exists_without_following(&candidate)? {
                quarantine = Some(candidate);
                break;
            }
        }
        let quarantine = quarantine.ok_or_else(|| {
            ShadowProjectionError::Chunk("immutable chunk quarantine is exhausted".into())
        })?;
        fs::rename(destination, &quarantine).map_err(chunk_io)?;
        sync_directory(&self.directory)
    }
}

fn path_exists_without_following(path: &Path) -> Result<bool, ShadowProjectionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(chunk_io(error)),
    }
}

fn verify_chunk(path: &Path, digest: &str, expected: &[u8]) -> Result<(), ShadowProjectionError> {
    let metadata = fs::symlink_metadata(path).map_err(chunk_io)?;
    if !metadata.file_type().is_file() {
        return Err(ShadowProjectionError::Chunk(
            "chunk destination is not a regular file".into(),
        ));
    }
    validate_file_mode(&metadata, path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path).map_err(chunk_io)?;
    if file.metadata().map_err(chunk_io)?.len() != expected.len() as u64 {
        return Err(ShadowProjectionError::Chunk(
            "immutable chunk length mismatch".into(),
        ));
    }
    let mut actual = Vec::with_capacity(expected.len());
    file.read_to_end(&mut actual).map_err(chunk_io)?;
    if actual != expected || sha256(&actual) != digest {
        return Err(ShadowProjectionError::Chunk(
            "immutable chunk verification failed".into(),
        ));
    }
    Ok(())
}

struct ChunkTemporary {
    path: PathBuf,
    file: Option<File>,
}

impl ChunkTemporary {
    fn create(parent: &Path) -> Result<Self, ShadowProjectionError> {
        for _ in 0..32 {
            let path = parent.join(format!(".chunk-tmp-{}", Uuid::new_v4()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
            }
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(chunk_io(error)),
            }
        }
        Err(ShadowProjectionError::Chunk(
            "could not allocate unique chunk temporary".into(),
        ))
    }

    fn write_and_sync(&mut self, bytes: &[u8]) -> Result<(), ShadowProjectionError> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| ShadowProjectionError::Chunk("chunk temporary is closed".into()))?;
        file.write_all(bytes).map_err(chunk_io)?;
        file.flush().map_err(chunk_io)?;
        file.sync_all().map_err(chunk_io)
    }

    fn close(&mut self) {
        self.file.take();
    }
}

impl Drop for ChunkTemporary {
    fn drop(&mut self) {
        self.close();
        let _ = fs::remove_file(&self.path);
    }
}

fn ensure_restricted_directory(path: &Path) -> Result<(), ShadowProjectionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => validate_directory_mode(&metadata, path),
        Ok(_) => Err(ShadowProjectionError::Chunk(format!(
            "projection path is not a real directory: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_restricted_directory(path).map_err(chunk_io)?;
            sync_directory(path.parent().ok_or_else(|| {
                ShadowProjectionError::Chunk("projection directory has no parent".into())
            })?)
        }
        Err(error) => Err(chunk_io(error)),
    }
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

#[cfg(unix)]
fn validate_directory_mode(
    metadata: &fs::Metadata,
    path: &Path,
) -> Result<(), ShadowProjectionError> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ShadowProjectionError::Chunk(format!(
            "projection directory permissions are not restrictive: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_directory_mode(
    _metadata: &fs::Metadata,
    _path: &Path,
) -> Result<(), ShadowProjectionError> {
    Ok(())
}

#[cfg(unix)]
fn validate_file_mode(metadata: &fs::Metadata, path: &Path) -> Result<(), ShadowProjectionError> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ShadowProjectionError::Chunk(format!(
            "projection chunk permissions are not restrictive: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_file_mode(_metadata: &fs::Metadata, _path: &Path) -> Result<(), ShadowProjectionError> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ShadowProjectionError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(chunk_io)
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ShadowProjectionError> {
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn chunk_io(error: std::io::Error) -> ShadowProjectionError {
    ShadowProjectionError::Chunk(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc, thread, time::Duration};

    use super::*;
    use crate::session_replay::ReplayEnd;

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/session-semantic-v1"
    );
    const SESSION_ID: &str = "fixture-session";
    const STREAM_ID: Uuid = Uuid::from_u128(0x10000000_0000_4000_8000_000000000001);

    fn wait_until(mut condition: impl FnMut() -> bool) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("projection worker condition was not reached");
    }

    fn live_supervisor(
        directory: &tempfile::TempDir,
        session_id: &str,
    ) -> crate::runtime_supervisor::InteractiveRuntimeSupervisor {
        let authority = crate::session_authority::SessionAuthority::open(
            &directory.path().join(format!("{session_id}.json")),
            session_id,
            "workspace",
            "generation",
            crate::session_authority::ActorIdentity {
                principal: "test".into(),
                ingress: "test".into(),
            },
            "2026-08-22T00:00:00Z",
        )
        .unwrap();
        crate::runtime_supervisor::InteractiveRuntimeSupervisor::with_authority(authority).unwrap()
    }

    fn replay_fixture(name: &str, end: ReplayEnd) -> (tempfile::TempDir, SessionReplay, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = directory.path().join("session.json");
        fs::copy(
            Path::new(FIXTURES).join(name),
            directory.path().join("session.authority.jsonl"),
        )
        .unwrap();
        let replay = SessionReplay::replay_prefix(&snapshot, SESSION_ID, STREAM_ID, end).unwrap();
        let root = directory.path().join("projections");
        (directory, replay, root)
    }

    #[test]
    fn four_projectors_publish_exact_ids_and_deterministic_rebuilds() {
        let (_directory, replay, root) = replay_fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::EndOfStream,
        );
        let coordinator = SessionProjectionCoordinator::open(&root).unwrap();
        let first = coordinator.publish(&replay, &ALL_SHADOW_PROJECTORS);
        assert!(first.iter().all(|report| matches!(
            report.status,
            ProjectorPublicationStatus::Published(ref outcome) if !outcome.idempotent
        )));
        assert_eq!(
            first
                .iter()
                .map(|report| report.projector.id())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "session.provider-history",
                "session.transcript",
                "session.frontend-snapshot",
                "session.compaction-checkpoint",
            ])
        );
        let bytes = ALL_SHADOW_PROJECTORS
            .map(|projector| fs::read(root.join(projector.id()).join("output.bin")).unwrap());
        let second = coordinator.publish(&replay, &ALL_SHADOW_PROJECTORS);
        assert!(second.iter().all(|report| matches!(
            report.status,
            ProjectorPublicationStatus::Published(ref outcome) if outcome.idempotent
        )));
        assert_eq!(
            bytes,
            ALL_SHADOW_PROJECTORS.map(|projector| {
                fs::read(root.join(projector.id()).join("output.bin")).unwrap()
            })
        );
    }

    #[test]
    fn worker_startup_catches_up_and_crash_restart_republishes_idempotently() {
        let directory = tempfile::tempdir().unwrap();
        let session_id = "worker-restart";
        let supervisor = live_supervisor(&directory, session_id);
        wait_until(|| {
            supervisor
                .projection_worker_snapshot()
                .is_some_and(|snapshot| snapshot.runs >= 1 && snapshot.worker_error.is_none())
        });
        let root = directory.path().join(format!("{session_id}.projections"));
        let cursors = ALL_SHADOW_PROJECTORS
            .map(|projector| fs::read(root.join(projector.id()).join("cursor.json")).unwrap());
        drop(supervisor);

        let restarted = live_supervisor(&directory, session_id);
        wait_until(|| {
            restarted
                .projection_worker_snapshot()
                .is_some_and(|snapshot| snapshot.runs >= 1 && snapshot.worker_error.is_none())
        });
        assert_eq!(
            cursors,
            ALL_SHADOW_PROJECTORS.map(|projector| {
                fs::read(root.join(projector.id()).join("cursor.json")).unwrap()
            })
        );
    }

    #[test]
    fn worker_coalesces_duplicate_hints_and_publishes_latest_frontier() {
        let directory = tempfile::tempdir().unwrap();
        let supervisor = live_supervisor(&directory, "worker-coalesce");
        wait_until(|| {
            supervisor
                .projection_worker_snapshot()
                .is_some_and(|value| value.runs >= 1)
        });
        let authority = supervisor.invocation_authority().unwrap();
        let baseline_runs = supervisor.projection_worker_snapshot().unwrap().runs;
        for index in 0..8 {
            authority
                .admit_prompt(
                    Uuid::new_v4(),
                    "2026-08-22T00:00:01Z",
                    crate::session_authority::PromptAdmitted {
                        submission_id: Uuid::new_v4(),
                        prompt_id: Uuid::new_v4(),
                        principal: "test".into(),
                        ingress: "test".into(),
                        queue_mode: crate::session_authority::QueueMode::UntilReady,
                        content: crate::session_authority::PromptContent {
                            text: format!("prompt {index}"),
                            attachments: Vec::new(),
                        },
                        metadata: serde_json::json!({}),
                    },
                )
                .unwrap();
        }
        let latest = authority.state().last_sequence;
        for _ in 0..32 {
            supervisor.projection_wake_for_test().unwrap().hint(false);
        }
        supervisor.flush_shadow_projections();
        wait_until(|| {
            supervisor
                .projection_worker_snapshot()
                .is_some_and(|value| value.last_frontier_sequence == Some(latest))
        });
        assert!(supervisor.projection_worker_snapshot().unwrap().runs <= baseline_runs + 2);
    }

    #[test]
    fn terminal_hint_flushes_and_projection_work_does_not_block_authority() {
        use crate::runtime_prompt::{ControlSurface, QueueMode, RuntimeActor};

        let directory = tempfile::tempdir().unwrap();
        let mut supervisor = live_supervisor(&directory, "worker-terminal");
        wait_until(|| {
            supervisor
                .projection_worker_snapshot()
                .is_some_and(|value| value.runs >= 1)
        });
        supervisor
            .admit_prompt(
                "terminal".into(),
                Vec::new(),
                RuntimeActor::tui(),
                ControlSurface::Tui,
                crate::operator_commands::PromptMetadata::default(),
                Some(QueueMode::UntilReady),
            )
            .unwrap();
        supervisor.start_next_turn().unwrap().unwrap();
        let identity = supervisor.current_identity().unwrap();

        let snapshot_state = supervisor.projection_snapshot_state_for_test().unwrap();
        let snapshot_lock = snapshot_state.lock().unwrap();
        let started = Instant::now();
        supervisor
            .submit_loop_terminal_intent(crate::runtime_turn::LoopTerminalIntent {
                identity,
                outcome: crate::runtime_turn::RuntimeTurnOutcome::Completed,
                reason_code: "test_completed".into(),
            })
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        let latest = supervisor
            .invocation_authority()
            .unwrap()
            .state()
            .last_sequence;
        drop(snapshot_lock);
        wait_until(|| {
            supervisor
                .projection_worker_snapshot()
                .is_some_and(|value| value.last_frontier_sequence == Some(latest))
        });
    }

    #[test]
    fn sessionless_supervisor_has_no_projection_worker() {
        let supervisor = crate::runtime_supervisor::InteractiveRuntimeSupervisor::default();
        assert!(supervisor.projection_worker_snapshot().is_none());
        assert!(supervisor.projection_start_error().is_none());
    }

    #[test]
    fn stale_tail_advances_each_projector_independently() {
        let (directory, prefix, root) = replay_fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::Sequence(2),
        );
        let coordinator = SessionProjectionCoordinator::open(&root).unwrap();
        coordinator.publish(&prefix, &ALL_SHADOW_PROJECTORS);
        let replay = SessionReplay::replay_prefix(
            &directory.path().join("session.json"),
            SESSION_ID,
            STREAM_ID,
            ReplayEnd::EndOfStream,
        )
        .unwrap();
        let reports = coordinator.publish(&replay, &ALL_SHADOW_PROJECTORS);
        assert!(reports.iter().all(|report| matches!(
            report.status,
            ProjectorPublicationStatus::Published(ref outcome)
                if outcome.frontier.authority == *replay.frontier()
        )));
    }

    #[test]
    fn coordinator_publishes_legacy_mixed_and_full_availability_envelopes() {
        for fixture in [
            "slice-1-closed.authority.jsonl",
            "mixed-legacy-full.authority.jsonl",
            "full-spine-crash-prefix.authority.jsonl",
        ] {
            let (_directory, replay, root) = replay_fixture(fixture, ReplayEnd::EndOfStream);
            let coordinator = SessionProjectionCoordinator::open(&root).unwrap();
            let reports = coordinator.publish(&replay, &ALL_SHADOW_PROJECTORS);
            assert!(
                reports.iter().all(|report| matches!(
                    report.status,
                    ProjectorPublicationStatus::Published(_)
                ))
            );
            for projector in ALL_SHADOW_PROJECTORS {
                let envelope: ProjectionEnvelopeV1 = serde_json::from_slice(
                    &fs::read(root.join(projector.id()).join("output.bin")).unwrap(),
                )
                .unwrap();
                envelope.validate().unwrap();
                assert_eq!(envelope.projector_id, projector.dto_id());
            }
        }
    }

    #[test]
    fn corrupt_chunk_is_repaired_without_blocking_peer_projectors() {
        let (_directory, replay, root) = replay_fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::EndOfStream,
        );
        let coordinator = SessionProjectionCoordinator::open(&root).unwrap();
        let model = SessionProjectionModel::from_replay(&replay).unwrap();
        let built = build_projection(&model, ShadowProjector::Transcript).unwrap();
        let (digest, _) = built.chunks.first().unwrap();
        let store = ImmutableChunkStore::open(&root, ShadowProjector::Transcript).unwrap();
        let path = store.path(digest).unwrap();
        fs::write(&path, b"corrupt").unwrap();
        let reports = coordinator.publish(&replay, &ALL_SHADOW_PROJECTORS);
        assert!(matches!(
            reports[1].status,
            ProjectorPublicationStatus::Published(_)
        ));
        assert!(
            reports
                .iter()
                .all(|report| matches!(report.status, ProjectorPublicationStatus::Published(_)))
        );
        assert!(
            root.join(ShadowProjector::Transcript.id())
                .join("cursor.json")
                .exists()
        );
        assert_eq!(
            fs::read(path.with_extension("corrupt")).unwrap(),
            b"corrupt"
        );
    }

    #[test]
    fn chunk_corruption_after_commit_repairs_and_preserves_output_and_cursor() {
        let (_directory, replay, root) = replay_fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::EndOfStream,
        );
        let coordinator = SessionProjectionCoordinator::open(&root).unwrap();
        let projector = ShadowProjector::Transcript;
        let reports = coordinator.publish(&replay, &[projector]);
        assert!(matches!(
            reports[0].status,
            ProjectorPublicationStatus::Published(_)
        ));
        let directory = root.join(projector.id());
        let output_before = fs::read(directory.join("output.bin")).unwrap();
        let cursor_before = fs::read(directory.join("cursor.json")).unwrap();
        let model = SessionProjectionModel::from_replay(&replay).unwrap();
        let built = build_projection(&model, projector).unwrap();
        let (digest, _) = built.chunks.first().unwrap();
        let store = ImmutableChunkStore::open(&root, projector).unwrap();
        fs::write(store.path(digest).unwrap(), b"corrupt").unwrap();

        let reports = coordinator.publish(&replay, &[projector]);
        assert!(matches!(
            reports[0].status,
            ProjectorPublicationStatus::Published(ref outcome) if outcome.idempotent
        ));
        assert_eq!(
            fs::read(directory.join("output.bin")).unwrap(),
            output_before
        );
        assert_eq!(
            fs::read(directory.join("cursor.json")).unwrap(),
            cursor_before
        );
    }

    #[test]
    fn one_cursor_identity_mismatch_rebuilds_without_rewriting_peers() {
        let (_directory, replay, root) = replay_fixture(
            "full-spine-crash-prefix.authority.jsonl",
            ReplayEnd::EndOfStream,
        );
        let coordinator = SessionProjectionCoordinator::open(&root).unwrap();
        coordinator.publish(&replay, &ALL_SHADOW_PROJECTORS);
        let mismatched = ShadowProjector::FrontendSnapshot;
        let cursor_path = root.join(mismatched.id()).join("cursor.json");
        let mut cursor: serde_json::Value =
            serde_json::from_slice(&fs::read(&cursor_path).unwrap()).unwrap();
        cursor["projector_id"] = serde_json::json!("other");
        fs::write(&cursor_path, serde_json::to_vec(&cursor).unwrap()).unwrap();

        let reports = coordinator.publish(&replay, &ALL_SHADOW_PROJECTORS);
        for report in reports {
            match report.status {
                ProjectorPublicationStatus::Published(outcome)
                    if report.projector == mismatched =>
                {
                    assert!(!outcome.idempotent);
                }
                ProjectorPublicationStatus::Published(outcome) => assert!(outcome.idempotent),
                ProjectorPublicationStatus::Failed { error, .. } => {
                    panic!("unexpected publication failure: {error}")
                }
            }
        }
    }

    #[test]
    fn immutable_chunks_deduplicate_concurrently_and_reject_missing_or_corrupt_data() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("projections");
        let coordinator = SessionProjectionCoordinator::open(&root).unwrap();
        drop(coordinator);
        let store =
            Arc::new(ImmutableChunkStore::open(&root, ShadowProjector::ProviderHistory).unwrap());
        let bytes = b"canonical chunk";
        let digest = sha256(bytes);
        let handles = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let digest = digest.clone();
                thread::spawn(move || store.put(&digest, bytes))
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        let path = store.path(&digest).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(&path).unwrap();
        assert!(verify_chunk(&path, &digest, bytes).is_err());
        fs::write(&path, b"wrong").unwrap();
        assert!(store.put(&digest, bytes).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn chunk_paths_modes_and_symlinks_are_secure() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("projections");
        SessionProjectionCoordinator::open(&root).unwrap();
        let store = ImmutableChunkStore::open(&root, ShadowProjector::Transcript).unwrap();
        let bytes = b"secure";
        let digest = sha256(bytes);
        store.put(&digest, bytes).unwrap();
        assert_eq!(
            fs::metadata(&store.directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(store.path(&digest).unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(store.path("../escape").is_err());

        let linked = store.path(&sha256(b"linked")).unwrap();
        symlink(directory.path().join("outside"), &linked).unwrap();
        assert!(store.put(&sha256(b"linked"), b"linked").is_err());
    }

    #[test]
    fn current_consumers_do_not_reference_shadow_projection_owner() {
        for (name, source) in [
            ("provider dispatch", include_str!("providers.rs")),
            ("ConversationState", include_str!("conversation.rs")),
            ("session save", include_str!("session.rs")),
            ("transcript command", include_str!("session_commands.rs")),
            ("compaction", include_str!("features/auto_compact.rs")),
            ("TUI", include_str!("tui/mod.rs")),
            ("ACP", include_str!("acp.rs")),
            ("ACP worker", include_str!("acp_worker.rs")),
            ("Web/IPC", include_str!("control_runtime.rs")),
            ("Web", include_str!("web/mod.rs")),
        ] {
            assert!(
                !source.contains("session_shadow_projection")
                    && !source.contains("session.provider-history")
                    && !source.contains("session.transcript")
                    && !source.contains("session.frontend-snapshot")
                    && !source.contains("session.compaction-checkpoint"),
                "{name} must remain independent of Slice 5.3 shadow outputs"
            );
        }
    }
}

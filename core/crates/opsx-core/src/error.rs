//! Error types for opsx-core.

#[derive(Debug, thiserror::Error)]
pub enum OpsxError {
    #[error("invalid transition: {entity} cannot go from '{from}' to '{to}'")]
    InvalidTransition {
        entity: String,
        from: String,
        to: String,
    },

    #[error("precondition failed: {0}")]
    PreconditionFailed(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("milestone '{0}' is frozen — no new scope can be added")]
    MilestoneFrozen(String),

    #[error("store error: {0}")]
    StoreError(String),

    #[error("schema version mismatch: expected {expected}, got {got}")]
    SchemaMismatch { expected: u32, got: u32 },
}

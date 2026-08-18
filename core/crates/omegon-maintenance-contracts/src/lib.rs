//! Versioned wire contracts shared by Omegon and its maintenance companion.
//!
//! This crate owns data framing, canonical key derivation, advisory lock
//! semantics, and pure recovery classification. It intentionally contains no
//! normal runtime startup, contribution loading, or mutation orchestration.

mod canonical;
mod key;
mod lock;
mod records;
mod recovery;
mod selector;

pub use canonical::{canonical_json, parse_record};
pub use key::{
    AuthorityKey, CommandSemanticsV1, canonical_digest, command_fingerprint,
    contribution_domain_key, derive_key, entry_key, path_key, resource_domain_key, scope_key,
    session_domain_key, session_key, workspace_key,
};
pub use lock::{LockMode, ProtocolLock};
pub use records::*;
pub use recovery::{
    DetachObservation, ReconciliationDecision, RecordObservation, reconcile_detach,
    reconcile_record,
};
pub use selector::{ContributionSelector, ListScope, resolve_list_scope};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("record exceeds the {MAX_RECORD_BYTES}-byte limit")]
    RecordTooLarge,
    #[error("record is not valid UTF-8 JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("record contains a duplicate object key: {0}")]
    DuplicateKey(String),
    #[error("record contains a floating-point number")]
    FloatingPoint,
    #[error("record has an invalid authority key: {0}")]
    InvalidKey(String),
    #[error("record uses unsupported schema version {0}")]
    UnsupportedSchema(u32),
    #[error("record kind mismatch: expected {expected}, got {actual}")]
    RecordKind {
        expected: &'static str,
        actual: String,
    },
    #[error("invalid protocol value: {0}")]
    InvalidValue(String),
    #[error("protocol lock operation failed: {0}")]
    Lock(#[source] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ContractError>;

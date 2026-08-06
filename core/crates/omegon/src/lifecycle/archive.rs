//! Binary compatibility façade for opsx-owned archive transactions.

#[cfg(test)]
pub use omegon_opsx::archive::{OpenSpecArchiveTransaction, archive_tx_path, write_archive_tx};
pub use omegon_opsx::archive::{
    archive_content_with_tx, recover_archive_transactions, remove_archive_tx,
    rollback_archive_content,
};

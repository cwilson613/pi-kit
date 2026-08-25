//! Binary compatibility façade for opsx-owned archive transactions.

pub use omegon_opsx::archive::recover_archive_transactions;
#[cfg(test)]
pub use omegon_opsx::archive::{
    OpenSpecArchiveTransaction, archive_content_with_tx, archive_tx_path, remove_archive_tx,
    rollback_archive_content, write_archive_tx,
};

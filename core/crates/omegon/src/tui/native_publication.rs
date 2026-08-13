use std::time::{Duration, Instant};

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

pub(super) const DEFAULT_MAX_BYTES: usize = 64 * 1024;
pub(super) const DEFAULT_MAX_RECORDS: usize = 64;
pub(super) const DEFAULT_MAX_ROWS: usize = 1_000;
pub(super) const DEFAULT_PREPARE_BUDGET: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PublicationIdentity {
    pub(super) attachment_epoch: u64,
    pub(super) base_revision: u64,
    pub(super) target_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreparedPublication {
    pub(super) identity: PublicationIdentity,
    pub(super) range: std::ops::Range<usize>,
    pub(super) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeliveryResult {
    Committed,
    KnownFailure,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettlementError {
    StaleAttachment,
    NonContiguous,
    WrongRevision,
    Degraded,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PreparationBudget {
    pub(super) max_bytes: usize,
    pub(super) max_records: usize,
    pub(super) max_rows: usize,
    pub(super) max_elapsed: Duration,
}

impl Default for PreparationBudget {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            max_records: DEFAULT_MAX_RECORDS,
            max_rows: DEFAULT_MAX_ROWS,
            max_elapsed: DEFAULT_PREPARE_BUDGET,
        }
    }
}

#[derive(Debug)]
pub(super) struct NativePublicationState {
    attachment_epoch: u64,
    committed_revision: u64,
    committed_prefix_revision: u64,
    committed_byte: usize,
    degraded: bool,
}

impl Default for NativePublicationState {
    fn default() -> Self {
        Self {
            attachment_epoch: 1,
            committed_revision: 0,
            committed_prefix_revision: FNV_OFFSET,
            committed_byte: 0,
            degraded: false,
        }
    }
}

impl NativePublicationState {
    pub(super) fn begin_attachment(&mut self) {
        self.attachment_epoch = self
            .attachment_epoch
            .checked_add(1)
            .expect("native publication attachment epoch exhausted");
        self.committed_revision = 0;
        self.committed_prefix_revision = FNV_OFFSET;
        self.committed_byte = 0;
        self.degraded = false;
    }

    pub(super) fn prepare(
        &self,
        canonical: &str,
        target_revision: u64,
        budget: PreparationBudget,
    ) -> Result<Option<PreparedPublication>, SettlementError> {
        if self.degraded {
            return Err(SettlementError::Degraded);
        }
        if self.committed_byte > canonical.len() || !canonical.is_char_boundary(self.committed_byte)
        {
            return Err(SettlementError::NonContiguous);
        }
        if self.committed_byte > 0
            && canonical_prefix_revision(&canonical[..self.committed_byte])
                != self.committed_prefix_revision
        {
            return Err(SettlementError::NonContiguous);
        }
        if self.committed_byte == canonical.len() {
            return Ok(None);
        }

        let started = Instant::now();
        let suffix = &canonical[self.committed_byte..];
        let mut bytes = 0;
        let mut rows = 0;
        for (records, record) in suffix.split_inclusive('\n').enumerate() {
            if records >= budget.max_records
                || rows >= budget.max_rows
                || started.elapsed() >= budget.max_elapsed
            {
                break;
            }
            let remaining = budget.max_bytes.saturating_sub(bytes);
            if remaining == 0 {
                break;
            }
            let take = floor_char_boundary(record, remaining.min(record.len()));
            if take == 0 {
                break;
            }
            bytes += take;
            rows += record[..take]
                .chars()
                .filter(|ch| *ch == '\n')
                .count()
                .max(1);
            if take < record.len() || bytes >= budget.max_bytes {
                break;
            }
        }
        if bytes == 0 {
            return Ok(None);
        }
        let end = self.committed_byte + bytes;
        Ok(Some(PreparedPublication {
            identity: PublicationIdentity {
                attachment_epoch: self.attachment_epoch,
                base_revision: self.committed_revision,
                target_revision,
            },
            range: self.committed_byte..end,
            text: canonical[self.committed_byte..end].to_owned(),
        }))
    }

    pub(super) fn settle(
        &mut self,
        prepared: &PreparedPublication,
        result: DeliveryResult,
    ) -> Result<(), SettlementError> {
        if self.degraded {
            return Err(SettlementError::Degraded);
        }
        if prepared.identity.attachment_epoch != self.attachment_epoch {
            return Err(SettlementError::StaleAttachment);
        }
        if prepared.identity.base_revision != self.committed_revision {
            return Err(SettlementError::WrongRevision);
        }
        if prepared.range.start != self.committed_byte {
            return Err(SettlementError::NonContiguous);
        }
        match result {
            DeliveryResult::Committed => {
                self.committed_byte = prepared.range.end;
                self.committed_prefix_revision = canonical_prefix_revision_from_parts(
                    self.committed_prefix_revision,
                    prepared.text.as_bytes(),
                );
                self.committed_revision = prepared.identity.target_revision;
            }
            DeliveryResult::KnownFailure => {}
            DeliveryResult::Ambiguous => self.degraded = true,
        }
        Ok(())
    }

    pub(super) fn is_degraded(&self) -> bool {
        self.degraded
    }

    #[cfg(test)]
    fn committed_byte(&self) -> usize {
        self.committed_byte
    }
}

fn canonical_prefix_revision(text: &str) -> u64 {
    canonical_prefix_revision_from_parts(FNV_OFFSET, text.as_bytes())
}

fn canonical_prefix_revision_from_parts(mut revision: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        revision ^= u64::from(*byte);
        revision = revision.wrapping_mul(FNV_PRIME);
    }
    revision
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_budget(max_bytes: usize) -> PreparationBudget {
        PreparationBudget {
            max_bytes,
            max_records: 10,
            max_rows: 10,
            max_elapsed: Duration::from_secs(1),
        }
    }

    #[test]
    fn successful_chunk_commits_only_contiguous_range() {
        let mut state = NativePublicationState::default();
        let first = state
            .prepare("abcdef", 1, small_budget(3))
            .unwrap()
            .unwrap();
        assert_eq!(first.range, 0..3);
        state.settle(&first, DeliveryResult::Committed).unwrap();
        let second = state
            .prepare("abcdef", 2, small_budget(3))
            .unwrap()
            .unwrap();
        assert_eq!(second.range, 3..6);
    }

    #[test]
    fn known_failure_preserves_cursor() {
        let mut state = NativePublicationState::default();
        let prepared = state
            .prepare("abcdef", 1, small_budget(3))
            .unwrap()
            .unwrap();
        state
            .settle(&prepared, DeliveryResult::KnownFailure)
            .unwrap();
        assert_eq!(state.committed_byte(), 0);
    }

    #[test]
    fn stale_attachment_and_noncontiguous_ranges_cannot_commit() {
        let mut state = NativePublicationState::default();
        let stale = state
            .prepare("abcdef", 1, small_budget(3))
            .unwrap()
            .unwrap();
        state.begin_attachment();
        assert_eq!(
            state.settle(&stale, DeliveryResult::Committed),
            Err(SettlementError::StaleAttachment)
        );

        let mut noncontiguous = state
            .prepare("abcdef", 1, small_budget(3))
            .unwrap()
            .unwrap();
        noncontiguous.range = 1..3;
        assert_eq!(
            state.settle(&noncontiguous, DeliveryResult::Committed),
            Err(SettlementError::NonContiguous)
        );
    }

    #[test]
    fn growing_canonical_transcript_resumes_after_committed_prefix() {
        let mut state = NativePublicationState::default();
        let first = state.prepare("abc", 10, small_budget(3)).unwrap().unwrap();
        state.settle(&first, DeliveryResult::Committed).unwrap();

        let appended = state
            .prepare("abcdef", 11, small_budget(3))
            .unwrap()
            .unwrap();
        assert_eq!(appended.range, 3..6);
        assert_eq!(appended.text, "def");
    }

    #[test]
    fn changed_canonical_prefix_requires_snapshot_rebuild() {
        let mut state = NativePublicationState::default();
        let first = state
            .prepare("abcdef", 1, small_budget(3))
            .unwrap()
            .unwrap();
        state.settle(&first, DeliveryResult::Committed).unwrap();

        assert_eq!(
            state.prepare("xyzdef", 2, small_budget(3)),
            Err(SettlementError::NonContiguous)
        );
    }

    #[test]
    fn ambiguous_delivery_disables_blind_retry() {
        let mut state = NativePublicationState::default();
        let prepared = state
            .prepare("abcdef", 1, small_budget(3))
            .unwrap()
            .unwrap();
        state.settle(&prepared, DeliveryResult::Ambiguous).unwrap();
        assert!(state.is_degraded());
        assert_eq!(
            state.prepare("abcdef", 1, small_budget(3)),
            Err(SettlementError::Degraded)
        );
    }

    #[test]
    fn oversized_unicode_chunk_ends_on_utf8_boundary_and_resumes() {
        let mut state = NativePublicationState::default();
        let canonical = "αβγδ";
        let first = state
            .prepare(canonical, 1, small_budget(5))
            .unwrap()
            .unwrap();
        assert_eq!(first.text, "αβ");
        assert!(canonical.is_char_boundary(first.range.end));
        state.settle(&first, DeliveryResult::Committed).unwrap();
        let second = state
            .prepare(canonical, 2, small_budget(5))
            .unwrap()
            .unwrap();
        assert_eq!(second.text, "γδ");
    }
}

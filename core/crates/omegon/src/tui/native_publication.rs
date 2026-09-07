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
    pub(super) automatic: AutomaticPublication,
    attachment_epoch: u64,
    committed_revision: u64,
    committed_prefix_revision: u64,
    committed_byte: usize,
    degraded: bool,
}

impl Default for NativePublicationState {
    fn default() -> Self {
        Self {
            automatic: AutomaticPublication::default(),
            attachment_epoch: 1,
            committed_revision: 0,
            committed_prefix_revision: FNV_OFFSET,
            committed_byte: 0,
            degraded: false,
        }
    }
}

/// Cursor into the canonical source, not another transcript. A batch owns only
/// the text that can fit in one bounded terminal insertion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InlineCursor {
    generation: u64,
    notice: Option<String>,
    segment: usize,
    field: usize,
    byte: usize,
    detail: Option<crate::surfaces::layout::UiPresentationLevel>,
    scan: Option<OutcomeScan>,
    summary: Option<String>,
    emitted_turn: Option<(Option<u64>, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OutcomeScan {
    index: usize,
    coordinate: Option<(Option<u64>, u32)>,
    summary: crate::surfaces::episodes::OutcomeSummary,
}

#[derive(Debug, Default)]
pub(super) struct AutomaticPublication {
    cursor: InlineCursor,
    degraded: bool,
}

pub(super) struct InlineBatch {
    base: InlineCursor,
    next: InlineCursor,
    pub(super) lines: Vec<String>,
}

pub(super) fn safe_inline_text(text: &str) -> String {
    text.split('\n')
        .map(super::segments::strip_terminal_control)
        .collect::<Vec<_>>()
        .join("\n")
}

impl AutomaticPublication {
    pub(super) fn generation(&self) -> u64 {
        self.cursor.generation
    }
    pub(super) fn has_pending(&self, boundary: usize) -> bool {
        self.cursor.notice.is_some() || self.cursor.segment < boundary
    }
    pub(super) fn apply_prune(&mut self, prune: &super::conversation::PublicationPrune) -> bool {
        if self.cursor.generation != prune.from_generation {
            return false;
        }
        for index in &prune.removed {
            if *index < self.cursor.segment {
                self.cursor.segment -= 1;
            } else if *index == self.cursor.segment && self.cursor.notice.is_none() {
                self.cursor.field = 0;
                self.cursor.byte = 0;
                self.cursor.detail = None;
                self.cursor.summary = None;
                self.cursor.scan = None;
            }
            if let Some(scan) = &mut self.cursor.scan
                && *index < scan.index
            {
                scan.index -= 1;
            }
        }
        // Also invalidate prepared batches when deletion was after the cursor.
        self.cursor.generation = prune.to_generation;
        true
    }

    pub(super) fn reconcile(
        &mut self,
        generation: u64,
        changed_at: usize,
        boundary: usize,
    ) -> bool {
        if changed_at > self.cursor.segment
            || (changed_at == self.cursor.segment
                && self.cursor.field == 0
                && self.cursor.byte == 0)
        {
            self.cursor.generation = generation;
            if self.cursor.scan.is_some() {
                self.cursor.scan = None;
                self.cursor.summary = None;
            }
            false
        } else {
            self.attach(generation, boundary);
            self.cursor.notice = Some(
                "Conversation boundary changed · previous output remains in scrollback".into(),
            );
            true
        }
    }
    pub(super) fn source_replaced(&mut self, generation: u64, boundary: usize) {
        self.attach(generation, boundary);
        self.cursor.notice =
            Some("Conversation boundary changed · previous output remains in scrollback".into());
    }

    pub(super) fn attach(&mut self, generation: u64, boundary: usize) {
        self.cursor = InlineCursor {
            generation,
            segment: boundary,
            notice: (boundary > 0).then(|| {
                format!("Session attached · {boundary} prior records available in fullscreen")
            }),
            ..Default::default()
        };
        // Uncertain physical output is not made retryable by a source rewrite.
    }

    pub(super) fn is_degraded(&self) -> bool {
        self.degraded
    }

    pub(super) fn prepare(
        &self,
        generation: u64,
        segments: &[super::segments::Segment],
        finalized: usize,
        detail: crate::surfaces::layout::UiPresentationLevel,
        width: u16,
        budget: PreparationBudget,
    ) -> Option<InlineBatch> {
        let started = Instant::now();
        self.prepare_with_elapsed(
            generation,
            segments,
            finalized,
            detail,
            width,
            budget,
            || started.elapsed(),
        )
    }

    #[allow(clippy::too_many_arguments)] // Same boundary with an injected cooperative clock for deterministic tests.
    fn prepare_with_elapsed(
        &self,
        generation: u64,
        segments: &[super::segments::Segment],
        finalized: usize,
        detail: crate::surfaces::layout::UiPresentationLevel,
        width: u16,
        budget: PreparationBudget,
        mut elapsed: impl FnMut() -> Duration,
    ) -> Option<InlineBatch> {
        use super::segments::SegmentContent;
        use unicode_width::UnicodeWidthChar;
        if self.degraded || width < 2 || generation != self.cursor.generation {
            return None;
        }
        let mut next = self.cursor.clone();
        let mut lines = Vec::new();
        let mut line = String::new();
        let mut cells = 0;
        let mut bytes = 0;
        let mut records = 0;
        let max_rows = budget.max_rows.min(65_536 / usize::from(width));
        'records: while (next.notice.is_some() || next.segment < segments.len())
            && records < budget.max_records
            && lines.len() < max_rows
            && bytes < budget.max_bytes
            && elapsed() < budget.max_elapsed
        {
            let notice = next.notice.as_ref().map(super::segments::Segment::system);
            let segment = if let Some(ref notice) = notice {
                notice
            } else {
                &segments[next.segment]
            };
            if notice.is_none()
                && next.segment >= finalized
                && !matches!(
                    segment.content,
                    SegmentContent::UserPrompt { .. } | SegmentContent::TurnSeparator
                )
            {
                break;
            }
            let level = *next.detail.get_or_insert(detail);
            if level != crate::surfaces::layout::UiPresentationLevel::Full
                && matches!(segment.content, SegmentContent::ToolCard { .. })
            {
                let coordinate = segment
                    .meta
                    .turn
                    .map(|turn| (segment.meta.runtime_turn, turn));
                if coordinate.is_some() && next.emitted_turn == coordinate {
                    next.segment += 1;
                    next.detail = None;
                    records += 1;
                    continue;
                }
                if next.summary.is_none() {
                    let scan = next.scan.get_or_insert_with(|| OutcomeScan {
                        index: next.segment,
                        coordinate,
                        summary: Default::default(),
                    });
                    loop {
                        let candidate = segments.get(scan.index).filter(|_| scan.index < finalized);
                        let done = candidate.is_none()
                            || (scan.index > next.segment
                                && (coordinate.is_none()
                                    || candidate.is_some_and(|s| {
                                        matches!(s.content, SegmentContent::UserPrompt { .. })
                                            || s.meta.turn.is_some_and(|turn| {
                                                Some((s.meta.runtime_turn, turn)) != coordinate
                                            })
                                    })));
                        if done {
                            next.summary = Some(scan.summary.display());
                            next.scan = None;
                            break;
                        }
                        if records >= budget.max_records || elapsed() >= budget.max_elapsed {
                            break 'records;
                        }
                        let candidate = candidate?;
                        if let SegmentContent::ToolCard {
                            name,
                            result_summary,
                            is_error,
                            ..
                        } = &candidate.content
                        {
                            let cost = name.len().min(512)
                                + result_summary.as_ref().map_or(0, |v| v.len().min(512));
                            if bytes + cost > budget.max_bytes {
                                break 'records;
                            }
                            bytes += cost;
                            scan.summary
                                .observe(name, result_summary.as_deref(), *is_error);
                        }
                        scan.index += 1;
                        records += 1;
                    }
                }
            }
            // Borrow source fields; do not export/clone an unbounded record.
            let fields: Vec<&str> = match &segment.content {
                SegmentContent::UserPrompt { text } => vec!["› ", text, "\n"],
                SegmentContent::AssistantText { text, thinking, .. } => {
                    if level == crate::surfaces::layout::UiPresentationLevel::Full
                        && !thinking.is_empty()
                    {
                        vec!["Thinking: ", thinking, "\n", text, "\n"]
                    } else {
                        vec![text, "\n"]
                    }
                }
                SegmentContent::ToolCard {
                    name,
                    args_summary,
                    detail_args,
                    result_summary,
                    detail_result,
                    is_error,
                    ..
                } => {
                    if let Some(summary) = next.summary.as_deref() {
                        vec![summary, "\n"]
                    } else {
                        let result = if level == crate::surfaces::layout::UiPresentationLevel::Full
                        {
                            detail_result.as_deref().or(result_summary.as_deref())
                        } else {
                            result_summary.as_deref()
                        };
                        vec![
                            if *is_error { "✗ " } else { "✓ " },
                            name,
                            " ",
                            detail_args
                                .as_deref()
                                .or(args_summary.as_deref())
                                .unwrap_or(""),
                            ": ",
                            result.unwrap_or("completed"),
                            "\n",
                        ]
                    }
                }
                SegmentContent::SystemNotification { text }
                | SegmentContent::LifecycleEvent { text, .. } => vec![text, "\n"],
                SegmentContent::PeerAgentText { label, text, .. } => vec![label, ": ", text, "\n"],
                SegmentContent::OperatorCopyBlock { label, text, .. } => {
                    vec![label, ":\n", text, "\n"]
                }
                SegmentContent::Image { path, alt, .. } => vec![
                    "[image] ",
                    alt,
                    " · ",
                    path.to_str().unwrap_or("non-UTF-8 path"),
                    "\n",
                ],
                SegmentContent::SkillEvent {
                    active_ref,
                    reason,
                    resolution,
                    ..
                } => vec![
                    "Skill: ", active_ref, " · ", reason, " · ", resolution, "\n",
                ],
                SegmentContent::TurnSeparator => vec!["\n"],
            };
            while next.field < fields.len() {
                let field = fields[next.field];
                if next.byte > field.len() || !field.is_char_boundary(next.byte) {
                    return None;
                }
                for ch in field[next.byte..].chars() {
                    if bytes + ch.len_utf8() > budget.max_bytes
                        || lines.len() >= max_rows
                        || elapsed() >= budget.max_elapsed
                    {
                        break 'records;
                    }
                    let cell_width = ch.width().unwrap_or(0);
                    if ch != '\n' && cells + cell_width > usize::from(width) {
                        lines.push(safe_inline_text(&std::mem::take(&mut line)));
                        cells = 0;
                        if lines.len() >= max_rows {
                            break 'records;
                        }
                    }
                    next.byte += ch.len_utf8();
                    bytes += ch.len_utf8();
                    if ch == '\n' {
                        lines.push(safe_inline_text(&std::mem::take(&mut line)));
                        cells = 0;
                    } else {
                        line.push(ch);
                        cells += cell_width;
                    }
                }
                next.field += 1;
                next.byte = 0;
            }
            records += 1;
            if next.summary.take().is_some() {
                next.emitted_turn = segment
                    .meta
                    .turn
                    .map(|turn| (segment.meta.runtime_turn, turn));
            }
            if next.notice.take().is_none() {
                next.segment += 1;
            }
            next.field = 0;
            next.detail = None;
        }
        if !line.is_empty() {
            lines.push(safe_inline_text(&line));
        }
        if next == self.cursor {
            None
        } else {
            Some(InlineBatch {
                base: self.cursor.clone(),
                next,
                lines,
            })
        }
    }

    pub(super) fn settle(&mut self, batch: InlineBatch, outcome: DeliveryResult) -> bool {
        if self.degraded || batch.base != self.cursor {
            return false;
        }
        match outcome {
            DeliveryResult::Committed => self.cursor = batch.next,
            DeliveryResult::KnownFailure => {}
            DeliveryResult::Ambiguous => self.degraded = true,
        }
        true
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

    fn prepare_view(
        cursor: &AutomaticPublication,
        view: &super::super::conversation::ConversationView,
        budget: PreparationBudget,
    ) -> InlineBatch {
        cursor
            .prepare(
                view.publication_generation(),
                view.segments(),
                view.segments().len(),
                crate::surfaces::layout::UiPresentationLevel::Active,
                120,
                budget,
            )
            .unwrap()
    }

    #[test]
    fn notification_prune_preserves_surviving_partial_record_and_rejects_stale_batch() {
        let mut view = super::super::conversation::ConversationView::new();
        for index in 0..64 {
            view.push_user(&format!("old prompt {index}"));
            view.push_system(&format!("old notice {index}"));
        }
        let assistant_index = view.segments().len();
        let text = "SURVIVING-ASSISTANT-PARTIAL-CONTENT-ONCE";
        view.append_streaming(text);
        view.finalize_message();
        let mut cursor = AutomaticPublication::default();
        cursor.cursor.segment = assistant_index;
        let first = prepare_view(&cursor, &view, inline_budget());
        let mut rendered = first.lines.concat();
        cursor.settle(first, DeliveryResult::Committed);
        let old_byte = cursor.cursor.byte;
        let stale = prepare_view(&cursor, &view, inline_budget());
        view.push_system("new 65");
        view.push_system("new 66");
        let prune = view.take_publication_prune().unwrap();
        assert!(cursor.apply_prune(&prune));
        assert_eq!(cursor.cursor.byte, old_byte);
        assert_eq!(cursor.cursor.segment, assistant_index - 2);
        assert!(!cursor.settle(stale, DeliveryResult::Committed));
        let remaining = prepare_view(&cursor, &view, PreparationBudget::default());
        rendered.push_str(&remaining.lines.concat());
        assert_eq!(rendered.matches(text).count(), 1, "{rendered}");
        assert!(!rendered.contains("old notice"));
        assert!(rendered.contains("new 65") && rendered.contains("new 66"));
    }

    #[test]
    fn notification_prune_resets_evicted_partial_record_but_not_synthetic_notice() {
        let mut view = super::super::conversation::ConversationView::new();
        view.push_system(&"OLD-LONG-CONTENT-".repeat(10));
        for index in 1..64 {
            view.push_system(&format!("retained-{index}"));
        }
        let mut cursor = AutomaticPublication::default();
        let first = prepare_view(&cursor, &view, inline_budget());
        cursor.settle(first, DeliveryResult::Committed);
        assert!(cursor.cursor.byte > 0);
        view.push_system("new 65");
        let prune = view.take_publication_prune().unwrap();
        assert!(cursor.apply_prune(&prune));
        assert_eq!(cursor.cursor.byte, 0);
        let rendered = prepare_view(&cursor, &view, PreparationBudget::default())
            .lines
            .concat();
        assert!(rendered.contains("retained-1"));
        assert!(!rendered.contains("OLD-LONG"));

        let mut synthetic = AutomaticPublication::default();
        synthetic.cursor.notice = Some("synthetic partial attachment".into());
        synthetic.cursor.byte = 7;
        assert!(synthetic.apply_prune(&prune));
        assert_eq!(synthetic.cursor.byte, 7);
    }

    #[test]
    fn notification_prune_after_cursor_invalidates_prepared_batch() {
        let mut view = super::super::conversation::ConversationView::new();
        view.push_user("UNPUBLISHED-PROMPT");
        for index in 0..64 {
            view.push_system(&format!("notice {index}"));
        }
        let mut cursor = AutomaticPublication::default();
        let batch = prepare_view(&cursor, &view, inline_budget());
        view.push_system("new notice");
        let prune = view.take_publication_prune().unwrap();
        assert!(cursor.apply_prune(&prune));
        assert_eq!(cursor.cursor.segment, 0);
        assert!(!cursor.settle(batch, DeliveryResult::Committed));
        assert!(
            prepare_view(&cursor, &view, PreparationBudget::default())
                .lines
                .concat()
                .contains("UNPUBLISHED-PROMPT")
        );
    }

    fn inline_budget() -> PreparationBudget {
        PreparationBudget {
            max_bytes: 16,
            max_rows: 3,
            max_elapsed: Duration::from_secs(1),
            ..Default::default()
        }
    }

    #[test]
    fn automatic_chunks_bound_rows_and_preserve_unicode_source() {
        let source = "界abcé".repeat(200);
        let segments = vec![super::super::segments::Segment::system(&source)];
        let mut cursor = AutomaticPublication::default();
        let mut output = String::new();
        for _ in 0..2000 {
            let Some(batch) = cursor.prepare(
                0,
                &segments,
                1,
                crate::surfaces::layout::UiPresentationLevel::Active,
                4,
                inline_budget(),
            ) else {
                break;
            };
            assert!(batch.lines.len() <= 3);
            for line in &batch.lines {
                assert!(unicode_width::UnicodeWidthStr::width(line.as_str()) <= 4);
                output.push_str(line);
            }
            assert!(cursor.settle(batch, DeliveryResult::Committed));
        }
        assert_eq!(output, source);
        assert_eq!(cursor.cursor.segment, 1);
    }

    #[test]
    fn automatic_does_not_publish_unfinalized_response_or_replay_uncertain_delivery() {
        let mut segment = super::super::segments::Segment::assistant_text();
        if let super::super::segments::SegmentContent::AssistantText { text, .. } =
            &mut segment.content
        {
            *text = "answer".into();
        }
        let source = vec![segment];
        let mut cursor = AutomaticPublication::default();
        let prepare = |cursor: &AutomaticPublication, boundary| {
            cursor.prepare(
                0,
                &source,
                boundary,
                crate::surfaces::layout::UiPresentationLevel::Active,
                80,
                inline_budget(),
            )
        };
        assert!(prepare(&cursor, 0).is_none());
        let batch = prepare(&cursor, 1).unwrap();
        assert!(cursor.settle(batch, DeliveryResult::KnownFailure));
        assert_eq!(cursor.cursor.segment, 0);
        let batch = prepare(&cursor, 1).unwrap();
        assert!(cursor.settle(batch, DeliveryResult::Ambiguous));
        assert!(prepare(&cursor, 1).is_none());
        cursor.attach(1, 0);
        assert!(cursor.is_degraded());
    }

    #[test]
    fn automatic_rejects_stale_generation_and_zero_geometry() {
        let source = vec![super::super::segments::Segment::system("answer")];
        let mut cursor = AutomaticPublication::default();
        assert!(
            cursor
                .prepare(
                    0,
                    &source,
                    1,
                    crate::surfaces::layout::UiPresentationLevel::Full,
                    0,
                    inline_budget()
                )
                .is_none()
        );
        let batch = cursor
            .prepare(
                0,
                &source,
                1,
                crate::surfaces::layout::UiPresentationLevel::Full,
                80,
                inline_budget(),
            )
            .unwrap();
        cursor.attach(1, 1);
        assert!(!cursor.settle(batch, DeliveryResult::Committed));
    }

    #[test]
    fn automatic_resume_and_replacement_publish_only_a_bounded_boundary_notice() {
        let source = vec![super::super::segments::Segment::system(
            "OLD_HISTORY_MUST_NOT_REPLAY",
        )];
        let mut cursor = AutomaticPublication::default();
        cursor.attach(7, 1);
        let batch = cursor
            .prepare(
                7,
                &source,
                1,
                crate::surfaces::layout::UiPresentationLevel::Active,
                90,
                PreparationBudget::default(),
            )
            .unwrap();
        assert!(batch.lines.concat().contains("1 prior records"));
        assert!(!batch.lines.concat().contains("OLD_HISTORY"));
        cursor.settle(batch, DeliveryResult::Committed);
        assert!(
            cursor
                .prepare(
                    7,
                    &source,
                    1,
                    crate::surfaces::layout::UiPresentationLevel::Active,
                    90,
                    PreparationBudget::default()
                )
                .is_none()
        );
        assert!(cursor.reconcile(8, 0, 1));
        let batch = cursor
            .prepare(
                8,
                &source,
                1,
                crate::surfaces::layout::UiPresentationLevel::Active,
                90,
                PreparationBudget::default(),
            )
            .unwrap();
        assert!(
            batch
                .lines
                .concat()
                .contains("Conversation boundary changed")
        );
        assert!(!batch.lines.concat().contains("OLD_HISTORY"));
    }

    #[test]
    fn automatic_clock_limits_work_without_scanning_committed_history() {
        let mut source = (0..10000)
            .map(|_| super::super::segments::Segment::system("old"))
            .collect::<Vec<_>>();
        source.push(super::super::segments::Segment::system("new".repeat(10000)));
        let mut cursor = AutomaticPublication::default();
        cursor.cursor.segment = 10000;
        let mut ticks = 0;
        let batch = cursor
            .prepare_with_elapsed(
                0,
                &source,
                source.len(),
                crate::surfaces::layout::UiPresentationLevel::Full,
                80,
                PreparationBudget {
                    max_elapsed: Duration::from_millis(5),
                    ..Default::default()
                },
                || {
                    ticks += 1;
                    Duration::from_millis(ticks)
                },
            )
            .unwrap();
        assert_eq!(batch.next.segment, 10000);
        assert!(batch.next.byte > 0 && batch.next.byte < 10);
        assert!(!batch.lines.concat().contains("old"));
        assert!(ticks <= 5);
    }

    #[test]
    fn automatic_outcome_scan_is_bounded_and_shares_semantic_reducer() {
        use super::super::segments::{Segment, SegmentContent};
        let mut expected = crate::surfaces::episodes::OutcomeSummary::default();
        let source = (0..100)
            .map(|index| {
                let mut segment = Segment::tool_card(index.to_string(), "read");
                segment.meta.runtime_turn = Some(1);
                segment.meta.turn = Some(1);
                if let SegmentContent::ToolCard {
                    result_summary,
                    complete,
                    is_error,
                    ..
                } = &mut segment.content
                {
                    *result_summary = Some(format!("result {index}"));
                    *complete = true;
                    *is_error = index == 33;
                    expected.observe("read", result_summary.as_deref(), *is_error);
                }
                segment
            })
            .collect::<Vec<_>>();
        let mut cursor = AutomaticPublication::default();
        let budget = PreparationBudget {
            max_records: 3,
            max_elapsed: Duration::from_secs(1),
            ..Default::default()
        };
        let mut output = Vec::new();
        let mut batches = 0;
        while let Some(batch) = cursor.prepare(
            0,
            &source,
            source.len(),
            crate::surfaces::layout::UiPresentationLevel::Active,
            120,
            budget,
        ) {
            output.extend(batch.lines.clone());
            assert!(cursor.settle(batch, DeliveryResult::Committed));
            batches += 1;
            assert!(batches < 200);
        }
        assert!(batches > 30);
        assert_eq!(output.join(""), expected.display());
    }

    #[test]
    fn automatic_rewrite_distinguishes_pending_coalescence_from_replacement() {
        let mut cursor = AutomaticPublication::default();
        cursor.attach(0, 5);
        assert!(!cursor.reconcile(1, 5, 5));
        assert_eq!(cursor.cursor.segment, 5);
        assert_eq!(cursor.generation(), 1);
        assert!(cursor.reconcile(2, 0, 2));
        assert_eq!(cursor.cursor.segment, 2);
    }

    #[test]
    fn automatic_resize_and_detail_change_preserve_partial_record() {
        use super::super::segments::{Segment, SegmentContent};
        use crate::surfaces::layout::UiPresentationLevel;
        let mut segment = Segment::assistant_text();
        if let SegmentContent::AssistantText { text, thinking, .. } = &mut segment.content {
            *text = "answer ".repeat(30);
            *thinking = "thinking ".repeat(30);
        }
        let source = vec![segment];
        let mut cursor = AutomaticPublication::default();
        let first = cursor
            .prepare(
                0,
                &source,
                1,
                UiPresentationLevel::Active,
                40,
                inline_budget(),
            )
            .unwrap();
        let mut output = first.lines.concat();
        cursor.settle(first, DeliveryResult::Committed);
        for _ in 0..1000 {
            let Some(batch) = cursor.prepare(
                0,
                &source,
                1,
                UiPresentationLevel::Full,
                56,
                inline_budget(),
            ) else {
                break;
            };
            output.push_str(&batch.lines.concat());
            cursor.settle(batch, DeliveryResult::Committed);
        }
        assert_eq!(output, "answer ".repeat(30));
    }

    #[test]
    fn automatic_untrusted_control_text_cannot_select_a_terminal_buffer() {
        let source = vec![super::super::segments::Segment::system(
            "safe\x1b[?1049hBAD\x1b]52;c;secret\x07done",
        )];
        let cursor = AutomaticPublication::default();
        let batch = cursor
            .prepare(
                0,
                &source,
                1,
                crate::surfaces::layout::UiPresentationLevel::Full,
                80,
                PreparationBudget::default(),
            )
            .unwrap();
        assert!(
            batch
                .lines
                .iter()
                .all(|line| !line.chars().any(char::is_control))
        );
    }

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

# TUI presentation — Delta Spec

## ADDED Requirements

### Requirement: Bounded contiguous native publication

Native-scrollback publication MUST prepare bounded contiguous ranges from canonical transcript state and MUST advance its canonical cursor only after the terminal insertion reports success.

#### Scenario: Successful chunk commits its range
Given canonical transcript content exceeds one publication chunk
When the first bounded chunk is inserted successfully
Then only that chunk's contiguous canonical range is committed
And the remaining canonical content stays pending for a later publication

#### Scenario: Failed insertion preserves the canonical cursor
Given a bounded chunk has been prepared but not committed
When terminal insertion reports failure
Then the canonical publication cursor remains unchanged
And the failed range is not intentionally appended again without recovery arbitration

### Requirement: Publication identity and stale work rejection

Each prepared range MUST identify the terminal attachment epoch, canonical base revision, target revision, and canonical byte range.

#### Scenario: Stale attachment result cannot commit
Given a publication was prepared for an earlier terminal attachment epoch
When its insertion result arrives after a new attachment becomes active
Then the stale publication does not advance the active attachment cursor
And the active attachment enters snapshot rebuild or remains on its current canonical range

#### Scenario: Noncontiguous range triggers rebuild
Given the committed cursor identifies one canonical boundary
When a prepared publication begins at a different boundary
Then the range is rejected
And bounded snapshot rebuild is requested instead of delta append

### Requirement: Ambiguous delivery degrades safely

The system MUST distinguish known failure from ambiguous physical delivery and MUST preserve canonical conversation as the source of truth.

#### Scenario: Ambiguous delivery disables blind retry
Given terminal delivery may have occurred but success was not confirmed
When publication recovery runs
Then the same bytes are not blindly appended again
And native publication degrades to bounded snapshot rebuild or managed-viewport presentation

### Requirement: Publication preparation is bounded

Preparation MUST enforce byte, record, visual-row, and elapsed-time budgets while preserving UTF-8 boundaries.

#### Scenario: Oversized Unicode content is split safely
Given one canonical record exceeds the publication byte budget and contains multibyte Unicode
When a publication chunk is prepared
Then the chunk ends at a valid UTF-8 boundary
And its byte size does not exceed the configured budget
And the remaining suffix is resumable from the next canonical boundary

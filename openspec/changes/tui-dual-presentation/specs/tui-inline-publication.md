# Incremental inline publication — Delta Spec

## ADDED Requirements

### Requirement: Inline output is a projection of finalized canonical work

Inline publishes accepted input and finalized conversation groups from the existing
canonical source. Mutable response/tool content remains a bounded live preview until
authoritative finalization. Active uses shared outcome rules; Full uses shared evidence
rules. Publication state contains a cursor and bounded scratch, not a second transcript.

#### Scenario: Streaming does not publish unstable revisions
Given an accepted prompt followed by multiple response deltas and changing tool metadata
When the turn becomes authoritatively terminal
Then the prompt appears once and the final projected group is published in canonical order
And superseded partial response text and intermediate tool revisions are absent from native history
And the latest bounded response preview was available during streaming

#### Scenario: Terminal control content remains data
Given finalized provider or tool text containing raw ESC and OSC sequences
When inline publication formats that text
Then existing safe display treatment prevents the content from executing terminal commands
And semantic producer and content provenance remain intact

#### Scenario: Failed or cancelled turn has a final outcome
Given a running turn with partial response or tool output
When authoritative failure or cancellation finalizes it
Then publication includes available finalized content and its truthful terminal outcome
And the live composer can accept the next turn independently of backlog delivery

#### Scenario: Resume does not flood native history
Given a saved session containing extensive completed history
When a new inline attachment resumes it
Then a bounded session summary is published and the automatic cursor begins at the attachment boundary
And full historical conversation remains available in the shared fullscreen transcript

#### Scenario: Fullscreen-first work becomes inline history
Given a fullscreen-first attachment with completed work since attachment
When the operator switches its base to inline
Then work completed since attachment is published in bounded ordered batches
And history predating attachment is not automatically replayed

### Requirement: Publication preparation and allocation are bounded

Each loop cycle admits input and authoritative lifecycle events before at most one
publication batch. Discovery, formatting, wrapping, and allocation share explicit
limits. Default maxima are 64 KiB source text, 64 records, 1,000 rendered rows,
65,536 cells, and a cooperative 5 ms preparation slice. No pre-budget full-history
export, hash, clone, or unbounded single-record parse is permitted.

#### Scenario: Persistent notice after published attachment
Given native scrollback already contains the attachment and an earlier system notice
When a control response or local command produces a persistent system notice
Then the new notice is appended once as a separate publication
And earlier printed text is not mutated or repeated

#### Scenario: Retained notification history rolls over
Given the retained system-notification limit has been reached and earlier notices were published
When a new notice prunes older retained notifications
Then the new notice remains eligible for publication once
And repeated rollover does not stop future output or replay old output
And retained notification history remains bounded

#### Scenario: Pruning before a partially published retained record
Given a retained record has a committed partial prefix and older notifications precede it
When notification retention prunes those older records
Then publication retains the committed field and byte position in the retained record
And its remaining content is published without repeating the prefix
And stale prepared batches cannot settle against the changed source

#### Scenario: Backlog remains interruptible
Given many completed groups accumulated while fullscreen is open
When inline resumes with a queued decision and cancellation input
Then interaction and lifecycle admission precede publication
And each batch respects every configured limit while subsequent cycles make progress

#### Scenario: Oversized Unicode record at narrow width
Given a single response larger than the byte budget with wide characters, combining marks, and long unbroken lines
When it publishes at a narrow terminal width
Then preparation and insertion allocate within the cell and row limits before calling Ratatui
And successive chunks reproduce the intended text without dropped or duplicated source content
And height never silently truncates through a u16 conversion

#### Scenario: Zero-size viewport
Given a terminal reporting zero width or height
When a publication cycle runs
Then it performs no insertion or cursor advancement
And resize to usable dimensions makes the pending work eligible again

### Requirement: Delivery settlement reflects physical uncertainty

Commit only after verified inline ownership, insertion, and flush succeed.
Known non-writes leave the cursor retryable. A write or flush failure that may have
partially reached the terminal degrades the attachment and disables automatic replay.
The UI retains transcript/export access and reports the limitation persistently.

#### Scenario: Successful repeated frames do not duplicate output
Given a successfully committed publication chunk
When another frame or a fullscreen round trip occurs without new finalized content
Then the committed chunk is not inserted again

#### Scenario: Fullscreen rejects automatic insertion
Given fullscreen owns the terminal with completed unpublished content
When automatic publication is considered
Then no insert_before call or settlement occurs
And the content remains eligible after a successful return to inline

#### Scenario: Known non-write retries without advancing
Given a preparation or ownership validation failure before any write attempt
When a later relevant event allows the pending chunk to publish
Then the original source range is delivered once and advances only after successful flush

#### Scenario: Partial write is not blindly retried
Given insertion or flush fails after output may have begun
When the event loop runs again or presentation changes
Then automatic publication remains disabled for that attachment
And a persistent delivery-uncertain indication provides managed transcript/export access
And no automatic attachment reset or duplicate replay occurs

### Requirement: Formatting changes and source replacement preserve cursor meaning

Publication cursors belong to an attachment and canonical generation. Width changes
invalidate uncommitted wrapping. Detail changes apply to records not yet started;
a partially committed record finishes at its original detail. Published terminal
history remains immutable. Source replacement invalidates stale prepared content.

#### Scenario: Resize and detail change during a partial record
Given part of a long Full-detail record is committed
When the terminal narrows and detail changes to Active
Then its remaining source is rewrapped at the new width using the same record detail
And later records use Active without replaying the committed prefix

#### Scenario: Canonical replacement rejects stale settlement
Given a prepared chunk from a prior session or pre-compaction generation
When the canonical source is replaced
Then settlement of that chunk is rejected
And printed history is preserved with a bounded boundary notice before new-generation output
And replacement history is not automatically dumped into scrollback

#### Scenario: Explicit export is separate from automatic delivery
Given inline with a committed prefix and pending automatic publication
When the operator invokes /session-export scrollback
Then a labeled explicit snapshot uses serialized terminal ownership
And inline geometry is restored without resetting the automatic cursor
And any intentional snapshot repetition is not reported as automatic exactly-once delivery

#### Scenario: Exit does not wait for the entire backlog
Given pending publication larger than one batch
When the operator exits normally
Then at most one bounded publication slice is drained before cleanup
And remaining history is identified as available in the managed or saved transcript

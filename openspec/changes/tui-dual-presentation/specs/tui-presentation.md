# Shared terminal presentation — Delta Spec

## ADDED Requirements

### Requirement: Entry defaults and explicit preferences are independent

Interactive `om` defaults to inline/Active and interactive `omegon` defaults to
fullscreen/Full. CLI overrides outrank explicit selected-profile values, which
outrank entry defaults, independently for each axis. Non-interactive execution
preserves its existing behavior and never acquires terminal modes for these defaults.

#### Scenario: Entry defaults
Given no explicit terminal or detail preference
When each entry starts an interactive session
Then om selects inline/Active and omegon selects fullscreen/Full
And the direct Cargo binary selects fullscreen/Full

#### Scenario: Independent override precedence
Given om with profile ui_terminal fullscreen and ui_presentation full
When it starts with --tui inline --ui active
Then it selects inline/Active without rewriting the profile
And om with only --ui full selects inline/Full when profile values are absent
And omegon with only --ui active selects fullscreen/Active when profile values are absent

#### Scenario: Launcher preserves invocation intent and arguments
Given installed om and omegon launchers and an inherited conflicting entry marker
When either launcher executes a resolved binary with arguments containing spaces and shell metacharacters
Then the child receives that launcher's recognized entry identity and the exact argument array
And maintenance, help, version, headless, and --which retain their existing execution contracts

#### Scenario: Default does not become a saved preference
Given a profile without either UI preference and an om session using its defaults
When an unrelated setting is saved and the session exits
Then both UI preferences remain absent
And the next omegon session still defaults to fullscreen/Full

#### Scenario: Explicit preference equal to a default survives
Given an omegon session with no persisted detail preference
When the operator explicitly selects /ui full
Then Full is saved as an explicit detail preference
And the next om session uses inline/Full

#### Scenario: Legacy density migration
Given a profile or UI command specifying om, lean, or slim as detail
When that preference is resolved
Then the effective detail is Active without changing terminal presentation
And subsequent explicit preference writes use active rather than a legacy name

### Requirement: Detail does not allocate terminal ownership

Active and Full change content detail. Terminal presentation determines layout and
terminal ownership. Stored surface preferences survive changes in layout eligibility.
The UI reports both axes and exposes temporary session switching through
`/ui terminal inline|fullscreen` using the existing UI command path.

#### Scenario: Full detail in inline presentation
Given inline with Active detail and a saved dashboard visibility preference
When the operator selects /ui full
Then detail changes without alternate-screen entry or persistent workspace panels
And the dashboard preference remains available when fullscreen is selected

#### Scenario: Session switch while a view is mounted
Given a fullscreen Project view covering a preserved draft
When the base presentation is changed to inline
Then Project remains fullscreen until its navigation root closes
And closing it returns to inline with the same draft and detail preference
And the temporary base change is not silently persisted

### Requirement: Both layouts use shared state and bounded live geometry

One application state, event loop, editor, interaction owner, and semantic content
path serve both layouts. Inline uses at most eight live rows, clipped to terminal
height. All widgets and cursor/hit areas use the actual viewport origin. Rich views
reuse existing components. Inline history represents images with text references.

#### Scenario: Nonzero origin and multiline input
Given an inline viewport starting below preexisting terminal text with a multiline Unicode draft
When the shared editor renders and accepts another character
Then drawing and cursor placement stay inside the viewport
And the full draft is retained while its visible portion scrolls
And preexisting text is not cleared

#### Scenario: Shared preparation under an inspector
Given a Project inspector covers an active turn
When the next scheduled frame is prepared after a dashboard or update notification change
Then shared presentation state refreshes once
And closing Project shows current state without replaying notifications

#### Scenario: Narrow geometry preserves usable decisions
Given inline at 40 columns with wrapped permission context
When the permission is displayed
Then each action appears once with its scope-correct label and context yields space first
And a decision whose complete actions cannot fit borrows fullscreen
And unusably small dimensions show a resize indication instead of misleading partial labels

#### Scenario: Completion and second submission in either presentation
Given an active turn in either presentation with AgentEnd withheld
When authoritative supervisor completion or an idle queue snapshot arrives
Then local busy and streaming gates are released
And the same runtime accepts a second turn without a restart or presentation switch

### Requirement: Rich navigation preserves its root and input precedence

Existing navigation ownership controls both visible interaction and input routing.
A rich root keeps its fullscreen requirement while covered. Decisions remain
responder-backed and ordered. Paste, mouse, and navigation keys never reach a
covered composer. Cancellation keeps its existing runtime meaning in both layouts.

#### Scenario: Inline Project permission round trip
Given inline with an unsent draft and an active turn
When a permission arrives while a filtered Project detail is open
Then the permission owns visible input while fullscreen remains owned by the mounted root
And resolving it restores the same filter, stable selection, and detail
And closing Project returns to the untouched inline draft

#### Scenario: Queued decisions do not cause surface oscillation
Given a fullscreen inspector from inline covered by a decision with another queued decision
When the first decision is answered
Then the next decision becomes visible in arrival order without an inline interlude
And the covered inspector and composer receive none of the decision input

#### Scenario: Cancel during browsing and publication backlog
Given an active turn, publication backlog, and a Project view covering an unsent draft
When Ctrl+C is pressed
Then the existing cancellation command is admitted before another publication batch
And the Project state and unsent draft remain intact

### Requirement: One terminal owner serializes transitions and output

Only the active presentation can write. Inline keeps the primary buffer and native
mouse behavior. Fullscreen uses the alternate buffer and saved mouse preference.
Successful terminal operations alone advance the mode ledger. Temporary visits,
export, shell handoff, signals, and ordinary cleanup use the same owner.

#### Scenario: Primary startup and repeated fullscreen visits
Given primary-buffer text before inline startup and mouse disabled in fullscreen preferences
When the client visits and leaves Project repeatedly
Then raw/paste modes are retained as appropriate and alternate-screen operations are balanced
And mouse capture stays disabled and the same inline Terminal is restored
And primary history survives and no inactive Terminal writes

#### Scenario: Inline startup has no fullscreen flash
Given primary-buffer text and ordinary inline startup without an explicit tutorial request
When startup initializes the renderer
Then no alternate-screen entry, fullscreen splash, or whole-screen clear is emitted
And image-protocol probing is deferred until needed by a rich view

#### Scenario: Resize while fullscreen is borrowed
Given a preserved inline Terminal and pending completed output beneath Project
When Project closes after terminal width and height change
Then inline geometry is refreshed before wrapping or insertion
And the live area redraws without replaying committed history

#### Scenario: Failed entry or restoration
Given fault injection at a mode operation, terminal creation, or geometry restoration
When the requested transition fails
Then the ledger records only successful operations and cleanup attempts all owned releases
And rendering stops if the expected ownership cannot be restored
And no output is published to an unverified surface

#### Scenario: Handoff and shutdown from either base
Given either presentation with tracked paste, keyboard, raw, and mouse modes
When an owned shell handoff, normal exit, catchable termination, or panic cleanup occurs
Then the existing cleanup owner restores the appropriate modes and cursor
And recoverable handoff re-anchors the active viewport without clearing prior history
And unrelated terminal processes are untouched

### Requirement: Routine terminal acceptance does not interrupt the desktop

Routine acceptance uses a private headless PTY. Native GUI acceptance requires an
explicit compatibility invocation and selected clients. Cleanup is separately
recorded from application exit and must not close unrelated operator sessions.

#### Scenario: No implicit native matrix
Given the native runner has no explicit GUI opt-in
When the runner parses its invocation
Then it rejects the invocation before launching a client or creating trial output

#### Scenario: Cleanup cannot close another session
Given a recorded trial window and native session identity
When cleanup detects other tabs or sessions or cannot establish ownership
Then it refuses window closure and records cleanup failure
And it launches no subsequent client in the matrix

#### Scenario: Failed process cleanup still attempts window cleanup
Given a trial that failed and process cleanup that raises an error
When the trial unwinds
Then it still attempts owned-window cleanup and writes the failure record

### Requirement: Initial interactive routing includes local declarations

Interactive setup admits local inference manifests and cached evidence before
selecting the first route. Network discovery enriches that inventory in the
background and does not gate visibility of local declarations.

#### Scenario: First turn precedes network discovery
Given a valid project inference offering and unfinished background discovery
When the initial interactive route is selected
Then the local offering is available to admission
And the first turn can reach its declared endpoint

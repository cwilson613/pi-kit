# Project TUI client - Delta Spec

## ADDED Requirements

### Requirement: Captured deterministic terminal acceptance

The repository provides an automated terminal scenario using the real executable, isolated local provider and user state, bounded waits, and attributable screen captures.

#### Scenario: Two turns and resize
Given a freshly built executable and a local streaming provider fixture
When the runner launches the TUI and submits two prompts through terminal input
Then both distinct fixture replies appear in captured terminal screens
And the second reply remains visible after resizing
And the evidence identifies the binary, source, process, dimensions, and capture hashes

### Requirement: Unified client interaction ownership

The reconstructed client uses one visible interaction owner for keyboard dispatch, with explicit return targets and stable domain identities.

#### Scenario: Approval during project browsing
Given the operator is browsing project work
When a runtime approval arrives
Then the visible interaction and keyboard owner agree
And resolving the approval returns to the prior stable work selection

### Requirement: Drawn event replay makes progress

Events released after a stream draw must not requeue behind their successors. Runtime lifecycle and queue authority events must bypass presentation buffering entirely.

#### Scenario: Completion backlog
Given a stream chunk followed by multiple completion events before the next draw
When the TUI acknowledges that draw
Then the completion backlog drains in order
And terminal input can submit a second turn

### Requirement: Initial semantic view precedes client launch

Session-backed interactive startup creates and validates its initial authority-derived projections before launching TUI or IPC consumers. It preserves recorded lineage rather than inventing semantic history for an empty session.

#### Scenario: Fresh session without projection caches
Given a fresh session has no background projection cache
When interactive startup initializes the session
Then the first screen reads a validated view of the created authority stream
And startup does not display a missing-cursor or empty-authority warning

### Requirement: Responder-backed decisions share visible and keyboard ownership

Permission and manual-action requests serialize in arrival order above passive surfaces. The queue is bounded and overflow resolves negatively.

#### Scenario: Multiple decisions while a passive surface is open
Given a permission request owns input above a Settings or copy surface
When a manual-action request arrives
Then it waits until the permission is resolved
And its prompt becomes visible when it becomes the keyboard owner
And resolving both requests preserves the prior passive surface state

#### Scenario: Decision queue capacity
Given one active decision and 64 queued decisions
When another permission request arrives
Then the new request is denied explicitly
And the active decision and existing queue remain intact

### Requirement: Profile tool permissions reach invocation policy

Applying a profile replaces the runtime permission policy with the profile's declared policy.

#### Scenario: Prompt rule in the active profile
Given the isolated profile declares write as prompt
When a model requests the write tool
Then a permission prompt is visible and receives operator input
And denying the prompt prevents the write

### Requirement: Passive overlays share navigation ownership

Rendering and input dispatch derive their overlay precedence from the same owner. Covered surfaces retain state and cannot receive keyboard or paste input through the visible overlay. Ctrl+C can cancel an active turn while browsing a passive overlay.

#### Scenario: Extension overlay above copy and Settings
Given an extension modal covers a copy surface and Settings
When the operator presses Escape
Then the visible extension modal closes
And the copy surface and Settings retain their state

#### Scenario: Menu paging does not move conversation
Given Settings owns input
When the operator presses PageDown
Then the background conversation scroll position remains unchanged

#### Scenario: Unsupported extension response
Given an extension action has no response transport
When the operator selects its numbered action
Then the client reports that limitation and retains the prompt
And the key is not inserted into the composer

### Requirement: Terminal modes have one success-ordered owner

The fullscreen client routes terminal mode changes through one shared owner. Ownership advances only after successful terminal operations. Primary-screen operations restore the saved mode preferences and invalidate the fullscreen renderer. Emergency restoration must not wait for a held ownership lock.

#### Scenario: Native transcript round trip
Given a fullscreen client with mouse capture disabled and two completed replies
When the operator invokes /session-export scrollback
Then the native primary screen contains the transcript
And the fullscreen client returns with both replies visible and mouse capture still disabled

#### Scenario: Partial transition failure
Given some terminal modes have changed successfully
When a subsequent mode operation fails
Then tracked state retains only successful changes
And retry skips completed operations

#### Scenario: Failed primary operation
Given fullscreen mode ownership
When a primary-screen write fails
Then the owner attempts to restore the exact saved modes
And it reports the failure without claiming publication success

#### Scenario: Shutdown release failure
Given several terminal modes are owned
When one release operation fails during shutdown
Then cleanup still attempts the other releases
And a later cleanup retries only modes whose release failed

### Requirement: Project browser preserves conversation context

F2 opens a local project browser with Sessions and Work tabs. It uses the existing workspace context, session inventory, and Workbench projections. Enter inspects an item; session resume is a separate explicit action routed through the existing session command. Escape returns from detail to the selected row, then to the untouched composer. Refresh preserves selection by stable item identity and returns to the list if that item disappears.

#### Scenario: Inspect and return with a draft
Given the composer contains an unsent draft
When the operator opens F2 and inspects the current session
Then the browser displays session identity and current turn information
And returning to the conversation preserves the draft

#### Scenario: Approval above project browsing
Given a project item is selected
When a permission request arrives
Then the permission owns input above the browser
And resolving it restores the same project selection

#### Scenario: Work refresh preserves identity
Given a workstream is selected in the Work tab
When a refresh inserts a different workstream before it
Then the original workstream remains selected by ID

#### Scenario: Selected work disappears
Given a workstream detail is open
When refresh removes that workstream
Then the browser returns to the work list without displaying stale detail

#### Scenario: Cancel while retaining the next draft
Given an active turn and an unsent draft while the Project browser is open
When the operator presses Ctrl+C
Then the existing runtime cancellation command is sent
And the browser and unsent draft remain intact

#### Scenario: Saved session inspection is read-only
Given a saved session is selected in the browser
When the operator presses Enter
Then its metadata opens without sending a resume command
And resuming requires a separate R action while no turn is active

### Requirement: Attributable native terminal operator kit

The operator kit provides launchers for installed supported native terminal clients, a fixed executable/runner identity, isolated fixture state, local output recordings and an explicit unassessed results sheet. Native terminal capability settings are retained. GUI/font fidelity requires native screenshot inspection by the agent or operator rather than an inferred result from terminal bytes.

#### Scenario: Launch a client trial
Given a prepared kit and an installed supported terminal client
When the operator opens that client's launcher
Then a new client window starts the isolated fixture against the recorded executable
And the trial stores client identity, terminal environment, dimensions and local evidence

#### Scenario: Changed prepared artifact
Given a prepared kit whose executable has been modified
When a trial is requested
Then verification rejects the changed artifact before launch

#### Scenario: Trial cancellation cleanup
Given a trial owns child processes including a separate process session
When the runner is terminated
Then cleanup targets that owned descendant tree
And unrelated terminal sessions remain outside the cleanup set

### Requirement: Agent-operated native compatibility acceptance

Native compatibility testing must run without operator input. The agent must drive
owned native windows, capture their rendered output and current text, and record
per-client outcomes against a fixed executable. Tests must distinguish pasted text
from decision keys and must not use retained history to satisfy current-view checks.

#### Scenario: Native interaction and denial

Given a fixed test executable and an installed terminal with programmatic controls,
When the agent drives project navigation, conversation turns and a write decision,
Then captures identify the owned window and executable and acceptance requires the
expected local request count, absent denied file and successful recorded exit.

#### Scenario: Incomplete or interrupted automation

Given a missing native control, ambiguous window identity or operator intervention,
When the affected trial cannot establish its expected outcome,
Then it is recorded as incomplete or diagnostic rather than a compatibility pass,
and the agent cleans up its own outstanding test process.

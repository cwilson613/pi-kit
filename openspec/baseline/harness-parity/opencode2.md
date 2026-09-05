# harness-parity/opencode2 - Baseline

### Requirement: Project instruction construction includes all applicable ancestors

Prompt construction must include each applicable ancestor AGENTS.md once, from
active worktree root through cwd, with source labels and complete UTF-8 content.
Global guidance retains its existing owner. Immutable core authority is unchanged.

#### Scenario: Intermediate ancestor and long root policy
Given distinct root, intermediate, and cwd AGENTS.md files
And root guidance exceeds 4000 bytes and includes multibyte characters
And the complete guidance fits the request budget
When the harness constructs the project instruction section
Then all three files appear completely in root-to-cwd order with source labels
And no source appears twice

#### Scenario: Linked worktree boundary
Given cwd is nested in a linked worktree with its own root AGENTS.md
And the main checkout and a directory above the worktree contain different guidance
When the harness constructs project instructions
Then it loads the active worktree ancestors
And it excludes main-checkout and above-worktree project guidance
And global operator guidance retains its existing separate loading behavior

#### Scenario: Canonical duplicate
Given two discovered paths resolve to the same permitted instruction file
When the harness constructs project instructions
Then that canonical file contributes content only once

#### Scenario: Missing ancestor file and non-Git directory
Given cwd is outside a Git worktree and contains an AGENTS.md
When the harness constructs project instructions
Then it loads the cwd file without scanning unrelated ancestors
And absent optional instruction files do not cause an error

### Requirement: Required project guidance is never silently omitted

Prompt preparation must distinguish missing files from read errors, preserve
complete required guidance, and fail actionably before dispatch if it cannot
read or fit that guidance. This requirement applies at existing construction
boundaries; it does not require live refresh or durable instruction generations.

#### Scenario: Unreadable applicable file
Given an applicable AGENTS.md exists but cannot be read
When the harness prepares a model request
Then preparation reports the source and a recoverable read error
And no model request is dispatched with silently omitted guidance

#### Scenario: Required guidance cannot fit
Given complete required instructions exceed the available model request budget
When the harness prepares a model request
Then it reports an actionable budget error before network dispatch
And it does not truncate policy to make the request fit

### Requirement: MCP phase budgets preserve legacy configuration fallback

MCP must support optional positive startup_timeout_secs, catalog_timeout_secs,
and execution_timeout_secs. An unset phase inherits timeout_secs and its existing
default. Explicit phase values must be validated before connection.

#### Scenario: Partial phase override
Given timeout_secs is 30 and execution_timeout_secs is 90
When the harness resolves MCP deadlines
Then startup and catalog inherit 30 seconds and execution receives 90 seconds

#### Scenario: Existing configuration
Given a previously supported MCP configuration without phase overrides
When the harness resolves MCP deadlines
Then its effective legacy timeout behavior remains unchanged

#### Scenario: Invalid explicit budget
Given an explicit phase budget is zero, negative, malformed, or overflows duration conversion
When the harness loads the MCP configuration
Then it rejects the invalid value with the phase identified before starting that server

### Requirement: MCP operations enforce their own phase deadlines

Startup covers connection and initialization. Catalog covers inventory discovery
including pagination. Execution covers tool calls, resource reads, and prompt
retrieval. Progress must not extend the hard deadline. Managed outer lifecycle
bounds and cancellation remain authoritative.

#### Scenario: Slow execution with fast discovery
Given startup and catalog complete within short configured budgets
And an execution operation completes after those durations but within its execution budget
When the harness runs a tool call, resource read, or prompt retrieval
Then the operation succeeds without being limited by the earlier phase budgets

#### Scenario: Catalog pagination stalls
Given a server has explicit startup/catalog budgets and returns one catalog page but stalls on the next
When the catalog phase deadline expires
Then the harness settles discovery with a catalog timeout diagnostic
And additional pages do not reset the phase deadline

#### Scenario: Legacy optional catalog stalls after tool discovery
Given a legacy configuration without startup or catalog overrides
And tools were discovered successfully before an optional catalog stalled
When the shared readiness deadline expires
Then completed tool inventory remains available with a partial-discovery diagnostic

#### Scenario: Shutdown waits for service settlement
Given the lifecycle owner is closing an MCP connection
When service settlement suspends shutdown
Then the client registry lock is released
And new invocations can observe server unavailability without waiting behind cleanup

#### Scenario: Startup stalls
Given a server does not finish initialization
When the startup deadline expires
Then the harness reports the startup budget and settles through its existing lifecycle owner

#### Scenario: Progress does not grant extra runtime
Given a running MCP operation emits progress repeatedly
When its execution deadline expires
Then the harness settles it as an execution timeout despite the progress

### Requirement: MCP cancellation reports bounded and truthful settlement

Cancellation must settle before a later operation deadline. Existing lifecycle
owners retain process-tree cleanup authority. A single operation timeout must
not kill unrelated calls without an explicit server-lifecycle consequence.
Remote work is not reported terminated without evidence.

#### Scenario: Local cancellation
Given a local MCP operation is active with a future deadline
When the operator cancels it
Then cancellation settles without waiting for the full deadline
And any required process cleanup covers descendants or reports incomplete cleanup

#### Scenario: Concurrent operation and remote uncertainty
Given two calls share a remote MCP transport
When one call times out
Then its result identifies the execution timeout and known cancellation outcome
And the other call remains active unless the lifecycle owner explicitly reports transport-wide failure
And the timed-out remote work is not reported stopped without evidence

### Requirement: Reconnect transitions preserve events and authoritative pending state

The transition from initial snapshot to live subscription must not lose events
emitted during snapshot delivery. Pending approval and delegate results must
remain retrievable after client detach. Daemon restart guarantees are evaluated
separately and must not be inferred from client reconnection.

#### Scenario: Event during snapshot delivery
Given a client is connecting while runtime events continue
When an event arrives while the initial snapshot is being sent
Then the client can observe that event through its live subscription or authoritative reconciliation
And snapshot delivery does not leave an unsubscribed event-loss window

#### Scenario: Pending approval and completed delegate
Given a session has a web-owned pending approval and a delegate completes while its client is detached
When a client reconnects to the same running session
Then pending approval and delegate results remain recoverable from authoritative state
And missed advisory events do not imply that the work disappeared

### Requirement: Duplicate-action verification distinguishes authority and transport contracts

Verification must exercise durable command deduplication and inspect whether
client retries carry a stable submission identity. Missing transport identity
must be recorded as an unsupported guarantee, not described as end-to-end deduplication.

#### Scenario: Repeated durable submission identity
Given an admitted input has a durable submission identity
When the same identity is submitted again across authority reload
Then the existing admission is retained without a second executed input

#### Scenario: Client retry has no submission identity
Given a transport input contract has no client-provided retry identity
When duplicate-action verification examines reconnect retries
Then the evidence explicitly records that retry deduplication is not supported by that contract
And it does not use durable-record deduplication as proof of transport-level safety

# Design

The runtime supervisor, session authority, work runtime, command registry, and semantic projections retain domain authority. Client navigation owns focus, drafts, stable selections, and return targets. Terminal presentation owns terminal modes. Visible interaction and keyboard routing must derive from one owner.

The neighboring checkout's two-terminal coordinator and publication seams are candidates for selective adaptation, not a bulk branch import. Its unresolved hidden-approval routing, secondary terminal owners, and publication backlogs require acceptance coverage before adoption.

## Captured acceptance foundation

Use Python's standard library and an existing tmux executable. A unique private tmux socket isolates each run from operator sessions. The real binary uses a temporary HOME/config/workspace and an explicit environment. A loopback Chat Completions fixture emits distinct deterministic SSE replies. No real model credentials are inherited.

Capture rendered terminal cells with tmux, plus logs and a JSON manifest containing source revision/dirty state, absolute binary path/hash, process identity, timestamps, geometry, and capture hashes. Evidence stays outside Git. Poll observable conditions under deadlines rather than using fixed startup sleeps. This establishes behavior at the terminal boundary, not screenshot/color fidelity or terminal-emulator portability.

Initial scope deliberately exercises the existing TUI before reconstruction. The project shell remains pending; do not present the acceptance runner as implementation of project navigation. Expand scenarios alongside each subsequent behavioral change, including approval while browsing, cancellation during backlog, and restored stable selection.

## Initial view and responder ownership

For session-backed startup, initialize session authority and synchronously prepare its derived caches before spawning interactive clients. The background worker starts afterward. A new stream without semantic steps truthfully retains its pre-spine lineage; startup does not create artificial step/context events to label it exact full history.

The first interaction owner covers permission and manual-action responders. It supplies keyboard precedence and the final visible prompt. A bounded FIFO preserves concurrent arrivals, including acknowledgement only when a manual-action request becomes visible. Existing menus and copy surfaces remain mounted with their state. A saved command prompt is restored after decisions finish. Mouse/paste input cannot mutate underlying surfaces during a decision. Overflow is explicitly denied/cancelled. General passive navigation and extension input unification remain separate work.

The captured write scenario uses an explicit per-tool prompt rule because temporary paths are allowed by default. Applying a profile must copy its permission policy into runtime Settings, which invocation admission reads. This propagation repair was discovered by the interactive scenario.

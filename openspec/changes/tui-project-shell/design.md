# Design

The runtime supervisor, session authority, work runtime, command registry, and semantic projections retain domain authority. Client navigation owns focus, drafts, stable selections, and return targets. Terminal presentation owns terminal modes. Visible interaction and keyboard routing must derive from one owner.

The neighboring checkout's two-terminal coordinator and publication seams are candidates for selective adaptation, not a bulk branch import. Its unresolved hidden-approval routing, secondary terminal owners, and publication backlogs require acceptance coverage before adoption.

## Captured acceptance foundation

Use Python's standard library and an existing tmux executable. A unique private tmux socket isolates each run from operator sessions. The real binary uses a temporary HOME/config/workspace and an explicit environment. A loopback Chat Completions fixture emits distinct deterministic SSE replies. No real model credentials are inherited.

Capture rendered terminal cells with tmux, plus logs and a JSON manifest containing source revision/dirty state, absolute binary path/hash, process identity, timestamps, geometry, and capture hashes. Evidence stays outside Git. Poll observable conditions under deadlines rather than using fixed startup sleeps. This establishes behavior at the terminal boundary, not screenshot/color fidelity or terminal-emulator portability.

Initial scope deliberately exercises the existing TUI before reconstruction. The project shell remains pending; do not present the acceptance runner as implementation of project navigation. Expand scenarios alongside each subsequent behavioral change, including approval while browsing, cancellation during backlog, and restored stable selection.

## Initial view and responder ownership

For session-backed startup, initialize session authority and synchronously prepare its derived caches before spawning interactive clients. The background worker starts afterward. A new stream without semantic steps truthfully retains its pre-spine lineage; startup does not create artificial step/context events to label it exact full history.

The first interaction owner covers permission and manual-action responders. It supplies keyboard precedence and the final visible prompt. A bounded FIFO preserves concurrent arrivals, including acknowledgement only when a manual-action request becomes visible. Existing menus and copy surfaces remain mounted with their state. A saved command prompt is restored after decisions finish. Mouse/paste input cannot mutate underlying surfaces during a decision. Overflow is explicitly denied/cancelled. The shared navigation owner now also determines passive overlay rendering and input precedence. Covered menus, selections, panels, and drafts remain mounted. Extension action responses still need a transport contract; until then selection reports the unsupported operation and does not claim success.

The captured write scenario uses an explicit per-tool prompt rule because temporary paths are allowed by default. Applying a profile must copy its permission policy into runtime Settings, which invocation admission reads. This propagation repair was discovered by the interactive scenario.

## Terminal transition ownership

Adapt the neighboring inline corpus's success-ordered mode transitions into the current fullscreen client. One shared handle owns raw mode, alternate screen, mouse capture, bracketed paste, and keyboard enhancement. Startup, mouse preference changes, native `/session-export scrollback`, shell suspension, tutorial handoff, and ordinary shutdown use that owner. The existing panic guard retains a nonblocking emergency restoration path.

A primary-screen scope suspends the exact tracked modes, executes its operation, and restores the saved preferences even on operation failure. Failed mode operations do not advance ownership state. Ordinary cleanup attempts all releases and retains failed modes for a later cleanup retry. Failed suspension prevents the primary operation. The renderer holds the same ownership lock during normal draws; primary round trips increment a presentation revision that forces framebuffer invalidation and a new draw. Partial restoration that loses raw/alternate ownership fails the draw and triggers teardown.

This is an adaptation of the ownership invariant, not installation of the neighboring two-Terminal coordinator. Persistent inline viewport layout, automatic bounded transcript publication, and publication backlog recovery remain pending. The current scope exercises explicit `/session-export scrollback` through the actual terminal. OS job-control suspension and tutorial process replacement remain unit/code-path coverage rather than captured acceptance.

## Project browser increment

F2 is a frontend-local navigation action, so it does not introduce another domain slash command. The browser composes the existing session inventory menu and Workbench projections into a project surface. Sessions includes the current session even before it appears in saved inventory. Enter inspects metadata; resuming a saved session requires a separate R action through the existing session command and its busy/session lifecycle checks. Work exposes the current plan and workstreams without inventing persisted work objects or execution evidence.

The browser owns its tab, stable selected row ID, and detail scroll. Escape backs out one level; F2 returns directly to the existing composer. Decisions retain precedence and leave browser state mounted. F5 refreshes existing read models and preserves stable selection; missing selected rows close stale details. Saved inventory is read on explicit open/refresh, not each draw. Execution/evidence drill-down, full work-source aggregation, and inline layout remain subsequent work.

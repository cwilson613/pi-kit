# TDD and terminal acceptance plan

Planning status: no implementation or runtime acceptance claimed by this change.
The research baseline and previous host-loader limitation are in research.md.

Planning checks on 2026-09-05: OpenSpec validation passed for this change (planned),
tui-project-shell (implementing), and tui-native-usability (implementing). Local
Markdown links resolve, no TODO placeholders remain, and Git whitespace checks
passed. All implementation tasks remain unchecked. No Rust tests or new native
trials were run for this document-only increment.

## Red-first sequence

Use the narrowest owning test per change. The red run must compile and fail on the
missing behavior; fix broken test setup before recording red evidence. After green,
run the relevant existing regression family. Record source revision/dirty state,
command, test names, result, and log path. Keep runtime artifacts outside Git.

| Task group | Behavioral oracle | Principal evidence |
|---|---|---|
| 1 | Independent entry/CLI/profile resolution and preserved explicitness | Pure Rust tests; fake-target launcher argv/environment tests |
| 2 | Real buffer ownership, ordered transitions, preserved primary text | Fault-injected terminal operations; locked-library PTY test |
| 3 | Same editor, decision owner, navigation state, and completion behavior | App tests and Ratatui TestBackend at offset origins |
| 4 | Finalized order, bounded work/allocation, truthful settlement | Cursor/projection tests, fake clock, counted source access, failing writer |
| 5 | Complete inline-to-inspector-to-inline interaction with second turn | Real binary in isolated PTY and one owned native client |
| 6 | Native geometry, readable choices, paste/resize/cleanup | Native screenshots, current-view text, terminal recording, fixture outcomes |
| 7 | Public entry defaults and final integration | Fixed-build launcher trials, crate/Clippy/script gates, docs/spec validation |

Test shared behavior once with presentation parameters where output ownership
matters. Do not duplicate provider/tool/permission suites for each layout. Keep
framework buffer tests distinct from emulator behavior: TestBackend alone cannot
establish alternate-screen, native scrollback, GUI/font, or shell restoration claims.

## Deterministic fixture and capture sequence

Extend existing runners and local SSE fixture. Do not add another terminal-control
framework or paid provider dependency. Drive readiness/stream/tool gates through
observable conditions with deadlines, not fixed delays or model prompts.

1. Launch an isolated fixture workspace and terminal with a unique primary-history
   marker. Record source, binary hash, absolute executable, launch arguments, PID,
   client/version, owned window or pane, timestamp, viewport, and terminal flags.
2. Start explicit inline/Active. Assert the marker survives in primary history,
   alternate screen is inactive, and the live area occupies at most eight rows.
3. Submit the first prompt and hold streaming at a fixture gate. Capture live preview
   and paste an unsent multiline draft. Verify the draft is never a provider request.
4. Open F2, search, and enter detail. Release a fixture write request under an explicit
   permission rule. Capture the decision above the same Project root. Deny it through
   the client's supported native input primitive. Assert the file remains absent.
5. Complete the turn while Project is mounted. Verify completion/counters update and
   no automatic primary publication occurs until returning. Close Project; assert
   restored filter/selection on reopen and the exact preserved draft on return.
6. Observe bounded catch-up, then submit a second prompt. Verify both terminal outcomes
   and the scenario's exact local provider-request count. Do not assume the previous
   runner's four-request constant is valid for every new scenario.
7. Resize narrow/wide and short/tall, change detail, and repeat a fullscreen visit.
   Check current-screen controls separately from primary history. Count stable
   publication markers in reconstructed primary history, not raw redraw bytes.
8. Exit and inspect restored shell/cursor behavior and recorder exit status. Hash
   recordings/screenshots, stop only owned process trees, and record the result.

Use a separate deterministic fixture for backlog/cancellation and for session
replacement. Inject lost AgentEnd, partial writes, and rollback failures at Rust
test boundaries; a screenshot cannot establish those fault cases.

## Compatibility matrix

| Layer/client | Required coverage |
|---|---|
| Isolated tmux/PTY | All four base/detail combinations; both public entries; exact mode bytes/current screen/history; queued input; lost completion events where injectable |
| Ghostty | Inline/Active and fullscreen/Full full sequence; supported paste/key/resize; native screenshots |
| iTerm2 | Both default presentations; contents cropped to current rows for live assertions; separately retained history |
| kitty | Both defaults through owned socket/window; native resize and input |
| WezTerm | Both defaults through socket belonging to the owned GUI PID; resize only owned panes/windows |
| Apple Terminal | Both defaults through owned window; supported do-script input, resize, screenshots; record unavailable raw Escape/bracketed-paste controls |

Cover the other two detail combinations in the PTY suite and at least one native
client. Do not multiply every pure layout assertion across every emulator. Use
40/56/90-column focused geometry, a normal workspace size, and short-height cases;
record actual native dimensions rather than assuming the resize request succeeded.

Capture the decision, Project return, inline composer, fullscreen workspace, and
post-exit primary history. Inspect images against readable scope-correct actions,
complete primary hints, preserved draft, and bounded inline occupancy. A historical
match does not satisfy a live assertion; an absent marker from a viewport does not
prove it is absent from scrollback.

## Failure and provenance rules

Use current source builds and the existing frozen operator kit. Build once with the
repository workflow; install only if launcher/assets require installation evidence.
`just link` builds for itself, so do not precede it with a redundant release build.
Fixed-build temporary launcher wrappers can test entry defaults without replacing
the operator's installed executable. Test `--which` against that same identity.

Recheck the prior macOS pre-entry stall with one bounded artifact probe when runtime
work begins. A pre-entry timeout is indeterminate, not a failed TUI assertion. Record
process samples, clean up owned processes, and keep blocked runtime/gate tasks open.
Do not reuse the older native screenshot batch as evidence for new layouts.

Native trials are agent-operated. Operator intervention, ambiguous window ownership,
wrong artifacts, or unsupported input makes the affected assertion diagnostic or
unverified. Do not ask the operator to complete a manual acceptance sheet. Missing
native coverage remains visible even when the equivalent PTY assertion passes.

## Landing commands and evidence ledger

Use the repository environment and canonical glyph selection. Focused examples:

```bash
just test-filter 'native_publication'
just test-filter 'terminal_presentation'
python3 scripts/tests/test_omegon_launcher.py
python3 scripts/tests/test_tui_acceptance.py
python3 scripts/tests/test_tui_operator_test.py
python3 scripts/tests/test_tui_native_acceptance.py
```

Final omegon gate is the serialized equivalent of `just test-crate omegon`:

```bash
OMEGON_NERD_FONT=1 cargo test -p omegon --locked -- --test-threads=1
just clippy-changed
just test-dev-scripts
python3 /Users/wilson/.agents/skills/openspec/scripts/openspec.py validate tui-dual-presentation
git diff --check
```

Unset NO_COLOR and OMEGON_ASCII_GLYPHS when running canonical glyph tests, consistent
with the existing acceptance environment. Use repository recipes and long-running
process monitoring; do not repeatedly restart cold Cargo gates at tool timeouts.
If shared contracts/crates change, add the required affected/reverse-dependent gate.
Record optional Pkl tooling availability and run repository-owned schema checks when
the profile vocabulary changes.

| Revision/build | Hypothesis/input | Test/runtime ID | Capture/log | Result | Remaining limitation |
|---|---|---|---|---|---|
| Planning only | Independent presentations with shared implementation | Not run | None | Awaiting implementation | Previous host-loader issue remains an empirical risk |

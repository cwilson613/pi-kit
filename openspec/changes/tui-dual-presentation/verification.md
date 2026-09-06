# Implementation and acceptance evidence

Status on 2026-09-05 (America/New_York): shared TUI implementation, scoped acceptance and
launcher installation are complete. The full installation recipe remains blocked
at catalog refresh by the separately tracked home-identity mismatch. Native GUI trials stopped after
the operator reported desktop disruption. All further checks run headlessly.

## Artifact identity

Final captured debug executable SHA-256:
`4b191c7c98b05fdf391b661be4c5e1d961dbe22ce4188eb8cc09d297ff63ea8d`.
Captures identify planning HEAD `181992fa` plus the dirty source inventory; they
must not be described as captures of a later commit or release binary. Each frozen
kit records its binary and support hashes. Each native trial records the driver,
helper, process/window identity, screenshots, timestamps and recording location.

Evidence is outside Git under `/Users/wilson/workspace/styrene-labs/`.
`omegon-dual-evidence-01/sha256.json` indexes retained gate logs and
`native-matrix.json` indexes the 12 completed native trials with their hashes.

## Red-first findings and fixes

- Launcher marker/literal-argument and legacy-density assertions failed before
  independent entry resolution and migration were implemented.
- Profile assertions failed when unrelated saves persisted inferred preferences
  or invocation overrides replaced explicit preferences. Explicit preference
  fields now remain separate from effective session values.
- Ctrl+G failed its two-level cycle assertion before Full returned to Active.
- Stress trial 02 exposed active Ctrl+C not being consumed by the supervisor.
  Active priority ingress now durably admits interruption and starts cancellation.
- Stress trial 03 exposed publication skipping the first reply after `/new`.
  Both source-replacement owners now invalidate the cursor before subsequent events.
- Cleanup regressions exposed unproven window ownership, missing session guards,
  and a process-cleanup exception skipping window cleanup. The headless regression
  suite passes after the fixes.

Other failed observations were fixture issues: the bare detachment PTY did not
answer cursor-position queries; Full readiness used a compact-only label; Apple
Terminal could preload history instead of showing the first-prompt placeholder;
and an outcome assertion raced the separately admitted lifecycle notification.
Those failures are not counted as product acceptance passes.

## Gates

All Rust tests used the canonical glyph environment: NO_COLOR and
OMEGON_ASCII_GLYPHS unset, OMEGON_NERD_FONT=1. Cargo gates ran serially.

| Gate | Recorded result | Retained log in omegon-dual-evidence-01 |
|---|---|---|
| omegon crate and integration suite via just test-crate | 5,125 unit tests passed, 10 ignored; integration groups passed after the startup ordering fix | omegon-dual-crate-final-03.log |
| TUI suite after final outcome/geometry changes | 1,276 passed, 1 ignored | omegon-dual-tui-final-04.log |
| Clippy, omegon all targets | Passed after the startup ordering fix | omegon-dual-clippy-final-07.log |
| Explicit PTY detachment test | Passed: idle and active under both bases | omegon-dual-detachment-02.log |
| Developer script gate and launcher tests | Passed | omegon-dual-dev-scripts.log; omegon-dual-launcher-final.log |
| Pkl Profile evaluation | Passed | omegon-dual-profile-schema.log |
| TUI Python unittest discovery | 13 passed on Python 3.14.7, including native cleanup contracts | omegon-tui-scripts-final-04.log |
| Standalone fixture contract script | Passed | omegon-dual-fixture-final-04.log |

The final crate gate includes the outcome notices and startup ordering fix. The
repository recipe uses libtest scheduling; the earlier crate gate used one test
thread. Cargo gates ran sequentially. No full-workspace or cross-platform gate is claimed.

## Scenario coverage

| Spec scenarios | Verification owner and result |
|---|---|
| Entry defaults; independent precedence; literal arguments | Resolver/settings and actual-launcher tests; om and omegon captured defaults passed |
| Absent/default-equal preferences; legacy migration | Profile capture/apply tests and Active/Full cycle assertions passed |
| Full detail inline; base change under mounted view | Shared App tests plus both mixed combinations in PTY and Ghostty passed |
| Offset origin; shared preparation; narrow decisions | TestBackend at 40/56/90 columns and short heights; shared composer and decision tests; native choice screenshots inspected |
| Completion/second submission; Project permission round trip; queued decisions | Existing App recovery and queued-decision tests parameterized for both bases; normal four-request captures passed |
| Cancellation under browsing/backlog | Six-request stress capture: gated large streaming reply, draft, Project, denial, active cancellation, reset, subsequent reply and exit passed |
| Primary startup; repeated visits; no fullscreen startup; borrowed resize | PTY history/mode checks and repeated native Project/export visits passed; inline startup code omits fullscreen splash/probe/clear |
| Failed entry/restoration | Every tracked mode operation has injected acquisition/release failures; inactive output guard tests passed. Terminal creation/geometry propagation reviewed, without a dedicated fault-injected TerminalBuffers backend |
| Handoff and shutdown | Existing primary-scope success/failure tests plus real PTY detachment in both bases; ordinary captured exits passed. New native shell-command handoff and every signal were not separately replayed |
| Stable streaming; safe terminal-control text; failed/cancelled outcomes | Automatic publication boundary/control tests and authoritative lifecycle outcome-once tests passed; stress capture verifies cancellation notice |
| Resume without history flood; fullscreen-first history | Cursor attach/boundary tests and mode-independent source retention; native/PTY switches passed. Resume UI is not covered by a fresh native campaign |
| Interruptible backlog; oversized Unicode; zero viewport | Bounded source/record/row/cell work, injected cooperative clock, UTF-8 continuation and zero-geometry tests passed; stress input remained usable |
| Once-only settlement; inactive fullscreen rejection; known non-write; ambiguous write | Cursor settlement and active-owner guard tests passed. Ambiguous delivery is injected at the settlement boundary, not through a physically failing native terminal writer |
| Resize/detail during partial record; replacement generation | Cursor continuation/stale-settlement tests passed; stress reset capture passed |
| Explicit export separation; bounded exit | Separate publication owners and cursor tests; captured primary export, preserved composer anchor and exit passed; exit does not drain an unlimited backlog |
| Quiet invocation; ownership-safe cleanup; cleanup exception | Headless native runner contracts passed; no further native GUI validation after the disruption report |

Unicode tests preserve source text with wide and combining characters; they do not
claim full grapheme shaping for pathological, arbitrarily large combining clusters.
The preparation time limit is cooperative, not a hard real-time deadline.

## Captured matrix

All directories below are siblings of this checkout. Standard trials make four
local fixture requests and verify the denied file is absent. No paid inference is
used. Both entry defaults exercise the real launcher script with a frozen binary.

| Layout/detail | PTY evidence | Native evidence |
|---|---|---|
| Inline/Active (om) | omegon-dual-stress-06: six-request stress pass | omegon-dual-native-inline-04/{ghostty,iterm,kitty,wezterm,terminal}: five passes |
| Fullscreen/Full (omegon) | omegon-dual-pty-full-full-01: pass | omegon-dual-native-full-04/{ghostty,iterm,kitty,wezterm} and omegon-dual-native-full-04b/terminal: five passes |
| Inline/Full | omegon-dual-pty-inline-full-01: pass | omegon-dual-native-inline-full-04/ghostty: pass |
| Fullscreen/Active | omegon-dual-pty-full-active-01: pass | omegon-dual-native-full-active-04/ghostty: pass |

Final screenshots inspected include inline Ghostty and WezTerm composers, kitty
permission choices, iTerm fullscreen permission choices, Apple Terminal exit and
Ghostty inline/Full. Choices remain distinct and readable; inline returns to its
small composer. Instrumentation visible in Full remains the baseline for the
separately planned telemetry retirement.

Apple Terminal do-script appends Return: physical Escape and native bracketed
paste are not verified there. Ghostty resize uses font zoom; WezTerm resize uses an
owned split. Recordings/current-view captures establish their supported actions,
not identical physical key coverage across clients.

## Desktop disruption correction

The matrix completed before the operator asked for quiet testing. Its original
PASS results establish fixture outcomes and recorder exit, **not window closure**.
A terminal can remain open after its child exits. Apple Terminal may also group
tabs, so closing a window without session checks is unsafe.

The native runner now requires --interactive-gui and explicit clients, removes
explicit activation commands, records the trial session, attempts exact cleanup
on success and failure, and checks all Spaces for surviving window/PID pairs.
Shared-window adapters refuse closure if other tabs/sessions are present. A cleanup
failure stops subsequent clients. These changes have headless contract coverage;
no new GUI launch was made to validate them. Previously recorded test identities
were checked for surviving owners; unrelated operator terminals were left alone.

Routine iteration uses the private PTY. It needs no operator keystrokes or visible
terminal windows. Native tests are reserved for a dedicated compatibility session.

## Release installation discoveries

`just link` built and installed both release companions and the `om`/`omegon`
launchers. Launcher byte comparisons and --which confirmed the expected checkout.
The recipe then stopped in catalog installation: the stored home device identity
was 16777231 and the current descriptor reports 16777233, with the same path and
inode. This is a pre-existing installation-state mismatch, not a TUI assertion.
No authority state was deleted or rebound. Recovery is tracked in
[maintenance-home-identity-recovery](../maintenance-home-identity-recovery/tasks.md).

Release PTY trial 01 exposed first-turn route admission racing background
discovery. Trials 02/03 narrowed it: an unsupported provider prefix failed provider
selection, and a distinct offering with a supported prefix was absent from the
initial snapshot. Local manifest refresh ran only after background network
discovery. Debug setup had been slow enough to hide the race. Setup now admits
local manifests and cached evidence before spawning network discovery. Six
inference-runtime tests pass, and `omegon-dual-metadata-pty-01` passes the full
four-request headless sequence with the changed debug build.

The fixture now uses the distinct `openai:omegon-tui-fixture` offering, avoiding
overrides of real bundled model entries. Historical captures retain their original
fixture identity and hashes. Final release verification follows this ordering fix;
the earlier failed release trials are retained as diagnostic evidence.

## Final installed artifact

Last production code commit: `1bd50f61`. The later documentation commits do not
change the installed production code. Release SHA-256: `d9e1044954c139aab01b20ec147a6472ded40377c5cdc4125c243c2912bed7b5`.
`omegon-dual-evidence-01/installed-release.json` records installed launcher bytes,
--which resolution, fallback binary equality and both release acceptance manifests.

Both final release runs passed four local requests, denied-file absence and shell
return: `omegon-dual-release-pty-04` (om, inline/Active) and
`omegon-dual-release-pty-full-01` (omegon, fullscreen/Full). These are headless
captures of the corrected release build, with no GUI launches.

The second `just link` rebuilt and installed both companions and launchers, then
again stopped at the existing catalog home-identity mismatch. Thus binary/launcher
installation passed; the whole link recipe did not. Catalog and subsequent
extension-install steps remain with the separate recovery change. Decorative
telemetry retirement also remains planned. This TUI change is left unarchived.

# Verification — 2026-09-05

## Implemented behavior

Permission context no longer embeds action choices. The prompt's structured action
list supplies the keys once and derives Shift+A's label from the request's actual
persistence scope. Rendering measures wrapped display cells, puts each action on
its own line and reserves their space before allocating context. Oversized context
is explicitly marked truncated rather than silently displacing decision keys.

The Project browser now routes `/`, query characters and Backspace through the
existing MenuState. Enter only opens details when a matching row exists. Search,
filter clearing and detail navigation retain their Escape order; F2 returns directly
to the draft. Refresh and covered permission decisions retain the active filter and
selected identity. Explicit idle-only saved-session resume remains unchanged.

Composer help fits complete hints into its display-cell budget, starting with the
send/run action. Secondary hints are omitted before the primary action can be clipped
by right alignment. At widths too small for even the complete primary hint, it is
omitted; the supported compact regression widths are 40, 56 and 90 columns.

## Test-first evidence

The initial test helper used a nonexistent editor accessor; that compile error was
corrected before the red run. All four new regressions then failed against the prior
implementation (`/tmp/omegon-usability-red.log`), establishing duplicate/clipped
choices, inert filtering, missing filter preservation and clipped primary hints.
All four passed after implementation (`/tmp/omegon-usability-green.log`).

The final compiled test artifact ran the full TUI suite directly:
`OMEGON_NERD_FONT=1 target/debug/deps/omegon-ea3e19912f3ebdb4 'tui::' --test-threads=1`
with NO_COLOR and OMEGON_ASCII_GLYPHS unset. Result: 1,260 passed, zero failed, one
ignored (`/tmp/omegon-usability-tui.log`). This includes the added filtered-refresh
assertions and existing permission routing, navigation and lifecycle regressions.
Cargo formatting passed. The four operator-kit Python contracts and three native
observation contracts passed; fixture contracts passed. Both OpenSpec changes validate.

## Earlier indeterminate gates (resolved by the subsequent campaign)

The full serialized `just test-crate omegon` compiled successfully and executed
through the surface tests, then stalled while a switch test launched its temporary
verifier shell. It did not finish (`/tmp/omegon-usability-crate.log`).
`just test-dev-scripts` reached its release-policy Cargo probe; a blackbox test
executable stopped before entry (`/tmp/omegon-usability-dev-scripts.log`). A process
sample records `_dyld_start + 0`, a 96 KiB footprint and no application frames
(`/tmp/omegon-usability-loader-sample.txt`). These are indeterminate gates, not
assertion failures or passes.

`just clippy-changed` stopped in its generated shell launcher, also before entry
(`/tmp/omegon-usability-clippy-loader.txt`). The equivalent direct path identified
only omegon, passed formatting, then stopped in the Clippy build-script executable
(`/tmp/omegon-usability-clippy-direct.log`). At that point Clippy had not completed. Owned stalled
gate trees were terminated after diagnosis; host security policy was not changed.

The rebuilt executable was frozen in
`/Users/wilson/workspace/styrene-labs/omegon-usability-operator-kit` with SHA-256
`a0dd018c537c1062a67db22f64d1a391a8fb3ca3e6c50c68c41d7f8dc0a2ce2f`.
Its manifest records base revision d1a06c26 and dirty source inventory. A direct
version probe did not enter within eight seconds. A native Ghostty attempt using
the outside-checkout bundle also failed before fixture/TUI readiness; evidence is
`/Users/wilson/workspace/styrene-labs/omegon-native-usability-01/ghostty/`.
The owned stalled native launcher was stopped. The prior 50 native screenshots
are baseline findings, not acceptance of this rebuilt interface.

The native driver now has an explicit `--usability` mode for empty/matching search,
unique permission choices and narrow send-hint assertions. The subsequent campaign
exercised these live assertions with the working build;
see the completion evidence below. The failed startup remains diagnostic evidence.

## Completion evidence

The subsequent dual-presentation campaign completed the omegon crate suite, final
TUI regression suite, Clippy and developer-script gates. Native usability trials
passed in both default presentations across all five installed clients, with
current-view assertions and inspected screenshots. See
[the attributed gate and capture ledger](../tui-dual-presentation/verification.md).
The old pre-entry stalls no longer block these gates. Apple Terminal input limits
remain explicit. The later GUI cleanup corrections have headless coverage only;
the native PASS records do not establish window closure. Archival remains open.

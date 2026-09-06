# Verification evidence

## Observed test-first failures

- Planner: large recent history in pressure/manual/overflow yielded no compaction
  plan before token selection. Log: `/tmp/omegon-retention-planner-red.log`.
- Budget: assembly-only target failed both requested-window/system-summary
  headroom tests. Log: `/tmp/omegon-retention-budget-red.log`.
- Advanced turn: actual loop-shaped history (counter advanced before messages)
  evicted four messages instead of three. Log:
  `/tmp/omegon-retention-advanced-turn-red.log`.

The first protected-over-budget fixture used a six-byte string and budget one;
that string fits the existing integer estimator. Correcting the fixture to a
larger active request was a test correction, not production red evidence.

## Focused results

- Final `token_retention` filter: 19 passed, zero failed. Includes planner,
  budget, loop adapter, manual handler, durable current-source alignment,
  post-request tool results, repeated summaries, reopened authority, and tamper
  rejection. Log: `/tmp/omegon-retention-all-focused-final.log`.
- Loop adapter pressure/overflow and real manual-handler integration fixtures passed.
- Effective budget tests passed for requested class and saturating system costs.
- Initial full crate gate and changed-crate Clippy passed before the subsequent
  authoritative-context repair. These are not final landing evidence.

## Authoritative context review

Review found that canonical eviction counts do not identify durable request items
when prior summaries shift positions. The latest request also omits semantic
messages admitted after that request. The repair verifies current-source alignment before mutation and validates retained
source identity and content on replay. The real semantic adapter fixture records
a tool response after its request, compacts, and verifies both complete results
remain in authoritative context. Another fixture compacts twice, reopens authority,
and verifies the newest retained message plus the new summary.

A further test reproduced non-monotonic restored turn order evicting two messages
instead of one. The planner now widens to a chronological suffix, and durable
admission rejects nonprefix legacy cuts. Log: `/tmp/omegon-retention-prefix-red.log`.

Two authority fixture corrections were needed: establishing full semantic lineage
with real step events, and removing an assumption that a snapshot file must exist
before a snapshot write. These are fixture corrections, not production red evidence.
Read-only independent review found no remaining concrete blocker after these fixes.

## Limits

Token counts remain estimates. Generated summaries have no new hard output cap.
The newest populated recent turn and connected exchange may exceed the target;
protected-only history has no eviction plan. Entirely old idle history can be
summarized. Incompatible local/durable projections, legacy/mixed lineage, and nonprefix
legacy selections fail before compaction admission. Existing provider overflow and failed-summary recovery remain separate.

## Landing and closure

Final implementation commit: `816fda57` on `feat/token-budgeted-retention`.

- `env -u NO_COLOR -u OMEGON_ASCII_GLYPHS just test-rust`: passed,
  5,663 tests passed, zero failed, 13 ignored across 47 suites. The repository
  recipe runs workspace tests serially. Log: `/tmp/omegon-retention-workspace-final.log`.
- `just lint`: passed, including formatting, workspace check, and workspace
  all-target Clippy with warnings denied. Log: `/tmp/omegon-retention-lint-final.log`.
- `cargo test -p omegon --locked --bin omegon token_retention -- --test-threads=1`:
  19 passed, zero failed.
- OpenSpec validation and whitespace checks passed before archival.

The tested Cargo-built `target/debug/omegon` SHA-256 is
`0d6bad6c3f76bbac08e40d50da0efa12cb5dd2d1169a8548e1ce54c55e6b0702`.
No installed launcher was changed. No Workbench plan was created for this pass;
OpenSpec is the task record. No upstream review marker was advanced.

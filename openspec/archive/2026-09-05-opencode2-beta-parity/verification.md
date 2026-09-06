# Verification evidence

## Scope

This pass implements instruction discovery, MCP phase deadlines, and bounded
reconnect repairs/verification. It does not claim complete OpenCode2 executable
parity. The source and registry identities remain in the comparison document.

## Test-first evidence

| Slice | Observed failure before production fix | Passing focused evidence |
|---|---|---|
| Instructions | `cargo test -p omegon --locked --bin omegon instruction_discovery -- --test-threads=1`: 2 failures, lost root policy and incorrect linked-worktree policy. | Prompt filter: 51 tests passed. Covers complete UTF-8, ancestor order, optional files, canonical deduplication, invalid reads, and worktree boundary. |
| Fixed-context admission | Oversized complete instructions passed a permissive admission stub and failed the rejection regression. | `cargo test -p omegon --bin omegon fixed_context_budget --locked`: 3 passed, including a real ReleaseCoupledLoopDriver with zero provider calls and failed terminal classification. |
| MCP | `cargo test -p omegon phase_config_regressions --locked --bin omegon`: explicit zero phase timeout was silently accepted. | `cargo test -p omegon plugins::mcp --locked --bin omegon`: 60 passed. |
| Snapshot handoff | Completion injected inside initial snapshot send was absent from the new subscriber. | Shared helper now subscribes before snapshot construction/delivery for both WebSocket routes. |
| Pending web approval | Fresh client snapshot lacked captured approval metadata. | `just test-filter parity_`: 12 passed, including 10 new tests. Includes redaction, session isolation, answered/reset/idle cleanup, and preserved TUI responder ownership. |

Independent review added two more red/green regressions: legacy tools disappeared
when optional catalog discovery stalled, and shutdown held the registry mutex
across asynchronous service settlement. Legacy partial inventory is retained,
and shutdown now releases the registry before waiting.

Additional compatibility fixtures cover MCP null/unset and invalid phase fields,
legacy shared readiness, all four paginated catalogs, concurrent operations,
progress that does not extend a deadline, and cancellation notification receipt.
Pkl schema evaluation and 14 concrete valid/invalid evaluations passed.

Existing operator-agency regressions passed: supervisor completion without
AgentEnd and subsequent input submission (1 test), plus authoritative idle
reconciliation (2 tests). Node executed the actual embedded UI functions and
verified actionable reconnect prompts and snapshot/live deduplication.

## Runtime evidence

`cargo test -p omegon --locked --test instruction_discovery_blackbox -- --test-threads=1`
passed both tests against the current Cargo-built `omegon` executable. The fixture
uses temporary home/configuration, a synthetic linked-worktree gitfile, and a
loopback provider. It verifies the complete root/intermediate/cwd text in the
actual outgoing request and zero provider requests after unreadable policy.
No personal credentials or external model service are used.

The CLI fixture initially failed because its task budget was invalid, then an
overly short fake-provider socket timeout failed under concurrent compilation.
Those were fixture defects, not production red evidence. The corrected fixture
retains bounded process-group termination and a bounded provider read timeout.

MCP uses real protocol messages over in-memory transport with a controlled clock.
A separate real `/bin/sh` startup-stall fixture verifies descendant termination.
Reconnect uses a deterministic sink inside the production snapshot handoff,
plus actual embedded browser functions in a DOM fixture; no live browser/socket
end-to-end run is claimed.

## Landing gates

Final gates passed:

- `env -u NO_COLOR -u OMEGON_ASCII_GLYPHS just test-rust`: 5,644 passed, 0 failed, 13 ignored across 47 suites, serialized by the repository recipe.
- `just lint` and `just clippy-changed`: passed.
- OpenAPI contract lint: 4 passed; embedded browser Node test: 1 passed.
- Pkl schema and 14 concrete configuration evaluations: passed.
- Current Cargo-built CLI integration: 2 passed, also included in the workspace gate.

The workspace gate executable `target/debug/omegon` SHA-256 is
`b1b42701e15549bce0c276a4774b8995ae1e3bc78bed4bcfc5b3d6ea763bb2dc`.
Implementation commits: `2d0498b8` (instructions), `03395ab1` (MCP), and
`d540a81d` (reconnect); test environment isolation is `7a8a4051`.
No installed launcher or upstream review marker was changed. No Workbench plan
was created for this pass; OpenSpec is the task record.

The initial
parallel and serialized crate runs each passed 5,041 tests and failed 17 tests
because inherited NO_COLOR forced ASCII and a credential test read external
machine OAuth state. The final gate removes presentation overrides only for
its subprocess and isolates that credential test without changing runtime policy.

## Explicit limits

- Legacy WebSocket `user_prompt` has no client submission identity. Repeated
  receipts remain separate inputs. Durable admission deduplication is not proof
  of transport retry safety.
- Approval replay covers web-owned requests already captured by a client.
  Disconnected-before-capture ownership remains a separate TUI/web decision.
- Delegate results survive client detach in a running runtime, not daemon restart.
- Snapshot/live transcript exactly-once delivery is not newly guaranteed.
- Instruction refresh remains construction-time; durable generations and live
  refresh are deferred.
- Token estimates retain the existing heuristic and cannot guarantee exact
  provider tokenization.
- Local Unix cleanup evidence does not establish remote or Windows-host process
  termination.

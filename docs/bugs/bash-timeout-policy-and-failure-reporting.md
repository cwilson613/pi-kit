+++
title = "Bug: Bash tool failures caused by overly aggressive timeouts"
tags = ["bug", "bash", "timeouts", "processes", "runtime"]
+++

# Bug: Bash tool failures caused by overly aggressive timeouts

**Status:** Repeatedly observed; ready for investigation  
**Suggested branch:** `fix/bash-timeout-policy`  
**Primary surfaces:** Bash execution, tool bus, process supervision

## Assignment brief

Inventory every deadline and cancellation layer involved in Bash execution across TUI, ACP, daemon, validation, and delegation. Establish which layer terminates representative healthy commands, then unify timeout semantics and typed outcomes. Do not treat this as merely increasing one constant before overlapping deadlines and stream-idle behavior are measured.

## Observed evidence

Cold builds, dependency resolution, test suites, repository operations, and temporarily quiet commands are terminated despite legitimate progress. Results are often rendered as command failures, encouraging expensive restarts or unnecessary migration to interactive terminals.

## Scope

- Tool-schema defaults and explicit timeout arguments.
- Tool/event-bus deadlines and executor-level deadlines.
- Wall-clock versus output-idle policy.
- Cancellation, termination grace, and descendant cleanup.
- Structured results and operator-facing diagnostics.
- Semantic parity across TUI, ACP, daemon, and delegated execution.

## Non-goals

- Making arbitrary commands permanently unbounded.
- Embedding command-name heuristics in the generic executor.
- Replacing the monitorable terminal-session facility.
- Calling a wrapper timeout a compiler/test failure without child-process evidence.
- Broad tool-runtime redesign not required by the deadline contract.

## Investigation targets

Search for `timeout`, `deadline`, `Duration`, `idle`, `heartbeat`, cancellation tokens, process groups, child waiting, stream closure, output truncation, and `ToolResult`. Inspect Bash schema defaults, event/tool bus execution, API/tool-call ceilings, validation wrappers, delegation, loops, transport response limits, and seconds/milliseconds conversion.

Produce a table containing layer, owner, default, maximum, idle semantics, cancellation source, and rendered outcome.

## Required outcome model

Keep these distinct:

- command exited non-zero;
- spawn failed;
- caller cancelled;
- wall-clock deadline expired;
- idle deadline expired;
- outer transport/tool-call deadline expired;
- output collection or serialization failed;
- descendant cleanup was incomplete.

A timeout result must report elapsed time, effective budget, firing policy layer, and termination status without exposing sensitive command arguments.

## Architectural constraints

- One effective deadline policy must be inspectable before dispatch.
- Inner layers must not silently clamp caller policy.
- Output activity may affect an idle deadline but not silently redefine a hard ceiling.
- Stream closure is not equivalent to child-process exit.
- Timeout and cancellation target a process group or platform equivalent.
- Long interactive work remains a terminal-session concern, but Bash defaults must support ordinary cold builds.

## Implementation sequence

1. Instrument and document every deadline layer.
2. Reproduce active-output and quiet-output premature termination.
3. Define the canonical deadline/outcome contract.
4. Remove duplicate or hidden clamps and propagate effective policy.
5. Harden stream and child-wait coordination.
6. Standardize process-tree termination and cleanup reporting.
7. Update result projection across all execution surfaces.
8. Add real-process race and descendant tests.

## Acceptance criteria

1. Representative cold Rust builds complete under default or explicit policy.
2. A live quiet command is not considered dead solely because output pauses or a stream closes.
3. Explicit timeouts are honored and never silently shortened.
4. Timeout, cancellation, exit failure, and spawn failure are structurally distinct.
5. Diagnostics identify the policy layer that fired.
6. Timeout/cancellation terminates descendants and reports survivors.
7. True hard-ceiling expiry terminates within a bounded grace period.
8. TUI, ACP, daemon, validation, and delegation share semantics.
9. Agents are not told to restart an indeterminate cold build as though compilation failed.

## Regression plan

Use real children and grandchildren to cover continuous output, multi-minute quiet periods, closed stdout, closed stderr, parent exit with live descendant, explicit short timeout, timeout/cancellation race, large near-deadline final output, and each dispatch surface.

## Validation

Run focused Bash/process tests plus:

```bash
cargo test -p omegon <bash-timeout-filter>
just clippy-changed
git diff --check
```

Use an interactive terminal for any cold broad gate that exceeds blocking tool-call limits; do not restart an indeterminate build.

## Dependencies and conflict risks

This overlaps with `background-process-and-terminal-lifecycle-leaks.md` around process groups and cleanup. Agree on shared process-supervision contracts before both branches alter them. Runtime-default changes may affect every tool consumer and require explicit compatibility notes.

## Definition of done

The deadline map is documented, hidden clamps are removed or explicit, outcomes are typed and projected consistently, process trees clean up reliably, the real-process regression matrix passes, and the focused branch is validated and committed.

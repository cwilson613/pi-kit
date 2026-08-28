# Runtime contribution lifecycle - Delta Spec

## MODIFIED Requirements

### Requirement: Codescan is one managed workspace index service

The optional release-coupled codescan contribution must run as a supervised native extension and expose a versioned codescan RPC interface to boot-captured host adapters. One serial extension-owned worker must exclusively own the workspace SQLite connection, indexing, HEAD freshness checks, and BM25 construction for `codebase_search`, `codebase_index`, and `request_context(kind="code")`. The concrete engine, worker, connection, `ScanCache`, and `Indexer` must not escape through the wire contract. The extension process must stop accepting work, cancel active and queued requests, join its worker, and close SQLite before graceful shutdown completes. The host extension supervisor must terminate and reap the complete process group when graceful shutdown fails.

#### Scenario: Tool and context requests share one extension worker
Given a compatible codescan extension was admitted at boot
When tool search, explicit indexing, and code-context requests execute concurrently
Then every request uses the captured extension RPC handle and one serial worker
And only that extension worker opens or mutates the workspace codescan database
And host adapters perform no ambient lookup or direct cache fallback

#### Scenario: Incremental path update is cancelled
Given the active index contains a previously committed path
When cancellation occurs after replacement preparation but before that path transaction commits
Then the transaction rolls back the complete path replacement
And the previously committed path remains searchable without partial replacement rows
And pruning and HEAD metadata do not advance for the incomplete run

#### Scenario: Full invalidation is cancelled
Given the active index contains a searchable committed generation
When `codebase_index` with `invalidate=true` is cancelled before rebuild commit
Then the complete rebuild transaction rolls back
And the prior searchable index, file state, pruning state, and HEAD metadata remain active

#### Scenario: Codescan shuts down
Given the active codescan extension owns its worker and SQLite writer
When normal host shutdown closes extension admission
Then admitted calls settle or are cancelled within the active-call deadline
And the extension joins its worker and closes SQLite before graceful completion
And a non-cooperative process tree is terminated and reaped by the host supervisor

#### Scenario: Codescan handle becomes unavailable
Given a consumer retained the boot-captured codescan RPC binding
When the extension is quarantined, stopped, incompatible, or retired
Then the binding returns typed unavailable evidence
And the consumer does not open the database or switch to another implementation

#### Scenario: Codescan is absent
Given the codescan extension is unavailable at boot
When a codescan tool or mixed context request executes
Then the host-owned codescan tool remains declared and returns typed unavailable evidence
And the code context part reports unavailable rather than no matches
And unrelated requested context kinds remain callable
And no extension process, SQLite connection, or direct indexing fallback is fabricated

## ADDED Requirements

### Requirement: Runtime doctor recommends explicit process replacement

The host must expose `/doctor` and `/runtime doctor` as read-only diagnostics over the published dynamic contribution inventory and live extension supervisors. A finding for an unavailable or unhealthy extension must identify the affected contribution, state observable evidence, and recommend `/runtime replace <name>`. Doctor must not restart, replace, reload, or otherwise mutate a contribution.

`/runtime replace <name>` must perform one bounded re-instantiation from the currently admitted immutable snapshot. It must preserve the published contribution generation, host-owned schemas, and existing supervisor-backed handles. It must not inspect newly installed source bytes, retry in a loop, consume automatic restart budget, or replace unrelated contributions.

#### Scenario: Doctor finds an unavailable extension
Given a published extension supervisor has no callable child process
When the operator runs `/doctor` or `/runtime doctor`
Then the report identifies that extension as unavailable
And it recommends `/runtime replace <name>`
And no process or contribution state is mutated

#### Scenario: Operator replaces one extension process
Given an extension was published from an admitted immutable snapshot
When the operator runs `/runtime replace <name>`
Then the host stops and reaps the prior process tree
And it spawns and handshakes one replacement from the retained snapshot
And existing host bindings route to the replacement without EventBus republication
And unrelated contributions remain unchanged

#### Scenario: Replacement fails
Given a published extension cannot complete replacement startup or compatibility checks
When `/runtime replace <name>` executes
Then the failed candidate process is reaped
And that extension remains unavailable with bounded diagnostic evidence
And no automatic retry loop starts

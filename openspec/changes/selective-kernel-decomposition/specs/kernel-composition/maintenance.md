# Kernel composition: independent maintenance - Delta Spec

## ADDED Requirements

### Requirement: Maintenance is an independent workspace artifact

The workspace must contain package `omegon-maintain` at `core/crates/omegon-maintain/`, producing executable `omegon-maintain`, plus package `omegon-maintenance-contracts` at `core/crates/omegon-maintenance-contracts/`. The contracts package owns only versioned deny, session-deny, ownership-record, exclusion-lock, transaction, audit, and package-manifest schemas and canonical key derivation. Both executables may depend on it, but `omegon-maintain` must not depend on package `omegon` or initialize the normal TUI, default loop, provider clients, project configuration, project contribution or extension code, MCP, mutable content packs, memory, lifecycle, or orchestration. The exact v1 wire and interoperability contract is defined by `maintenance-protocol-v1.md`.

#### Scenario: Normal integration startup is broken
Given the normal Omegon executable cannot complete startup
And the installed release artifacts are otherwise intact
When the operator launches `omegon-maintain identity`
Then maintenance reaches a diagnostic-ready state without invoking normal startup
And reports its own artifact identity and compiled exclusions

#### Scenario: Corrupt project contribution exists
Given the selected workspace contains malformed or incompatible project contributions
When maintenance starts for an inert inspection command
Then contribution-controlled code, prompts, hooks, templates, and dynamic configuration are not evaluated
And diagnostics can still identify the inert entry and its filesystem type

### Requirement: Slice-zero command vocabulary is explicit and bounded

The initial executable must expose only this command tree:

```text
omegon-maintain
  identity
  doctor
  composition inspect
  contribution list [--scope <user|project>]
  contribution inspect <selector> --scope <user|project>
  contribution disable <selector> --scope <user|project>
  contribution quarantine <selector> --scope <user|project>
  session list
  session inspect <session-id> --workspace <absolute-path>
  session quarantine <session-id> --workspace <absolute-path>
  resource list --workspace <absolute-path>
  resource prune-stale --workspace <absolute-path>
  release inspect
  release verify --archive <path> --manifest <path> --bundle <path>
  audit inspect [--cursor <cursor>]
  audit verify
```

Global options are `--json`, `--deadline <duration>`, `--home <absolute-path>`, `--config-home <absolute-path>`, `--workspace <absolute-path>`, `--dry-run`, and `--request-id <uuid>`. Project contribution, selected-session, and resource commands require explicit `--workspace`; mutation targets are never inferred from Git, project configuration, or contribution metadata. Every mutation, including dry-run mutation planning, requires explicit `--deadline`. Durations use an unsigned integer plus `ms`, `s`, or `m`, reject zero/overflow, and round neither upward nor downward. Read-only commands default to 30 seconds, offline release verification defaults to 5 minutes, and no accepted deadline may exceed 10 minutes.

`--home`, `--config-home`, and `--workspace` are explicit operator authority grants, not paths reinterpreted from untrusted metadata. Each must name an existing absolute directory owned by the effective user, must not be `/` or alias another granted root, and must be opened once without following a final symlink. The opened descriptor identity is authoritative for the command. Mutating commands reject group/other-writable roots, unsupported filesystems lacking required no-follow/atomic primitives, and root identity changes. Read-only commands report unsafe ownership, modes, aliases, or filesystem capabilities as degraded and do not upgrade to mutation.

#### Scenario: Unknown or deferred command is requested
Given the operator requests generic shell, patch, semantic session recovery, contribution enable/purge, process kill, network update, or rollback
When CLI validation runs
Then the command is rejected as unsupported in the Slice-zero contract
And no project or contribution-controlled input is evaluated

#### Scenario: Project-scoped mutation omits workspace
Given the operator requests a project contribution mutation without `--workspace`
When CLI validation runs
Then the command fails before filesystem mutation
And diagnostics identify the missing explicit scope

### Requirement: Structured output and exit status are stable

On normal termination with writable stdout, `--json` makes stdout contain exactly one JSON object with `schema_version`, `command`, `status`, `request_id`, artifact identity, maintenance composition identity and exclusions, deadline evidence, diagnostics, mutations, and errors. `status` is `success`, `failure`, or `degraded`. Exit 0 means every requested operation settled successfully; exit 1 means definite failure, refusal, invalid arguments, unsupported operation, dry-run refusal, or timeout before mutation dispatch; exit 2 means partial diagnostic or mutation, unverifiable evidence, deadline after possible dispatch, unknown settlement, audit-settlement failure, or output failure after mutation. After `--json` is recognized, argument and admission failures use the same envelope; progress and logs use stderr only.

Each mutation entry has `planned`, `prepared`, `dispatched`, `applied`, `settled`, or `unknown` state plus retry safety. A failed quarantine that retains a settled deny record is `degraded` with exit 2, not a total failure. `--dry-run` may bootstrap/acquire maintenance-owned OS lock files and durably append a dry-run audit record, but must not create a transaction fence, deny record, quarantine entry, session-deny record, or alter an ownership record; output uses `planned`. Signal cancellation before `Dispatched` settles as failure; after `Dispatched`, the process completes or marks the transaction unknown before honoring a catchable signal. `SIGKILL`, process abort, unwritable stdout, and uninterruptible kernel I/O are outside the output/deadline guarantee and are reconciled from durable transaction state on restart.

#### Scenario: Diagnostic is partially available
Given composition inspection succeeds but one inert entry is unreadable
When `doctor --json` completes
Then stdout contains one valid result with `status: degraded`
And successful findings remain present
And the process exits 2

#### Scenario: Human output is requested
Given `--json` is absent
When a command completes
Then stdout contains bounded human-readable output derived from the same result DTO
And logs or progress do not alter the machine-readable contract used by JSON mode

### Requirement: Slice-zero contribution operations are inert and non-destructive

Contribution list and inspect may read only bounded, non-executable metadata from compiled allowlisted contribution roots. They must not follow contribution-entry symlinks, expand environment substitutions, resolve commands, fetch network content, load prompt/skill bodies, or execute probes. `disable` writes a maintenance-owned deny record consulted before normal runtime contribution parsing. The contracts package defines a per-scope exclusion protocol: normal startup holds a shared lock from before deny lookup through contribution-controlled parsing and activation, while maintenance holds the exclusive lock from before deny preparation through detach settlement. A malformed or unreadable deny store fails closed for affected contribution activation. Cached composition cannot bypass a newer deny generation.

`quarantine` first settles the deny record and then atomically renames a real entry into a securely created same-filesystem maintenance quarantine, or unlinks only the entry itself when it is a symlink. The destination uses a request-ID-derived nonexisting name and an atomic no-replace operation. Maintenance validates source identity under the exclusive lock, verifies source disappearance and destination identity after rename, and reports unknown/degraded rather than claiming success where platform primitives cannot bind those facts. It never edits, overwrites, copies, or recursively deletes contribution-controlled contents.

#### Scenario: Symlink contribution is inspected
Given an allowlisted contribution entry is a symbolic link
When maintenance inspects it
Then output reports the link and bounded uninterpreted link text
And the symlink target is not opened

#### Scenario: Contribution is disabled
Given a contribution ID and explicit scope resolve to an allowlisted entry
When `contribution disable` settles
Then an idempotent deny record and audit entry are durably written under maintenance-owned state
And output states that future activation is denied but no running process was terminated

#### Scenario: Startup races contribution quarantine
Given normal startup is reading or activating a contribution under the shared exclusion lock
When maintenance requests quarantine for the same scope
Then maintenance cannot prepare or detach the entry until startup releases the lock
And normal startup cannot begin contribution parsing after the exclusive maintenance lock is held

#### Scenario: Contribution cannot be atomically quarantined
Given a real contribution directory cannot be renamed into the same-filesystem quarantine
When `contribution quarantine` runs
Then the operation fails without copy-and-delete fallback
And the original entry remains in place with the deny record retained

### Requirement: Slice-zero session operations preserve original bytes

Session list and inspect validate snapshot/metadata framing, stored normalized workspace identity, IDs, filenames, schema/version, file types, size, and digest. Selection by ID requires explicit workspace and fails closed on zero or multiple exact `(session_id, workspace_identity)` matches; comparison does not open project contents. `session quarantine` installs a maintenance-owned deny record preventing resume while preserving session and metadata bytes. Every normal resume path, including interactive, daemon, ACP, and stale-cache restoration, consults the versioned record before deserializing session-controlled bytes; malformed deny state fails closed for the selected session. Slice zero must not truncate, rewrite, synthesize terminal events, reconstruct conversation history, or claim the current LLM-facing snapshot is canonical event truth.

#### Scenario: Workspace slug collides
Given two session records have colliding filesystem slugs but different canonical working directories
When a workspace-scoped session is selected
Then exact canonical metadata equality is required
And the other session is not inspected or quarantined

#### Scenario: Session pair is malformed
Given session JSON or metadata is missing, malformed, mismatched, symlinked, or unexpected file type
When session inspection runs
Then the original bytes remain unchanged
And diagnostics report the framing defect and whether quarantine is available

### Requirement: Slice-zero resource-record pruning uses durable ownership evidence

Resource commands require explicit workspace and report only versioned durable Omegon ownership records with boot ID, runtime/generation identity, PID/process-group where available, process-start token, lifecycle boundary, heartbeat, and cleanup capability. Normal runtime writers adopt this schema in Slice zero. Legacy, malformed, filename-inferred, or incomplete records are inspect-only and unverifiable. `resource prune-stale` may delete a runtime record only when heartbeat expiry and dead process identity are both proven against the recorded boot and process-start tokens. It must not kill arbitrary or previously spawned processes. Recorded cross-boundary cleanup capability or historical status is `best_effort` or `unverifiable`; Slice zero does not perform or claim process-tree settlement.

#### Scenario: PID is alive but identity does not match
Given a stale record's PID has been reused by another process
When pruning evaluates process identity
Then the record is not authority to signal that process
And cleanup is refused or reported degraded

### Requirement: Slice-zero release verification is offline and fail-closed

`release verify` may inspect only explicit archive, signed package manifest, and Sigstore bundle operands. The bundle contains the signature, certificate chain, Rekor inclusion proof, and signed checkpoint required for offline verification against compiled Fulcio/Rekor trust roots and a versioned compiled repository/workflow/issuer/ref policy. The signed manifest binds archive digest, exact target/version/tag/commit, build provenance, and digests plus identities of both executables. Verification streams archive members without extracting or executing them and matches exact member bytes to the manifest. It rejects invalid chains, absent or internally inconsistent transparency evidence, claim/subject mismatch, traversal, duplicate or case-colliding paths, absolute/platform-prefixed paths, links, devices, oversized entries, unexpected executables, and archives lacking matching `omegon` and `omegon-maintain` identities. Slice zero does not discover, download, install, activate, update, switch, or roll back releases.

#### Scenario: Archive contains a traversal entry
Given a signed archive contains a path escaping its extraction root
When offline verification runs
Then verification fails before extraction or execution
And no archive member is written to the filesystem

### Requirement: Mutation roots and durable writes are constrained

Maintenance may write only under the maintenance state root, its audit/lock/deny/session-deny children, same-filesystem quarantine directories under compiled allowlisted contribution parents, and stale records under the explicit workspace `.omegon/runtime/` root. It must not mutate project source/configuration, `.git`, `ai`, `docs`, `openspec`, memory/lifecycle/secrets stores, session snapshot contents, package-manager-owned installations, symlink targets, or arbitrary manifest-supplied paths.

Path traversal must use descriptor-relative no-follow operations with file-identity revalidation before mutation. Quarantine roots are securely created beneath already-open allowlisted parents, are owned by the effective user, reject group/other write, and are never accepted through symlinks. Writes use unique create-exclusive temporary files, restrictive permissions, flush and fsync, atomic no-replace rename where uniqueness matters, and parent-directory fsync. Lock acquisition and cleanup consume one monotonic absolute command deadline and never fall back to unbounded waits, overwrite, or remove-then-create replacement.

Before any target mutation, maintenance durably writes a request transaction `Prepared`, including command fingerprint and exact root/target identities, then `Dispatched` immediately before the first external mutation. It settles as `Settled` or `Unknown`. The prepared record includes a durable per-domain fence consulted before later mutations. Reusing a request ID with the same fingerprint resumes deterministic reconciliation; conflicting reuse is refused. Restart reconciliation compares recorded identities and deny/destination/record state without repeating an unknown mutation, then settles or retains the fence. Audit records are sequence-numbered and hash-chained with checkpoints; `audit verify` claims structural continuity only, not authenticity against an external attacker.

The deadline starts before root admission. It is cooperative rather than a hard real-time guarantee: maintenance checks the monotonic deadline before every lock and potentially blocking filesystem or verification operation, dispatches no mutation without remaining budget, and classifies an overrun after dispatch as unknown/degraded. Slice-zero commands spawn no child process. A deadline never extends because progress occurred.

Version 1 limits are: 1 MiB per inert metadata or session metadata file, 4 KiB symlink text, 10,000 inert/session/resource entries per command, 4 MiB JSON or human output, 2 GiB archive bytes, 4 GiB aggregate uncompressed member bytes, 100,000 archive members, 1 GiB per archive member, and 100,000 audit records per verification invocation. One-over-limit inputs fail closed or make an aggregate diagnostic degraded without consuming or displaying unbounded bytes. Supported archive formats and target-specific path normalization are compiled and reported by `identity`.

#### Scenario: Contribution ID contains traversal
Given a contribution ID contains separators, parent traversal, NUL, or platform prefix
When target resolution runs
Then the operation fails before opening a mutation target

#### Scenario: Settlement write fails
Given a mutation was applied but audit or result settlement cannot be durably written
When the command exits
Then the pre-existing durable transaction fence blocks subsequent maintenance mutations
And output reports degraded or unknown settlement with exit 2 rather than success

### Requirement: Maintenance is packaged and documented from the first slice

Source, linked-development, and direct-install paths plus the repository's platform archives, Homebrew formula, Nix package, and OCI profile must build and independently launch-test `omegon-maintain`. Upon Slice-zero implementation, each supported package containing `omegon` must contain and expose `omegon-maintain` as a release-coupled companion; omission or identity/version mismatch fails closed. Platform archives place both executable members at archive root. The release matrix records for each path whether signing applies and tests direct companion invocation plus missing/incompatible-companion failure. The same lane updates durable architecture/operator docs, public install/recovery pages, and canonical command snippets.

#### Scenario: Packaging omits maintenance
Given a supported release packaging path
When package composition is validated
Then omission, incompatibility, or failure to launch `omegon-maintain` fails the packaging gate

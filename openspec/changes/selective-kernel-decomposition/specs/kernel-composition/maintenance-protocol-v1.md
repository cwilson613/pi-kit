# Kernel composition: maintenance protocol v1 - Delta Spec

## ADDED Requirements

### Requirement: Protocol v1 has one canonical encoding

All maintenance-owned records use UTF-8 JSON with no BOM, duplicate object keys, floating-point numbers, or unknown fields. Writers emit lexicographically sorted object keys, shortest decimal integers, and one trailing newline. Digests cover the bytes before that trailing newline using SHA-256. Readers accept reordered keys but reject every other noncanonical condition before granting authority. Every record contains integer `schema_version: 1`, `record_kind`, and `record_id`. Wire fixtures for every record and one-corruption variants must live in the shared contracts crate and be consumed unchanged by both executables.

Paths are never authority-bearing display strings. A `PathIdentityV1` contains `dialect` (`unix` in Slice zero), base64url-no-pad canonical absolute path bytes, device and inode/file-index identity, and `key`. Root canonical bytes come from the opened descriptor's kernel-resolved absolute path after rejecting a final symlink; lexical child normalization removes `.` and rejects `..`, absolute children, NUL, and platform prefixes without following links. Under the Unix dialect, backslash is an ordinary raw basename byte; the separate archive verifier rejects backslash as a portable archive-path separator.

All authority keys use `H(label, fields...) = sha256("omegon-maint-v1\0" || u64be(len(label)) || label || each(u64be(len(field)) || field))`. The formulas are: path key `H("path", dialect, canonical_path_bytes)`; scope key `H("scope", kind, scope, parent_path_key)`; valid entry key `H("entry", kind, scope_key, raw_basename_bytes)`; opaque selector `entry:sha256:` plus that entry key in lowercase hex; session key `H("session", session_id, workspace_key)`; workspace key `H("workspace", dialect, lexical_absolute_workspace_bytes)`; resource domain key `H("resource", workspace_key)`; contribution domain key `H("contribution", scope_key)`; session domain key `H("session-domain", session_key)`; and command fingerprint `H("command", canonical JSON of command, semantic options, opened root keys, and selector)`. Formatting flags, request ID, deadline, and dry-run are excluded from the command fingerprint.

Record IDs are kind-specific: installation state `H("installation", installation_uuid)`; deny state `H("deny-state", scope_key, u64be(generation))`; deny entry `H("deny", scope_key, entry_key, request_id)`; session deny `H("session-deny", session_key, request_id)`; transaction `H("transaction", request_id)`; fence `H("fence", domain_key, transaction_record_id)`; audit `H("audit", installation_uuid, u64be(sequence))`; audit checkpoint `H("audit-checkpoint", installation_uuid, u64be(last_sequence), last_digest)`; audit frontier `H("audit-frontier", installation_uuid, u64be(current_segment_start), current_segment_previous_digest-or-zero, u64be(previous_segment_start-or-zero), previous_segment_previous_digest-or-zero)`; audit receipt `H("audit-receipt", installation_uuid, request_id, command, outcome, u64be(sequence), audit_digest)`; ownership `H("ownership", workspace_key, runtime_id, generation_id)`; and package manifest `H("package", archive_digest, target, version)`. Display paths are optional redacted diagnostics. Slice zero supports the current Linux and macOS release targets; unsupported path dialects fail before mutation.

#### Scenario: The binaries disagree about a record
Given the same canonical v1 fixture is loaded by `omegon` and `omegon-maintain`
When each derives IDs, keys, and digests
Then byte-for-byte results are equal
And duplicate, unknown, oversized, or noncanonical authority fields fail closed

### Requirement: Default roots and allowed relative paths are exact

Absent explicit overrides, maintenance home resolves from absolute `OMEGON_HOME`, otherwise absolute `$HOME/.omegon`; missing/nonabsolute values fail. Config home defaults to absolute `$HOME/.config/omegon`; Slice zero defines no config-home environment variable. Explicit CLI overrides supersede those defaults. Equal descriptor identities are aliases and rejected; ordinary nesting is allowed but each command remains confined to its declared root descriptor.

The complete v1 read/mutation path map is:

| Domain | Scope | Descriptor-relative entry |
|---|---|---|
| extension | user | `<home>/extensions/<entry>` |
| plugin | user | `<home>/plugins/<entry>` |
| skill | user | `<home>/skills/<entry>` |
| prompt | user | `<home>/prompts/<entry>.md` |
| catalog | user | `<home>/catalog/<entry>` |
| plugin | project | `<workspace>/.omegon/plugins/<entry>` |
| skill | project | `<workspace>/.omegon/skills/<entry>` |
| prompt | project | `<workspace>/.omegon/prompts/<entry>.md` |
| workflow | project | `<workspace>/.omegon/workflows/<entry>.toml` |
| session pair | config | `<config-home>/sessions/<legacy-workspace-slug>/<session-id>{.json,.meta.json}` |
| ownership | project | `<workspace>/.omegon/runtime/<runtime-id>/ownership-v1.json` |
| maintenance state | user | `<home>/maintain/v1/` as defined below |

Environment-selected plugin/pack directories, executable-relative shipped data, `armory/personas`, and arbitrary catalog bundle paths are diagnostics-only and never Slice-zero mutation roots. Missing canonical roots are empty inventory, not permission to scan ancestors. Session reads preserve the legacy directory layout but require metadata equality; no slug is treated as identity.

Contribution selectors are `<kind>:<id>` for valid IDs matching `[A-Za-z0-9][A-Za-z0-9._-]{0,127}`. Invalid or colliding directory entries are listed with opaque selector `entry:sha256:<hex>`, derived from kind, scope key, and raw basename bytes; that exact selector may be inspected, denied, or quarantined without interpreting the basename. Prompt/workflow suffixes are framing and excluded from the logical ID.

#### Scenario: An environment points outside canonical roots
Given `OMEGON_PLUGIN_DIR` or contribution metadata names an arbitrary directory
When maintenance lists or mutates contributions
Then that directory is not opened as an authority root
And diagnostics identify it as excluded startup input

### Requirement: Maintenance state has a finite bootstrap layout

The complete state tree is:

```text
<home>/maintain/v1/
  state.json
  locks/bootstrap.lock
  locks/audit.lock
  locks/contribution-<scope-key>.lock
  locks/session-<session-key>.lock
  locks/resource-<workspace-key>.lock
  deny/<scope-key>/state.json
  session-deny/<session-key>.json
  transactions/<request-id>.json
  fences/<domain-key>.json
  audit/segments/<first-sequence>.jsonl
  audit/checkpoint.json
  audit/frontier.json
  audit/receipts/<request-id>.json
```

Each allowlisted contribution parent may additionally contain
`.omegon-maintain-quarantine/<request-id>-<entry-key>` on that parent's
filesystem. It is not a child of the maintenance state root.

Secure creation of the fixed `.omegon-maintain-quarantine` directory is
transaction-exempt protocol infrastructure: it occurs under the contribution
domain lock before transaction preparation, uses descriptor-relative
create-exclusive mode `0700`, validates an existing directory by owner, mode,
type, and descriptor identity, fsyncs the contribution parent after creation,
and is included in the command audit outcome. It never creates a destination
entry before the transaction is prepared.

Bootstrap may create only missing components in this tree, descriptor-relative, with user-only permissions, create-exclusive files/directories, and parent fsync. The first creator establishes `bootstrap.lock`; later processes open and lock it before creating another component. Normal startup and maintenance share this bootstrap code. Infrastructure creation is not a target mutation and is exempt from transaction recursion, but every creation is recorded in the command audit and cannot overwrite existing state. Existing wrong-type, symlinked, wrongly owned, or group/other-writable components fail closed. Dry-run may bootstrap `state.json`, `audit/`, and required lock files but no transaction, fence, deny, quarantine, session-deny, or ownership target.

The fixed empty `deny/`, `session-deny/`, `transactions/`, and `fences/`
directories are finite bootstrap infrastructure rather than target records and
may be created by dry-run; dry-run creates no file or quarantine entry within
them.

Lock ordering is bootstrap, then one domain lock, then transaction/fence writes,
then `audit.lock` only while assigning and appending an audit sequence. Code never
acquires a domain or bootstrap lock while holding `audit.lock`. Contribution and
session commands use their named domain locks; resource commands use the
workspace resource lock; audit-only commands use only `audit.lock`. Restart
reconciliation acquires the same domain lock before reading its fence.

Slice zero protects against malformed or untrusted stored bytes and races among
cooperating Omegon processes. It cannot contain a process already executing as
the same effective user, because that process can rewrite any user-owned source
or maintenance state. Quarantine reports the pathname entry atomically moved by
the syscall and its post-observed identity; it reports `unknown` if that identity
differs from the prepared observation.

`state.json` binds the opened home identity, schema version, next audit sequence, and installation UUID. A different home identity or duplicate installation UUID is refused rather than silently adopted.

`audit/frontier.json` is the canonical bounded segment-recovery frontier and binds the current and immediately previous segment boundaries to their predecessor digests. `audit/receipts/<request-id>.json` is the canonical durable settlement receipt binding one request, command, outcome, sequence, and audit digest. Both are authority-bearing records covered by the canonical encoding, validation, fixture, and corruption requirements above; receipt filenames are lowercase request UUIDs.

#### Scenario: First mutation starts with no maintenance state
Given the trusted home exists and `maintain/v1` does not
When an admitted mutation starts
Then bootstrap creates only the listed maintenance-owned components
And target mutation does not dispatch until bootstrap and audit preparation are durable

### Requirement: Deny and exclusion records interoperate with normal startup

`DenyRecordV1` contains record identity, scope/path identity key, contribution kind, entry key, raw-name digest, monotonically increasing scope generation, state `denied`, request ID, and UTC creation time. `DenyStateV1` contains the scope key, generation, and a map of entry key to complete deny record in one canonical file, limited to 16 MiB and 10,000 entries. An update writes one complete generation and atomically replaces only `state.json`; deny record and generation cannot diverge. Re-disabling an equal existing deny is idempotent and does not increment generation. `SessionDenyRecordV1` contains record identity, session key, session ID, workspace key, state `resume_denied`, request ID, and UTC creation time. Deny filenames use only derived hex keys, never untrusted IDs.

The contribution scope key hashes kind plus the opened contribution-parent `PathIdentityV1`; the session key hashes the exact session ID plus workspace key. Advisory file locks use whole-file exclusive/shared locks on the corresponding pre-created lock file. Slice-zero supported targets use `flock`-equivalent non-inheritable locks. Failure to establish equivalent semantics refuses activation or mutation.

On first use of a scope, normal startup or maintenance acquires its exclusive lock and atomically creates and fsyncs an empty `DenyStateV1` at generation zero before releasing the lock. The state file itself is the initialization marker. A loser in the initialization race reopens and validates the winner's state; any non-absent conflict fails closed. Normal contribution startup then acquires the scope shared lock before reading deny state, an unresolved domain fence, cached composition, or contribution bytes and holds it through parse/activation publication. An unresolved fence or missing/malformed state after lock initialization fails closed. `disable` and `quarantine` acquire the exclusive lock before reading prior state, hold it through deny and audit settlement, and for quarantine through detach settlement. A non-idempotent update increments generation exactly once in the atomic deny-state replacement. A cache records generation and is reusable only while checked under the same shared lock against equal generation.

Every session resume path acquires the session shared lock before deny lookup and holds it through snapshot/metadata deserialization and resume admission. Session quarantine holds the exclusive lock through deny/audit settlement. Malformed state fails closed for that session. A successful deny/quarantine therefore governs resume or activation attempts beginning after settlement; already-published runtime behavior is not revoked.

#### Scenario: Disable races startup
Given startup has not acquired the shared contribution lock
When disable acquires the exclusive lock and settles generation N
Then later startup observes generation N and the deny before opening contribution bytes
And a cache from generation N-1 is unusable

### Requirement: Mutation transactions have step-level crash semantics

`TransactionV1` contains a lowercase UUID request ID, canonical command fingerprint, domain key, complete opened root identities, ordered `steps`, current state, UTC timestamps, and audit sequence. Each step contains `kind`, complete source parent identity, base64url-no-pad source basename bytes plus their SHA-256 digest, exactly one of expected existing target identity or expected absence, intended canonical-content digest for record writes, state, and observed post-state. Persisting the validated basename bytes is required so restart reconciliation can prove source absence even for opaque non-UTF-8 entries; the digest must match those bytes and the decoded value must remain one safe child name. A real-entry quarantine rename additionally contains the complete already-open destination-parent identity and equally framed destination basename; settlement proves source absence and destination identity without reading or inventing a recursive directory-content digest. Symlink quarantine uses the distinct `quarantine_symlink_unlink` step and settles only by proving source absence with no destination. Record IDs, session keys, timestamps, state/step combinations, and evidence combinations are validated as one semantic unit. Transaction infrastructure writes and audit writes are not represented as recursively nested target steps.

The state machine is:

```text
Prepared -> StepDispatched(n) -> StepSettled(n) -> ... -> TargetsSettled -> Settled
                              \-> Unknown
Prepared -> Aborted
```

The transaction and domain fence are fsynced before step 1. Each step is fsynced as dispatched immediately before its syscall and settled immediately after post-observation. New targets record parent identity, basename, and expected absence. Deny-state replacement binds the exact initialized state-file identity and intended next-generation content digest; it is never a create-over-absence operation. Quarantine uses separate deny-generation and detach steps. A deadline/cancellation before a dispatched step marks `Aborted` and clears the fence after audit settlement. Once any step is dispatched, failure to prove its post-state marks `Unknown` and retains the fence.

Restart reconciliation holds the domain exclusion lock and permits no new domain mutation. It compares exact recorded identities and contents:

| Observation | Recovery result |
|---|---|
| no step dispatched | abort, audit, clear fence |
| idempotent record equals intended canonical bytes | settle that step without rewriting |
| expected source absent and exact destination identity/content present | settle detach |
| source exact identity present and destination absent, and detach was not dispatched | mark step aborted; do not retry automatically |
| source exact identity present and destination absent, and detach was dispatched | retain `Unknown` fence |
| conflicting source, destination, record, or generation | retain `Unknown` fence |
| required observation unavailable | retain `Unknown` fence |

Same request ID plus identical fingerprint invokes reconciliation and returns its result. Same ID plus different fingerprint is `request_id_conflict`. No unknown step is automatically repeated or compensated.

#### Scenario: Crash occurs between detach dispatch and result persistence
Given the transaction durably records the exact source and absent destination
And detach is `StepDispatched`
When maintenance restarts
Then it settles only if exact source disappearance and exact destination presence are proven
And otherwise retains an unknown fence without retrying rename

### Requirement: Session framing v1 is explicit without rewriting legacy bytes

Slice-zero session identity uses `workspace_key = H("workspace", "unix", lexical_absolute_workspace_bytes)`. Lexical normalization removes `.` and resolves `..` without following workspace symlinks; the exact opened workspace descriptor identity is recorded separately for the command. A moved workspace has a different key unless its stored absolute bytes match the explicit workspace. Non-UTF-8 path bytes are base64url encoded in protocol records.

Legacy `SessionMeta` is read as a bounded object requiring `session_id` and `cwd`; unknown fields are inert. The snapshot is a bounded object requiring integer `schema_version`. A pair is selected only when filename ID, metadata ID, normalized metadata cwd key, and explicit workspace key match uniquely. Inspection opens both regular files without following links, records identity and size before read, computes SHA-256 while reading, and requires unchanged identity/size/mtime after read. Concurrent change is degraded and never authorizes quarantine based on the unstable pair. Quarantine keys the deny to session ID plus workspace key; it never writes either legacy file.

#### Scenario: Session changes during inspection
Given the metadata or snapshot identity, size, or mtime changes while being read
When inspection completes
Then the pair is reported unstable and degraded
And no digest is represented as a stable framing result

### Requirement: Ownership-record pruning has a complete decision table

`OwnershipRecordV1` contains runtime ID, generation ID, workspace key, boot ID, PID, optional process group, platform process-start token, lifecycle boundary, cleanup capability, writer artifact identity, UTC heartbeat, monotonic-since-boot heartbeat ticks, and 300-second expiry. Writers atomically refresh both clocks under the runtime directory. A heartbeat more than 300 seconds in the future is unverifiable clock skew.

Pruning decisions are:

| Evidence | Result |
|---|---|
| current boot differs, UTC heartbeat expired | prune |
| same boot, heartbeat expired, PID absent | prune |
| same boot, heartbeat expired, PID exists with different start token | prune record only; never signal PID |
| same boot, PID/start token match | retain regardless of stale heartbeat |
| heartbeat not expired | retain |
| permission denied, unsupported token, skew, malformed/legacy record | inspect-only, unverifiable |

Linux boot ID is `/proc/sys/kernel/random/boot_id` and the process-start token is `/proc/<pid>/stat` field 22 interpreted with the kernel clock tick rate. macOS boot identity is the kernel boot time tuple and process-start token is PID plus `proc_pidinfo` start time. Failure to retrieve these values never proves death.

The v1 string encodings are `linux:<boot-uuid>` and
`linux:<decimal-field-22>` on Linux, and
`macos:<seconds>:<microseconds>` for both boot and process-start tuples on
macOS. `heartbeat_monotonic_ticks` is nanoseconds from the platform monotonic
clock on the recorded boot. Different-boot decisions use expired UTC evidence;
same-boot pruning requires compatible monotonic evidence plus a definitive PID
absence or start-token mismatch. Missing, future, overflowing, or disagreeing
clock evidence is unverifiable rather than authority to prune.

#### Scenario: A stale PID has been reused
Given heartbeat is expired and the live PID start token differs from the record
When pruning runs
Then only the stale record is removed
And no signal is sent to the live process

### Requirement: Offline package verification uses fixed v1 evidence

Slice zero supports gzip-compressed POSIX tar archives named `omegon-<version>-<target>.tar.gz` for the release targets enumerated by checked-in fixture `../../fixtures/maintenance-release-policy-v1.json`. The only executable root members are `omegon` and `omegon-maintain`; regular non-executable root metadata members must be allowlisted by policy. PAX path overrides, sparse files, hard/symbolic links, devices, FIFOs, absolute paths, `..`, backslash separators, duplicate raw paths, and Unicode/case-fold collisions are rejected.

`PackageManifestV1` is canonical JSON containing schema version, repository, workflow identity, issuer, ref/tag, commit, version, target, archive filename/digest, and exact member path/mode/size/digest records. The supplied Sigstore bundle v0.3 signs the exact canonical manifest bytes and contains the certificate chain, Rekor inclusion proof, and signed checkpoint. Verification uses Fulcio/Rekor roots embedded by the Sigstore verifier dependency plus exact compiled identity policy. Certificate validity is evaluated at Rekor integrated time; offline verification does not invent current-time freshness. V1 policy/root rotation requires installing a newer release-coupled verifier through an independently authenticated package path; `release verify` accepts no policy override or trust-root operand. Archive bytes and both executable subjects must match the signed manifest; the archive is not trusted merely because a detached signature covers different bytes.

The checked-in policy and fixtures include every accepted target, exact issuer `https://token.actions.githubusercontent.com`, exact workflow repository `styrene-lab/omegon`, release workflow path, tag-ref pattern, trusted roots, valid bundle, wrong-manifest signature, wrong subject digest, wrong workflow/ref, duplicate/path-confusion archive, and companion mismatch.

#### Scenario: Bundle signs a different manifest
Given archive and manifest fields appear internally consistent
But the Sigstore bundle signature covers different bytes
When offline verification runs
Then verification fails before archive members are trusted

### Requirement: Every command has fixed scope and outcome semantics

Command-specific rules are:

| Command | Scope and result |
|---|---|
| `identity` | compiled artifact, protocol, limits, targets, and exclusions only |
| `doctor` | runs identity, state-root, compiled composition, canonical-root, session-root, ownership-schema, package-companion, and audit structural checks; partial checks degrade |
| `composition inspect` | compiled maintenance profile only, never live graph authority |
| `contribution list` | no scope/workspace lists user; `--scope user` lists user; `--workspace` without scope lists user plus project; `--scope project --workspace` lists project; `--scope user --workspace` and project without workspace are rejected; deterministic kind/scope/raw-byte order |
| `contribution inspect` | explicit scope required; project scope also requires workspace; selector resolves uniquely within that scope |
| contribution mutations | require explicit scope; project requires workspace; dry-run legal |
| `session list` | explicit workspace optional; when present filters by workspace key; never mutates |
| session inspect/quarantine | explicit workspace required and exact unique pair required |
| resource commands | explicit workspace required |
| `release inspect` | inspects only package manifest adjacent to the running executable and reports absent/mismatch as degraded; does not trust it as signed release evidence |
| `release verify` | requires archive, manifest, and bundle; dry-run is invalid |
| `audit inspect` | reads a bounded newest-first page selected by `--cursor`; reports sequence and structural status |
| `audit verify` | verifies up to the v1 per-segment record limit from a supplied/default checkpoint; an older continuation is accepted only after its segment anchor is independently derived through structurally valid successor segments from the current checkpoint |

`--dry-run` is rejected for commands other than contribution/session/resource mutations. `--scope` is legal only for contribution commands. Unknown options fail before root opening.

The v1 result object has required fields named by the maintenance spec. Diagnostics contain stable `code`, `severity`, `scope`, `message`, and optional bounded evidence. Errors contain stable `code`, `phase`, `retry_safe`, and bounded message. Mutations contain `domain_key`, `kind`, `state`, and `retry_safe`. Arrays are emitted in deterministic order and stop before the 4 MiB envelope limit; the object then sets `truncated: true` and `next_cursor` where pagination is supported. A mutation result may never truncate its own mutation or error entries. Stable v1 error families are `cli_*`, `root_*`, `path_*`, `limit_*`, `lock_*`, `record_*`, `deny_*`, `session_*`, `resource_*`, `release_*`, `transaction_*`, `audit_*`, `deadline_*`, and `output_*`. Initial frozen codes include `root_unsafe`, `record_invalid`, `transaction_unknown`, `deadline_after_dispatch`, and `output_failed`; additive codes remain within a declared family. Valid record/result fixtures, key/digest vectors, and the checked-in corruption recipe inventory are consumed unchanged by later executables.

#### Scenario: A list exceeds the output envelope
Given more deterministic entries exist than fit in 4 MiB
When a list command runs with JSON output
Then it emits the largest complete bounded prefix, `truncated: true`, and a stable cursor
And exits degraded rather than emitting invalid or oversized JSON

# Binary composition inventory and kernel admission criteria

Measured on branch `refactor/minimal-default-binary` on 2026-08-14.

## Current composition

| Measure | Default interactive | Headless (`--no-default-features`) |
|---|---:|---:|
| Unique normal dependency tree lines | 778 | 620 |
| Local debug artifact | 236 MiB | not measured separately |
| Local release artifact | 39 MiB | not measured separately |

The TUI feature accounts for 158 additional unique dependency-tree lines. This is a useful compatibility boundary, but it is not yet a product boundary: the headless artifact still admits the provider stack, control plane, lifecycle engines, web server, plugin/skill management, archive/signature support, and other optional operational domains.

Compile-time content embedded directly in the main binary is approximately 332 KiB of source material:

| Content family | Source size |
|---|---:|
| `skills/` | 104 KiB |
| `catalog/` | 88 KiB |
| `data/` | 76 KiB |
| `pkl/` | 48 KiB |
| `prompts/` | 16 KiB |

This content is small relative to native code, but embedding it is architecturally significant: it makes contribution packs part of the kernel release cadence and prevents replacement without rebuilding the binary.

## Kernel admission criteria

Code or content belongs in the default binary only when all of the following hold:

1. **Universal execution:** every supported product mode needs it to start, admit work, enforce safety, or execute the provider-neutral agent loop.
2. **Kernel-owned contract:** removing it would break a stable runtime protocol rather than remove an optional workflow, renderer, integration, or content pack.
3. **Failure isolation:** its initialization cannot require optional credentials, external daemons, platform services, or mutable contribution state.
4. **Replacement neutrality:** downstream packs can extend behavior through a renderer-neutral/provider-neutral contract without linking their implementation into the kernel.
5. **Measured justification:** binary/dependency cost is recorded and accepted when a smaller interface cannot provide the capability.

A component failing any criterion defaults to an external contribution pack, optional feature artifact, or separate companion binary.

## Classification

### Kernel

- provider-neutral agent loop and work admission
- command/projection contracts shared by TUI, ACP, daemon, and IPC
- capability/RBAC enforcement and secret redaction boundaries
- configuration loading and stable protocol schemas
- minimal filesystem/process/network tools required by the core coding-agent contract
- extension discovery and contribution-pack contract (not bundled contributions)

### Optional artifact or contribution pack

- terminal renderer and image/syntax presentation stack
- embedded dashboard assets and operational dashboards
- local embeddings/ONNX runtime
- managed-agent integrations and platform-specific transports
- OCI signing/archive workflows
- MQTT/voice/chat integrations
- lifecycle methodologies, language conventions, personas, prompts, and catalog agents
- demo projects and onboarding content

## First extraction slice

Extract bundled skills from the binary into an installed **contribution pack directory** while preserving the existing `~/.omegon/skills` discovery contract.

Why this slice first:

- the interface already exists and is filesystem-based;
- `just link` already installs bundled skills/catalog content;
- no runtime protocol redesign is required;
- removing `include_str!` makes content independently replaceable and versionable;
- failure is bounded: missing packs degrade inventory/install commands, not the agent kernel.

The default binary will discover shipped skills from a compile-time path supplied by packaging, then user and project roots. It will not embed skill markdown. Packaging remains responsible for installing the shipped pack.

## External-agent interoperability

Existing skills owned by other coding agents are not implicit Omegon search roots. Directly reading mutable Claude, Codex, Cursor, or similar directories would couple runtime behavior to foreign precedence, trust, metadata, lifecycle, and path-resolution semantics.

The supported boundary is explicit import through a format adapter:

```text
external-agent skill → discover → adapt/validate/preview → Omegon user or project skill
```

### Ownership layers and precedence

1. **Shipped contribution pack** — immutable vendor baseline under the packaged contribution root; upgrades may replace it.
2. **Operator-owned Omegon skills** — `~/.omegon/skills`; never overwritten by contribution-pack installation.
3. **Project-owned Omegon skills** — `<project>/.omegon/skills`; highest local precedence.
4. **External-agent sources** — imported explicitly; never silently admitted as runtime instruction roots.

Resolved precedence remains project → user → extension → shipped. An operator copy may shadow a shipped skill without modifying vendor content.

### Import modes

- **Copy** is the default: import the complete reviewed bundle into an Omegon-owned root, including validated relative assets and scripts. External changes cannot silently alter Omegon behavior.
- **Link** is an explicit opt-in: retain an externally managed canonical source, mark it as such in inventory, validate containment, and surface broken/unavailable state. Omegon does not modify the source.

Both modes require collision handling before mutation. Imports never silently overwrite an operator-owned skill. A shipped name may be shadowed only after the resulting precedence is previewed.

### Adaptation and provenance

Adapters map only fields with verified semantic equivalence. Unknown provider metadata, tool restrictions, hooks, model selectors, and script semantics are preserved as provenance or reported as unsupported; they are not reinterpreted silently.

Imported skills retain source kind, provider, canonical source path, import mode, source digest, and adapter version in an Omegon-owned provenance record. This supports deterministic status, diff, and refresh operations. Refresh must show a diff and require an explicit conflict decision when the imported copy has local edits.

### Reverse interoperability

Export uses provider-specific adapters into a staging destination. Omegon does not write automatically into another agent's managed directories. The transformed bundle is validated and reviewed before the operator installs it for that agent.

### Security and failure semantics

- canonical-path containment applies to linked roots and referenced assets;
- traversal-bearing names and escaping symlinks are rejected;
- executable hooks/scripts require explicit safety review rather than metadata translation;
- missing external sources degrade the imported entry to unavailable and do not prevent native skill discovery;
- uninstalling another agent cannot remove an Omegon-owned copied import;
- contribution-pack upgrades never mutate user/project imports or provenance.

### Deferred implementation contract

Before external-agent import is implemented, specify a provider-neutral `SkillImportSource`, adapter diagnostics, provenance schema, collision preview, copy/link settlement, and refresh state machine. The current contribution-pack extraction must preserve these extension points but must not add foreign directory scanning as a shortcut.

## Success criteria

- no `include_str!(...skills/*/SKILL.md)` remains in non-test Omegon code;
- `omegon skills list/install` reads a deterministic shipped-pack manifest/directory;
- missing shipped content returns an actionable error and does not panic;
- project/user skill discovery and override precedence remain unchanged;
- `just link` and package workflows install the contribution pack;
- default and headless compile matrices remain green.

---

## Unified runtime capability-admission contract

The first extraction slice proved that optional content can leave the binary without changing the operator contract. The next boundary is broader: binary features, runtime features, model tools, skills, operator commands, and surface projections currently decide availability independently. Further extraction must not add another allowlist. Every surface must consume one canonical admission snapshot.

### Design status

**Decided.** This section is the target contract for subsequent implementation slices. It does not claim that the current runtime already conforms.

The first Slice-2 contract foundation is implemented in `omegon_traits::runtime_contributions`. Its version-1 renderer-neutral vocabulary distinguishes composition generations, contribution generations, and process identity; keeps owner tier, requested trust, and requested confinement separate; binds canonical invocations and aliases to one capability; and declares dependencies, conflicts, replacements, groups, platform requirements, effects, lifecycle, execution, transition, cleanup, and surface support before activation. Requested trust or confinement is not an admission grant, and owner-enforced deduplication is distinct from idempotency and ordinary call-ID propagation.

The contracts include validated scoped identities, fail-closed protocol/schema decoding, typed generation/lifecycle states, and diagnostics with explicit stable ordering. Representative declaration, generation, and diagnostic JSON fixtures freeze the v1 wire shape. This foundation does not yet make the graph authoritative: legacy capability inventory and dispatch remain unchanged until graph validation, activation, readiness, and compatibility-adapter gates land in later Slice-2 tasks. There is therefore no public command, configuration, or site behavior change in this contract-only lane.

The next Slice-2 layer is a pure deterministic candidate-graph builder. It receives frozen declarations, explicit host protocol/platform facts, and requested or observed effect evidence; it does not read process environment, activate code, or publish registrations. Validation accumulates stable all-error diagnostics for duplicate contribution/generation/group/capability ownership, ambiguous or dangling bindings, replacement and dependency cycles, missing requirements, conflicts, protocol/platform mismatch, dangling groups, and undeclared effects. Explicit acyclic replacement chains can resolve superseded owners structurally, but replacement authorization remains a later trust-admission decision. A valid result contains immutable owner/binding/group indexes, negotiated protocols, dependency edges, and prerequisite-first activation waves. Any error returns no graph, never a valid subset, and legacy composition/dispatch remains authoritative until task 2.3 migrates that boundary.

Static setup now stages feature implementations outside the published EventBus surface, freezes and adapts their tool, command, alias, internal-binding, safety, surface, provenance, and conservative effect metadata, validates one candidate graph, checks activation-plan membership and implementation parity as the synchronous static readiness gate, and only then commits graph-derived legacy caches. Tool, command, and internal binding collisions fail closed without registration-order fallback. Rejected additions or replacements are dropped while the previous accepted feature set and dispatch caches remain active; setup and live ACP/model-budget rebuilds use the fallible boundary. Event and context delivery include only published features, and plugin/extension admission guards remain held through publication. This static compatibility adapter does not claim dynamic trust preflight, quarantine, resource rollback, readiness deadlines, or lifecycle-generation promotion, which remain tasks 2.4 and 2.5.

Dynamic preflight now has a separate version-1 renderer-neutral contract. It binds stable contribution identity, immutable source digest, source kind, protocol range, minimum dependencies, requested trust/confinement, probe operations, timeout, and conservative effects before code evaluation, spawn, or connection. Host-produced trust admission is separately source-bound: trusted-code evidence identifies kernel-release or explicit operator-policy authority, while verified-confinement evidence fails validation unless an OS/OCI boundary prevents direct filesystem, process, network, and secret access and forces privileged effects through brokers. No current execution substrate is implicitly certified by this vocabulary, and manifest requests, installation, enablement, maintenance admission, or trusted-directory state cannot mint admission evidence.

Native and OCI extension startup now enforces that contract. `permissions.trustedContributionCode` is a distinct operator-policy list of stable IDs such as `extension:example`; selected or installed extensions absent from it are denied before secret preflight, secret resolution, spawn, or protocol probing. A permit binds the accepted ID and complete snapshotted source-tree digest and is revalidated at the low-level process boundary on initial launch and transport-error respawn. The current OCI launcher is not treated as verified confinement, so it also requires explicit trusted-code admission. Test-only unsnapshotted launch helpers are not compiled into production.

Executable plugin paths now use the same policy and permit. A guarded plugin directory is identified as `plugin:<directory-name>` and denied before Pkl evaluation, dynamic context generation, script/OCI execution, HTTP registration, or plugin-declared MCP connection. Production Armory, HTTP, and MCP constructors require a permit and revalidate it at deferred execution/send boundaries. Project MCP uses the separately frozen `mcp:project` identity, while ACP-submitted server configuration uses `mcp:acp-client` and is admitted authoritatively in the worker before secret-template resolution, process spawn, or network connection. Red tests use marker processes to prove untrusted plugin context and project MCP configuration cannot execute during discovery.

Slice 2.5 lifecycle records add owner and composition-generation identity, last completed lifecycle boundary, bounded coded reasons, restart/backoff and heartbeat evidence, cleanup assurance, and cleanup outcome. Separate resource records cover process trees, tasks, sockets, subscriptions, temporary directories, durable writers, and remote services. Validation rejects unbounded reasons, strict cleanup paired with unverified outcomes, and false host-ownership claims for remote services. These renderer-neutral records describe evidence produced by the lifecycle owner and concrete transport adapters.

The transport-neutral lifecycle owner now models one quarantined candidate with one absolute readiness deadline, per-contribution lifecycle records, and generation-bound resource cleanup callbacks. Promotion requires every contribution to reach readiness and every strict-cleanup owner to have strict resource assurance; the non-awaiting publication callback must succeed before active-generation state changes. Rejection and publication failure run bounded cleanup in reverse activation order and preserve the prior active generation. Successful replacement promotes the new generation before retiring the old resource set, recording unverified cleanup rather than claiming settlement when a deadline expires. Current setup and ACP adapters apply these invariants directly to extension and MCP resources while EventBus atomically preserves or replaces the accepted graph and legacy dispatch caches.

Extension negotiation now applies one absolute manifest readiness deadline across optional initialization, required tool discovery, configuration delivery, and secret delivery. Timeout and handshake failure retain the canonical child handle long enough to shut down, kill the dedicated process group, and reap it before reporting failure. Setup also explicitly shuts down every successfully started extension supervisor when graph publication or later runtime-ownership startup fails, preserving admission locks through extension cleanup and reporting degraded cleanup instead of relying solely on synchronous drop backstops.

MCP connection and discovery now use one absolute per-server readiness deadline across transport startup, required tool discovery, and optional resource, template, and prompt discovery. A shared MCP supervisor owns every accepted `RunningService`, performs bounded explicit close, remains held through graph publication, and is retained through daemon or interactive runtime shutdown. Startup and ACP graph rejection close candidate services before reporting failure; timeout or join failure is surfaced as degraded cleanup rather than hidden behind rmcp's asynchronous drop guard.

Extension transport recovery now uses a generation-local restart controller. Failures consume a fixed restart budget, apply deterministic capped exponential backoff, and transition to terminal quarantine when the budget is exhausted; later invocations cannot silently start another process. Constructing a changed extension generation creates a fresh controller, while ordinary successful respawn does not erase prior crash evidence.

Armory context, script-tool, and OCI-tool processes now remain under explicit child ownership while output is drained. They run as dedicated process groups with kill-on-drop backstops; timeout and cancellation kill the complete group, wait for the child, and settle output tasks before returning. Script paths must be normal relative paths inside the admitted snapshot, preventing absolute-path and parent-traversal execution.

Dynamic feature adapters now freeze their negotiated lifecycle and transition policy with the candidate declaration instead of inheriting zero-valued static defaults. Extensions publish their manifest readiness deadline, bounded restart budget, quarantine disposition, and platform-honest cleanup assurance; MCP, Armory, and HTTP plugins publish bounded readiness and best-effort cleanup policy appropriate to their transport boundaries. EventBus validates these values as part of the candidate graph and changes the accepted graph, feature set, and compatibility dispatch caches only after the complete candidate succeeds. Slice 2.5 does not issue generation-bound invocation leases or drain active calls; those enforcement semantics remain Slice 3 work.

Each successful EventBus publication now mints a distinct `composition:<uuid>` generation only after the candidate graph and graph-derived compatibility caches pass validation and parity. Failed candidates preserve the prior graph, dispatch caches, and composition identity. New interactive, daemon, bounded, and ACP session-authority lineages persist that composition identity instead of the process instance ID. Existing session generation strings remain valid opaque legacy IDs: resume retains the session's original value for later turns and does not rewrite it to the current process or composition generation. Slice 2 still performs no live session migration.

One renderer-neutral composition diagnostic projection now supplies native and ACP `/status`. It carries the accepted generation, effective declarations, negotiated protocols, activation waves, replacement edges, owner and contribution-generation provenance, active health, cleanup assurance and state, coded diagnostics from the latest accepted or rejected candidate, and explicit `graph_derived_legacy` compatibility dispatch with parity status. Renderers consume this projection rather than reconstructing graph policy; the projection itself does not issue invocation authority.

Model-tool dispatch now crosses one kernel-owned invocation service before host delegation or local owner execution. The service resolves the invocation against the accepted graph, combines the declaration-derived RBAC ceiling with layered permission policy and operator approval, and issues a call-, principal-, scope-, capability-, owner-, effect-, transition-, and composition-generation-bound lease. EventBus revalidates the accepted generation and owner immediately before execution; stale, mismatched, closed, reused, unknown, or incompletely declared calls receive no executable authority. Lease claim and terminal closure are exactly once, and rejected candidate publications leave leases for the retained generation valid.

Authority-backed model-tool calls now persist `invocation.prepared` after admission and approval but before returning a lease, then persist `invocation.dispatched` after lease claim and generation revalidation but before host or local owner entry. Preparation captures the complete lease policy plus stable invocation, lease, visible call, optional owner-enforced deduplication, session, and turn identities. Interactive, ACP, daemon, and bounded turns share the single session-authority writer; explicit no-session compatibility calls remain ephemeral. Preparation failure issues no lease, dispatch-write failure revokes before handoff, and JSONL authority remains committed if its replaceable snapshot cache cannot refresh.

Owner execution now receives one cloneable acknowledgement control. Local owners acknowledge on entry; host delegation, extension RPC, and MCP acknowledge at their selected transport boundary. Completed, failed, cancelled, timed-out, and revoked results persist terminal settlement before `ToolEnd`, result return, or lease closure. External transport loss after acknowledgement is durably classified as unknown completion and is not automatically replayed; restart recovery applies the same classification to every unsettled dispatched or acknowledged invocation, including calls from an already-closed legacy turn, while preserving prepared calls as not handed off.

Mutating execution declarations now require a validated durable mutation domain and fence key. If acknowledgement, unknown classification, or terminal settlement persistence fails after dispatch, the lease writes append-only emergency evidence through a writer independent of the failed authority JSONL, withholds ordinary completion, and denies later matching mutations before preparation. Malformed evidence fails closed, and an emergency-writer failure poisons mutation admission for the running authority. Runtime execution exposes no fence-removal shortcut; deterministic reconciliation or an explicit audited operator recovery path must own clearing.

Retry denial for mutating unknown-completion calls and migration of direct operator, extension, host-effect, and privileged internal compatibility paths remain later Slice-3 work. Reactive path grants and extension HostAction approval can only narrow the upstream lease decision.

Capability execution declarations now include typed eligible principal classes, timeout class, retry/idempotency/deduplication policy, serial or parallel-safe scheduling, and explicit transaction behavior alongside required effects and transition policy. Candidate validation rejects empty principal sets, zero attempts, unsafe non-idempotent retry, parallel rollback, and mutation/effect contradictions. Leased model-tool admission derives RBAC from declared effects rather than the visible tool name, captures the complete policy, and revalidates it before owner execution. The scheduler uses declared parallelism and best-effort rollback eligibility; caller timeout arguments may narrow but not widen the declaration-class ceiling, including for host-delegated calls.

Static features and legacy tool providers can publish precise policy through the runtime tool-policy hook. Missing hooks receive a conservative serial, non-retrying host adapter, and external tools retain a full host-effect envelope. The behavioral tool catalog remains available for guidance and loop heuristics but no longer grants model-tool scheduling or rollback authority. Retry metadata is persisted for later classification but does not yet authorize replay, and direct compatibility execution retains its legacy timeout behavior until the broader privileged-path migration.

### Design laws

1. **Composition is not admission.** Compiled or installed capability means resident, not callable or visible.
2. **Visibility is not authority.** A capability may be discoverable without being invocable; a hidden capability must not become executable merely because a caller knows its identifier.
3. **One decision, many projections.** TUI, CLI, ACP, IPC, WebSocket, prompt construction, and audit surfaces derive from the same versioned snapshot.
4. **Operator authority dominates inference.** Explicit operator enablement may admit an eligible capability; model inference may recommend or request admission but cannot bypass policy, RBAC, safety, or unavailable dependencies.
5. **Safety can only narrow.** Posture, workspace evidence, model limits, surface support, RBAC, and runtime health intersect. No downstream adapter may widen an upstream denial.
6. **Execution rechecks admission.** Schema projection is advisory evidence, not a security boundary. Dispatch validates the current snapshot and caller before side effects.
7. **Canonical evidence is retained.** Withholding a capability body, schema, command, or renderer does not discard its inventory, provenance, denial reason, or prior audit records.
8. **Removal is observable and generation-scoped.** Admission changes publish a new generation. Work admitted under an older generation either completes under an explicit lease or is revoked according to the capability's transition policy; it never silently inherits new authority.

### Capability identity and ownership

Every runtime contribution declares one or more capabilities under stable, namespaced identifiers:

```rust
pub struct CapabilityId(String); // e.g. "tool:read", "command:context.compact"

pub enum CapabilityKind {
    KernelService,
    Tool,
    OperatorAction,
    Skill,
    ContextProvider,
    Projection,
    TransportAdapter,
    Workflow,
}

pub struct CapabilityDeclaration {
    pub id: CapabilityId,
    pub kind: CapabilityKind,
    pub owner: ContributionRef,
    pub version: u32,
    pub dependencies: Vec<CapabilityRequirement>,
    pub conflicts: Vec<CapabilityId>,
    pub supported_surfaces: SurfaceSet,
    pub audience: AudienceSet,
    pub safety: CapabilitySafety,
    pub activation: ActivationPolicy,
    pub transition: TransitionPolicy,
}
```

`ContributionRef` identifies the kernel, a compiled feature, an installed contribution pack, an extension, or an operator/project-owned contribution. Tool names and slash aliases remain transport vocabulary; they resolve to capability IDs rather than serving as authority keys.

A declaration is metadata, not executable authority. Duplicate capability IDs are a startup/configuration error unless an explicit replacement contract names the superseded declaration. The current `EventBus::finalize` first-registration-wins behavior must not remain the policy boundary.

### Admission state machine

A capability has one canonical state per runtime generation:

```text
Absent
  ↓ composition/install
Resident
  ↓ dependency + compatibility resolution
Eligible
  ↓ policy/evidence/operator decision
Admitted
  ↓ caller + surface + safety + health check
Callable
```

The states mean:

| State | Meaning | May appear in inventory? | May expose body/schema? | May execute? |
|---|---|---:|---:|---:|
| `Absent` | Not compiled, installed, or discovered | no | no | no |
| `Resident` | Declaration and implementation/content are present | yes | metadata only | no |
| `Eligible` | Dependencies, platform, configuration, and surface compatibility pass | yes | discovery metadata | no |
| `Admitted` | Active policy permits use in this runtime scope | yes | audience-appropriate projection | not necessarily |
| `Callable` | Admitted and authorized for this caller/surface at this moment | yes | yes | yes |
| `Degraded` | Resident/admitted but temporarily unhealthy | yes | bounded status | only if degradation policy permits |
| `Revoked` | Previously admitted, explicitly withdrawn for this generation | yes | denial/status | no new execution |

`Degraded` and `Revoked` are outcome states attached to the last nonterminal admission state; they are not aliases for disabled-name sets.

### Admission inputs and precedence

The admission engine evaluates typed inputs in this order:

1. **Kernel invariant** — required safety and control-plane capabilities cannot be disabled by model or posture policy.
2. **Composition evidence** — compiled feature, installed contribution, adapter availability, platform compatibility.
3. **Dependency health** — required binaries/services/configuration and declared capability dependencies.
4. **Workspace policy** — project rules, workspace admission, sandbox, trust grants, and repository-local configuration.
5. **Operator profile** — explicit enabled/disabled capabilities and named posture defaults.
6. **Task evidence** — declared project signals, prompt triggers, focused lifecycle state, or other bounded evidence matchers.
7. **Model constraints** — context/tool-schema budget and provider protocol support; these may reduce projection but never grant authority.
8. **Caller authorization** — principal, RBAC capability, transport, safety class, confirmation, and current runtime health.

Explicit deny wins over inferred allow at the same or lower layer. Kernel safety requirements and RBAC denial cannot be overridden. An explicit operator enable can override posture or evidence defaults only when the capability is otherwise eligible. The model may call a request-admission action such as `manage_tools`, but that action is itself policy-bound and records the requester and reason.

### Canonical snapshot

The admission engine publishes an immutable snapshot:

```rust
pub struct CapabilityAdmissionSnapshot {
    pub generation: u64,
    pub runtime_scope: RuntimeScopeId,
    pub entries: BTreeMap<CapabilityId, CapabilityAdmission>,
    pub created_at: SystemTime,
}

pub struct CapabilityAdmission {
    pub declaration: CapabilityDeclarationSummary,
    pub state: CapabilityState,
    pub reasons: Vec<AdmissionReason>,
    pub evidence: Vec<AdmissionEvidenceRef>,
    pub allowed_callers: CallerPolicy,
    pub allowed_surfaces: SurfaceSet,
    pub lease_policy: TransitionPolicy,
}
```

Reasons are structured and redacted: `not_installed`, `dependency_missing`, `profile_disabled`, `workspace_denied`, `signal_absent`, `rbac_denied`, `surface_unsupported`, `budget_withheld`, `temporarily_unhealthy`, or `conflict`. Operator-facing projections may summarize them; diagnostics and audit retain the typed form.

Updates are computed off to the side, validated, and atomically activated. Invalid declarations or dependency cycles retain the last-known-good snapshot and publish diagnostics. Consumers receive a generation change event and never reconstruct admission from local settings.

### Audience-specific projection

Admission has separate audiences; there is no universal "visible" Boolean.

| Audience | Projection rule |
|---|---|
| Model tool schema | Only callable tool capabilities for the model principal, current turn, and provider budget |
| Model prompt/context | Only admitted context/skill capabilities; withheld bodies are not loaded into prompt assembly |
| Local operator inventory | All resident entries plus state, provenance, and bounded denial reasons |
| Remote operator surface | Entries callable or inspectable under that principal's RBAC and transport support |
| Renderer/menu | Admitted operator actions supported by that renderer; unavailable entries may appear only in an explicit inventory/status view |
| Audit/diagnostics | All decisions and generation transitions, subject to redaction |

Schema compaction and lazy projection are presentation optimizations over the callable set. They must not create a second admission state. In particular, turn-one "show everything" behavior is removed as an authority concept: a schema can only be shown if the capability is callable, and execution still rechecks the snapshot.

### Tools

The current `EventBus` registration cache becomes a declaration/implementation registry, not the source of policy. `DisabledTools`, `TOOL_GROUPS`, `is_core_tool`, `is_dynamic_tool`, and model-hidden-name checks migrate into declarations and policy inputs.

Required behavior:

- `tool_definitions*` projects the snapshot for the model principal and provider budget;
- `execute_tool*` rejects absent, non-callable, stale-generation, or unauthorized calls before locating the implementation;
- internal execution uses a distinct kernel/service principal and explicit internal audience, not a fallback that can execute any disabled registered tool;
- tool groups are data-owned collections of capability IDs validated against the registry;
- `manage_tools` requests profile/session admission changes and reports resulting snapshot state; it does not mutate a shared disabled-name set directly;
- duplicate tool vocabulary cannot silently select the first owner.

### Skills and prompt contributions

`SkillDisclosure` becomes an adapter into the same admission engine:

- installed skill metadata maps to `Resident`;
- resolvability and format validation map to `Eligible`;
- activation metadata, project signals, prompt triggers, explicit subsets, and conflict policy determine `Admitted`;
- prompt assembly receives bodies only for admitted skill capabilities;
- explicit parent/child skill subsets are operator/orchestrator evidence, not an untyped bypass;
- provenance and suppression reasons remain visible in inventory and activation events.

Personas, tones, prompt packs, lifecycle methodologies, and memory/context providers use the same declaration and admission vocabulary rather than bespoke loaded/active flags.

### Operator actions and commands

The canonical control-action registry is the declaration source for operator actions. Slash commands, CLI subcommands, IPC methods, WebSocket messages, and TUI menus are bindings to action capability IDs.

Required behavior:

- transport availability is declaration metadata, then narrowed by admission and RBAC;
- generic `run_slash_command`/`slash_command` tunnels resolve to a canonical action and pass the same authorization path, or refuse actions without a remote-safe binding;
- command menus project admitted actions instead of renderer-local hidden-name lists;
- command execution owner, safety class, confirmation, and prompt-injection sensitivity participate in the admission/callability decision;
- an adapter cannot advertise an action that the snapshot denies or the transport cannot execute.

### Projections and capability advertisement

`_runtime/capabilities`, ACP initialization metadata, IPC/WS discovery, TUI menus, web controls, help, and doctor output project the same snapshot with audience-specific filtering. They share stable capability IDs and generation numbers.

A transport may expose fewer capabilities than the runtime, never more. Projection tests must assert that advertised actions route successfully or carry an explicit unavailable status in an inventory-only view. Surface-specific allowlists are forbidden except as generated transport bindings validated against declarations.

### Binary and process composition

Cargo features and separate artifacts remain coarse composition boundaries:

- `omegon-kernel`/headless composition owns the loop, admission engine, safety boundaries, minimal tools, and protocol contracts;
- TUI, web/dashboard, lifecycle, embeddings, managed integrations, signing/archive, and contribution content are optional compiled features, companion artifacts, or installed packs;
- compiling a feature makes its declarations resident but does not admit them;
- installed packs are discovered without executing optional code during kernel startup;
- missing optional contributions cannot prevent kernel startup unless an explicit profile marks them required.

A future crate split is subordinate to this contract. Moving code between crates without changing declarations, dependencies, and admission does not count as surface reduction.

### Runtime transitions and concurrency

Admission changes are transactional and generation-scoped:

1. validate requested policy/configuration change;
2. resolve declarations, dependencies, conflicts, and evidence;
3. build and validate a candidate snapshot;
4. atomically publish the new generation;
5. notify projections and invalidate schema/context caches;
6. revoke or grandfather active leases according to declaration policy;
7. append an audit event with actor, reason, old generation, and new generation.

Default transition policies:

- read-only/context projection capabilities switch at the next projection boundary;
- new tool/action calls require the current generation;
- running non-destructive calls may complete under a captured lease;
- permission, secret, destructive, or externally side-effecting capabilities revoke immediately when authority narrows;
- kernel interrupt, cancellation, audit, and redaction capabilities are non-disableable.

### Assumptions resolved

- **Dynamic admission is required.** Profiles, `manage_tools`, skill evidence, extensions, and runtime health already change during a session; startup-only composition cannot be canonical.
- **Inventory must include denied capabilities.** Operators need provenance and actionable reasons; model and remote projections remain filtered.
- **Model self-service is request authority, not grant authority.** A model can request activation only through an admitted action whose policy permits it.
- **Commands and tools share admission semantics but remain distinct kinds.** They have different audiences, safety metadata, and execution adapters; forcing them into one invocation ABI would erase useful constraints.
- **Presentation level is orthogonal.** Om/Active/Full changes density, not capability authority.
- **Feature absence is not a runtime denial.** `Absent` is composition truth; `Resident` plus a denial reason is policy truth.
- **Compatibility aliases do not create capabilities.** Aliases resolve to one canonical ID and inherit its decision.

## Migration plan

### Slice 1 — declarations and read-only inventory

- Add capability IDs, kinds, declaration metadata, admission states/reasons, and immutable snapshot types to `omegon-traits` or a narrow shared crate.
- Adapt existing tool and command registries into declarations without changing behavior.
- Project a diagnostic inventory alongside current `ToolInventorySnapshot`.
- Detect duplicate IDs/vocabulary, dangling groups, dependency cycles, and surface advertisements without owners.

**Exit gate:** current runtime behavior is unchanged, but every registered tool and built-in command has one validated capability declaration and provenance owner.

### Slice 2 — model-tool authority

- Replace `DisabledTools` mutation with profile/session policy inputs.
- Make EventBus schema projection and execution consume the snapshot.
- Convert tool groups, core/dynamic classification, hidden/internal tools, slim mode, posture overrides, lazy injection, and constrained-model budgets.
- Add captured-generation execution leases and denial results.

**Exit gate:** no model tool can execute unless the canonical snapshot marks it callable; schema inventory equals callable inventory for that audience.

### Slice 3 — skills and context contributions

- Adapt progressive skill disclosure to declarations and admission evidence.
- Move persona, tone, prompt pack, lifecycle context, and memory/context-provider admission onto the same snapshot.
- Ensure withheld content is never loaded into model prompt buffers.

**Exit gate:** prompt assembly can enumerate every injected contribution by capability ID, generation, owner, and admission reason.

### Slice 4 — canonical operator actions and surfaces

- Bind command registry rows, CLI, TUI, ACP, IPC, and WebSocket actions to capability IDs.
- Remove renderer-local hidden command lists and narrow generic slash tunnels.
- Generate capability advertisement/help/menu projections from the snapshot and transport bindings.

**Exit gate:** parity tests prove no surface advertises an unrouteable or unauthorized action and no adapter widens admission.

### Slice 5 — composition extraction

- Define the minimal kernel Cargo/artifact profile from declarations proven universal.
- Move lifecycle methods, rich presentation, web/dashboard, embeddings, managed integrations, signing/archive, remaining prompt/catalog/persona content, and optional provider transports behind features, companion artifacts, or packs.
- Add startup degradation behavior for absent optional packs.

**Exit gate:** minimal headless/kernel and default interactive matrices compile and pass contract tests; optional domains can be absent without kernel startup failure.

### Slice 6 — budgets and deletion

- Remove legacy disabled-name sets, hard-coded tool groups, first-registration-wins arbitration, per-surface capability allowlists, and duplicate loaded/active flags.
- Establish dependency, artifact-size, schema-token, default-callable-count, and startup-task budgets.
- Report budget deltas in CI and require explicit approval for regressions.

**Exit gate:** one admission engine and snapshot remain as the authority; legacy compatibility adapters are read-only or removed.

## Enforcement gates

The following tests become mandatory as slices land:

1. **Registry integrity:** unique IDs and invocation vocabulary, valid owners, dependencies, groups, aliases, and transport bindings.
2. **Monotonic narrowing:** randomized policy combinations prove downstream layers never widen an upstream denial.
3. **Projection/dispatch parity:** every advertised callable capability dispatches through the same snapshot; every denied capability is rejected even if invoked by name.
4. **Cross-surface parity:** TUI, CLI, ACP, IPC, WebSocket, and web projections are subsets of one generation and principal policy.
5. **Prompt provenance:** every injected skill/context body has an admitted capability ID and no withheld body reaches provider input.
6. **Generation safety:** stale calls, concurrent profile changes, immediate revocation, and grandfathered leases follow transition policy exactly once.
7. **Absence/degradation:** optional feature, pack, executable, credential, or service absence degrades locally and does not prevent kernel startup unless required.
8. **Security:** RBAC, confirmation, redaction, path boundaries, and secret handling are rechecked at execution; model requests cannot self-grant.
9. **Feature matrices:** minimal kernel/headless, default interactive, and selected optional-feature combinations compile and run contract tests.
10. **Budgets:** CI records binary size, dependency count, startup task count, default model schema count/tokens, and resident/admitted capability counts.

## Explicit non-goals

- A plugin ABI or dynamic-library loader in the first four slices.
- Treating fewer visible controls as proof of a smaller or safer kernel.
- Coupling UI presentation density to capability authority.
- Allowing task classifiers or LLM inference to grant privileges.
- Replacing RBAC, permission mediation, or tool-specific input validation with admission metadata.
- Forcing tools, commands, skills, and projections into one execution interface.
- Removing compatibility aliases before canonical bindings and migration telemetry exist.

## Final success criteria

The harness-surface reduction program is complete when:

- the default model, every operator transport, and prompt assembly consume one versioned capability-admission snapshot;
- compiled/installed, discoverable, admitted, visible, and callable are distinct and observable states;
- execution cannot bypass an admission denial through internal, generic slash, stale schema, or surface-specific paths;
- optional workflows and content can be absent or replaced without rebuilding or destabilizing the kernel;
- the minimal artifact contains only capabilities satisfying the kernel-admission criteria;
- default surface and binary budgets are measured and enforced against regression;
- each capability's owner, provenance, policy inputs, denial reasons, and generation are inspectable without exposing secrets.

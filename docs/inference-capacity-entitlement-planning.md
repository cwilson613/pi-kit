+++
title = "Inference Capacity and Entitlement Planning"
tags = ["inference","capacity","quota","entitlements","planning","providers"]
+++

+++
id = "321f0619-a0ba-4f74-8acb-8dc1487a26a1"
kind = "design_node"

[data]
title = "Inference Capacity and Entitlement Planning"
status = "exploring"
issue_type = "feature"
priority = 1
dependencies = []
open_questions = [
  "[assumption] The authenticated Codex status source used by native tooling is permitted and sufficiently stable for bounded read-only reuse; verify endpoint, schema, and terms before implementation.",
  "[assumption] Claude OAuth credentials expose an automation-safe account usage mechanism equivalent to Claude Code /usage; verify credential scope and endpoint stability before implementation.",
  "What explicit error-observation DTO should carry telemetry from failed inference without fabricating a successful completion event?",
  "What cache TTL and manual refresh semantics should apply per provider after live contract verification?"
]
+++

## Overview

# Inference Capacity and Entitlement Planning

# Inference Capacity and Entitlement Planning

## Overview

Omegon must plan work against the inference access the operator can actually sustain. Authentication and model availability are necessary but insufficient: commercial routes may be constrained by subscription windows, premium-request pools, API rate buckets, prepaid credits, billing budgets, organization policy, or an opaque fair-use off-switch. Local inference has no commercial off-switch but remains constrained by compute, memory, queue depth, and elapsed time.

The system must answer:

> Given a proposed work plan, which admitted routes have enough policy-allowed capacity to complete it, what evidence supports that conclusion, and what fallback or operator checkpoint is required if capacity is uncertain?

This design extends the existing inference inventory, admission, subscription-route, provider telemetry, and usage surfaces. It does not replace them.

## System boundaries

```text
conceptual model
  -> concrete inference route
  -> entitlement/account
  -> applicable resource pools
  -> observed balances/windows
  -> operator policy and reserves
  -> work estimate and reservation
  -> execution usage ledger
```

Existing responsibilities remain separate:

- **Inference inventory**: which routes and offerings exist.
- **Admission**: whether route evidence is sufficient for selection.
- **Authentication**: whether credentials exist and are usable.
- **Provider telemetry**: what upstream responses reveal now.
- **Capacity inventory**: what commercial or compute resources constrain a route.
- **Planning policy**: what Omegon may spend or reserve.
- **Usage ledger**: what a run actually consumed.

## Design principles

1. **Unknown is not unlimited.** Missing account telemetry is represented as opaque, not healthy.
2. **Preserve provider semantics.** A Copilot premium request, Anthropic five-hour utilization, OpenAI TPM bucket, and API-dollar budget are not interchangeable units.
3. **Route economics attach to concrete routes.** Claude through Copilot does not consume direct Anthropic quota.
4. **Entitlement identity is separate from provider identity.** Accounts, organizations, API keys, projects, and seats may have different pools.
5. **Observed capacity and operator policy are separate.** Provider headroom answers “can”; operator reserves and caps answer “should.”
6. **Evidence carries authority and freshness.** Every value records source, observation time, and confidence.
7. **Planning is probabilistic.** Work estimates are ranges that tighten using actual usage during execution.
8. **Local inference is sovereign, not infinite.** Represent compute and time constraints rather than fake monetary capacity.
9. **No hidden account probes.** Account/billing discovery is read-only, credential-scoped, bounded, and operator-visible.
10. **Degrade safely.** Opaque or stale capacity produces staged execution, fallback reservation, or an operator checkpoint.
11. **Capacity inspection never consumes inference.** `/usage`, `/limits`, startup status, and refresh paths must not synthesize a model turn to discover quota.
12. **Account capacity and response telemetry are distinct evidence classes.** Dedicated authenticated read-only capacity observations are primary for subscription windows; inference headers remain fallback operational evidence and must be labelled as such.

## Immediate `/usage` and `/limits` direction

The native Codex and Claude Code surfaces establish the minimum useful product contract:

- resolve and display the active provider and model before any completed turn;
- show authenticated account or plan identity when safely exposed;
- show aggregate subscription windows with used/remaining percentage and exact reset time;
- show model-specific windows independently from aggregate windows;
- show credits or overage state without inferring a monetary balance;
- keep local session tokens, cost, and duration separate from remote account capacity;
- attach an authoritative provider dashboard URL where one exists.

The two commands differ by **scope and interaction cost**, not by separate underlying data models:

- **`/limits` — magazine check.** A fast, compact readout of hard or provider-enforced buckets that can stop work: subscription pools, request/token ceilings, premium-request pools, credit exhaustion, concurrency caps, and their reset/refill times. It answers “how much hard capacity is left right now?” It excludes soft estimates, local session accounting, cost narratives, and historical analytics unless needed to explain a hard bucket.
- **`/usage` — full navigable usage surface.** A proper menu backed by the same capacity observations, including every `/limits` bucket plus local session tokens/cost/duration, provider and model breakdowns, historical consumption, evidence provenance/freshness, account/plan context, credits/overage state, and authoritative links. It supports navigation and drill-down rather than rendering as a larger static report.

There is no `/runway` command or alias. Planning guidance derived from hard buckets belongs in the compact `/limits` magazine check; broader consumption and evidence exploration belongs in `/usage`.

Both commands resolve the active route before the first completed turn. A stale or absent cache may trigger a credential-scoped, read-only capacity probe; it never triggers synthetic inference. `/limits` must remain fast: render cached hard buckets immediately when available and make freshness explicit rather than blocking on broad usage-history collection. A failed refresh retains the last safe observation, marks it stale, and reports the refresh failure without failing inference.

### Projection contract

Both surfaces consume one normalized observation store but apply different projections:

```rust
struct LimitsProjection {
    route: RouteIdentity,
    hard_buckets: Vec<HardBucketStatus>,
    observed_at: Timestamp,
    freshness: Freshness,
    recommendation: Option<String>,
}

struct UsageProjection {
    limits: LimitsProjection,
    session: SessionUsage,
    breakdowns: Vec<UsageBreakdown>,
    history: Vec<UsagePeriod>,
    entitlement: Option<EntitlementSummary>,
    evidence: Vec<CapacityEvidenceSummary>,
    links: Vec<AuthorityLink>,
}
```

A bucket qualifies for `/limits` only when exhaustion or a provider-enforced maximum can block or throttle work. Examples include subscription utilization windows, API request/token maxima, premium-request pools, hard credit balances, and concurrency slots. Estimated spend, average burn, token history, and advisory budgets remain `/usage` data unless an operator policy turns one into an enforced hard cap.

### Provider capacity probe contract

```rust
#[async_trait]
trait ProviderCapacityProbe: Send + Sync {
    fn provider(&self) -> ProviderId;
    fn credential_class(&self) -> CredentialClass;
    fn freshness_ttl(&self) -> Duration;
    async fn probe_capacity(&self, route: &RouteSnapshot) -> CapacityProbeResult;
}

struct CapacityObservation {
    provider: ProviderId,
    route: RouteId,
    principal: Option<RedactedPrincipal>,
    plan: Option<String>,
    observed_at: Timestamp,
    expires_at: Option<Timestamp>,
    source: EvidenceSource,
    authority: EvidenceAuthority,
    windows: Vec<CapacityWindow>,
    credits: Option<CreditState>,
    dashboard_url: Option<String>,
    raw_fields: BTreeMap<String, RedactedValue>,
}

struct CapacityWindow {
    id: String,
    label: String,
    scope: CapacityScope,
    used_percent: Option<Decimal>,
    remaining_percent: Option<Decimal>,
    reset_at: Option<Timestamp>,
    state: KnowledgeState,
}

enum CapacityScope {
    Account,
    Subscription,
    Model { model: String },
    Project { fingerprint: String },
    RequestRate,
}
```

Adapters declare their endpoint or local status mechanism, required credential class, polling safety, minimum refresh interval, TTL, redaction rules, and failure semantics. The cache key includes provider, redacted principal/account scope, and concrete route where capacity is model-specific.

### Source precedence

1. Authenticated read-only account/subscription capacity endpoint for the active principal.
2. Authenticated provider status mechanism with equivalent account-window semantics.
3. Fresh cached account observation.
4. Per-response quota/rate telemetry from actual operator inference.
5. Curated provider documentation.
6. Historical estimate.

Lower-authority evidence may fill a missing field but must not overwrite a fresher, higher-authority observation. Unknown remains unknown; a successful inference response does not imply ample subscription capacity.

### Initial adapter order

1. **OpenAI Codex OAuth** — reproduce account plan, aggregate weekly window, model-specific weekly windows, reset timestamps, credits state, and dashboard link from the authenticated status source used by Codex tooling.
2. **Claude OAuth** — reproduce current-session and weekly subscription windows, model-specific windows, reset timestamps, and usage-credit state where the credential permits it.
3. **GitHub Copilot** — retain as the first broader entitlement adapter because premium-request pools and meta-provider routes exercise the normalized model.
4. **API-key providers** — use documented account/project endpoints when authorized; otherwise expose response rate windows explicitly as operational fallback telemetry.

### Explicitly rejected approach

Do not warm `/usage` or `/limits` by sending a minimal inference request. That approach spends quota to inspect quota, adds latency and provider history, fails at exhaustion, and usually exposes request-rate headers rather than subscription entitlement. Likewise, a failed inference must never emit a fabricated successful `Done` event merely to transport telemetry; failure telemetry needs an explicit error-observation path or dedicated capacity result.

## Domain model

### Entitlement

Represents why an account or key can use an inference route.

```rust
struct Entitlement {
    id: EntitlementId,
    provider: ProviderId,
    principal: PrincipalRef,
    kind: EntitlementKind,
    plan: Option<String>,
    automation_policy: AutomationPolicy,
    evidence: EvidenceRef,
}

enum EntitlementKind {
    Subscription,
    ApiBilling,
    PrepaidCredit,
    OrganizationSeat,
    FreeTier,
    LocalCompute,
}
```

`PrincipalRef` may identify an account, organization, project, API key fingerprint, machine, or local runtime. Never persist raw credentials.

### Resource pool

Represents one independently constrained resource.

```rust
struct ResourcePool {
    id: ResourcePoolId,
    entitlement_id: EntitlementId,
    scope: PoolScope,
    unit: ResourceUnit,
    window: PoolWindow,
    limit: Option<Decimal>,
    remaining: Option<Decimal>,
    used: Option<Decimal>,
    reset_at: Option<Timestamp>,
    state: KnowledgeState,
    hard_limit: bool,
    evidence: EvidenceRef,
}
```

Candidate units:

- request
- premium request
- input token
- output token
- weighted token
- utilization percent
- currency minor unit
- credit
- compute second
- concurrent slot

Candidate windows:

- instantaneous/concurrency
- fixed minute/day/month
- rolling duration
- billing period
- non-renewing balance
- unbounded local observation

### Evidence

```rust
struct CapacityEvidence {
    source: EvidenceSource,
    authority: EvidenceAuthority,
    observed_at: Timestamp,
    expires_at: Option<Timestamp>,
    raw_fields: BTreeMap<String, RedactedValue>,
    confidence: Confidence,
}

enum EvidenceSource {
    ResponseHeader,
    ResponseBody,
    AuthenticatedAccountEndpoint,
    AdminUsageEndpoint,
    BillingEndpoint,
    OperatorConfiguration,
    LocalObservation,
    InferredFromHistory,
}

enum KnowledgeState {
    Known,
    Estimated,
    Stale,
    Opaque,
    Exhausted,
    Unavailable,
}
```

Provider-returned raw names and values should be retained in a safe structured supplement. Normalized fields are projections, not replacements for source evidence.

### Route consumption contract

```rust
struct RouteConsumption {
    route_id: RouteId,
    pool_id: ResourcePoolId,
    fixed_units_per_request: Option<Decimal>,
    input_units_per_token: Option<Decimal>,
    output_units_per_token: Option<Decimal>,
    model_multiplier: Option<Decimal>,
    context_tiers: Vec<ContextCostTier>,
    confidence: Confidence,
    evidence: EvidenceRef,
}
```

Unknown multipliers remain unknown. Marketing documentation may seed a curated contract, but authenticated provider metadata wins when it describes the operator's actual account.

### Work estimate and reservation

```rust
struct InferenceWorkEstimate {
    task_id: String,
    expected_turns: Range<u64>,
    input_tokens: Range<u64>,
    output_tokens: Range<u64>,
    concurrent_children: Range<u32>,
    expected_duration: Range<Duration>,
    required_capabilities: BTreeSet<Capability>,
    confidence: Confidence,
}

struct CapacityReservation {
    work_id: String,
    route_id: RouteId,
    pool_claims: Vec<PoolClaim>,
    fallback_routes: Vec<RouteId>,
    expires_at: Timestamp,
    status: ReservationStatus,
}
```

Reservations are initially advisory Omegon ledger entries. They prevent parallel planners from promising the same scarce capacity twice; they do not pretend to reserve resources at the provider.

## Feasibility classification

A work plan receives one of:

- **safe**: known capacity exceeds upper estimate plus operator reserve.
- **constrained**: likely feasible, but upper estimate approaches a reserve or reset boundary.
- **opaque**: required pool exists but remaining capacity cannot be established.
- **insufficient**: a hard pool cannot cover the lower estimate.
- **policy-blocked**: provider capacity exists but operator policy forbids the spend or automation mode.
- **capability-blocked**: capacity exists on routes that cannot satisfy the task.

The planner must include its rationale and evidence age. It must not emit a bare color or score.

## Provider evidence matrix

| Provider route | Immediate response evidence | Account/entitlement evidence | Monetary evidence | Initial status |
|---|---|---|---|---|
| Anthropic API | request/input/output token windows, resets, retry | organization tier with admin authority | usage/cost reports with admin key; otherwise operator cap | partial implementation |
| Claude OAuth | 5h/7d unified utilization, reset when exposed | subscription identity/plan where exposed | sunk subscription; no dollar balance assumed | partial implementation |
| OpenAI API | request/token limit, remaining, reset | project/org identity | usage/cost APIs require admin credential; ordinary key may be opaque | partial implementation |
| OpenAI Codex OAuth | primary/secondary used %, reset, active limit, credits flag | account plan/status where exposed | opaque unless provider returns credits state | strongest current subscription telemetry |
| GitHub Copilot | usage/rate observations | authenticated account status, plan, premium quota/reset, organization policy | seat/overage policy; no upstream-vendor spend | first account adapter |
| Google Gemini API | response `usageMetadata`, rate failures/headers | Cloud project quota with suitable Google credentials | billing budget/spend with billing authority; API key alone insufficient | research required |
| Google Antigravity | response observations | product-specific OAuth status | opaque | research required |
| OpenRouter | generic rate headers and response usage | authenticated key status/limits | key usage, limits, credit balance where returned | account adapter candidate |
| Groq | generic/provider rate headers | account tier/limits if API exists | dashboard/operator cap otherwise | response adapter first |
| xAI | generic rate headers | team/project tier if exposed | billing API if authorized; operator cap otherwise | response adapter first |
| Mistral | generic rate headers and response usage | workspace plan if exposed | billing/credit endpoint if documented | response adapter first |
| Cerebras | rate headers | account tier if exposed | dashboard/operator cap otherwise | matrix gap: auth exists, registry offering absent |
| Moonshot | generic rate headers | account status | balance endpoint where deployment supports it | endpoint verification required |
| Perplexity | response usage and rate headers | API plan | official credit endpoint if available; otherwise operator cap | research required |
| OpenCode Go | response usage | product plan/status | opaque until verified | research required |
| Ollama Cloud | response telemetry | cloud account plan/status | opaque until verified | research required |
| Hugging Face Router | router/provider headers | HF account entitlement | HF billing/credits if authorized | preserve routed backend |
| Ollama local | tokens, latency, queue | local machine/runtime | not applicable | compute adapter |
| DwarfStar local | tokens, latency, health | local machine/runtime | not applicable | compute adapter |

## Current repository evidence

Implemented today:

- `auth.rs` has canonical credential descriptors for direct API, OAuth, hosted, and local providers.
- `providers.rs::parse_rate_limit_snapshot` captures Anthropic unified utilization, Codex windows, generic request/token remaining values, retry/reset hints, and request IDs.
- `usage.rs` preserves Anthropic and Codex semantics in headroom classifications and operator-facing reports.
- Per-turn provider telemetry can be persisted and correlated with provider/model usage.
- Inference inventory and admission already distinguish concrete routes and prevent unverified routes from being treated as curated.

Gaps:

- `ProviderTelemetrySnapshot` has fixed fields rather than a typed pool collection.
- Generic parsing retains remaining values but not all corresponding limits, independent reset windows, or input/output distinctions.
- Telemetry is provider-scoped rather than entitlement/principal-scoped.
- No account/billing discovery adapter interface exists.
- No operator budget/reserve schema exists.
- No usage ledger maps actual consumption to entitlement pools.
- No planner reservation or feasibility preflight exists.
- Unknown/stale/opaque are not uniformly represented across providers.

## Evidence acquisition contract

Each provider adapter must declare:

```text
provider and route kinds
observation types supported
credential class required for each observation
endpoint/method or response hook
polling safety and minimum interval
freshness TTL
authority level
known redaction requirements
failure semantics
```

Supported observation classes:

- response usage
- response rate windows
- subscription utilization
- account entitlement
- credit balance
- historical usage
- historical cost
- project quota
- local compute capacity

Account discovery failures must not fail inference. They downgrade capacity knowledge to stale or opaque and retain the last safe observation with provenance.

## Copilot first-adapter design

Copilot is the first end-to-end account adapter because it forces correct separation of account entitlement, meta-provider routes, premium units, and model-specific consumption.

### Required observations

- authenticated GitHub principal
- individual/business/enterprise plan when exposed
- account or organization quota scope
- premium requests total, used, or remaining
- reset timestamp/window
- overage policy if exposed
- included/unmetered model set if exposed
- model multiplier if exposed
- organization policy restrictions
- raw model catalog and route capabilities

### Authority order

1. Authenticated account/status response for this principal.
2. Authenticated model metadata for this principal.
3. Per-response quota and retry evidence.
4. Curated provider documentation.
5. Historical estimates.

Never infer that a Copilot Claude route consumes Anthropic quota, or that a Copilot GPT route consumes OpenAI API budget.

### Safety

- Read-only requests only.
- Reuse existing OAuth material; never log bearer tokens.
- Record account identifiers only in redacted/fingerprinted form where persistence is needed.
- Bound refresh rate and cache observations.
- Treat endpoint/schema drift as `stale` or `opaque`, not zero balance.
- Store raw field names needed for diagnostics, with sensitive values redacted.

## Research plan

### Phase 1: repository inventory

For every configured inference provider:

- identify concrete client/transport implementation;
- identify auth class and principal scope;
- enumerate response headers/body usage currently available;
- locate existing provider-specific status/account requests;
- classify what is captured, logged-only, or discarded;
- identify tests and live-smoke fixtures.

Deliverable: checked provider evidence matrix with code pointers.

### Phase 2: upstream contract verification

For every provider, gather primary-source evidence for:

- response usage fields;
- rate-limit headers;
- account/status endpoint;
- subscription/quota semantics;
- billing/credit/usage endpoint and required credential class;
- reset behavior;
- terms or automation constraints relevant to polling and unattended use.

Record:

- URL and retrieval date;
- documented versus observed status;
- sample redacted schema;
- permission requirements;
- stability caveats.

Do not treat third-party blog posts or client reverse engineering as authoritative. They may identify a lead, but the matrix must label such evidence as observed/experimental.

### Phase 3: live bounded probes

With operator-authorized credentials:

- capture redacted response headers from one minimal request per route;
- call documented read-only account/status endpoints;
- compare account response to dashboard-visible values where feasible;
- verify principal/account scope;
- confirm reset units and timestamp semantics;
- store fixtures stripped of identifiers and secrets.

No paid bulk requests. Each probe must state expected cost and stop after the first conclusive response.

### Phase 4: schema and adapter implementation

- introduce entitlement and typed resource-pool DTOs;
- adapt current telemetry into typed pools without dropping legacy fields;
- implement OpenAI Codex OAuth and Claude OAuth capacity discovery before the broader Copilot adapter;
- persist snapshots by redacted principal and route;
- project `/limits` as a compact hard-bucket magazine check over the shared cache;
- project `/usage` as a navigable TUI menu containing limits, session usage, breakdowns, history, entitlement, evidence, and links;
- remove `/runway` registration, aliasing, dispatch, tests, and documentation;
- add operator reserve/cap configuration;
- implement advisory feasibility without autonomous rerouting.

### Phase 5: planning integration

- derive initial work estimates from historical sessions;
- reserve expected capacity before cleave/delegate waves;
- reconcile reservations against actual usage after each turn/wave;
- require operator confirmation for opaque or constrained high-cost plans;
- add fallback route plans and exhaustion handling.

## Acceptance criteria for the first slice

1. Every configured inference provider appears in the evidence matrix, including registry/auth mismatches.
2. Every value identifies unit, scope, window, source, freshness, and knowledge state.
3. Copilot quota is scoped to a redacted GitHub principal and never attributed to upstream model vendors.
4. Account discovery is read-only, bounded, cached, and non-fatal to inference.
5. Missing quota data renders `opaque`, never `unlimited` or `healthy`.
6. Existing Anthropic, Codex, and generic telemetry remains available during migration.
7. `/limits` contains only hard/provider-enforced buckets and reset/refill information; `/usage` includes those same buckets in a navigable full-scope usage menu.
8. `/runway` is not registered, dispatched, documented, or retained as an alias.
9. No raw credential or sensitive account payload is persisted or logged.
10. Tests cover schema drift, stale cache, partial fields, zero balance, reset, multiple accounts, limits projection filtering, and usage-menu navigation.
11. Planning integration remains advisory until estimates are calibrated against observed usage.

## Open questions

- [assumption] Existing provider OAuth tokens have sufficient authority to call read-only account/status endpoints without requesting new scopes.
- [assumption] Copilot account/status responses expose premium-request data consistently enough for bounded planning.
- Which account identifiers may be persisted, and what fingerprinting scheme is stable without becoming correlatable across projects?
- Should operator budgets attach to entitlement, credential fingerprint, provider project, or concrete route?
- How should organization-pooled quota be coordinated across concurrent Omegon workspaces?
- What historical sample size is sufficient before task estimates graduate from opaque to estimated?
- Should stale known balance be more conservative than opaque balance, or should both require the same confirmation threshold?
- Which account probes are safe to perform automatically when `/usage` or `/limits` observes a stale cache versus only on an explicit refresh action?
- How do providers communicate overage charging separately from hard exhaustion?
- Which local-compute metrics are portable enough across Ollama and DwarfStar to support planning without platform-specific false precision?

## First evidence tasks

1. Trace GitHub Copilot authentication and request code to identify existing account/status calls and OAuth scopes.
2. Capture the exact Copilot model-discovery schema already observed by Omegon.
3. Locate any quota, premium-request, plan, reset, or policy fields in current responses/log fixtures.
4. Verify GitHub's primary documentation for premium-request accounting and model multipliers.
5. Identify the authenticated read-only endpoint used by current official Copilot clients, classify its stability, and prepare a redacted one-request probe.
6. In parallel, inventory direct API providers for official usage/cost endpoints and credential requirements.

## Open Questions

## Evidence gathered — GitHub Copilot

### Repository contract

- Omegon's GitHub Copilot OAuth defaults to `read:user` only (`core/crates/omegon/src/auth.rs`). That scope supports principal identity, not organization billing administration.
- Copilot token exchange uses `GET /copilot_internal/v2/token` with the GitHub token (`core/crates/omegon/src/github_copilot.rs`). The typed response retains `token`, `expires_at`, `refresh_in`, and `endpoints`; no quota or plan fields are projected.
- Model discovery then uses the exchanged Copilot token against `{api}/models`. The raw payload is retained for contract probes, so schema research can inspect unknown fields without changing inference behavior.
- Inference response telemetry already flows through the generic rate-limit parser, but no account-scoped Copilot capacity pool exists.

### Primary upstream evidence

- GitHub's documented Copilot plans and premium-request contract defines monthly premium-request allowances, model-dependent multipliers, first-of-month reset, and paid-plan overage pricing. These are curated pricing rules, not proof of one principal's remaining balance.
- GitHub's stable billing APIs are enterprise/organization scoped: `GET /enterprises/{enterprise}/settings/billing/premium_request/usage` and `GET /organizations/{org}/settings/billing/premium_request/usage`. They return dated aggregate usage items with product, SKU, model, unit type, quantity, gross amount, discount, and net amount.
- Those billing endpoints require billing authority (`manage_billing:copilot` for classic PATs or billing read permission for fine-grained tokens). Omegon's current `read:user` OAuth credential cannot be assumed to authorize them.
- GitHub's Copilot Metrics REST API requires organization read permission and Copilot Business/Enterprise ownership. GitHub documents that API as closing on 2026-04-02, so it is not a durable quota-discovery foundation.

### Evidence classification

| Value | Personal OAuth | Org/enterprise billing authority | Authority |
|---|---:|---:|---|
| GitHub principal | yes | yes | documented GitHub user API / token identity |
| Copilot API endpoint and expiry | yes | yes | observed token-exchange contract |
| Effective model catalog | yes | yes | authenticated `/models` response |
| Published plan allowance | infer from known plan only | infer from known plan only | curated documentation |
| Personal remaining premium requests | not documented | not applicable | opaque unless an observed product endpoint is adopted |
| Organization aggregate premium usage | no | yes | documented billing API |
| Model multiplier | curated docs; authenticated metadata if present | same | documented/observed |
| Reset | first of month by documented policy | usage-period query | documented billing policy/API |
| Overage enabled/cap | not established | organization settings/administration | opaque without authority |

### Design consequence

Copilot capacity needs two credential lanes rather than one overloaded probe:

1. **Personal subscription lane** — existing `read:user` OAuth; principal, token contract, model catalog, response telemetry; remaining premium-request balance stays `opaque` unless a safe account-status contract is verified.
2. **Organization billing lane** — separately configured credential with explicit billing-read authority; aggregate premium-request usage and monetary amounts; never silently requested during normal login.

The first implementation must therefore project an opaque personal Copilot entitlement honestly before adding any undocumented account-status probe. Organization billing discovery is a separate optional adapter.

### Sources

- GitHub Docs: REST API endpoints for enhanced billing — premium request usage for enterprise and organization.
- GitHub Docs: Copilot requests — premium request allowances, reset, multipliers, and overage.
- GitHub Docs: REST API endpoints for Copilot metrics — permissions and announced API closure.
- Repository: `core/crates/omegon/src/auth.rs`, `core/crates/omegon/src/github_copilot.rs`, `core/crates/omegon/src/providers.rs`.

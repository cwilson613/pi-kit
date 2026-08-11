---
id: model-catalog-and-ux-grouping
title: "Model Catalog and UX Grouping — curated favorites over complete provider inventory"
status: decided
parent: provider-route-conceptual-model-matrix
tags: [tui, acp, ux, model-catalog, routing, discovery]
open_questions: []
dependencies:
  - route-matrix-registry-migration
  - inference-discovery-producers
related:
  - modern-command-palettes
---

# Model Catalog and UX Grouping — curated favorites over complete provider inventory

## Overview

Omegon MUST preserve every selectable model route reported by each configured provider while keeping the normal model-selection surface comprehensible. The operator-facing default is a provider-grouped shortlist. Each provider receives 3–5 declared bootstrap favorites when the operator has not customized it. Operators can browse the provider's complete live inventory and toggle concrete provider/model routes as favorites. Favorites persist as an unordered set; the projection applies deterministic ordering, so no drag-and-drop or separate ordering mechanism is required.

This is provider-neutral. Ollama Cloud motivates the work because its public `/api/tags` inventory is materially larger and more volatile than the embedded registry, but the components and interactions apply equally to Anthropic, OpenAI, GitHub Copilot, Moonshot, OpenRouter, local Ollama, and every future endpoint with a supported discovery contract.

## Goals

- Preserve the complete discovered route inventory; curation is a projection, never destructive filtering.
- Put 3–5 common, obvious choices per configured provider within immediate reach.
- Let operators browse every selectable route for a provider and add or remove favorites.
- Persist favorites by concrete route ID (`provider:model-id`) because credentials, transport, quota, and failures are route-specific.
- Reuse one renderer-neutral projection in TUI and ACP rather than adding another surface-specific model list.
- Keep dynamic, ungraded models explicitly selectable without claiming an inferred quality ranking.
- Preserve current and favorited routes through temporary discovery or credential failures with honest availability state.

## Non-goals

- Algorithmically identify the objectively "best" model.
- Auto-favorite models based on grade, benchmark, name, recency, or usage.
- Reorder favorites manually.
- Collapse equivalent conceptual models across provider routes.
- Remove provider routes from inventory merely because they are absent from the shortlist.
- Make raw live discovery authoritative for autonomous routing quality or capability admission.

## Evidence: current route suite

### Embedded endpoint deployments

The current embedded endpoint block declares the following callable deployments and discovery/auth shapes:

| Endpoint | Protocol / enumeration | Authentication | Catalog implication |
|---|---|---|---|
| `anthropic` | Anthropic `/v1/models` | OAuth or API key | Dynamic complete list when configured |
| `openai` | OpenAI-compatible `/models` | `OPENAI_API_KEY` | Dynamic complete list when configured |
| `openai-codex` | OpenAI-compatible execution; enumeration intentionally unverified | OAuth | Bootstrap/current routes only until contract is verified |
| `github-copilot` | Copilot token exchange + `/models` | GitHub Copilot OAuth | Dynamic complete list when configured |
| `openrouter` | enriched OpenRouter `/models` | `OPENROUTER_API_KEY` | Dynamic complete list when configured |
| `groq` | OpenAI-compatible `/models` | `GROQ_API_KEY` | Dynamic complete list when configured |
| `mistral` | OpenAI-compatible `/models` | `MISTRAL_API_KEY` | Dynamic complete list when configured |
| `xai` | OpenAI-compatible `/models` | `XAI_API_KEY` | Dynamic complete list when configured |
| `moonshot` | OpenAI-compatible `/models` | `MOONSHOT_API_KEY` | Dynamic complete list when configured |
| `huggingface-router` | OpenAI-compatible `/models` | `HF_TOKEN` | Dynamic list if upstream supports the contract |
| `gemini-openai` / logical `google` | Google Generative Language listing | `GEMINI_API_KEY` | Normalize endpoint/provider identity before projection |
| `ollama` | native `/api/tags` | none | Installed local inventory |
| `ollama-cloud` | public native `https://ollama.com/api/tags` | none for discovery; `OLLAMA_API_KEY` for execution | Complete public inventory, execution remains credential-gated |

The provider/model records also contain logical routes for `google`, `ollama-cloud`, `opencode-go`, and `perplexity` that are not represented uniformly in the current endpoint array. Implementation MUST resolve that registry/inventory mismatch rather than introducing another hardcoded exception in the TUI.

### Current consumers

- `ModelCatalog::discover()` projects embedded registry plus persisted discovery and is the current TUI source.
- `App::open_model_selector()` flattens the entire catalog directly into one selector.
- ACP independently enumerates local Ollama and then iterates the embedded registry, so it does not currently share live catalog projection.
- A test-only TUI helper contains a separate hardcoded Anthropic/OpenAI/Codex list.
- The inference runtime refreshes provider discovery asynchronously at startup and persists last-known-good results.

These consumers MUST converge on one catalog and curation projection. Renderer-specific label formatting can remain local; membership, availability, current-route preservation, favorites, and seed behavior cannot.

## Decisions

### Complete inventory and curated projection are separate layers

**Status:** accepted

`ModelCatalog` and `InventorySnapshot` retain all chat-compatible, selectable offerings. A new curation projection chooses which routes appear in the easy-reach view. The full provider browser always derives from the complete catalog. No shortlist operation mutates or truncates discovery results.

### Favorites are concrete route IDs

**Status:** accepted

Persist `provider:model-id`, not conceptual model ID. `moonshot:kimi-k3` and `ollama-cloud:kimi-k3` remain independent favorites because they have different auth, transport, quotas, context claims, and runtime failure state.

### Provider seeds are declared bootstrap data

**Status:** accepted

Each supported provider may declare 3–5 obvious seed routes. Seeds are product defaults, not inferred rankings. A seed appears only if its route is currently represented by the effective inventory. Missing or retired seeds are omitted rather than synthesized.

For a provider with no operator favorites:

1. show available declared seeds;
2. ensure the provider default is present when selectable;
3. if fewer than three choices remain, fill deterministically from available routes up to three without attaching a quality claim.

Once an operator changes favorites for a provider, that provider's explicit set replaces its seeds. An explicit empty set is valid and differs from "never customized."

### Favorites are unordered

**Status:** accepted

Persistence stores a set. Projection order is:

1. current model, if in the current view;
2. provider display name;
3. model display name;
4. route ID as a stable tie-breaker.

No drag-and-drop, ordinal field, or independent curation UI is introduced.

### Stale and unavailable favorites remain visible

**Status:** accepted

A favorite absent from the latest successful discovery remains visible with an `unavailable` or `not currently advertised` state. It is not silently deleted. A route absent from a successful provider enumeration is not selectable unless another active inventory layer still establishes it as callable. Temporary fetch failure retains the provider's last-known-good result and freshness evidence.

### Public discovery and execution credentials are distinct gates

**Status:** accepted

Ollama Cloud `/api/tags` enumeration runs without `OLLAMA_API_KEY`. Selecting or invoking an Ollama Cloud route remains credential-gated and offers `/login ollama-cloud` or `/secrets set OLLAMA_API_KEY` remediation. Other providers enumerate only when their discovery transport has the required credentials.

### Shared semantic projection, surface-specific interaction

**Status:** accepted

TUI and ACP consume the same membership and state projection. TUI may use nested selectors and keyboard actions; ACP may expose a flat select option list initially because the ACP protocol's configuration selector is one-dimensional. ACP MUST still receive curated options by default and preserve the unavailable current route. A future explicit full-catalog ACP capability can expose provider browsing without changing curation semantics.

## Component design

### `surfaces/model_menu.rs` — renderer-neutral semantics

Introduce a projection module that depends on catalog records and preference data but not on ratatui:

```rust
pub struct ModelMenuProjection {
    pub current_route: String,
    pub favorite_groups: Vec<ModelProviderGroupProjection>,
    pub providers: Vec<ModelProviderSummaryProjection>,
}

pub struct ModelProviderSummaryProjection {
    pub provider_id: String,
    pub display_name: String,
    pub model_count: usize,
    pub favorite_count: usize,
    pub freshness: Option<String>,
    pub availability: ProviderAvailabilityProjection,
}

pub struct ModelProviderGroupProjection {
    pub provider_id: String,
    pub display_name: String,
    pub models: Vec<ModelRouteProjection>,
}

pub struct ModelRouteProjection {
    pub route_id: String,
    pub provider_id: String,
    pub conceptual_model_id: Option<String>,
    pub display_name: String,
    pub description: String,
    pub context_input: usize,
    pub capabilities: Vec<String>,
    pub favorite: bool,
    pub seeded: bool,
    pub current: bool,
    pub selectable: bool,
    pub availability_detail: Option<String>,
    pub freshness: Option<String>,
}
```

Primary functions:

```rust
pub fn project_model_menu(
    catalog: &ModelCatalog,
    preferences: &ModelMenuPreferences,
    current_route: &str,
) -> ModelMenuProjection;

pub fn project_provider_inventory(
    catalog: &ModelCatalog,
    preferences: &ModelMenuPreferences,
    current_route: &str,
    provider_id: &str,
) -> Option<ModelProviderGroupProjection>;
```

`ModelCatalog` needs stable provider IDs in addition to display-name map keys. Curation MUST NOT reverse-map display labels to provider IDs.

### Preference model and persistence

Add a global operator preference document under the Omegon user configuration directory rather than project `Profile`. Model favorites are personal navigation state and should not travel in a repository profile or alter routing policy.

Proposed backward-compatible shape:

```json
{
  "schemaVersion": 1,
  "providers": {
    "ollama-cloud": {
      "customized": true,
      "favorites": [
        "ollama-cloud:gpt-oss:120b",
        "ollama-cloud:qwen3.5:397b"
      ]
    }
  }
}
```

Requirements:

- atomic write (temporary file plus rename);
- missing/corrupt file falls back to seeds and emits a bounded diagnostic;
- validate that favorite route IDs have the declared provider prefix;
- preserve unknown providers and routes so a temporarily missing extension/provider does not erase preferences;
- toggle is idempotent and updates the active menu immediately after a successful write;
- no secret or credential material enters this document.

Suggested owner: `model_preferences.rs`, with paths supplied by `paths.rs`. Do not place favorites in runtime `Settings`; session snapshots and project profile capture should not accidentally redefine global menu curation.

### Seed declaration

Add optional seed route IDs to provider-level registry metadata, or a dedicated data asset keyed by endpoint ID if the registry schema migration cannot land in the same slice. Data is preferred over a Rust `match`. Registry validation MUST enforce:

- at most five seeds per provider;
- no duplicates;
- each embedded seed resolves to a model route for that provider;
- provider default is among seeds when the default resolves;
- absent live-discovered routes are tolerated at runtime and simply omitted.

### TUI components and state machine

Replace the single flattened `SelectorKind::Model` flow with three semantic states:

```text
ModelShortlist
  Enter on model -> select route
  Enter on Browse all -> ModelProviders

ModelProviders
  Enter on provider -> ModelProviderInventory(provider_id)
  Esc -> ModelShortlist

ModelProviderInventory(provider_id)
  Enter -> select route
  Space / favorite action -> toggle favorite, keep browser open, refresh rows
  Esc -> ModelProviders
```

The shortlist contains provider headings/groups plus a terminal `Browse all models by provider…` action. Because the existing `Selector` supports only selectable rows and Enter/Escape, implementation should either:

1. extend selector options with row kind and secondary action; or
2. use the existing renderer-neutral `MenuProjection` machinery for model browsing.

The preferred path is a model-specific projection rendered through shared menu semantics, because favorites require secondary actions and nested navigation. Do not encode fake route values such as `__browse__` into a model selector without a typed action boundary.

Search/filtering can follow after the first implementation slice if the generic menu surface does not yet support text filtering. Full keyboard reachability and scrolling are mandatory in the first slice.

### ACP projection

`AcpAgent::build_config_options()` MUST stop probing local Ollama and iterating `ModelRegistry` independently. It should consume the same catalog and shortlist projection. Initial ACP behavior:

- expose curated shortlist routes;
- include current route first when unavailable or outside favorites;
- use the same concrete route IDs and availability gate;
- do not claim the curated list is the complete provider inventory;
- retain a future extension point for full provider enumeration.

### CLI/control projection

`/model list` should remain a complete evidence-oriented listing unless its contract is explicitly changed. Add a curated/default view only if the command surface can distinguish it clearly, for example `/model favorites` versus `/model list`. The model menu and model list answer different questions: navigation convenience versus inventory truth.

## Interaction examples

### Default shortlist

```text
Select Model

Anthropic
  Claude Fable 5
  Claude Sonnet 5
  Claude Haiku 4.5

Ollama Cloud
  GPT OSS 120B
  Qwen3.5 397B
  Kimi K3

Browse all models by provider…
```

### Provider browser

```text
Model Providers
  Ollama Cloud       18 models · 3 favorites · confirmed 2m ago
  GitHub Copilot     26 models · seeds · confirmed 8m ago
  Moonshot AI         4 models · 1 favorite · confirmed 5m ago
```

### Full provider inventory

```text
Ollama Cloud — all models
  ★ GPT OSS 120B
  ☆ DeepSeek V4 Flash
  ☆ DeepSeek V4 Pro
  ☆ Gemma 4 31B
  ★ Kimi K3
  ★ Qwen3.5 397B
```

`Enter` selects; `Space` toggles favorite; unavailable rows carry an explicit badge/detail and do not execute.

## Assumptions resolved

- **[resolved assumption]** Operators want favorites to persist globally, not in a repository profile. This follows from favorites being personal navigation state rather than project routing policy.
- **[resolved assumption]** Provider discovery may return models with no reviewed grade. Such routes remain explicitly selectable but receive no synthetic rank.
- **[resolved assumption]** A successful live enumeration is authoritative for current provider advertisement; a fetch failure retains last-known-good state.
- **[resolved assumption]** ACP cannot initially reproduce nested TUI browsing through the current one-dimensional configuration option contract. It consumes the same curated projection and preserves current-route truth.
- **[resolved assumption]** The default shortlist does not need manual ordering. Deterministic projection is sufficient.

## Implementation sequence

1. **Route-suite normalization**
   - Ensure every provider represented by model records resolves to an endpoint/provider identity or is explicitly classified as bootstrap-only/non-enumerable.
   - Confirm Ollama Cloud startup discovery writes all `/api/tags` routes to the persisted cache.
   - Add a fixture representing a complete multi-model Ollama Cloud response.

2. **Preference and projection core**
   - Add seed data and registry validation.
   - Implement atomic global favorites persistence.
   - Implement renderer-neutral shortlist and provider-inventory projections.

3. **TUI interaction**
   - Add shortlist, provider browser, and full-provider inventory states.
   - Add favorite toggle and immediate projection refresh.
   - Preserve current/unavailable route behavior and credential remediation.

4. **Consumer convergence**
   - Route ACP options through the shared curated projection.
   - Remove the independent ACP local-Ollama/static-registry enumeration.
   - Remove or rewrite the test-only hardcoded TUI model selector helper.

5. **Verification and hardening**
   - Validate full inventory accessibility, persistence, churn, auth, stale routes, keyboard navigation, and cross-surface membership.

## Test matrix

### Inventory preservation

- A fixture with 18 Ollama Cloud models produces 18 chat-selectable routes in provider browsing.
- Embedded routes absent from a successful live listing are disabled and omitted from selectable full inventory.
- Non-chat/embedding/internal routes remain filtered.
- A discovery failure retains last-known-good routes and marks freshness honestly.

### Seed projection

- A configured provider with no customization shows 3–5 available declared seeds.
- A retired seed is omitted.
- A provider with fewer than three seeds fills deterministically without a quality badge.
- An explicit empty customized set shows no favorites for that provider.

### Favorites

- Toggling a route persists and survives process restart.
- Toggling the same route twice restores the original set.
- Equivalent conceptual models on two providers can be favorited independently.
- Unknown/missing favorite IDs survive persistence round trips.
- Corrupt preference data falls back safely without damaging the file silently.

### Discovery churn

- A newly discovered route appears immediately in full provider browsing after refresh.
- A missing favorite remains visible but unavailable.
- A returned favorite becomes selectable again without re-adding it.
- Current route remains visible even when outside the shortlist.

### Authentication and availability

- Ollama Cloud enumeration succeeds without `OLLAMA_API_KEY`.
- Ollama Cloud execution remains blocked without the key and offers remediation.
- Credentialed providers are absent or marked unavailable according to the shared provider gate.
- Local Ollama remains available without cloud credentials.

### TUI

- Default model menu contains only projected favorites/seeds plus browse action.
- Provider browser reports complete model and favorite counts.
- Full inventory is keyboard-scrollable and every route is reachable.
- Space toggles favorite without selecting the model or closing the browser.
- Enter on available model performs the existing model-controller transition.
- Enter on unavailable model does not mutate runtime route.
- Esc returns through inventory → providers → shortlist without losing state.

### Cross-surface consistency

- TUI and ACP curated route ID sets match for the same catalog/preferences/auth snapshot.
- ACP preserves an unavailable current route as a labeled current option.
- `/model list` still exposes complete inventory rather than silently adopting shortlist semantics.
- No surface maintains a second hardcoded provider/model shortlist.

## Implementation file scope

Expected first implementation slice:

- `data/model-registry.json` and registry schema/validator — provider seeds and endpoint normalization.
- `core/crates/omegon/src/model_preferences.rs` — global favorites persistence.
- `core/crates/omegon/src/surfaces/model_menu.rs` — shared semantic projection.
- `core/crates/omegon/src/surfaces/mod.rs` — module export.
- `core/crates/omegon/src/model_catalog.rs` — stable provider identity and projection inputs.
- `core/crates/omegon/src/inference_discovery.rs` / `inference_runtime.rs` — full Ollama Cloud startup/cache verification.
- `core/crates/omegon/src/tui/mod.rs`, `tui/input.rs`, and model-menu state/rendering modules — nested browsing and favorite toggle.
- `core/crates/omegon/src/acp.rs` / `acp/model_options.rs` — shared curated membership.
- Focused tests in the owning modules and `tui/tests.rs`.

## Tradeoffs

- **Declared seeds age:** provider defaults can become stale. Live inventory omission prevents dead seeds from remaining selectable; registry maintenance still owns useful bootstrap choices.
- **Global persistence adds a file:** this avoids polluting project profiles and session snapshots, at the cost of one small atomic preference store.
- **ACP initially lacks full nested browsing:** shared curation still removes membership divergence; full ACP inventory needs a richer protocol surface later.
- **Unavailable favorites add visual noise:** preserving operator intent and avoiding silent data loss is more important than a perfectly clean list.
- **No algorithmic rank:** the menu is less "smart" than opaque scoring, but it remains evidence-honest and predictable.

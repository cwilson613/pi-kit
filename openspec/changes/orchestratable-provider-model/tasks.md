# Orchestratable Provider Model — Tasks

## 1. ProviderInventory + BridgeFactory (routing.rs — new)
<!-- specs: routing -->

- [ ] 1.1 Define `CapabilityTier` enum: `Leaf`, `Mid`, `Frontier`, `Max` with Display impl
- [ ] 1.2 Define `ProviderEntry` struct: `provider_id`, `has_credentials`, `is_reachable`, `capability_tier` (max tier this provider supports), `models` (Vec of available model IDs), `cost_tier` (Free/Cheap/Standard/Premium)
- [ ] 1.3 Define `OllamaModelInfo` struct: `name`, `size_bytes`, `is_running` (loaded in VRAM), `vram_bytes`
- [ ] 1.4 Define `ProviderInventory` struct: `entries: Vec<ProviderEntry>`, `ollama_models: Vec<OllamaModelInfo>`, `probed_at: Instant`
- [ ] 1.5 Implement `ProviderInventory::probe()` — iterate auth::PROVIDERS, call `resolve_api_key_sync()` for credential check, populate entries. For Ollama: async HTTP to /api/tags and /api/ps with 300ms timeout
- [ ] 1.6 Implement `ProviderInventory::refresh()` — re-probe, replacing current entries
- [ ] 1.7 Implement `ProviderInventory::providers_with_credentials()` → iter of ProviderEntry
- [ ] 1.8 Define `CapabilityRequest` struct: `tier: CapabilityTier`, `prefer_local: bool`, `avoid_providers: Vec<String>`
- [ ] 1.9 Define `ProviderCandidate` struct: `provider_id: String`, `model_id: String`, `score: f32`
- [ ] 1.10 Implement `route(req: &CapabilityRequest, inventory: &ProviderInventory) -> Vec<ProviderCandidate>` — score each provider by tier match, cost, and preference; return sorted desc by score
- [ ] 1.11 Define `BridgeFactory` struct wrapping `HashMap<String, Box<dyn LlmBridge + Send + Sync>>`
- [ ] 1.12 Implement `BridgeFactory::get_or_create(provider_id, model_id) -> Result<&dyn LlmBridge>` — delegates to `resolve_provider()` from providers.rs on cache miss
- [ ] 1.13 Unit tests: probe with mock env vars, route with mock inventory (various tier combinations, empty inventory, prefer_local), BridgeFactory cache hit/miss

### Testing Requirements
- Test route() with inventory containing [anthropic, ollama] → Frontier request prefers anthropic
- Test route() with inventory containing [ollama only] → Leaf request returns ollama
- Test route() with empty inventory → returns empty Vec
- Test route() with prefer_local=true → ollama scores higher than cloud at same tier
- Test CapabilityTier ordering: Leaf < Mid < Frontier < Max
- Test ProviderEntry serialization roundtrip

## 2. OllamaManager (ollama.rs — new)
<!-- specs: ollama -->

- [ ] 2.1 Define `OllamaManager` struct with `host: String`, `client: reqwest::Client`
- [ ] 2.2 Implement `OllamaManager::new()` — reads OLLAMA_HOST or defaults to localhost:11434
- [ ] 2.3 Implement `is_reachable() -> bool` — GET /api/tags with 200ms timeout
- [ ] 2.4 Implement `list_models() -> Result<Vec<OllamaModel>>` — GET /api/tags, parse response
- [ ] 2.5 Implement `list_running() -> Result<Vec<RunningModel>>` — GET /api/ps, parse VRAM info
- [ ] 2.6 Implement `hardware_profile() -> HardwareProfile` — sysinfo total memory, on Apple Silicon estimate VRAM = total (unified), recommend max model params
- [ ] 2.7 Replace `OpenAICompatClient::from_env_ollama()` TCP connect with `OllamaManager::is_reachable()` call
- [ ] 2.8 Register `mod ollama` in main.rs
- [ ] 2.9 Unit tests: parse /api/tags JSON response, parse /api/ps JSON response, hardware_profile on current system

### Testing Requirements
- Test list_models() JSON parsing with sample Ollama /api/tags response
- Test list_running() JSON parsing with sample Ollama /api/ps response
- Test is_reachable() returns false when no server (don't depend on Ollama running in CI)
- Test hardware_profile() returns non-zero total_memory

## 3. Provider bridge integration (providers.rs — modified)
<!-- specs: routing -->

- [ ] 3.1 Add `pub mod routing;` and `pub mod ollama;` to main.rs module declarations
- [ ] 3.2 Refactor `auto_detect_bridge()` to use `route()` internally: build default CapabilityRequest from model_spec, call route() against a freshly probed inventory, create bridge from top candidate
- [ ] 3.3 Preserve backward compat: if model_spec contains a provider prefix (e.g. "anthropic:model"), use that provider directly without routing
- [ ] 3.4 Export `ProviderInventory` and `route` from providers module for cleave to use

### Testing Requirements
- Test auto_detect_bridge("anthropic:claude-sonnet-4-6") still returns Anthropic directly
- Test auto_detect_bridge("") uses route() fallback path
- Test backward compat: all existing auto_detect_bridge tests still pass

## 4. Cleave per-child routing (orchestrator.rs + state.rs — modified)
<!-- specs: cleave -->

- [ ] 4.1 Add `provider_id: Option<String>` to `ChildState`
- [ ] 4.2 Add `inventory: Option<Arc<tokio::sync::RwLock<routing::ProviderInventory>>>` to `CleaveConfig`
- [ ] 4.3 Implement `infer_capability_tier(child: &ChildPlan) -> CapabilityTier` — scope.len() ≤ 2 → Leaf, ≤ 5 → Mid, else → Frontier
- [ ] 4.4 In `dispatch_child()`: if inventory is available and child has no explicit execute_model, call route() to get provider+model, use that for --model flag
- [ ] 4.5 Populate `ChildState.execute_model` and `ChildState.provider_id` from the routed result
- [ ] 4.6 Fallback: if route() returns empty, use `config.model` (global default)
- [ ] 4.7 If ChildPlan has an explicit `execute_model`, use it directly (skip routing)

### Testing Requirements
- Test infer_capability_tier: 1 file → Leaf, 3 files → Mid, 7 files → Frontier
- Test dispatch with inventory → child gets routed model, not global config.model
- Test dispatch without inventory → child gets global config.model (backward compat)
- Test explicit execute_model in plan → used directly

## 5. Startup integration (main.rs + startup.rs — modified)

- [ ] 5.1 Create `ProviderInventory` at startup after splash probes complete, store as `Arc<RwLock<ProviderInventory>>`
- [ ] 5.2 Populate from splash probe results (reuse ProbeResult data) — don't re-probe
- [ ] 5.3 Pass inventory to CleaveConfig when constructing it
- [ ] 5.4 On `/login` success: call `inventory.write().refresh()`
- [ ] 5.5 On `/model` provider change: call `inventory.write().refresh()`

### Testing Requirements
- Test that CleaveConfig receives the inventory (not None)
- Test that /login triggers inventory refresh

## Cross-cutting constraints (verified by integration)

- [ ] C.1 auto_detect_bridge backward compat — existing callers unchanged
- [ ] C.2 Interactive bridge Arc<RwLock<Box<dyn LlmBridge>>> preserved
- [ ] C.3 Cleave children are processes — --model flag is the interface
- [ ] C.4 OllamaManager async-safe — no blocking in tokio context
- [ ] C.5 ProviderInventory::probe() completes in <500ms
- [ ] C.6 All existing 844 tests continue to pass

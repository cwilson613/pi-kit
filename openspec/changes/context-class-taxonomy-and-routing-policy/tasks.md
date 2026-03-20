# Context Class Taxonomy and Routing Policy — Tasks

## 1. omegon-pi/extensions/lib/context-class.ts (new)

- [ ] 1.1 Define `ContextClass` enum: `Squad` (128k), `Maniple` (272k), `Clan` (400k), `Legion` (1m+)
- [ ] 1.2 Token threshold constants mapping class to min/max token ranges
- [ ] 1.3 `classifyContextWindow(tokenCount: number): ContextClass` — classify raw token count into a context class
- [ ] 1.4 `contextClassLabel(cls: ContextClass): string` — operator-facing label (e.g. "Squad (128k)")
- [ ] 1.5 `contextClassOrd(cls: ContextClass): number` — ordinal for comparison (Squad=0 < Maniple=1 < Clan=2 < Legion=3)
- [ ] 1.6 Unit tests in `context-class.test.ts` — boundary cases, classification, ordering

## 2. omegon-pi/extensions/lib/route-envelope.ts (new)

- [ ] 2.1 `RouteEnvelope` type: `{ provider, modelId, contextCeiling, contextClass, breakpointZones?, tier, maxThinking }`
- [ ] 2.2 `DowngradeClassification` enum: `Compatible`, `CompatibleWithCompaction`, `Degrading`, `Ineligible`
- [ ] 2.3 `classifyRoute(envelope: RouteEnvelope, requiredFloor: number, currentClass: ContextClass): DowngradeClassification`
- [ ] 2.4 `loadRouteMatrix(): RouteEnvelope[]` — loads checked-in `route-matrix.json`, validates, returns typed array
- [ ] 2.5 `buildRouteMatrixFromRegistry(models: RegistryModel[]): RouteEnvelope[]` — builds dynamic matrix from available models
- [ ] 2.6 Unit tests in `route-envelope.test.ts` — classification logic, matrix building, edge cases

## 3. omegon-pi/data/route-matrix.json (new)

- [ ] 3.1 Reviewed snapshot: Anthropic (Claude Opus 4.6 = 1M, Sonnet 4.6 = 1M), OpenAI (GPT-5.4 = 272k, GPT-5.4 Pro = 1.05M, GPT-5.4 mini = 400k), GitHub Copilot (Claude 4.6 = 128k, GPT-5.4 = 400k), Codex (GPT-5.4 = 272k), local Ollama (262k–1M)
- [ ] 3.2 Include breakpoint zones: Anthropic 200k operational boundary, OpenAI 272k pricing breakpoint
- [ ] 3.3 Schema: array of `{ provider, modelIdPattern, contextCeiling, breakpointZones, tier, lastReviewed }`

## 4. omegon-pi/extensions/lib/routing-state.ts (new)

- [ ] 4.1 `RoutingSessionState` type: `{ activeContextWindow, activeContextClass, requiredMinContextWindow, requiredMinContextClass, pinnedFloor?, observedUsage?, headroom?, downgradeSafetyArmed }`
- [ ] 4.2 `initRoutingState(currentModel: ResolvedTierModel, routeMatrix: RouteEnvelope[]): RoutingSessionState`
- [ ] 4.3 `updateUsage(state: RoutingSessionState, observedTokens: number): RoutingSessionState`
- [ ] 4.4 `pinFloor(state: RoutingSessionState, minClass: ContextClass): RoutingSessionState`
- [ ] 4.5 Wire into `sharedState` — add `routingContext?: RoutingSessionState` field
- [ ] 4.6 Unit tests in `routing-state.test.ts`

## 5. omegon-pi/extensions/lib/downgrade-policy.ts (new)

- [ ] 5.1 `evaluateDowngrade(current: RoutingSessionState, candidates: RouteEnvelope[], policy: ProviderRoutingPolicy): DowngradeEvaluation`
- [ ] 5.2 `DowngradeEvaluation` type: `{ recommendation: 'auto-reroute' | 'auto-compact' | 'operator-confirm' | 'no-viable-route', targetRoute?, compactionNeeded?, contextClassDelta?, reason }`
- [ ] 5.3 Auto-reroute: find compatible route satisfying tier + thinking + floor
- [ ] 5.4 Auto-compact: find compatible-with-compaction route where compaction is safe and no pinned floor is crossed
- [ ] 5.5 Operator-confirm: large multi-class drops (Legion→Squad), pinned floor violations, or no safe compact
- [ ] 5.6 Integration with existing `CapabilityRuntimeState` cooldowns — ineligible routes include cooled-down providers
- [ ] 5.7 Unit tests in `downgrade-policy.test.ts` — each classification path, pinned floor, multi-class drop

## 6. omegon-pi/extensions/effort/index.ts (modified)

- [ ] 6.1 On session_start: initialize `RoutingSessionState` from resolved model + route matrix
- [ ] 6.2 On tier switch: evaluate downgrade policy before switching; if degrading, surface confirmation
- [ ] 6.3 Wire `set_model_tier` tool handler to check context class compatibility before switching
- [ ] 6.4 Add context class info to effort status display
- [ ] 6.5 Update dashboard state with context class on every tier change

## 7. omegon-pi/extensions/lib/model-routing.ts (modified)

- [ ] 7.1 Add `contextCeiling?: number` to `CapabilityCandidate` type
- [ ] 7.2 Enhance `resolveTier()` to accept optional `RoutingSessionState` and filter candidates by context floor
- [ ] 7.3 Add context class to `ResolvedTierModel` output: `contextClass?: ContextClass`
- [ ] 7.4 Enhance `buildProviderSummary()` to include context class per tier

## 8. omegon-pi/extensions/lib/shared-state.ts (modified)

- [ ] 8.1 Add `routingContext?: RoutingSessionState` to SharedState
- [ ] 8.2 Add `activeContextClass?: ContextClass` to SharedState for dashboard display

## 9. omegon-pi/extensions/dashboard/ (modified)

- [ ] 9.1 Display active context class in footer/dashboard (e.g. "Legion" next to model name)
- [ ] 9.2 Show context headroom indicator when available

## Cross-cutting constraints

- [ ] C.1 Internal routing compares exact token counts; operators see named classes only
- [ ] C.2 Downgrade evaluation compares against session's required minimum context floor, not current prompt size
- [ ] C.3 Automatic compaction allowed only when no pinned floor is crossed
- [ ] C.4 Large multi-class drops (e.g. Legion→Squad) always require explicit operator confirmation
- [ ] C.5 Route selection begins by filtering to authenticated providers
- [ ] C.6 Anthropic preferred by default when multiple routes satisfy all hard constraints
- [ ] C.7 Runtime routing consumes only reviewed local snapshot, not live provider responses

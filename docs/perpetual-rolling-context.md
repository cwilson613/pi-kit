---
id: perpetual-rolling-context
title: Perpetual rolling context — provider-agnostic conversation buffer with projection-based LLM requests
status: exploring
parent: rust-agent-loop
tags: [architecture, context, providers, strategic, conversation]
open_questions: []
jj_change_id: totulnyvrrzuplprmzuwrlzpuksvkslp
issue_type: epic
priority: 1
---

# Perpetual rolling context — provider-agnostic conversation buffer with projection-based LLM requests

## Overview

Instead of treating the LLM's context window as the conversation state, Omegon maintains a perpetual in-memory conversation buffer (up to ~10MB / 1M+ tokens). When making an LLM request, a projection is computed — a subset of the buffer tailored to the provider's context window, token budget, and wire protocol requirements.

This decouples conversation state from provider constraints. Provider-specific concerns (thinking signatures, tool ID formats, role alternation, context limits) become projection-time transforms, not storage-time constraints. Compaction becomes optional (for token cost), not mandatory (for survival). Provider switching becomes trivial — just re-project from the same buffer.

Current pain points this resolves:
- Codex tool IDs breaking Anthropic (projection strips/reformats IDs per provider)
- Unsigned thinking blocks after compaction (raw blocks stored in buffer, omitted from projection if provider doesn't support them)
- Orphaned tool_results after decay (projection ensures structural integrity)
- Context overflow (projection respects provider limits by construction)
- Emergency compaction failures cascading into malformed requests

## Research

### Current architecture and why it breaks

Today's `ConversationState` (conversation.rs, ~1700 lines) serves dual duty: it's both the canonical conversation store AND the LLM-facing message builder. `build_llm_view()` transforms canonical messages into `LlmMessage` variants, applying decay, orphan stripping, and role alternation. Then each provider's `build_messages()` transforms `LlmMessage` into provider-specific JSON.

The problem: structural constraints from ANY provider leak into the canonical store. Codex compound IDs (`call_abc|fc_1`) are stored directly. Anthropic thinking signatures are stored in the `raw` field. When switching providers, these provider-specific artifacts cause 400 errors.

In rc.19–rc.24 we added 5 fixes that are all symptoms of this coupling:
1. `sanitize_tool_id()` — Codex IDs → Anthropic format
2. Omit thinking blocks without signatures
3. `strip_orphaned_tool_results()` — structural integrity after decay
4. `enforce_role_alternation()` — provider role rules after compaction
5. Emergency compaction + malformed history recovery

Each fix is correct but they're band-aids. The rolling context would eliminate the root cause.

### Memory budget analysis

Token-to-memory mapping (chars/4 heuristic):
- 200k tokens (Anthropic Pro) ≈ 800KB
- 1M tokens (Anthropic extended) ≈ 4MB
- 2M tokens (Gemini) ≈ 8MB
- Full-day intensive session: ~500 tool calls × ~2KB avg result = 1MB of tool results, plus ~200KB of user/assistant text ≈ 1.2MB total

Even an extreme session fits in 10MB. Rust `Vec<Message>` with arena-style allocation would keep this cache-friendly. No disk I/O needed for the active buffer — persistence only at session save checkpoints.

The projection step (selecting what to send) is the only per-request cost. With a sorted buffer and token budget, this is O(n) over messages — sub-millisecond for typical sessions.

### Projection architecture sketch

The projection is a function: `project(buffer, provider, token_budget) → Vec<ProviderMessage>`

Stages:
1. **Budget allocation**: Reserve tokens for system prompt (~2k), tools (~3k per tool × count), response (~16k). Remainder is conversation budget.
2. **Mandatory window**: Last N turns (configurable, default 3-5) always included. These are the immediate context the model needs.
3. **Summary zone**: Older turns beyond the mandatory window get a compaction summary IF one exists. If not, they get the decay skeleton (tool name + truncated result).
4. **Relevance boost**: Messages that reference files in the current working set, or contain terms from the user's latest prompt, get priority for inclusion.
5. **Structural integrity**: When a message is included, its paired messages (tool_use ↔ tool_result) are also included. Tool_use/tool_result blocks are atomic units.
6. **Provider formatting**: The final step transforms the selected messages into the provider's wire format — this is where ID sanitization, thinking block handling, role alternation, and content block formatting happen.

Key insight: steps 1-5 are provider-agnostic. Only step 6 is provider-specific. Today, steps 1-6 are interleaved across ConversationState, ContextManager, and each provider's build_messages().

### Provider wire protocol audit — three distinct formats

All LLM providers fall into exactly three wire protocol families. The projection layer needs one implementation per family, not per provider.

**1. Anthropic Messages API** (anthropic)
- Messages: content blocks array (`text`, `tool_use`, `tool_result`, `thinking`)
- Tool IDs: `^[a-zA-Z0-9_-]+$` (strict regex, 400 on violation)
- Thinking: requires `signature` field for round-tripping, omitted if unavailable
- Tools: `input_schema` format, OAuth remaps tool names to PascalCase
- Auth: `x-api-key` (API key) or `Authorization: Bearer` (OAuth) + `anthropic-beta` flags
- Role: strict user/assistant alternation; tool_result goes inside user content blocks

**2. OpenAI Chat Completions** (openai, openrouter, groq, xai, mistral, cerebras, huggingface, ollama)
- Messages: `role`/`content` + `tool_calls` array on assistant, `tool` role for results
- Tool IDs: flexible string format, no strict regex
- Thinking: not supported (ignored)
- Tools: `function` type with `parameters` schema
- Auth: `Authorization: Bearer` for all
- Role: system → user/assistant/tool alternation

**3. Codex Responses API** (openai-codex)
- Input items: `input_text`, `output_text`, `function_call`, `function_call_output`
- Tool IDs: compound `call_id|item_id` for round-tripping, stripped on output
- Thinking: not in wire format
- Tools: `function` type, similar to Chat Completions but `strict: null`
- Auth: JWT Bearer + `chatgpt-account-id` header
- Role: flat item list, no role alternation

Key: OpenAI Chat Completions covers 8 of 10 providers. Only Anthropic and Codex need custom projectors. Everything else delegates to the Chat Completions projector.

### Interface definitions (Rust trait sketch)

```rust
// ─── Layer 1: Buffer ────────────────────────────────────────────

/// A single entry in the conversation buffer. Provider-agnostic.
pub struct BufferEntry {
    pub turn: u32,
    pub role: EntryRole,
    pub timestamp: Instant,
}

pub enum EntryRole {
    User {
        content: String,
        images: Vec<ImageData>,
    },
    Assistant {
        text: String,
        thinking: Option<String>,
        tool_calls: Vec<CanonicalToolCall>,
        /// Opaque provider blob — only useful for round-tripping with
        /// the SAME provider that generated it. Projectors for other
        /// providers ignore this entirely.
        provider_blob: Option<ProviderBlob>,
    },
    ToolResult {
        /// Canonical call ID — matches CanonicalToolCall.id
        call_id: String,
        tool_name: String,
        content: Vec<ContentBlock>,
        is_error: bool,
        args_summary: Option<String>,
    },
}

/// A tool call in canonical (provider-agnostic) format.
pub struct CanonicalToolCall {
    /// Canonical ID. Generated by Omegon, not the provider.
    /// Format: `omg_{uuid}`. Provider-specific IDs are stored in
    /// the provider_blob for round-tripping.
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Opaque provider-specific data attached to an assistant message.
pub struct ProviderBlob {
    pub provider_id: String,
    pub data: Value,
}

pub struct ConversationBuffer {
    entries: Vec<BufferEntry>,
    /// Compaction waypoints — summaries of evicted ranges.
    /// The selector can include these as synthetic context.
    summaries: Vec<CompactionSummary>,
    intent: IntentDocument,
}

impl ConversationBuffer {
    pub fn push_user(&mut self, content: String, images: Vec<ImageData>);
    pub fn push_assistant(&mut self, entry: AssistantEntry);
    pub fn push_tool_result(&mut self, result: ToolResultEntry);
    pub fn entries(&self) -> &[BufferEntry];
    pub fn estimated_tokens(&self) -> usize;
    pub fn save_session(&self, path: &Path) -> Result<()>;
    pub fn load_session(path: &Path) -> Result<Self>;
}

// ─── Layer 2: Selector ──────────────────────────────────────────

pub struct SelectionBudget {
    /// Total tokens available for conversation messages.
    pub conversation_tokens: usize,
    /// How many recent turns to always include.
    pub mandatory_recent_turns: usize,
}

pub struct SelectionSignals<'a> {
    pub user_prompt: &'a str,
    pub recent_tools: &'a [String],
    pub recent_files: &'a [PathBuf],
    pub referenced_turns: &'a HashSet<u32>,
}

/// Select a subset of buffer entries that fits the budget.
/// Returns indices into the buffer, preserving order.
pub fn select(
    buffer: &ConversationBuffer,
    budget: &SelectionBudget,
    signals: &SelectionSignals,
) -> Vec<usize>;

// ─── Layer 3: Projector ─────────────────────────────────────────

/// Provider-specific wire format transformer.
/// One implementation per wire protocol family.
pub trait WireProjector: Send + Sync {
    /// Format selected buffer entries into the provider's wire format.
    /// Returns the complete request body as JSON.
    fn format_request(
        &self,
        system_prompt: &str,
        entries: &[&BufferEntry],
        tools: &[ToolDefinition],
        options: &ProjectionOptions,
    ) -> Value;

    /// Parse a provider's streaming response into a BufferEntry.
    /// Called when the stream completes to store the result in the buffer.
    fn parse_response(
        &self,
        text: String,
        thinking: Option<String>,
        tool_calls: Vec<WireToolCall>,
        raw: Value,
    ) -> BufferEntry;

    /// Map a canonical tool call ID to the provider's ID format.
    /// Used when the provider's response references tool call IDs.
    fn map_tool_id(&self, canonical_id: &str, provider_blob: Option<&ProviderBlob>) -> String;

    /// Provider family identifier (for logging/diagnostics).
    fn family(&self) -> &str;
}

/// Options for projection.
pub struct ProjectionOptions {
    pub model: String,
    pub reasoning: Option<String>,
    pub is_oauth: bool,
}
```

### Provider-specific projector implementations:

| Projector | Providers | Wire protocol |
|-----------|-----------|---------------|
| `AnthropicProjector` | anthropic | Messages API (content blocks, thinking, signatures) |
| `ChatCompletionsProjector` | openai, openrouter, groq, xai, mistral, cerebras, huggingface, ollama | OpenAI Chat Completions |
| `CodexResponsesProjector` | openai-codex | Responses API (input items, compound IDs) |

### Canonical tool call IDs

Critical design choice: the buffer stores **canonical IDs** (`omg_{short_uuid}`), NOT provider-specific IDs. Each projector maps canonical → provider format and back:
- Anthropic: `toolu_{base64}` (generated by Anthropic, stored in provider_blob)
- OpenAI: `call_{hex}` (generated by OpenAI, stored in provider_blob)
- Codex: `call_{hex}|fc_{n}` (compound, stored in provider_blob)

When projecting to a DIFFERENT provider than the one that generated the message, the projector uses the canonical ID directly (sanitized to match the target's regex). When projecting to the SAME provider, it can use the original provider ID from the blob for perfect round-tripping.

### Adversarial assessment — landmines and mitigations



### Landmine 1: Token counting divergence (CRITICAL)

The selector needs to pick a subset that fits the provider's context window. But token counts vary by provider:
- Different tokenizers (Claude tokenizer vs tiktoken cl100k vs o200k vs llama tokenizer)
- Same text can be 15-30% different in token count between providers
- The chars/4 heuristic is wrong for code (operators/short names inflate tokens), JSON (heavily tokenized), and CJK text

If the selector picks "200k tokens worth" using chars/4 but the provider counts 240k, we still get a 429. We've just moved the failure from storage to selection — same failure mode.

**Mitigation**: Two-pronged approach:
1. The WireProjector trait gets `fn estimate_tokens(&self, entries: &[&BufferEntry]) -> usize` — each projector provides provider-specific estimates. AnthropicProjector uses chars/3.5, ChatCompletionsProjector uses chars/4 (or embeds tiktoken-rs if accuracy matters).
2. The selector targets 80% of the budget, not 100%. The 20% margin absorbs tokenizer variance.
3. Track actual vs estimated per-request using provider-reported usage (see landmine 2) and calibrate the estimator over time.

### Landmine 2: Provider usage data is discarded (REAL BUG)

Every provider response includes actual token usage in message_delta (Anthropic) or the final chunk (OpenAI). We currently `tracing::trace!()` this and throw it away. This is the only source of ground truth for token counting, and we're not capturing it.

**Mitigation**: The streaming response parser should extract and return `Usage { input_tokens, output_tokens }` alongside the AssistantMessage. The loop stores this in the buffer entry. Over time, we can compare estimated vs actual and calibrate.

### Landmine 3: Tool definitions eat a fixed budget (MODERATE)

34 tools × ~500 tokens each = ~17k tokens consumed before any conversation. This is 8.5% of a 200k window. The selector must subtract this from the conversation budget, not treat the full context window as available for messages.

Additionally, tool descriptions are stripped/compacted by some projectors (AnthropicProjector strips parameter descriptions). The projector knows the actual tool token cost — this should feed into the budget calculation.

**Mitigation**: The projector computes tool token overhead: `fn tool_overhead(&self, tools: &[ToolDefinition]) -> usize`. The selector's budget is `context_window - system_prompt_tokens - tool_overhead - output_reserve - margin`.

### Landmine 4: Output token reservation (MODERATE)

Context window = input + output. If we fill 195k of a 200k window with input, the model can only generate 5k tokens — not enough for complex tool calls. The current `max_tokens: 16384` is hardcoded but not factored into the budget.

**Mitigation**: The selector reserves `max_tokens` (currently 16384) from the budget. This is already implicit in the "budget allocation" step of the projection architecture, but must be explicit in code.

### Landmine 5: Anthropic prompt caching conflict (LOW)

Anthropic caches the KV cache for repeated message prefixes, reducing cost. If the selector changes which messages are included between turns, cache hits drop. Dynamic selection could increase costs.

**Mitigation**: The selector should be STABLE — prefer extending the existing selection over reshuffling. A turn that was included last time should remain included if budget allows. This is a soft preference, not a hard constraint.

### Landmine 6: Response parsing lives in the bridge, not the projector (DESIGN TENSION)

The projector formats requests (clean, stateless). But response parsing is streaming and stateful (accumulate deltas → build complete message). Having the projector own both request AND response creates a coupling with the streaming state machine.

**Mitigation**: The projector only does final assembly: `parse_response(text, thinking, tool_calls, raw) → BufferEntry`. The streaming accumulation stays in the bridge's SSE parser. The bridge calls `projector.parse_response()` when the stream completes, passing the accumulated parts. Clean boundary: bridge owns the stream, projector owns the format.

### Landmine 7: Canonical ID mapping during tool dispatch (MODERATE)

When the model responds with tool calls, the provider assigns IDs (toolu_xxx, call_xxx). We need to:
1. Generate canonical IDs (omg_xxx)
2. Store the mapping in ProviderBlob
3. Dispatch tools using canonical IDs
4. Store tool_results with canonical call_ids
5. On next projection, map canonical → provider ID for same-provider round-tripping

This mapping is per-turn, per-assistant-message. If the projector generates the BufferEntry from the response (landmine 6 mitigation), it naturally generates the canonical IDs and stores the provider-to-canonical mapping in the blob. The loop dispatches using canonical IDs. The next projection reads the blob to recover the original provider IDs.

**Risk**: If the mapping is lost (blob is None after session resume), the projector falls back to canonical IDs. These MUST satisfy the target provider's regex. The `omg_` prefix + alphanumeric guarantees this.

### Landmine 8: Per-model billing and rate limits (LOW, EXTERNAL)

Token usage tracking needs to know: which model, which provider, how many input/output tokens, cached vs uncached. This is billing/observability data, not architectural. The buffer can store per-entry usage metrics without affecting the core design.

### NOT a landmine: session format migration

The constraint "old sessions must load" is achievable. The old format stores LlmMessage variants. A `BufferEntry::from_legacy()` converter can:
- Extract text/tool_calls from LlmMessage::Assistant
- Store the raw field as ProviderBlob (inferring provider from field structure)
- Generate canonical IDs deterministically from old call_ids (hash-based, so tool_result pairing survives)
- Detect old format by presence of "role" key in the JSON

### Revised WireProjector trait with token estimation and usage parsing

```rust
/// Provider-specific wire format transformer.
/// One implementation per wire protocol family.
pub trait WireProjector: Send + Sync {
    // ─── Request formatting ─────────────────────────────────────
    
    /// Format selected buffer entries into the provider's request body.
    fn format_request(
        &self,
        system_prompt: &str,
        entries: &[&BufferEntry],
        tools: &[ToolDefinition],
        options: &ProjectionOptions,
    ) -> Value;

    // ─── Response parsing ───────────────────────────────────────
    
    /// Convert streamed response parts into a canonical BufferEntry.
    /// Called when the stream completes.
    /// Generates canonical tool call IDs and stores provider IDs in the blob.
    fn parse_response(
        &self,
        text: String,
        thinking: Option<String>,
        tool_calls: Vec<WireToolCall>,
        raw: Value,
        turn: u32,
    ) -> BufferEntry;

    /// Extract token usage from the provider's response metadata.
    /// Called on message_delta (Anthropic) or final chunk (OpenAI).
    fn parse_usage(&self, response_meta: &Value) -> Option<Usage>;

    // ─── Token estimation ───────────────────────────────────────
    
    /// Estimate token count for buffer entries in this provider's format.
    /// Includes JSON structural overhead, content block wrappers, etc.
    fn estimate_tokens(&self, entries: &[&BufferEntry]) -> usize;

    /// Estimate token overhead for tool definitions in this provider's format.
    fn tool_overhead(&self, tools: &[ToolDefinition]) -> usize;

    /// Estimate token count for a system prompt.
    fn system_prompt_tokens(&self, prompt: &str) -> usize;

    // ─── ID mapping ─────────────────────────────────────────────
    
    /// Map a canonical tool call ID to this provider's format.
    /// Uses ProviderBlob for same-provider round-tripping.
    fn map_tool_id(
        &self,
        canonical_id: &str,
        blob: Option<&ProviderBlob>,
    ) -> String;

    /// Provider family identifier (for logging/diagnostics).
    fn family(&self) -> &str;
}

/// Token usage from a provider response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Anthropic: cache_read_input_tokens
    pub cache_read_tokens: Option<u32>,
    /// Anthropic: cache_creation_input_tokens
    pub cache_creation_tokens: Option<u32>,
}
```

### Budget computation in the selector:

```rust
fn compute_conversation_budget(
    projector: &dyn WireProjector,
    context_window: usize,
    system_prompt: &str,
    tools: &[ToolDefinition],
    max_output_tokens: usize,
) -> usize {
    let system_cost = projector.system_prompt_tokens(system_prompt);
    let tools_cost = projector.tool_overhead(tools);
    let output_reserve = max_output_tokens;
    let margin_factor = 0.80;
    
    let available = context_window
        .saturating_sub(system_cost)
        .saturating_sub(tools_cost)
        .saturating_sub(output_reserve);
    
    (available as f64 * margin_factor) as usize
}
```

This means the selector IS provider-aware via the projector's estimation methods, but only for token counting — never for message formatting. The selector's logic (which turns to include, structural integrity) remains provider-agnostic.

### Simplified token estimation — estimator lives on the buffer, not the projector

With self-calibrating estimation, token counting doesn't need to be provider-specific. The `TokenEstimator` lives on the `ConversationBuffer`, not the `WireProjector`.

The projector still needs `tool_overhead()` because the JSON structural overhead for tool definitions IS format-specific:
- Anthropic: `input_schema` wrapper + description + property schemas
- Chat Completions: `function` wrapper + `parameters` schema
- Codex: similar to Chat Completions + `strict: null`

But the overhead difference between formats is ~10%, well within the 20% margin. A single `chars/4` estimate for tool definitions works. The projector doesn't need `estimate_tokens()` or `system_prompt_tokens()`.

**Revised WireProjector trait** (simplified):

```rust
pub trait WireProjector: Send + Sync {
    fn format_request(
        &self,
        system_prompt: &str,
        entries: &[&BufferEntry],
        tools: &[ToolDefinition],
        options: &ProjectionOptions,
    ) -> Value;

    fn parse_response(
        &self,
        text: String,
        thinking: Option<String>,
        tool_calls: Vec<WireToolCall>,
        raw: Value,
        turn: u32,
    ) -> BufferEntry;

    fn parse_usage(&self, response_meta: &Value) -> Option<Usage>;

    fn map_tool_id(
        &self,
        canonical_id: &str,
        blob: Option<&ProviderBlob>,
    ) -> String;

    fn family(&self) -> &str;
}
```

Removed from projector: `estimate_tokens()`, `tool_overhead()`, `system_prompt_tokens()`. These all use the buffer's `TokenEstimator` with the shared chars/N ratio. One estimator, all providers, self-calibrating.

The selector's budget computation becomes:

```rust
fn compute_budget(
    estimator: &TokenEstimator,
    context_window: usize,
    system_prompt: &str,
    tools: &[ToolDefinition],
    max_output_tokens: usize,
) -> usize {
    let system_cost = estimator.estimate(system_prompt.len());
    // Tool defs: sum char lengths of names + descriptions + param schemas
    let tools_chars: usize = tools.iter().map(|t| t.char_count()).sum();
    let tools_cost = estimator.estimate(tools_chars);
    let available = context_window
        .saturating_sub(system_cost)
        .saturating_sub(tools_cost)
        .saturating_sub(max_output_tokens);
    (available as f64 * 0.80) as usize
}
```

Clean, universal, zero dependencies.

## Decisions

### Decision: Three-layer architecture: Buffer → Selector → Projector

**Status:** exploring
**Rationale:** The conversation path is split into three layers with clean interfaces:

### Decision: Provider-specific knowledge is quarantined to WireProjector implementations — one file per protocol family

**Status:** exploring
**Rationale:** Today, provider-specific knowledge is spread across:
- `providers.rs` (2349 lines) — credential resolution, HTTP clients, message builders, SSE parsers, tool formatters, response parsers, ALL interleaved
- `conversation.rs` — orphan stripping, role alternation (provider constraints leaking into conversation logic)
- `loop.rs` — error classification for provider-specific error messages

The new layout quarantines all provider knowledge:

```
core/crates/omegon/src/
  buffer.rs              # ConversationBuffer — provider-agnostic store
  selector.rs            # select() — budget-aware subset selection
  projection/
    mod.rs               # WireProjector trait definition
    anthropic.rs         # Anthropic Messages API format
    chat_completions.rs  # OpenAI-compatible format (covers 8 providers)
    codex_responses.rs   # Codex Responses API format
  providers/
    mod.rs               # Provider registry, credential resolution, routing
    anthropic.rs         # AnthropicClient (HTTP + auth, uses AnthropicProjector)
    openai.rs            # OpenAIClient (HTTP + auth, uses ChatCompletionsProjector)
    codex.rs             # CodexClient (HTTP + auth, uses CodexResponsesProjector)
    compat.rs            # OpenAICompatClient (base URL swap, uses ChatCompletionsProjector)
    openrouter.rs        # OpenRouterClient (OpenAI + model prefix, uses ChatCompletionsProjector)
```

When Anthropic adds a new content block type or changes their ID regex:
1. ONLY `projection/anthropic.rs` changes
2. The buffer, selector, loop, conversation, and all other providers are untouched
3. The change is isolated, testable, and reviewable in one file

When a new provider appears that speaks OpenAI Chat Completions:
1. Add a new `providers/newprovider.rs` with auth/HTTP
2. Wire it to `ChatCompletionsProjector` — zero projection code needed
3. Add it to the provider registry

The blast radius for any upstream API change is exactly one projection file.

### Decision: Canonical tool IDs generated by Omegon, provider IDs stored in ProviderBlob

**Status:** exploring
**Rationale:** The root cause of tool ID incompatibility (Codex pipes, Anthropic regex) is that we store the PROVIDER's ID as the canonical ID. Every downstream consumer must sanitize.

Instead: Omegon generates its own IDs (`omg_{short_uuid}`, guaranteed alphanumeric+underscore). The provider's original ID is stored in ProviderBlob.data for round-tripping. The projector maps canonical ↔ provider IDs:

- Same provider as generated: use original ID from blob (perfect fidelity)
- Different provider: use canonical ID (safe for all regexes)
- No blob (compacted/decayed): use canonical ID (always safe)

This eliminates sanitize_tool_id() entirely — correctness by construction instead of sanitization after the fact.

### Decision: Compaction becomes an optional cost-optimization layer, not a structural necessity

**Status:** exploring
**Rationale:** With the buffer holding everything and the selector managing what gets sent, compaction is no longer needed for survival. It becomes a token-cost optimization:

- The selector already picks a subset that fits the budget
- Old messages outside the budget are simply not selected — they're still in the buffer
- Compaction can optionally run to create summary waypoints — the selector includes these summaries instead of the raw messages when budget is tight
- If compaction fails (LLM error), nothing breaks — the selector just skips old messages without a summary

This eliminates:
- Emergency compaction (no longer needed — selection handles overflow)
- Compaction failure cascades (failure is harmless)
- Orphaned tool_results after compaction (buffer never drops data)

Compaction remains useful for COST: a summary of 50 old messages costs fewer tokens than decayed skeletons of those 50 messages. But it's an optimization, not a survival mechanism.

### Decision: Projection is stateless and re-computed per request — no caching across turns

**Status:** exploring
**Rationale:** Projection caching adds complexity for minimal gain. The projection step is O(n) over selected messages — sub-millisecond for typical sessions. The HTTP request + LLM response latency is 500ms-30s. Caching the projection saves microseconds while adding invalidation complexity (model change, thinking level change, tool set change, provider switch — all would invalidate).

Keep projection stateless: `fn project(entries, tools, options) → body`. Simple, testable, correct.

### Decision: Session persistence serializes the full buffer; ProviderBlobs are best-effort on resume

**Status:** exploring
**Rationale:** The full buffer (including ProviderBlobs) is serialized to the session JSON. On resume:

- Same provider: ProviderBlobs are valid — projector uses them for perfect round-tripping (thinking signatures, original tool IDs)
- Different provider: ProviderBlobs are ignored — projector uses canonical IDs and omits provider-specific features (thinking blocks without signatures)

This is strictly better than today where session resume with a different provider causes 400 errors. The buffer's provider-agnostic core (text, tool calls, tool results) always works. The blobs are a bonus for same-provider fidelity.

### Decision: Selection uses turn-atomic groups with budget-fit scoring, not individual message ranking

**Status:** exploring
**Rationale:** The selector operates on turn groups (user prompt + assistant response + tool results), not individual messages. A turn is the atomic unit — you can't include a tool_result without its tool_use, or an assistant reply without the user prompt it responds to.

Selection algorithm:
1. Mandatory window: last 3-5 turns always included (configurable)
2. Summary waypoints: if compaction summaries exist, include the most recent one as a synthetic user message
3. Referenced turns: turns whose tool results contain files/symbols mentioned in the last user prompt get a boost
4. Budget fill: remaining budget filled with turns working backwards from the mandatory window, preferring turns with file reads over turns with only text

Signals: user prompt keywords, recent_files from ContextManager, IntentDocument.files_modified. NO external retrieval (memory recall, embeddings) — the selector is fast and deterministic. Memory injection remains in the system prompt via the ContextManager, which is the right place for cross-session knowledge.

### Decision: Memory stays in the system prompt — the buffer is session-scoped, memory is cross-session

**Status:** exploring
**Rationale:** The rolling context buffer and the memory store serve different purposes:
- Buffer: what happened THIS session (conversation turns, tool calls, results)
- Memory: what's true ACROSS sessions (architecture facts, decisions, constraints)

Memory facts should NOT be injected as synthetic conversation messages — that would confuse the model about what it actually said vs what the harness told it. Memory stays in the system prompt via ContextManager injections, exactly as it works today.

The buffer and memory interact at one point: the selector can use memory-recalled file paths as relevance signals (boost turns that touched files the memory says are architecturally important). But memory content never enters the buffer.

### Decision: Token usage tracked per-entry from provider responses — estimator calibrates against actuals

**Status:** exploring
**Rationale:** Provider responses include actual token usage (input_tokens, output_tokens, cached). This is ground truth we're currently discarding.

New data flow:
1. SSE parser extracts usage from message_delta (Anthropic) or final chunk (OpenAI/Codex)
2. Usage is returned alongside the AssistantMessage from consume_llm_stream()
3. The buffer stores Usage on the BufferEntry for the assistant response
4. The selector can now compute: actual tokens used by turn N = sum of entry usages up to turn N
5. Over time, the ratio of actual/estimated gives a per-provider calibration factor

This gives us:
- Accurate budget targeting (use actual token counts from recent turns, not estimates)
- Per-session cost tracking (sum input_tokens × price + output_tokens × price)
- Per-provider calibration (if chars/4 consistently overestimates for Anthropic, adjust)
- Observability (which turns are expensive, where the tokens go)

The WireProjector trait gains:
```rust
fn estimate_tokens(&self, entries: &[&BufferEntry]) -> usize;
fn tool_overhead(&self, tools: &[ToolDefinition]) -> usize;
```

And the buffer entry gains:
```rust
pub struct Usage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_read_tokens: Option<u32>,
    pub cache_creation_tokens: Option<u32>,
    pub provider_id: String,
    pub model_id: String,
}
```

### Decision: Selector targets 80% of provider budget with margin for tokenizer variance

**Status:** exploring
**Rationale:** Token estimation can never be perfect — different tokenizers, different overhead for JSON wrapping, content block structure, etc. The selector should target 80% of the available conversation budget:

```
available = context_window - system_prompt_estimate - tool_overhead - output_reserve
target = available × 0.80
```

The 20% margin absorbs:
- Tokenizer variance between estimate and actual (typically 10-15%)
- JSON structural overhead added by the projector (content block wrappers, role/type fields)
- System prompt growth from context injections

If the provider reports actual usage and it's consistently under 70% of the window, the margin can tighten. If it's over 90%, the margin widens. Self-calibrating.

The output_reserve defaults to max_tokens (16384) and tool_overhead is computed by the projector's tool_overhead() method.

### Decision: No tokenizer libraries — self-calibrating estimation from provider-reported usage

**Status:** exploring
**Rationale:** Embedding model-specific tokenizers (tiktoken, SentencePiece, etc.) creates a per-model dependency treadmill. tiktoken-rs only covers OpenAI. The 8 providers behind Chat Completions use at least 4 different tokenizer families (tiktoken, SentencePiece, Qwen BPE, proprietary). Every new model or provider would require another tokenizer crate. Local inference models are completely open-ended — any GGUF on Ollama could have any tokenizer.

Instead: **chars/N baseline + provider-reported usage feedback → self-calibrating ratio.**

```rust
struct TokenEstimator {
    /// Chars-per-token ratio. Starts at 4.0 (conservative default).
    /// Updated after each provider response with actual usage.
    ratio: f64,
    /// Exponential moving average weight for ratio updates.
    alpha: f64,  // 0.3 — recent turns weighted more heavily
}

impl TokenEstimator {
    fn estimate(&self, char_count: usize) -> usize {
        (char_count as f64 / self.ratio) as usize
    }

    fn calibrate(&mut self, estimated_chars: usize, actual_tokens: u32) {
        let observed_ratio = estimated_chars as f64 / actual_tokens as f64;
        self.ratio = self.alpha * observed_ratio + (1.0 - self.alpha) * self.ratio;
    }
}
```

Behavior:
- **Turn 1**: Uses chars/4.0 with 80% budget margin. Conservative, safe.
- **Turn 1 response**: Provider reports `input_tokens: 1234` for text we estimated at 1100 tokens. Ratio adjusts.
- **Turn 3+**: Estimate converges to within 5% of actual for the current model.
- **Model switch**: Ratio resets to 4.0 (new model, new tokenizer).
- **Local inference**: Same approach. Ollama reports usage in its OpenAI-compat response.

This works for every model that exists today and every model that will exist tomorrow. Zero binary size increase. Zero maintenance burden. Zero provider-specific code for token counting.

The only provider that DOESN'T reliably report usage is... none. All major providers include usage in their response metadata. If a provider omits it, the estimator keeps its current ratio (graceful degradation).

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `core/crates/omegon/src/buffer.rs` (new) — NEW — ConversationBuffer (replaces ConversationState). Append-only message store with canonical tool IDs, IntentDocument, compaction summaries. Provider-agnostic.
- `core/crates/omegon/src/selector.rs` (new) — NEW — select() function. Budget-aware subset selection with structural integrity, relevance scoring, recency window.
- `core/crates/omegon/src/projection/mod.rs` (new) — NEW — WireProjector trait definition, ProjectionOptions, ProviderBlob.
- `core/crates/omegon/src/projection/anthropic.rs` (new) — NEW — AnthropicProjector. Content blocks, thinking signatures, tool_use/tool_result, OAuth name remapping, ID sanitization.
- `core/crates/omegon/src/projection/chat_completions.rs` (new) — NEW — ChatCompletionsProjector. role/content/tool_calls format. Covers openai, openrouter, groq, xai, mistral, cerebras, huggingface, ollama.
- `core/crates/omegon/src/projection/codex_responses.rs` (new) — NEW — CodexResponsesProjector. Input items, compound IDs, function_call/function_call_output.
- `core/crates/omegon/src/conversation.rs` (deleted) — DELETED after migration — replaced by buffer.rs. Current decay, orphan stripping, role alternation logic moves to selector.rs and projection/.
- `core/crates/omegon/src/providers.rs` (modified) — SPLIT — HTTP clients stay (credential resolution, reqwest, SSE parsing). Message builders (build_messages, build_input, build_tools) move to projection/. File shrinks from 2349 to ~1200 lines.
- `core/crates/omegon/src/loop.rs` (modified) — MODIFIED — replace build_llm_view() call with select() + project(). Emergency compaction/decay logic simplified (selector handles budget overflow by construction).
- `core/crates/omegon/src/bridge.rs` (modified) — MODIFIED — LlmMessage may be replaced or simplified. LlmBridge::stream() may take pre-projected Value instead of LlmMessage slice.

### Constraints

- ConversationBuffer must serialize/deserialize identically to current session format for backwards compatibility — old sessions must load into the new buffer
- WireProjector implementations must produce byte-identical output to current build_messages/build_input for same inputs — verified by snapshot tests before the old code is removed
- The selector must never produce structurally invalid output (orphaned tool_results, broken role alternation) — this is enforced by construction, not post-hoc validation
- ProviderBlob.data is treated as opaque by everything outside the originating projector — no code may inspect or depend on its contents except the projector that created it
- Canonical tool call IDs use a fixed format (omg_ prefix + alphanumeric) that satisfies ALL known provider regexes simultaneously

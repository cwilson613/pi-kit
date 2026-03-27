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

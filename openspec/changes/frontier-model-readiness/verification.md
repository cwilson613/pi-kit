# Verification

## Upstream evidence

- [GPT-6 Astra model specification](https://developers.openai.com/api/docs/models/gpt-6-astra): exact ID, API context 1,050,000, output 128,000, reasoning low/medium/high/xhigh/max and extended-context pricing threshold.
- [Astra migration guidance](https://developers.openai.com/api/docs/guides/latest-model): tool calls require Responses; unsupported sampling/logprob fields are omitted; none/minimal effort maps to low.
- [Stateless reasoning continuation](https://developers.openai.com/api/docs/guides/reasoning#preserve-reasoning-without-stored-responses): preserve output items including encrypted reasoning and assistant phase when replaying tool work.
- [Fable 5.1 specification](https://platform.claude.com/docs/en/models/fable-5-1/overview): current model, context/output and adaptive-thinking behavior match existing support. The live Anthropic drift check reported no current models missing.
- Installed Codex catalog: exact Astra slug is listed and supported, with `context_window=272000` and `max_context_window=872000`. Sanitized model-only evidence is retained; credentials were not copied. The subscription route uses this separate maximum, not the API ceiling. Codex-specific ultra delegation behavior is not exposed as an API effort option.

## Test-first evidence

Logs are retained outside Git in the sibling `omegon-frontier-model-evidence-01` directory.

| Boundary | Initial failing evidence |
| --- | --- |
| Registry | Astra was missing from native OpenAI routes |
| Transport | Tool-bearing Astra request targeted Chat Completions |
| Eligibility | Codex Astra access failure lacked route-specific guidance |
| Reasoning | xhigh did not parse, max collapsed to high, Astra used the short stall allowance |
| Admission | Embedded OpenAI HTTP metadata incorrectly entered custom-manifest secret admission |
| Continuation | A second request dropped encrypted reasoning and assistant phase |

## Review decisions

Independent review found the native/manifest admission boundary and lossy Responses continuation. The native exception must honor embedded ownership of endpoint transport, adapter and secrets, plus offering endpoint/native identity; overrides retain manifest admission. Opaque output replay is restricted to the same provider and exact wire model. Request extras cannot attach a different remote conversation.

Fifteen Pkl fixtures passed: xhigh/max accepted and preserved, ultra rejected, across Profile, TaskSpec, both AgentManifest settings locations and CodexIntegration. Existing lower-cost grades and explicit profile selections are retained. Fresh disconnected startup remains connection-first.

## Final gates and runtime

- The complete Omegon crate gate passed after the default/effort expectations were updated: 5,200 unit tests passed, 10 ignored, plus its integration checks.
- Fourteen focused Astra tests passed, including actual two-request HTTP continuation, empty-output fallback, partial-output rejection, model picker visibility, route ownership and reasoning levels.
- A final small compatibility adjustment retains the old High mapping for Max on legacy OpenAI/manual-Anthropic adapters and clamps GPT-OSS effort to its supported high ceiling. All 139 provider tests passed after that adjustment.
- Changed-crate Clippy, formatting, registry validation, and the live Anthropic model drift check passed.
- The earlier preview's generated Sonnet selection was backed up and removed using an exact pre-recorded file hash. Other project preferences remain unchanged; no credentials were cleared.

Installation and capture results follow after completion.

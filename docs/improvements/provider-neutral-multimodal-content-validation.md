+++
title = "Improvement: Provider-neutral multimodal content validation"
tags = ["bug", "improvement", "providers", "multimodal", "validation", "anthropic"]
+++

# Improvement: Provider-neutral multimodal content validation

**Status:** Confirmed provider-switch failure; ready for investigation  
**Suggested branch:** `fix/provider-neutral-content-validation`  
**Primary surfaces:** Canonical conversation, route selection, provider adapters

## Assignment brief

Define and enforce a provider-neutral content contract plus provider-specific capability and wire-shape validation. Prevent malformed requests when conversations containing images, tool results, or empty text move between providers. Validation must occur after route selection and every fallback/reroute, before network dispatch and provider token use.

## Observed evidence

After switching to Anthropic, a turn containing an image failed with:

```text
Anthropic 400 Bad Request: messages: text content blocks must be non-empty
```

The rendered turn contained an image and local path, but Anthropic received an empty text content block. Canonical content accepted on one route became structurally invalid under another adapter.

## Scope

- Canonical block invariants independent of providers.
- Provider capability declarations.
- Provider-safe projection and pre-dispatch validation.
- Image-only, mixed, attachment-only, and tool-result messages.
- Route switch, fallback, and reroute behavior.
- Typed diagnostics for unsupported or malformed content.

## Non-goals

- Redesigning all conversation storage.
- Fabricating text merely to satisfy a provider.
- Silently dropping images or unsupported blocks.
- Adding media transcoding unless separately justified.
- Provider-specific rules leaking into semantic conversation ownership.

## Investigation targets

Search canonical message/content types, image attachments, tool results, provider request builders, Anthropic content blocks, OpenAI/Codex Responses inputs, route/fallback execution, empty-string filtering, media support, local file resolution, and provider errors. Trace the exact screenshot turn from capture through canonical persistence to Anthropic JSON.

## Contract layers

### Canonical validation

Reject or normalize structurally meaningless blocks while preserving legitimate image-only and attachment-only messages. Empty text adjacent to valid non-text content should be omitted rather than converted into an invalid provider block.

### Capability negotiation

Each adapter declares supported roles, content kinds, media types, tool-result attachments, ordering constraints, and size/count limits. Route selection or dispatch compares the message projection against these capabilities.

### Wire validation

After serialization—but before network I/O—validate provider invariants such as Anthropic's non-empty text blocks. Diagnostics identify provider, message role/index, block index/type, and invariant without including attachment bytes or secrets.

## Design decision

Choose one bounded model:

1. a provider-safe intermediate projection created from canonical blocks and capabilities; or
2. adapter projection returning typed `UnsupportedContent`/`InvalidContentBlock` failures.

Either is acceptable if provider rules stay adapter-owned, canonical messages remain provider-neutral, and all routes use the same pre-dispatch gate.

## Security and data constraints

- Never include image bytes, data URIs, secret headers, or full private paths in diagnostics.
- Validate local attachment accessibility and media type before dispatch.
- Do not fetch arbitrary remote attachments during validation without explicit policy.
- Preserve conversation history even when one provider cannot consume a block; report incompatibility rather than mutate history destructively.

## Implementation sequence

1. Reproduce and capture the invalid Anthropic request shape in a test.
2. Document canonical block invariants and provider capability descriptors.
3. Add canonical validation at message construction/projection boundaries.
4. Add adapter conversion errors instead of silent dropping/coercion.
5. Add post-serialization provider-wire validation.
6. Invoke validation after route selection and every fallback/reroute.
7. Project typed local diagnostics to all surfaces.
8. Complete the cross-provider regression matrix.

## Acceptance criteria

1. No adapter emits a provider-forbidden empty text block.
2. Valid image-only and mixed messages reach capable providers intact.
3. Tool-result images are preserved where supported and rejected explicitly otherwise.
4. Provider switching never silently drops or fabricates content.
5. Validation reruns when fallback changes the provider.
6. Unsupported content fails locally before network dispatch/token use.
7. Diagnostics locate the invalid role/block and provider without leaking data.
8. Canonical history remains unchanged after a route incompatibility.
9. OpenAI/Codex-to-Anthropic and Anthropic-to-OpenAI/Codex continuations are covered.

## Regression plan

Cover the following matrix:

- Image-only user message to Anthropic.
- Mixed text/image to Anthropic.
- Tool-result image to Anthropic.
- OpenAI/Codex conversation continued under Anthropic.
- Anthropic conversation continued under OpenAI/Codex.
- Empty canonical text adjacent to a valid image.
- Fallback after the original adapter accepted another shape.
- Unsupported media type.
- Missing/inaccessible local attachment.
- Adapter that does not support tool-result images.

Tests should assert both canonical preservation and exact provider wire shape; avoid live provider calls for structural validation.

## Validation

Run focused conversation/provider adapter tests plus:

```bash
cargo test -p omegon <multimodal-provider-filter>
just clippy-changed
git diff --check
```

Live smoke is optional and must not replace deterministic request-shape tests.

## Dependencies and conflict risks

This intersects provider adapters, semantic conversation projection, route fallback, and image/tool-result handling. Coordinate with existing image-preservation fixes. Avoid parallel incompatible changes to canonical message enums or provider request builders.

## Definition of done

Canonical and provider-wire invariants are explicit, all dispatch paths validate after final route selection, unsupported content returns typed local errors, the full deterministic matrix passes, no content is silently lost, and focused validation is clean.

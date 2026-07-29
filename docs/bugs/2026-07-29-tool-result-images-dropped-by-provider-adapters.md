+++
title = "Bug: Tool-returned images are discarded by multimodal provider adapters"
tags = ["bug","multimodal","images","providers","openai","codex","tool-results","regression"]
+++

# Bug: Tool-returned images are discarded by multimodal provider adapters

# Bug: Tool-returned images are discarded by multimodal provider adapters

**Date:** 2026-07-29
**Status:** Confirmed; fix not yet implemented
**Severity:** High
**Affected routes:** OpenAI Codex Responses and OpenAI-compatible chat completions
**Observed model:** `openai-codex:gpt-5.6-sol`

## Summary

Omegon correctly reads image files, represents them as multimodal tool results, and stores them in the canonical conversation. The OpenAI Codex provider adapter then silently discards those image attachments while constructing the next inference request.

As a result, a multimodal model receives only a textual placeholder such as:

```text
[image output: image/png at /Users/wilson/Downloads/tmp.png]
```

It does not receive the image pixels. The assistant may consequently behave as if OCR or another external image-processing mechanism is required, even though the selected model and provider route support image input.

The ordinary OpenAI chat-completions adapter has the same structural defect: images attached directly to user messages are serialized, while images attached to tool results are ignored.

## Operator-visible incident

The operator asked the assistant to explain `/Users/wilson/Downloads/tmp.png`. The assistant invoked the image-capable `view` tool, but did not analyze the visible screenshot. It instead attempted image preprocessing, searched for OCR tooling, and delegated to a local model. The operator ultimately had to transcribe the screenshot manually.

This behavior was not caused by a lack of image capability in the selected model. The image was lost inside Omegon between canonical conversation projection and provider-specific request serialization.

## Expected behavior

When an image-capable tool returns a `ContentBlock::Image`:

1. Omegon preserves the image in the canonical conversation.
2. The active provider adapter includes the image in the next inference request using that provider's supported image-input representation.
3. The model can inspect the image directly.
4. If the active route cannot accept tool-returned images, Omegon reports that limitation explicitly before inference instead of silently reducing the image to text.

## Actual behavior

1. `view` reads the image and returns a base64 data URI in `ContentBlock::Image`.
2. Conversation projection creates an `ImageAttachment` and stores it in `LlmMessage::ToolResult.images`.
3. `CodexClient::build_input` matches the tool result with `..`, ignores `images`, and emits only a textual `function_call_output`.
4. `OpenAIClient::build_wire_messages` likewise ignores images on tool results and emits only a textual tool message.
5. The inference endpoint receives the placeholder text but no image content.

## Confirmed data path

### 1. The `view` tool produces an image content block

`core/crates/omegon/src/tools/view.rs:88-125` reads the file, determines its media type, encodes it as a base64 data URI, and returns both text and image blocks:

```rust
ToolResult {
    content: vec![
        ContentBlock::Text {
            text: file_header(path),
        },
        ContentBlock::Image {
            url: data_uri,
            media_type: mime.into(),
        },
    ],
    // ...
}
```

This establishes that image acquisition is working.

### 2. Canonical conversation projection preserves the image

`core/crates/omegon/src/conversation.rs:1925-1952` converts image content blocks into `ImageAttachment` values:

```rust
omegon_traits::ContentBlock::Image { media_type, .. } => {
    if let Some(image) = ImageAttachment::from_content_block(block, source_path.clone()) {
        images.push(image);
    }
    text_blocks.push(format!("[image output: ...]"));
}
```

`core/crates/omegon/src/bridge.rs:44-79` defines `LlmMessage::ToolResult` with a dedicated image collection:

```rust
ToolResult {
    call_id: String,
    tool_name: String,
    content: String,
    images: Vec<ImageAttachment>,
    is_error: bool,
    args_summary: Option<String>,
}
```

The canonical representation is therefore capable of carrying the pixels.

### 3. The Codex adapter drops tool-result images

`core/crates/omegon/src/providers.rs:2841-2854` serializes a tool result as follows:

```rust
LlmMessage::ToolResult {
    call_id, content, ..
} => {
    input.push(json!({
        "type": "function_call_output",
        "call_id": cid,
        "output": content
    }));
}
```

The `..` discards the `images` field. No `input_image` is added to the Responses API input.

The same function correctly serializes images attached to ordinary user messages as `input_image`. The defect is therefore specifically the missing provider adaptation for images originating in tool results—not general absence of Codex image support.

### 4. The OpenAI chat-completions adapter has the same asymmetry

`core/crates/omegon/src/providers.rs:1815-1849` emits user-message images as `image_url` blocks, but its `LlmMessage::ToolResult` arm ignores all fields except `call_id` and `content`:

```rust
LlmMessage::ToolResult {
    call_id, content, ..
} => {
    wire_msgs.push(json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": content
    }));
}
```

Tool-returned images are therefore lost on this route as well.

## Root cause

The canonical message model supports images on both user messages and tool results, but provider serialization was implemented only for user-message images. Provider adapters treat tool output as text-only, despite the richer canonical type.

This is an interface-contract mismatch:

- **Producer contract:** tools may return `ContentBlock::Image`.
- **Conversation contract:** tool results may contain `ImageAttachment` values.
- **Provider-adapter assumption:** tool results contain only text.

The adapter does not reject the unsupported shape or report capability loss. It silently degrades the image to placeholder text, making the defect difficult to distinguish from model incompetence.

## Contributing behavior defect

After receiving only the placeholder, the assistant attempted OCR and local-model workarounds instead of recognizing that a multimodal tool result had not reached inference.

The harness should make modality loss observable. A model cannot reliably infer from placeholder text whether the image was intentionally omitted, unsupported by the provider, removed by context decay, or lost by an adapter defect.

## Required remediation

### 1. Preserve image content across tool-result serialization

For OpenAI Codex Responses:

1. Emit the required `function_call_output` for the tool call.
2. If the tool result contains images, append a provider-supported message containing `input_image` blocks.
3. Include bounded provenance text associating the promoted images with the originating tool and call ID.

For OpenAI chat completions:

1. Preserve the tool-role response required by tool-call semantics.
2. Promote tool-result images into a subsequent image-capable user message using `image_url` blocks, unless the endpoint supports multimodal tool content directly.
3. Preserve ordering so the model can associate image content with the correct tool result.

Provider-specific formats must be verified against their actual API contracts rather than assuming that image blocks are legal directly inside tool-output payloads.

### 2. Add explicit modality-loss detection

Before sending a request, compare canonical message modalities with the serialized provider request. If the canonical conversation contains images that the selected route cannot encode, return a typed preflight error such as:

```text
The selected provider route cannot transmit images returned by tools.
Choose an image-capable route or attach the image directly to the prompt.
```

Silent replacement with a placeholder is not acceptable.

### 3. Record modality provenance

Diagnostics should distinguish:

- user-attached images;
- tool-returned images;
- image metadata retained only for display;
- images transmitted to the provider;
- images omitted due to route capability or context policy.

### 4. Correct fallback guidance

If image content was not transmitted, the assistant should not be encouraged to improvise OCR tooling as though the model lacked vision. The system should surface the transport failure and offer a route change or direct attachment promotion.

## Acceptance criteria

1. A PNG returned by `view` survives as an `ImageAttachment` in `LlmMessage::ToolResult`.
2. `CodexClient::build_input` emits an `input_image` carrying the same MIME type and base64 payload.
3. The Codex request preserves the associated `function_call_output` and valid call ordering.
4. `OpenAIClient::build_wire_messages` emits an image-capable message for tool-result images while preserving tool-call protocol validity.
5. Multiple images from one tool result retain their order.
6. Text and images from one tool result remain associated through explicit provenance text or provider-native structure.
7. Text-only tool results produce the same wire representation as before.
8. Unsupported providers return a typed modality-preflight error rather than silently dropping images.
9. Context compaction and session persistence either preserve tool-result images or explicitly report their removal.
10. An integration test asks an image-capable model to identify deterministic content in a fixture image returned by `view`; the response must demonstrate that pixels reached inference.

## Required tests

### Unit tests

- `view_image` returns one text block and one image block with the expected MIME type.
- `ImageAttachment::from_content_block` preserves base64 data, MIME type, and source path.
- Conversation projection includes both placeholder text and the image attachment.
- Codex serialization promotes tool-result images to `input_image`.
- OpenAI serialization promotes tool-result images to `image_url`.
- Multiple tool-result images are serialized in stable order.
- Text-only tool results remain unchanged.

### Contract tests

Validate generated provider payloads against captured or documented API shapes for:

- Codex Responses `function_call_output` followed by image-bearing input;
- OpenAI chat-completions tool output followed by multimodal user content.

### End-to-end regression test

Use a small deterministic fixture containing large text and simple geometric features. Invoke `view`, then ask the model to transcribe or identify those features. Assert that the provider request contains image data; model-output quality may be a secondary assertion rather than the sole transport test.

## Security and operational constraints

- Do not log base64 image payloads.
- Diagnostic logs may include MIME type, byte count, source classification, and a non-reversible content hash.
- Preserve existing workspace-boundary checks for local files.
- Avoid duplicating large images unnecessarily in persisted transcripts.
- Account for image payload size separately from text-character token estimates.

## Impact

- Multimodal models appear unable to inspect images returned by tools.
- Agents waste time invoking OCR, preprocessing, shell utilities, or delegated models.
- Operators may be forced to transcribe image content manually.
- Repeated `view` calls cannot recover because every call traverses the same lossy adapter.
- The TUI can display the image while the inference model receives no pixels, creating a misleading discrepancy between operator-visible and model-visible state.

## Workaround

Until the adapter is fixed, attach the image directly to the user prompt through a path that creates `LlmMessage::User.images`. Direct user-image serialization is already implemented for the affected OpenAI routes.

This workaround is incomplete: it depends on the active surface exposing direct attachment and does not repair tool-produced diagrams, screenshots, renders, or other image artifacts.

## Conclusion

The selected model was multimodal and the `view` tool successfully produced image data. Omegon discarded that data in provider adaptation because tool-result images were never serialized. The correct bugfix is to preserve or explicitly reject multimodal tool results at the provider boundary, with tests proving that image bytes—not merely placeholder text—reach inference.

# Stable external client API direction

Status: active direction

## Intent

Omegon should not require the Ratatui TUI to be the only practical operator interface. The goal is not to remove the TUI; the goal is to make the TUI prove it is one client of a stable interface boundary. That practice yields operational benefits: clearer seams, transport reuse, lower coupling, and easier automation/client testing.

## Boundary model

The intended dependency direction is:

```text
frontend adapters -> ui_runtime / surfaces / operator_commands -> runtime execution
```

- `operator_commands` owns inbound operator command envelopes and `InterfaceControlRequest`.
- `ui_runtime` owns renderer-neutral UI action/outcome contracts.
- `surfaces` owns outbound semantic projections.
- `control_runtime` executes control requests but must not be the API a frontend names.
- TUI/Web/IPC/ACP are adapters, not owners of command shape.

The local guardrail is:

```bash
just check-interface-boundary
```

It asserts semantic surfaces stay renderer/backend-neutral and frontend entrypoints name the interface boundary instead of backend request types.

## External client v1 root

`core/crates/omegon/src/ui_runtime/client_api.rs` defines the first stable external API root:

- `CLIENT_API_VERSION = 1`
- `ClientEnvelope`
- `ClientEnvelopeDirection`
- `ClientEnvelopeKind`
- `ClientCapabilityHello`

This is intentionally a small versioned envelope with generic JSON payloads. It stabilizes protocol/version/routing semantics before freezing every internal Rust command variant as a public wire DTO.

Example control request envelope:

```json
{
  "protocolVersion": 1,
  "envelopeId": "env-1",
  "sessionId": "session-1",
  "clientId": "replacement-ui",
  "direction": "clientToRuntime",
  "kind": "controlRequest",
  "payload": {
    "name": "contextStatus"
  }
}
```

Example capability hello:

```json
{
  "clientName": "replacement-ui",
  "clientVersion": "0.1.0",
  "protocolVersions": [1],
  "surfaces": ["conversation", "footer", "dashboard"],
  "commands": ["controlRequest", "uiAction"]
}
```

## Replacement-client integration path

A replacement client should:

1. Negotiate `ClientCapabilityHello`.
2. Subscribe to semantic surface projections rather than scrape TUI state.
3. Emit operator intent through versioned `ClientEnvelope` messages.
4. Use `operator_commands::InterfaceControlRequest` as the in-process semantic target, or a transport-specific DTO that maps into it.
5. Treat `control_runtime` as runtime execution internals, not a client API.

## Near-term next slices

1. Add typed DTOs for the most common control requests behind `ClientEnvelopeKind::ControlRequest`.
2. Add a transport adapter that validates `ClientEnvelope` version/kind before dispatch.
3. Add golden JSON fixtures for v1 envelopes.
4. Move mature DTOs toward a dedicated interface crate once the shape stabilizes.

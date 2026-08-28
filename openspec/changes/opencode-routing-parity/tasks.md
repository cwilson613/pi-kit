## 1. Strict route admission
<!-- specs: provider-routing/parity -->

- [x] Add failing tests for unknown exact identities, disabled offerings, and tool-deficient exact routes.
- [x] Make exact selection resolve and validate the current inventory before bridge replacement or dispatch.
- [x] Remove execution-boundary fallback of unknown model identities to Anthropic while preserving declared aliases.
- [x] Verify failed switches preserve the active route and return actionable diagnostics.

## 2. Deterministic provider preference
<!-- specs: provider-routing/parity -->

- [ ] Add failing routing tests where equally eligible candidates differ only by configured provider order.
- [ ] Apply provider order as a deterministic post-admission score or tie-break.
- [ ] Verify avoidance, provider-only, credential, and grade filters remain stronger than preference.

## 3. Credential provenance
<!-- specs: provider-routing/parity -->

- [ ] Add failing tests for API-key and OAuth environment variables selected by the credential ledger.
- [ ] Classify the selected environment credential using declared provider authentication semantics.
- [ ] Verify route metadata distinguishes OAuth from API-key environment execution.

## 4. Server-directed retry timing
<!-- specs: provider-routing/parity -->

- [ ] Add failing tests for `Retry-After-Ms`, numeric seconds, HTTP dates, malformed values, and missing headers.
- [ ] Carry bounded retry timing through structured upstream failure data without exposing raw headers.
- [ ] Use valid server timing in central scheduling while retaining durable attempt facts and exhaustion ceilings.

## 5. Model-level capability gates
<!-- specs: provider-routing/parity -->

- [ ] Add failing tests for unsupported tools, unsupported reasoning, insufficient evidence, and supported dialect normalization.
- [ ] Gate tool-bearing and explicit-reasoning requests against the admitted offering.
- [ ] Preserve provider contribution authority over schema dialect and normalization.

## 6. Executable manifest endpoints
<!-- specs: provider-routing/parity -->

- [ ] Add failing tests for supported and unsupported manifest adapters, secret resolution, and native model aliases.
- [ ] Construct an OpenAI-compatible bridge only from an admitted supported HTTP endpoint.
- [ ] Preserve selected offering, native model, endpoint, credential, and contribution-generation provenance.
- [ ] Verify unsupported adapters fail closed without network dispatch.

## 7. Verification and documentation
<!-- specs: provider-routing/parity -->

- [ ] Validate the OpenSpec change and reconcile every scenario with implementation evidence.
- [ ] Run focused provider, route, inventory, retry, and credential tests while iterating.
- [ ] Run `just test-crate omegon` and `just clippy-changed` as the landing gate.
- [ ] Update `[Unreleased]` and operator/contributor documentation for observable routing behavior.

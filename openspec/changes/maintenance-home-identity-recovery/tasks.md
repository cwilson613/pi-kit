# Implementation tasks

## 1. Establish the recovery contract
<!-- specs: home-identity -->

- [ ] Identify the cause of this machine's mismatch and inventory every dependent identity/key.
- [ ] Resolve continuity evidence and active-session/quiescence requirements.
- [ ] Add red tests for mismatched identity, preserved deny policy and interrupted recovery.

## 2. Implement and verify recovery
<!-- specs: home-identity -->

- [ ] Implement the explicit maintenance transaction with existing locks and audit records.
- [ ] Verify replay/rollback, unchanged-home behavior and refusal for unproven continuity.
- [ ] Recover the observed installation through the supported command, then finish catalog/extension installation and verify real-home admission without GUI launches.

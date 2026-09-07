# Implementation tasks

## 1. Establish the recovery contract
<!-- specs: home-identity -->

- [x] Inventory dependent keys and resolve explicit legacy rebind, stable evidence, and quiescence requirements; historical cause remains unknown.
- [x] Record red tests for CLI recovery and cached admission; add regression coverage for preserved denies, admission contention, interrupted recovery, tampering and replay.

## 2. Implement and verify recovery
<!-- specs: home-identity -->

- [x] Implement descriptor-bound recovery with bootstrap/domain/audit locking and immutable intent plus atomic resumable phase journal.
- [x] Add supported stable-volume continuity without weakening path/inode or descriptor race checks.
- [x] Verify unchanged-home behavior, all interrupted phases, busy guards, active transactions/fences, and refusal for unproven continuity.
- [ ] Recover the observed installation through the supported command, finish catalog/extension installation, and verify real-home admission without GUI launches.

Focused evidence: `/tmp/omegon-recovery-maint-final.log` (nine unit recovery
regressions and four CLI recovery cases), `/tmp/omegon-recovery-maint-contracts-all.log`
(43 protocol tests). Full companion landing gates and real installation recovery
remain separate final validation.

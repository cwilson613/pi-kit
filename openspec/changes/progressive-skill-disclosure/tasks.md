## 1. Disclosure domain model
<!-- specs: skills/progressive-disclosure -->

- [x] 1.1 Add `SkillDisclosureTier`, `SkillDisclosureEntry`, and `SkillDisclosure` to `omegon-skills`, carrying name, description, activation, signal-match state, and admission decision.
- [x] 1.2 Implement deterministic admission per the activation table, including the never-admit path for absent/unknown activation.
- [x] 1.3 Implement workspace signal matching as existence-only literal-or-shallow-glob resolution with no content reads.
- [x] 1.4 Unit-test each activation variant, the unlabelled case, and the name-inference rejection case.

## 2. Retrieval-key lint
<!-- specs: skills/progressive-disclosure -->

- [x] 2.1 Add description quality lint rejecting missing, sub-24-character, and placeholder descriptions.
- [x] 2.2 Surface findings through the existing skill doctor report.
- [x] 2.3 Test that every bundled skill in `skills/*/SKILL.md` passes the lint.

## 3. Inventory adapter
<!-- specs: skills/progressive-disclosure -->

- [x] 3.1 Extend the installed-skill inventory in `omegon/src/skills.rs` to expose bodies for admitted entries instead of discarding them.
- [x] 3.2 Build the projection from installed skills plus workspace evidence and the current operator prompt.
- [x] 3.3 Test that unmatched bundled skills are resident-only in a Rust-only workspace.

## 4. Verification
<!-- specs: skills/progressive-disclosure -->

- [x] 4.1 Run focused skills tests plus crate checks and lint.
- [x] 4.2 Update `CHANGELOG.md` `[Unreleased]`.
- [x] 4.3 Reconcile lifecycle state and commit.

## Remaining

None. Retrieval-key lint findings are emitted per external skill bundle and counted in the operator-facing `omegon skills doctor` summary.

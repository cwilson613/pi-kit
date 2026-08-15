---
state: implementing
---
# Progressive Skill Disclosure

## Intent

Document and reconcile the completed progressive skill-disclosure implementation so prompt admission is deterministic, workspace signals are checked without reading file contents, installed skill bodies remain available for admitted entries, and unusable retrieval descriptions are reported through skill diagnostics.

## Scope

- Model disclosure tiers and admission decisions in `omegon-skills`.
- Admit skills from explicit activation metadata and current workspace/operator evidence.
- Match project signals using existence-only literal or shallow-glob checks.
- Preserve installed skill bodies for entries admitted into prompt context.
- Lint missing, placeholder, or underspecified skill descriptions and surface findings through `omegon skills doctor`.

## Out of scope

- Semantic or content-based workspace scanning for activation.
- Inferring activation from a skill name when activation metadata is absent.
- Automatically rewriting third-party skill manifests.

## Success criteria

- Activation variants produce deterministic disclosure decisions.
- Unknown or absent activation metadata does not silently admit a skill.
- Workspace signal checks do not read matched file contents.
- Admitted installed skills expose their bodies to prompt construction.
- Bundled skills pass retrieval-key lint and external findings appear in doctor output.

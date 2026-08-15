# Binary composition inventory and kernel admission criteria

Measured on branch `refactor/minimal-default-binary` on 2026-08-14.

## Current composition

| Measure | Default interactive | Headless (`--no-default-features`) |
|---|---:|---:|
| Unique normal dependency tree lines | 778 | 620 |
| Local debug artifact | 236 MiB | not measured separately |
| Local release artifact | 39 MiB | not measured separately |

The TUI feature accounts for 158 additional unique dependency-tree lines. This is a useful compatibility boundary, but it is not yet a product boundary: the headless artifact still admits the provider stack, control plane, lifecycle engines, web server, plugin/skill management, archive/signature support, and other optional operational domains.

Compile-time content embedded directly in the main binary is approximately 332 KiB of source material:

| Content family | Source size |
|---|---:|
| `skills/` | 104 KiB |
| `catalog/` | 88 KiB |
| `data/` | 76 KiB |
| `pkl/` | 48 KiB |
| `prompts/` | 16 KiB |

This content is small relative to native code, but embedding it is architecturally significant: it makes contribution packs part of the kernel release cadence and prevents replacement without rebuilding the binary.

## Kernel admission criteria

Code or content belongs in the default binary only when all of the following hold:

1. **Universal execution:** every supported product mode needs it to start, admit work, enforce safety, or execute the provider-neutral agent loop.
2. **Kernel-owned contract:** removing it would break a stable runtime protocol rather than remove an optional workflow, renderer, integration, or content pack.
3. **Failure isolation:** its initialization cannot require optional credentials, external daemons, platform services, or mutable contribution state.
4. **Replacement neutrality:** downstream packs can extend behavior through a renderer-neutral/provider-neutral contract without linking their implementation into the kernel.
5. **Measured justification:** binary/dependency cost is recorded and accepted when a smaller interface cannot provide the capability.

A component failing any criterion defaults to an external contribution pack, optional feature artifact, or separate companion binary.

## Classification

### Kernel

- provider-neutral agent loop and work admission
- command/projection contracts shared by TUI, ACP, daemon, and IPC
- capability/RBAC enforcement and secret redaction boundaries
- configuration loading and stable protocol schemas
- minimal filesystem/process/network tools required by the core coding-agent contract
- extension discovery and contribution-pack contract (not bundled contributions)

### Optional artifact or contribution pack

- terminal renderer and image/syntax presentation stack
- embedded dashboard assets and operational dashboards
- local embeddings/ONNX runtime
- managed-agent integrations and platform-specific transports
- OCI signing/archive workflows
- MQTT/voice/chat integrations
- lifecycle methodologies, language conventions, personas, prompts, and catalog agents
- demo projects and onboarding content

## First extraction slice

Extract bundled skills from the binary into an installed **contribution pack directory** while preserving the existing `~/.omegon/skills` discovery contract.

Why this slice first:

- the interface already exists and is filesystem-based;
- `just link` already installs bundled skills/catalog content;
- no runtime protocol redesign is required;
- removing `include_str!` makes content independently replaceable and versionable;
- failure is bounded: missing packs degrade inventory/install commands, not the agent kernel.

The default binary will discover shipped skills from a compile-time path supplied by packaging, then user and project roots. It will not embed skill markdown. Packaging remains responsible for installing the shipped pack.

## External-agent interoperability

Existing skills owned by other coding agents are not implicit Omegon search roots. Directly reading mutable Claude, Codex, Cursor, or similar directories would couple runtime behavior to foreign precedence, trust, metadata, lifecycle, and path-resolution semantics.

The supported boundary is explicit import through a format adapter:

```text
external-agent skill → discover → adapt/validate/preview → Omegon user or project skill
```

### Ownership layers and precedence

1. **Shipped contribution pack** — immutable vendor baseline under the packaged contribution root; upgrades may replace it.
2. **Operator-owned Omegon skills** — `~/.omegon/skills`; never overwritten by contribution-pack installation.
3. **Project-owned Omegon skills** — `<project>/.omegon/skills`; highest local precedence.
4. **External-agent sources** — imported explicitly; never silently admitted as runtime instruction roots.

Resolved precedence remains project → user → extension → shipped. An operator copy may shadow a shipped skill without modifying vendor content.

### Import modes

- **Copy** is the default: import the complete reviewed bundle into an Omegon-owned root, including validated relative assets and scripts. External changes cannot silently alter Omegon behavior.
- **Link** is an explicit opt-in: retain an externally managed canonical source, mark it as such in inventory, validate containment, and surface broken/unavailable state. Omegon does not modify the source.

Both modes require collision handling before mutation. Imports never silently overwrite an operator-owned skill. A shipped name may be shadowed only after the resulting precedence is previewed.

### Adaptation and provenance

Adapters map only fields with verified semantic equivalence. Unknown provider metadata, tool restrictions, hooks, model selectors, and script semantics are preserved as provenance or reported as unsupported; they are not reinterpreted silently.

Imported skills retain source kind, provider, canonical source path, import mode, source digest, and adapter version in an Omegon-owned provenance record. This supports deterministic status, diff, and refresh operations. Refresh must show a diff and require an explicit conflict decision when the imported copy has local edits.

### Reverse interoperability

Export uses provider-specific adapters into a staging destination. Omegon does not write automatically into another agent's managed directories. The transformed bundle is validated and reviewed before the operator installs it for that agent.

### Security and failure semantics

- canonical-path containment applies to linked roots and referenced assets;
- traversal-bearing names and escaping symlinks are rejected;
- executable hooks/scripts require explicit safety review rather than metadata translation;
- missing external sources degrade the imported entry to unavailable and do not prevent native skill discovery;
- uninstalling another agent cannot remove an Omegon-owned copied import;
- contribution-pack upgrades never mutate user/project imports or provenance.

### Deferred implementation contract

Before external-agent import is implemented, specify a provider-neutral `SkillImportSource`, adapter diagnostics, provenance schema, collision preview, copy/link settlement, and refresh state machine. The current contribution-pack extraction must preserve these extension points but must not add foreign directory scanning as a shortcut.

## Success criteria

- no `include_str!(...skills/*/SKILL.md)` remains in non-test Omegon code;
- `omegon skills list/install` reads a deterministic shipped-pack manifest/directory;
- missing shipped content returns an actionable error and does not panic;
- project/user skill discovery and override precedence remain unchanged;
- `just link` and package workflows install the contribution pack;
- default and headless compile matrices remain green.

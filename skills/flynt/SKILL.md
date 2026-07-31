+++
id = "8d7961f6-4742-416f-89eb-bef9f6cc12f6"
name = "flynt"
description = "Interlinked markdown conventions for Flynt workspaces and knowledge bases"
tags = []
aliases = []
activation = "domain_detected"
profile = ["docs"]
project_signals = ["*.md", "docs/**/*.md"]
+++

# Flynt Skill — Interlinked Markdown Conventions

Write markdown that renders beautifully in Flynt, mdserve, Obsidian, and GitHub.

## Wikilink Syntax

Use `[[wikilinks]]` to create navigable connections between documents:

```markdown
See [[vision]] for the big picture.
Related: [[design-tree|Design Exploration Tree]]
```

- `[[target]]` — links to the file whose slug matches `target`
- `[[target|Display Text]]` — links with custom display text
- Slugs are case-insensitive, spaces become hyphens
- Both filename-only (`[[vision]]`) and path slugs (`[[docs/vision]]`) resolve

Unresolved wikilinks render as styled concept references (italic, muted) — they're safe to use as forward references or concept tags.

## Workspace Documents

Use the workspace's existing organization and frontmatter conventions. For a new
Flynt note, include a title when the active surface or project template does not
supply one:

```yaml
---
title: Architecture Decision Record
status: decided
tags: [architecture, storage]
---
```

Common fields include `title`, `status`, `tags`, and `date`, but structured
surfaces may manage their own metadata. Do not add generic frontmatter to
Excalidraw wrappers, design-board wrappers, generated files, or established
project documents that use a different schema.

Choose the artifact that matches the operator's active Flynt surface:

- markdown notes for durable prose and linked knowledge;
- D2 for text-authored structural diagrams;
- Excalidraw for freeform drawings;
- design boards for component/layout exploration;
- flow graphs for editable node-and-edge workflows.

Use `flynt_surface_guide` when that capability is exposed and the correct
surface is unclear. When the operator refers to "the open document" or "what I
have open," use `get_ui_state` before asking them to identify it.

## File Organization

Respect the existing workspace hierarchy. Typical durable locations include:

```text
project/
  docs/               # long-lived project documentation
  openspec/           # lifecycle changes and specifications, when enabled
  drawings/           # Excalidraw assets and their markdown wrappers
  diagrams/           # text-authored D2 sources
  boards/             # Flynt design-board artifacts and wrappers
```

Do not invent legacy `ai/design` or generated memory directories. Use the
surface-specific creation tools when available because they preserve wrappers,
indexes, and sidecar contracts.

## Graph-Friendly Patterns

- Link related durable documents where the relationship is useful.
- Prefer meaningful links over reciprocal-link quotas.
- Use hub/index pages only when they improve navigation.
- Follow the repository's filename conventions; use kebab-case only when no
  local convention exists.

## Viewing and Navigation

Flynt's native notes, graph, kanban, drawings, boards, and flow surfaces are the
primary workspace viewers. Use the exposed Flynt tools to inspect or mutate
those artifacts. If the native surface is unavailable, markdown remains
portable to GitHub and other CommonMark-compatible viewers; do not install a
separate viewer unless the operator explicitly requests one.

# Excalidraw Factory Layer — Upstream Tracking

## Source

Vendored from [`@swiftlysingh/excalidraw-cli`](https://github.com/swiftlysingh/excalidraw-cli)
- **Version at vendor time:** 1.1.0
- **Commit:** `e1338e8342dfa740119aac5e83aab71673c70c68` (main, 2026-03-03)
- **License:** MIT
- **npm:** `@swiftlysingh/excalidraw-cli@1.1.0`

## What We Vendored

| Upstream File | Local File | Changes |
|---|---|---|
| `src/types/excalidraw.ts` | `types.ts` | Minimal — removed unused `ExcalidrawImage`, `ExcalidrawFileData`; added our `SemanticPurpose` and palette types |
| `src/factory/element-factory.ts` | `elements.ts` | Merged node-factory, connection-factory, text-factory into single module; replaced `nanoid` with built-in `crypto.randomUUID`; added semantic palette integration |
| `src/factory/node-factory.ts` | (merged into `elements.ts`) | — |
| `src/factory/connection-factory.ts` | (merged into `elements.ts`) | — |
| `src/factory/text-factory.ts` | (merged into `elements.ts`) | — |

### Files NOT vendored (unnecessary for our use case)

- `src/cli.ts` — CLI entry point
- `src/parser/dsl-parser.ts` — DSL parser (`(Start) -> [Process]` syntax)
- `src/parser/dot-parser.ts` — Graphviz DOT parser
- `src/parser/json-parser.ts` — JSON input parser
- `src/layout/elk-layout.ts` — ELK.js auto-layout (we do manual positioning)
- `src/layout/arrow-router.ts` — Arrow routing (simplified inline)
- `src/generator/excalidraw-generator.ts` — Full pipeline orchestrator
- `src/factory/image-factory.ts` — Image element support

## How to Sync with Upstream

When upstream updates (or if something breaks):

```bash
# 1. Check what changed
cd /tmp && git clone https://github.com/swiftlysingh/excalidraw-cli.git
diff -u excalidraw-cli/src/types/excalidraw.ts \
  ~/.pi/agent/git/github.com/cwilson613/pi-kit/extensions/render/excalidraw/types.ts

# 2. Key files to diff:
#    src/types/excalidraw.ts → types.ts
#    src/factory/element-factory.ts → elements.ts (base element creation)
#    src/factory/node-factory.ts → elements.ts (shape creation)
#    src/factory/connection-factory.ts → elements.ts (arrow creation)
#    src/factory/text-factory.ts → elements.ts (text creation)

# 3. Watch for:
#    - New required properties in ExcalidrawElementBase
#    - Changed binding format (ExcalidrawArrowBinding)
#    - New element types
#    - Changed default values
```

## Excalidraw Format Version Risks

The `.excalidraw` JSON format is defined by the Excalidraw project, not by excalidraw-cli.
Key things that could break:

1. **New required fields on elements** — Excalidraw sometimes adds mandatory fields.
   Check: https://github.com/excalidraw/excalidraw/blob/master/packages/element/src/types.ts
2. **Binding format changes** — Arrow bindings (`startBinding`/`endBinding`) have changed before.
   The `mode: "orbit" | "point"` and `fixedPoint: [number, number]` format is current as of v0.18.
3. **Version field** — Currently `"version": 2`. If they bump to 3, older files may not load.
4. **`index` field** — Fractional indexing for element ordering. Currently alphabetic (`"a"`, `"ab"`).

## Also Relevant

- **Render pipeline** (`references/excalidraw/`): Uses `@excalidraw/excalidraw` from esm.sh CDN.
  If the CDN version updates and introduces breaking changes to `exportToSvg`, the renderer breaks.
  Pin version in `render_template.html` import URL to mitigate.
- **Official skeleton API**: `@excalidraw/element` has `convertToExcalidrawElements()` but is
  prerelease-only with a hard DOM canvas dependency. If they ship a stable Node-compatible version,
  consider switching from vendored factories to the official API.

## Last Synced

- **Date:** 2026-03-03
- **Upstream version:** 1.1.0
- **By:** cwilson (pi-kit session)

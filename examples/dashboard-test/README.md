# Dashboard Test Fixture

A self-contained example project that populates all three dashboard data
sources with realistic seed data. Run `pi` from this directory to see the
unified dashboard footer in action.

## Usage

```bash
cd examples/dashboard-test
pi
```

Then press **Ctrl+Shift+B** to toggle between compact and raised modes, or
type `/dashboard` as a slash command.

## What you'll see

### Design Tree (from `design/*.md`)
- **api-redesign** — exploring, 3 open questions
- **auth-migration** — decided, all questions resolved
- **rbac-model** — implementing, linked to `feature/rbac-model` branch
- **cache-layer** — seed, 2 open questions

### OpenSpec (from `openspec/changes/`)
- **auth-migration** — tasks stage, 9/12 done (partial progress)
- **api-redesign** — specs stage, no tasks yet
- **cache-layer** — specs stage, proposal + specs only

### Cleave (from `extensions/seed-cleave-state.ts`)
- Active dispatch with 5 children: 1 done, 2 running, 2 pending
- Simulates a mid-flight `auth-migration` execution

## Standalone renderer

For offline testing without pi:

```bash
npx tsx examples/dashboard-test/render.ts          # both modes
npx tsx examples/dashboard-test/render.ts compact   # compact only
npx tsx examples/dashboard-test/render.ts raised     # raised only
```

This runs 7 scenarios × 2 modes × 3 widths = 42 renders exercising all
display code paths including width-responsive breakpoints (80/120/160 cols).

# Omegon Project Directives

> Global directives (attribution, completion standards, memory sync, branch hygiene) are deployed automatically to `~/.config/omegon/AGENTS.md` by the defaults extension on first session start. They apply to all projects. To customize, edit that file — but remove the `<!-- managed by Omegon -->` marker or your changes will be overwritten on update.

## Contributing

This repo follows trunk-based development on `main`. The full policy is in `CONTRIBUTING.md` — read it with the `read` tool if you need branch naming conventions, the memory sync architecture details, or scaling guidance.

Key points for working on Omegon itself:

- **Direct commits to `main`** for single-file fixes, typos, config tweaks
- **Feature branches** (`feature/<name>`, `refactor/<name>`) for multi-file or multi-session work
- **Conventional commits** required — see `skills/git/SKILL.md` for the spec
- The `.gitattributes` in this repo declares `merge=union` for `ai/memory/facts.jsonl`
- The `ai/.gitignore` excludes `memory/*.db` files — only `facts.jsonl` is tracked
- **Type checking**: `npx tsc --noEmit` must pass before committing TypeScript changes. Run `npm run typecheck` or `npm run check` (typecheck + tests).

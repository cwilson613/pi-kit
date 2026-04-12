---
task_id: 1
label: landing
siblings: [0:changelog, 2:install-docs]
---

# Task 1: landing

## Root Directive

> Full pass on CHANGELOG.md, landing page, and install docs to reflect current 0.15.10-rc state: brew RC channel, upgrade path, release flow fixes, and accurate feature descriptions.

## Mission

Update site/src/pages/index.astro landing page: add RC install option alongside the brew stable line (brew install styrene-lab/tap/omegon-rc), update the hero-alt-install to mention both stable and RC channel, verify and update stats (providers count, tools count, binary size). Keep the existing tone and style.

## Scope

- `site/src/pages/index.astro`

**Depends on:** none (independent)

## Siblings

- **changelog**: Update CHANGELOG.md Unreleased section to add all changes since 0.15.10 stable entry: brew RC channel (omegon-rc formula, brew install styrene-lab/tap/omegon-rc), brew-managed upgrade guard in update.rs (detects Cellar path, redirects to brew upgrade), developer just cut-rc command, release workflow fixes (draft-before-upload, lifecycle doctor skip, rc validation split), TUI fixes (nuclear panel clear #36, table body trailing pipe #37), and glibc legacy build enforcement with Linux ABI validation matrix.
- **install-docs**: Update site/src/pages/docs/install.astro: add RC channel section under Homebrew with brew install styrene-lab/tap/omegon-rc and brew upgrade styrene-lab/tap/omegon-rc, update the Updates section to note that brew-managed installs should use brew upgrade (not the install script), document that the in-app /upgrade command redirects brew-managed installs to brew upgrade automatically.

## Dependency Versions

Use these exact versions — do not rely on training data for API shapes:

```toml
# site/package.json
[dependencies]
@astrojs/sitemap = "^3.3.0"
astro = "^5.7.0"
markdown-it = "^14.1.1"
[devDependencies]
gray-matter = "^4.0.3"
```



## Testing Requirements

### Test Convention

Write tests for new functions and changed behavior — co-locate as *.test.ts


## Contract

1. Only work on files within your scope
2. Follow the Testing Requirements section above
3. If the task is too complex, set status to NEEDS_DECOMPOSITION

## Finalization (REQUIRED before completion)

You MUST complete these steps before finishing:

1. Run all guardrail checks listed above and fix failures
2. Commit your in-scope work with a clean git state when you are done
3. Commit with a clear message: `git commit -m "feat(<label>): <summary>"`
4. Verify clean state: `git status` should show nothing to commit

Do NOT edit `.cleave-prompt.md` or any task/result metadata files. Those are orchestrator-owned and may be ignored by git.
Return your completion summary in your normal final response instead of modifying the prompt file.

> ⚠️ Uncommitted work will be lost. The orchestrator merges from your branch's commits.

## Result

**Status:** PENDING

**Summary:**

**Artifacts:**

**Decisions Made:**

**Assumptions:**

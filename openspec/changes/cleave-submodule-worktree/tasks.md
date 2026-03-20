# Cleave worktree submodule failures — root cause and fix — Tasks

## 1. Rust orchestrator: submodule commits on both paths (orchestrator.rs)
<!-- specs: cleave/submodule -->

- [ ] 1.1 Extract submodule commit + auto-commit into a helper `salvage_worktree_changes(wt_path, label, scope)` that runs `commit_dirty_submodules` then `auto_commit_worktree`
- [ ] 1.2 Call `salvage_worktree_changes` from BOTH the `Ok(output)` and `Err(e)` match arms of the child result handler
- [ ] 1.3 On the Err path, log at warn level that we're attempting to salvage changes from a failed child
- [ ] 1.4 Test: verify salvage runs on both paths (unit test with mock worktree)

## 2. Rust worktree: scope health check (worktree.rs)
<!-- specs: cleave/submodule -->

- [ ] 2.1 Add `verify_scope_accessible(worktree_path, scope) -> Result<Vec<String>>` that stats each scope file, returns list of inaccessible paths
- [ ] 2.2 Empty scope returns Ok (vacuous pass)
- [ ] 2.3 Call from orchestrator after submodule_init — if any scope file missing, mark child failed with actionable error
- [ ] 2.4 Test: accessible file passes, missing file fails, empty scope passes

## 3. Rust orchestrator: submodule context in task files (orchestrator.rs)
<!-- specs: cleave/submodule -->

- [ ] 3.1 After submodule_init, call `detect_submodules(worktree_path)` to get submodule paths
- [ ] 3.2 Check if any scope file starts with a submodule path prefix
- [ ] 3.3 If yes, inject a "## Submodule Context" section into the task file noting which paths are submodules and that the orchestrator handles commits
- [ ] 3.4 Test: scope crossing submodule gets note, scope not crossing submodule gets no note

## 4. TS dirty-tree preflight: submodule classification (git-state.ts + index.ts)
<!-- specs: cleave/submodule -->

- [ ] 4.1 Add `parseGitmodules(repoPath) -> Map<string, string>` to git-state.ts — reads .gitmodules, returns map of path→url
- [ ] 4.2 Extend `GitStatusEntry` with `submodule: boolean` field
- [ ] 4.3 In `inspectGitState()`, cross-reference entries against submodule paths — set `submodule: true` for matches
- [ ] 4.4 In cleave/index.ts `checkpointRelatedChanges()`, when a submodule path is dirty, log a warning about submodule HEAD consistency
- [ ] 4.5 Add deprecation comment to TS worktree.ts noting native dispatch owns worktree creation
- [ ] 4.6 Tests: submodule entry classified correctly, no .gitmodules = no submodules

## Cross-cutting constraints

- [ ] commit_dirty_submodules must run on both Ok and Err paths
- [ ] Worktree health check must verify at least one scope file is readable
- [ ] Task files must include submodule context when scope crosses submodule boundary
- [ ] TS worktree.ts is legacy — do not add submodule support there, only in Rust orchestrator

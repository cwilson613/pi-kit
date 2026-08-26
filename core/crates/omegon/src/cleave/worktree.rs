//! Git worktree management for cleave children.
//!
//! Delegates to the boot-captured managed Git service. This module is a thin
//! adapter that maps cleave-specific conventions (branch naming, workspace
//! layout, child labels) onto the generic git API.

use anyhow::Result;
use std::path::{Path, PathBuf};

// ── Worktree lifecycle ──────────────────────────────────────────────────

/// Create a worktree/workspace for a child.
///
/// Native cleave currently relies on git-branch-based merge semantics: each
/// child is created on a named branch and later squash-merged by that branch
/// name. jj workspaces do not satisfy that contract yet because they create
/// workspace-local changes without a corresponding git branch for merge.
///
/// Until cleave grows a jj-aware harvest/merge path, always use a git worktree
/// here, even in co-located jj repos.
pub async fn create_worktree(
    git: &crate::git_service::GitBinding,
    workspace_path: &Path,
    child_id: usize,
    label: &str,
    branch: &str,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<PathBuf> {
    let worktree_dir = workspace_path.join(format!("{}-wt-{}", child_id, label));
    let response = git
        .invoke(crate::git_service::GitRequest::CreateWorktree {
            workspace_path: worktree_dir.clone(),
            name: label.to_string(),
            branch: branch.to_string(),
            mode: crate::git_service::GitWorktreeMode::Git,
            cancellation,
        })
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    if !matches!(response, crate::git_service::GitResponse::Worktree(_)) {
        anyhow::bail!("managed Git service returned an invalid worktree response");
    }
    Ok(worktree_dir)
}

/// Remove a child worktree.
///
/// This matches `create_worktree`: cleave children always use git worktrees,
/// so cleanup should remove the git worktree directly.
pub async fn remove_worktree(
    git: &crate::git_service::GitBinding,
    worktree_path: &Path,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<()> {
    git.invoke(crate::git_service::GitRequest::RemoveWorktree {
        workspace_path: worktree_path.to_path_buf(),
        name: String::new(),
        mode: crate::git_service::GitWorktreeMode::Git,
        cancellation,
    })
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(())
}

/// Delete a child branch after merge.
///
/// Cleave children are always created on git branches, even in co-located jj
/// repos, so the branch should always be removed through git.
pub async fn delete_branch(
    git: &crate::git_service::GitBinding,
    branch: &str,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<()> {
    git.invoke(crate::git_service::GitRequest::DeleteBranch {
        branch: branch.to_string(),
        cancellation,
    })
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(())
}

// ── Merge ───────────────────────────────────────────────────────────────

/// Merge result kept as a cleave-specific compatibility enum.
/// changing all orchestrator call sites at once.
#[derive(Debug)]
pub enum MergeResult {
    Success,
    NoChanges,
    Conflict(String),
    Failed(String),
}

/// Squash-merge a child's branch into the current HEAD.
///
/// All diary commits on the child branch are compressed into one commit.
/// This is the default for cleave children — their intermediate commit
/// history has no bisect/revert value.
pub async fn squash_merge_branch(
    git: &crate::git_service::GitBinding,
    branch: &str,
    message: &str,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<MergeResult> {
    let response = git
        .invoke(crate::git_service::GitRequest::Merge {
            branch: branch.to_string(),
            message: message.to_string(),
            squash: true,
            cancellation,
        })
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let crate::git_service::GitResponse::Merge(result) = response else {
        anyhow::bail!("managed Git service returned an invalid merge response");
    };
    match result {
        crate::git_service::GitMergeOutcome::Success { .. } => Ok(MergeResult::Success),
        crate::git_service::GitMergeOutcome::NoChanges => Ok(MergeResult::NoChanges),
        crate::git_service::GitMergeOutcome::Conflict { files } => {
            Ok(MergeResult::Conflict(files.join(", ")))
        }
        crate::git_service::GitMergeOutcome::Failed(detail) => Ok(MergeResult::Failed(detail)),
    }
}

/// Legacy no-ff merge (kept for backward compatibility and fallback).
pub async fn merge_branch(
    git: &crate::git_service::GitBinding,
    branch: &str,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<MergeResult> {
    let message = format!("cleave: merge {}", branch);
    let response = git
        .invoke(crate::git_service::GitRequest::Merge {
            branch: branch.to_string(),
            message,
            squash: false,
            cancellation,
        })
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let crate::git_service::GitResponse::Merge(result) = response else {
        anyhow::bail!("managed Git service returned an invalid merge response");
    };
    match result {
        crate::git_service::GitMergeOutcome::Success { .. } => Ok(MergeResult::Success),
        crate::git_service::GitMergeOutcome::NoChanges => Ok(MergeResult::NoChanges),
        crate::git_service::GitMergeOutcome::Conflict { files } => {
            Ok(MergeResult::Conflict(files.join(", ")))
        }
        crate::git_service::GitMergeOutcome::Failed(detail) => Ok(MergeResult::Failed(detail)),
    }
}

// ── Submodule operations ────────────────────────────────────────────────

/// Initialize submodules in a worktree.
///
/// No-op when jj is active — jj workspaces share the full repo tree,
/// no submodule init needed. Also no-op in a monorepo with no submodules.
pub async fn submodule_init(
    git: &crate::git_service::GitBinding,
    worktree_path: &Path,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<()> {
    git.invoke(crate::git_service::GitRequest::InitSubmodules {
        path: worktree_path.to_path_buf(),
        cancellation,
    })
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(())
}

/// Detect active submodules in a repo/worktree.
pub async fn detect_submodules(
    git: &crate::git_service::GitBinding,
    repo_path: &Path,
    cancellation: tokio_util::sync::CancellationToken,
) -> Vec<(String, PathBuf)> {
    let paths = match git
        .invoke(crate::git_service::GitRequest::ListSubmodules {
            path: repo_path.to_path_buf(),
            cancellation,
        })
        .await
    {
        Ok(crate::git_service::GitResponse::Submodules(paths)) => paths,
        _ => Vec::new(),
    };
    paths
        .into_iter()
        .map(|path| {
            let full = repo_path.join(&path);
            (path, full)
        })
        .collect()
}

/// Commit dirty submodules in a worktree after a child finishes.
///
/// Uses the managed service for each dirty submodule and pointer commit,
/// then commits the pointer updates in the parent.
pub async fn commit_dirty_submodules(
    git: &crate::git_service::GitBinding,
    worktree_path: &Path,
    child_label: &str,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<usize> {
    match git
        .invoke(crate::git_service::GitRequest::CommitDirtySubmodules {
            path: worktree_path.to_path_buf(),
            label: child_label.to_string(),
            cancellation,
        })
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?
    {
        crate::git_service::GitResponse::DirtySubmodulesCommitted(count) => Ok(count),
        _ => anyhow::bail!("managed Git service returned an invalid submodule response"),
    }
}

// ── Scope verification ──────────────────────────────────────────────────

/// Verify that scope files are accessible in a worktree after submodule init.
pub fn verify_scope_accessible(worktree_path: &Path, scope: &[String]) -> Vec<String> {
    let mut missing = Vec::new();
    for path_str in scope {
        let full_path = worktree_path.join(path_str);
        if !full_path.exists() {
            let parent = full_path.parent();
            if parent.is_none() || !parent.unwrap().exists() {
                missing.push(path_str.clone());
            }
        }
    }
    missing
}

/// Build a submodule context note for a task file.
pub async fn build_submodule_context(
    git: &crate::git_service::GitBinding,
    worktree_path: &Path,
    scope: &[String],
    cancellation: tokio_util::sync::CancellationToken,
) -> Option<String> {
    let submodules = detect_submodules(git, worktree_path, cancellation).await;
    build_submodule_context_from_list(&submodules, scope)
}

/// Inner function that takes an explicit submodule list — testable without git.
pub(crate) fn build_submodule_context_from_list(
    submodules: &[(String, PathBuf)],
    scope: &[String],
) -> Option<String> {
    if submodules.is_empty() {
        return None;
    }

    let mut affected: Vec<&str> = Vec::new();
    for (name, _path) in submodules {
        let prefix = format!("{name}/");
        if scope.iter().any(|s| s.starts_with(&prefix) || s == name) {
            affected.push(name);
        }
    }

    if affected.is_empty() {
        return None;
    }

    let paths = affected
        .iter()
        .map(|p| format!("`{p}/`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "## Submodule Context\n\n\
         The following paths in your scope are inside git submodules: {paths}\n\n\
         **Edit files normally.** The orchestrator handles all git submodule commits \
         after your task completes — you do NOT need to run any special git commands \
         for submodule files. Just use the `edit` tool as usual.\n\n\
         **Do NOT run `cargo build` or `cargo test` inside the submodule** — \
         the worktree has no build cache and compilation will be slow. Focus on \
         making your edits and let the orchestrator handle verification.\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_path_format() {
        let workspace = Path::new("/tmp/ws");
        let expected = workspace.join("2-wt-my-task");
        let result_path = workspace.join(format!("{}-wt-{}", 2, "my-task"));
        assert_eq!(result_path, expected);
    }

    #[test]
    fn merge_result_variants() {
        let s = MergeResult::Success;
        assert!(format!("{s:?}").contains("Success"));
        let n = MergeResult::NoChanges;
        assert!(format!("{n:?}").contains("NoChanges"));
        let c = MergeResult::Conflict("file.rs".into());
        assert!(format!("{c:?}").contains("file.rs"));
        let f = MergeResult::Failed("error".into());
        assert!(format!("{f:?}").contains("error"));
    }

    #[test]
    fn verify_scope_empty_is_vacuous_pass() {
        let dir = tempfile::tempdir().unwrap();
        let missing = verify_scope_accessible(dir.path(), &[]);
        assert!(missing.is_empty());
    }

    #[test]
    fn verify_scope_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        let missing = verify_scope_accessible(dir.path(), &["src/main.rs".to_string()]);
        assert!(missing.is_empty());
    }

    #[test]
    fn verify_scope_missing_file_with_existing_parent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let missing = verify_scope_accessible(dir.path(), &["src/new_file.rs".to_string()]);
        assert!(missing.is_empty());
    }

    #[test]
    fn verify_scope_missing_file_and_parent() {
        let dir = tempfile::tempdir().unwrap();
        let missing = verify_scope_accessible(dir.path(), &["core/crates/lib.rs".to_string()]);
        assert_eq!(missing, vec!["core/crates/lib.rs"]);
    }

    #[test]
    fn submodule_context_with_crossing_scope() {
        let submodules = vec![("core".to_string(), PathBuf::from("/repo/core"))];
        let scope = vec!["core/crates/omegon-secrets/src/vault.rs".to_string()];
        let result = build_submodule_context_from_list(&submodules, &scope);
        assert!(result.is_some());
        let note = result.unwrap();
        assert!(note.contains("`core/`"));
        assert!(note.contains("Edit files normally"));
    }

    #[test]
    fn submodule_context_without_crossing_scope() {
        let submodules = vec![("core".to_string(), PathBuf::from("/repo/core"))];
        let scope = vec!["extensions/cleave/index.ts".to_string()];
        let result = build_submodule_context_from_list(&submodules, &scope);
        assert!(result.is_none());
    }

    #[test]
    fn submodule_context_no_submodules() {
        let result = build_submodule_context_from_list(&[], &["anything.rs".to_string()]);
        assert!(result.is_none());
    }

    #[test]
    fn submodule_context_multiple_submodules() {
        let submodules = vec![
            ("core".to_string(), PathBuf::from("/repo/core")),
            ("vendor".to_string(), PathBuf::from("/repo/vendor")),
        ];
        let scope = vec![
            "core/crates/lib.rs".to_string(),
            "vendor/dep/src/lib.rs".to_string(),
        ];
        let result = build_submodule_context_from_list(&submodules, &scope);
        assert!(result.is_some());
        let note = result.unwrap();
        assert!(note.contains("`core/`"));
        assert!(note.contains("`vendor/`"));
    }

    #[tokio::test]
    async fn create_worktree_in_git_repo() {
        let cwd = std::env::current_dir().unwrap();
        // Use omegon_git to discover the repo
        if let Ok(Some(model)) = omegon_git::RepoModel::discover(&cwd) {
            let (mut bus, git) =
                crate::git_service::bounded_binding(model.repo_path().to_path_buf())
                    .await
                    .unwrap();
            let workspace = tempfile::tempdir().unwrap();
            let branch_name = format!("test-wt-{}", std::process::id());
            let result = create_worktree(
                &git,
                workspace.path(),
                0,
                "test",
                &branch_name,
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

            if let Ok(wt_path) = result {
                assert!(wt_path.exists(), "worktree should exist");

                let branch_exists = std::process::Command::new("git")
                    .args([
                        "show-ref",
                        "--verify",
                        "--quiet",
                        &format!("refs/heads/{branch_name}"),
                    ])
                    .current_dir(model.repo_path())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                assert!(
                    branch_exists,
                    "cleave worktree must create a git branch so merge can address it"
                );

                let _ = remove_worktree(&git, &wt_path, tokio_util::sync::CancellationToken::new())
                    .await;
                let _ = delete_branch(
                    &git,
                    &branch_name,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
            }
            assert!(
                bus.shutdown_managed_services()
                    .await
                    .all_resources_settled()
            );
        }
    }
}

//! Workspace identity and Git status projected into Workbench chrome.
//!
//! This module owns filesystem/Git inspection only. Ratatui layout and rendering
//! remain in the adapter modules that consume these plain strings.

use std::path::Path;

pub(super) fn repo_display_name(cwd: &Path) -> Option<String> {
    let repo = git2::Repository::discover(cwd).ok()?;
    remote_repo_name(&repo)
}

pub(super) fn git_branch(cwd: &Path) -> Option<String> {
    let repo = git2::Repository::discover(cwd).ok()?;
    git_branch_for_repo(&repo)
}

pub(super) fn workspace_dir_basename(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string()
}

fn git_branch_for_repo(repo: &git2::Repository) -> Option<String> {
    let head = repo.head().ok()?;
    let mut label = if head.is_branch() {
        head.shorthand()
            .filter(|branch| !branch.is_empty())?
            .to_string()
    } else {
        let short = head
            .target()
            .map(|oid| oid.to_string().chars().take(7).collect::<String>())?;
        format!("HEAD@{short}")
    };

    if let Some((ahead, behind)) = ahead_behind(repo, &head) {
        if ahead > 0 {
            label.push_str(&format!(" ↑{ahead}"));
        }
        if behind > 0 {
            label.push_str(&format!(" ↓{behind}"));
        }
    }

    if has_tracked_changes(repo) {
        label.push_str(" *");
    }

    if let Some(state) = state_label(repo.state()) {
        label.push_str(" · ");
        label.push_str(state);
    }

    Some(label)
}

fn ahead_behind(repo: &git2::Repository, head: &git2::Reference<'_>) -> Option<(usize, usize)> {
    let branch_name = head.shorthand()?;
    let local_oid = head.target()?;
    let upstream = repo
        .find_branch(branch_name, git2::BranchType::Local)
        .ok()?
        .upstream()
        .ok()?
        .get()
        .target()?;
    repo.graph_ahead_behind(local_oid, upstream).ok()
}

fn has_tracked_changes(repo: &git2::Repository) -> bool {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    repo.statuses(Some(&mut opts))
        .map(|statuses| {
            statuses
                .iter()
                .any(|entry| entry.status() != git2::Status::CURRENT)
        })
        .unwrap_or(false)
}

fn state_label(state: git2::RepositoryState) -> Option<&'static str> {
    match state {
        git2::RepositoryState::Clean => None,
        git2::RepositoryState::Merge => Some("merge"),
        git2::RepositoryState::Revert | git2::RepositoryState::RevertSequence => Some("revert"),
        git2::RepositoryState::CherryPick | git2::RepositoryState::CherryPickSequence => {
            Some("cherry-pick")
        }
        git2::RepositoryState::Bisect => Some("bisect"),
        git2::RepositoryState::Rebase
        | git2::RepositoryState::RebaseInteractive
        | git2::RepositoryState::RebaseMerge => Some("rebase"),
        git2::RepositoryState::ApplyMailbox | git2::RepositoryState::ApplyMailboxOrRebase => {
            Some("apply")
        }
    }
}

fn remote_repo_name(repo: &git2::Repository) -> Option<String> {
    let remote = repo
        .find_remote("upstream")
        .or_else(|_| repo.find_remote("origin"))
        .ok()?;
    remote.url().and_then(repo_name_from_remote_url)
}

fn repo_name_from_remote_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let tail = without_git
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(without_git)
        .trim();

    (!tail.is_empty()).then(|| tail.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_name_from_remote_url_handles_common_forms() {
        assert_eq!(
            repo_name_from_remote_url("git@github.com:styrene-labs/omegon.git"),
            Some("omegon".to_string())
        );
        assert_eq!(
            repo_name_from_remote_url("https://github.com/styrene-labs/omegon.git"),
            Some("omegon".to_string())
        );
        assert_eq!(
            repo_name_from_remote_url("ssh://git@github.com/styrene-labs/omegon"),
            Some("omegon".to_string())
        );
        assert_eq!(repo_name_from_remote_url(""), None);
    }

    #[test]
    fn remote_repo_name_prefers_upstream_over_origin() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("origin", "git@github.com:fork/local-checkout-name.git")
            .unwrap();
        repo.remote("upstream", "git@github.com:styrene-labs/canonical-name.git")
            .unwrap();

        assert_eq!(remote_repo_name(&repo), Some("canonical-name".to_string()));
    }

    #[test]
    fn repo_display_name_uses_remote_not_checkout_dir() {
        let dir = tempfile::tempdir().unwrap();
        let checkout = dir.path().join("local-checkout-name");
        std::fs::create_dir(&checkout).unwrap();
        let repo = git2::Repository::init(&checkout).unwrap();
        repo.remote("origin", "git@github.com:styrene-labs/canonical-name.git")
            .unwrap();

        assert_eq!(
            repo_display_name(&checkout),
            Some("canonical-name".to_string())
        );
        assert_eq!(workspace_dir_basename(&checkout), "local-checkout-name");
    }

    #[test]
    fn git_branch_includes_ahead_behind_and_dirty_markers() {
        let dir = tempfile::tempdir().unwrap();
        let mut init_opts = git2::RepositoryInitOptions::new();
        init_opts.initial_head("main");
        let repo = git2::Repository::init_opts(dir.path(), &init_opts).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Omegon Test").unwrap();
            config
                .set_str("user.email", "omegon@example.invalid")
                .unwrap();
        }
        std::fs::write(dir.path().join("file.txt"), "base\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        let base = repo
            .commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .unwrap();
        let base_commit = repo.find_commit(base).unwrap();
        repo.branch("upstream", &base_commit, false).unwrap();
        repo.find_branch("main", git2::BranchType::Local)
            .unwrap()
            .set_upstream(Some("upstream"))
            .unwrap();

        std::fs::write(dir.path().join("file.txt"), "next\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "next", &tree, &[&base_commit])
            .unwrap();
        std::fs::write(dir.path().join("file.txt"), "dirty\n").unwrap();

        assert_eq!(git_branch_for_repo(&repo).as_deref(), Some("main ↑1 *"));
    }

    #[test]
    fn git_branch_reports_detached_head() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Omegon Test").unwrap();
            config
                .set_str("user.email", "omegon@example.invalid")
                .unwrap();
        }
        std::fs::write(dir.path().join("file.txt"), "base\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        let commit_id = repo
            .commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .unwrap();
        repo.set_head_detached(commit_id).unwrap();

        let label = git_branch_for_repo(&repo).unwrap();
        assert!(label.starts_with("HEAD@"), "{label}");
    }
}

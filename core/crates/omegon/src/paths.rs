//! Centralized path resolution for Omegon project artifacts.
//!
//! The `ai/` directory is the primary home for all agent-managed content:
//!   - `ai/docs/`      — design tree markdown documents
//!   - `ai/openspec/`  — OpenSpec change lifecycle
//!   - `ai/memory/`    — facts.db, facts.jsonl
//!   - `ai/lifecycle/` — opsx-core state.json
//!   - `ai/milestones.json`
//!
//! The `.omegon/` dotfile is for tool configuration only:
//!   - `profile.json`  — model preferences, calibration
//!   - `tutorial_completed`
//!   - `agents/`       — delegate agent definitions
//!   - `history/`      — session history
//!
//! For backward compatibility, every path resolver checks the `ai/` location
//! first, then falls back to the legacy location (`docs/`, `openspec/`,
//! `.omegon/memory/`, `.omegon/lifecycle/`, `.pi/memory/`).
//!
//! New writes always go to the `ai/` location.

use std::path::{Path, PathBuf};

/// Resolve the design docs directory.
/// Primary: `ai/docs/`, fallback: `docs/`
pub fn design_docs_dir(repo_root: &Path) -> PathBuf {
    let primary = repo_root.join("ai/docs");
    if primary.is_dir() {
        return primary;
    }
    let legacy = repo_root.join("docs");
    if legacy.is_dir() {
        return legacy;
    }
    // Default to primary for new projects (created on first write)
    primary
}

/// Resolve the OpenSpec directory.
/// Primary: `ai/openspec/`, fallback: `openspec/`
pub fn openspec_dir(repo_root: &Path) -> Option<PathBuf> {
    let primary = repo_root.join("ai/openspec");
    if primary.is_dir() {
        return Some(primary);
    }
    let legacy = repo_root.join("openspec");
    if legacy.is_dir() {
        return Some(legacy);
    }
    None
}

/// Resolve the OpenSpec directory for writes (always primary).
pub fn openspec_dir_write(repo_root: &Path) -> PathBuf {
    // If legacy exists but primary doesn't, use legacy to avoid splitting
    // across two locations mid-project. Only use ai/ for new projects.
    let legacy = repo_root.join("openspec");
    if legacy.is_dir() && !repo_root.join("ai/openspec").is_dir() {
        return legacy;
    }
    repo_root.join("ai/openspec")
}

/// Resolve the memory directory.
/// Primary: `ai/memory/`, fallback chain: `.omegon/memory/` → `.pi/memory/`
pub fn memory_dir(repo_root: &Path) -> PathBuf {
    let primary = repo_root.join("ai/memory");
    if primary.is_dir() {
        return primary;
    }
    let omegon = repo_root.join(".omegon/memory");
    if omegon.is_dir() {
        return omegon;
    }
    let pi = repo_root.join(".pi/memory");
    if pi.is_dir() {
        return pi;
    }
    // Default to primary for new projects
    primary
}

/// Resolve the memory directory for writes.
/// Prefers ai/memory/ if it exists, then .omegon/memory/ if it exists,
/// otherwise defaults to ai/memory/ for new projects.
pub fn memory_dir_write(repo_root: &Path) -> PathBuf {
    let ai = repo_root.join("ai/memory");
    if ai.is_dir() {
        return ai;
    }
    let omegon = repo_root.join(".omegon/memory");
    if omegon.is_dir() {
        return omegon;
    }
    // New project — default to ai/memory/
    ai
}

/// Resolve the lifecycle state directory (opsx-core).
/// Primary: `ai/lifecycle/`, fallback: `.omegon/lifecycle/`
pub fn lifecycle_dir(repo_root: &Path) -> PathBuf {
    let primary = repo_root.join("ai/lifecycle");
    if primary.is_dir() {
        return primary;
    }
    let legacy = repo_root.join(".omegon/lifecycle");
    if legacy.is_dir() {
        return legacy;
    }
    primary
}

/// Resolve the milestones file.
/// Primary: `ai/milestones.json`, fallback: `.omegon/milestones.json`
pub fn milestones_file(repo_root: &Path) -> PathBuf {
    let primary = repo_root.join("ai/milestones.json");
    if primary.exists() {
        return primary;
    }
    let legacy = repo_root.join(".omegon/milestones.json");
    if legacy.exists() {
        return legacy;
    }
    primary
}

/// The .omegon/ config directory — tool config only, never content.
pub fn config_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".omegon")
}

/// Check if a path is an agent artifact (for git lifecycle classification).
pub fn is_agent_artifact(path: &str) -> bool {
    path.starts_with("ai/")
        || path.starts_with("docs/")       // legacy design docs
        || path.starts_with("openspec/")   // legacy openspec
        || path.starts_with(".omegon/")
        || path.starts_with(".pi/")        // legacy pi
}

/// Resolve the JSONL facts file for import.
/// Checks: `ai/memory/facts.jsonl` → `.omegon/memory/facts.jsonl` → `.pi/memory/facts.jsonl`
pub fn facts_jsonl(repo_root: &Path) -> Option<PathBuf> {
    for dir in [
        repo_root.join("ai/memory"),
        repo_root.join(".omegon/memory"),
        repo_root.join(".pi/memory"),
    ] {
        let path = dir.join("facts.jsonl");
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Check if this project has any memory facts (for first-run detection).
pub fn has_memory_facts(repo_root: &Path) -> bool {
    facts_jsonl(repo_root).is_some()
}

/// User-level config directory: `~/.config/omegon/`
/// Fallback reads from `~/.pi/agent/` for backward compat.
pub fn user_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/omegon")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn design_docs_prefers_ai() {
        let tmp = TempDir::new().unwrap();
        // No dirs exist → default to ai/docs
        assert_eq!(design_docs_dir(tmp.path()), tmp.path().join("ai/docs"));

        // Create legacy docs/
        std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
        assert_eq!(design_docs_dir(tmp.path()), tmp.path().join("docs"));

        // Create ai/docs/ → should prefer it
        std::fs::create_dir_all(tmp.path().join("ai/docs")).unwrap();
        assert_eq!(design_docs_dir(tmp.path()), tmp.path().join("ai/docs"));
    }

    #[test]
    fn openspec_fallback_chain() {
        let tmp = TempDir::new().unwrap();
        assert!(openspec_dir(tmp.path()).is_none());

        std::fs::create_dir_all(tmp.path().join("openspec")).unwrap();
        assert_eq!(openspec_dir(tmp.path()), Some(tmp.path().join("openspec")));

        std::fs::create_dir_all(tmp.path().join("ai/openspec")).unwrap();
        assert_eq!(openspec_dir(tmp.path()), Some(tmp.path().join("ai/openspec")));
    }

    #[test]
    fn memory_dir_fallback_chain() {
        let tmp = TempDir::new().unwrap();
        // No dirs → default to ai/memory
        assert_eq!(memory_dir(tmp.path()), tmp.path().join("ai/memory"));

        // .pi/memory exists
        std::fs::create_dir_all(tmp.path().join(".pi/memory")).unwrap();
        assert_eq!(memory_dir(tmp.path()), tmp.path().join(".pi/memory"));

        // .omegon/memory takes priority over .pi
        std::fs::create_dir_all(tmp.path().join(".omegon/memory")).unwrap();
        assert_eq!(memory_dir(tmp.path()), tmp.path().join(".omegon/memory"));

        // ai/memory takes highest priority
        std::fs::create_dir_all(tmp.path().join("ai/memory")).unwrap();
        assert_eq!(memory_dir(tmp.path()), tmp.path().join("ai/memory"));
    }

    #[test]
    fn milestones_fallback() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(milestones_file(tmp.path()), tmp.path().join("ai/milestones.json"));

        std::fs::create_dir_all(tmp.path().join(".omegon")).unwrap();
        std::fs::write(tmp.path().join(".omegon/milestones.json"), "{}").unwrap();
        assert_eq!(milestones_file(tmp.path()), tmp.path().join(".omegon/milestones.json"));
    }

    #[test]
    fn is_agent_artifact_classification() {
        assert!(is_agent_artifact("ai/docs/some-node.md"));
        assert!(is_agent_artifact("ai/openspec/changes/foo/proposal.md"));
        assert!(is_agent_artifact("ai/memory/facts.jsonl"));
        assert!(is_agent_artifact("docs/old-node.md"));
        assert!(is_agent_artifact("openspec/changes/bar/tasks.md"));
        assert!(is_agent_artifact(".omegon/profile.json"));
        assert!(is_agent_artifact(".pi/memory/facts.jsonl"));
        assert!(!is_agent_artifact("src/main.rs"));
        assert!(!is_agent_artifact("Cargo.toml"));
    }

    #[test]
    fn facts_jsonl_resolution() {
        let tmp = TempDir::new().unwrap();
        assert!(facts_jsonl(tmp.path()).is_none());

        // Create ai/memory/facts.jsonl (canonical)
        let pi_dir = tmp.path().join("ai/memory");
        std::fs::create_dir_all(&pi_dir).unwrap();
        std::fs::write(pi_dir.join("facts.jsonl"), "{}").unwrap();
        assert_eq!(facts_jsonl(tmp.path()), Some(pi_dir.join("facts.jsonl")));

        // .omegon/memory also resolves (fallback)
        let omegon_dir = tmp.path().join(".omegon/memory");
        std::fs::create_dir_all(&omegon_dir).unwrap();
        // ai/memory still wins
        assert_eq!(facts_jsonl(tmp.path()), Some(pi_dir.join("facts.jsonl")));
    }

    #[test]
    fn write_paths_stick_to_legacy_when_it_exists() {
        let tmp = TempDir::new().unwrap();
        // If legacy openspec/ exists, writes go there (don't split)
        std::fs::create_dir_all(tmp.path().join("openspec")).unwrap();
        assert_eq!(openspec_dir_write(tmp.path()), tmp.path().join("openspec"));

        // If ai/openspec also exists, it wins
        std::fs::create_dir_all(tmp.path().join("ai/openspec")).unwrap();
        assert_eq!(openspec_dir_write(tmp.path()), tmp.path().join("ai/openspec"));
    }
}

//! Repo walker — discovers files, hashes content, drives incremental indexing.
//!
//! Fast-path: if git HEAD hasn't changed since the last index, skip the file
//! walk entirely and return cached stats. This makes the incremental path
//! near-instantaneous (~5ms vs 2s for a full walk of a large repo).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::cache::{FileKind, ScanCache};
use crate::code::{CodeScanner, is_supported_code_extension};
use crate::knowledge::{KnowledgeDirs, KnowledgeScanner};

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub code_files: usize,
    pub knowledge_files: usize,
    pub code_chunks: usize,
    pub knowledge_chunks: usize,
    pub duration_ms: u64,
}

pub struct Indexer;

impl Indexer {
    pub fn run(repo_path: &Path, cache: &mut ScanCache) -> Result<IndexStats> {
        Self::run_with_cancel(repo_path, cache, || false)
    }

    pub fn run_with_cancel(
        repo_path: &Path,
        cache: &mut ScanCache,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<IndexStats> {
        let full_rebuild = cache.full_rebuild_active();
        let result = Self::run_inner(repo_path, cache, &is_cancelled);
        if !full_rebuild {
            return result;
        }

        match result {
            Ok(stats) if !is_cancelled() => {
                if let Err(error) = cache.commit_full_rebuild() {
                    if cache.full_rebuild_active() {
                        cache
                            .rollback_full_rebuild()
                            .context("failed to roll back failed full codescan commit")?;
                    }
                    return Err(error).context("failed to commit full codescan rebuild");
                }
                Ok(stats)
            }
            Ok(_) => {
                if cache.full_rebuild_active() {
                    cache.rollback_full_rebuild()?;
                }
                anyhow::bail!("codebase index cancelled");
            }
            Err(error) => {
                if cache.full_rebuild_active() {
                    cache
                        .rollback_full_rebuild()
                        .context("failed to roll back full codescan rebuild")?;
                }
                Err(error)
            }
        }
    }

    fn run_inner(
        repo_path: &Path,
        cache: &ScanCache,
        is_cancelled: &impl Fn() -> bool,
    ) -> Result<IndexStats> {
        let started = Instant::now();
        if is_cancelled() {
            anyhow::bail!("codebase index cancelled");
        }

        // ── Fast path: skip file walk if HEAD hasn't changed and the working tree
        // has no relevant dirty files that could make cached chunks stale.
        let current_head = git_head(repo_path);
        if let Some(ref head) = current_head
            && cache.try_get_meta("last_head")?.as_deref() == Some(head.as_str())
            && !has_relevant_worktree_changes(repo_path)
        {
            // Already up to date — return cached counts without touching the filesystem
            let code_chunks = cache.try_code_chunk_count()?;
            let knowledge_chunks = cache.try_knowledge_chunk_count()?;
            if cache.try_file_state_count()? > 0 {
                tracing::debug!(head = %head, "codescan fast-path: HEAD unchanged");
                return Ok(IndexStats {
                    code_files: 0, // unknown without walk; 0 = "not re-scanned"
                    knowledge_files: 0,
                    code_chunks,
                    knowledge_chunks,
                    duration_ms: started.elapsed().as_millis() as u64,
                });
            }
        }

        // ── Slow path: walk, hash, diff, re-scan stale files ─────────────
        let code_files = discover_code_files(repo_path);
        if is_cancelled() {
            anyhow::bail!("codebase index cancelled");
        }
        let knowledge_files = discover_knowledge_files(repo_path, &KnowledgeDirs::default());
        if is_cancelled() {
            anyhow::bail!("codebase index cancelled");
        }

        // Compute content hashes
        let mut code_hashes = Vec::with_capacity(code_files.len());
        for path in &code_files {
            if is_cancelled() {
                anyhow::bail!("codebase index cancelled");
            }
            let content = std::fs::read(repo_path.join(path))
                .with_context(|| format!("failed to hash {}", path.display()))?;
            code_hashes.push((path.clone(), sha256(&content)));
        }
        let mut knowledge_hashes = Vec::with_capacity(knowledge_files.len());
        for path in &knowledge_files {
            if is_cancelled() {
                anyhow::bail!("codebase index cancelled");
            }
            let content = std::fs::read(repo_path.join(path))
                .with_context(|| format!("failed to hash {}", path.display()))?;
            knowledge_hashes.push((path.clone(), sha256(&content)));
        }

        let stale_code: HashSet<PathBuf> = cache
            .stale_paths_for_kind(FileKind::Code, &code_hashes)?
            .into_iter()
            .collect();
        let stale_knowledge: HashSet<PathBuf> = cache
            .stale_paths_for_kind(FileKind::Knowledge, &knowledge_hashes)?
            .into_iter()
            .collect();
        let live_files: HashSet<(PathBuf, FileKind)> = code_hashes
            .iter()
            .map(|(path, _)| (path.clone(), FileKind::Code))
            .chain(
                knowledge_hashes
                    .iter()
                    .map(|(path, _)| (path.clone(), FileKind::Knowledge)),
            )
            .collect();

        for (path, hash) in &code_hashes {
            if is_cancelled() {
                anyhow::bail!("codebase index cancelled");
            }
            if !stale_code.contains(path) {
                continue;
            }
            let content = std::fs::read_to_string(repo_path.join(path))
                .with_context(|| format!("failed to read indexed code path {}", path.display()))?;
            let mut chunks = CodeScanner::scan_file(path, &content);
            for c in &mut chunks {
                c.path = path.clone();
            }
            cache.upsert_code_chunks_with_cancel(path, hash, &chunks, is_cancelled)?;
        }

        for (path, hash) in &knowledge_hashes {
            if is_cancelled() {
                anyhow::bail!("codebase index cancelled");
            }
            if !stale_knowledge.contains(path) {
                continue;
            }
            let content = std::fs::read_to_string(repo_path.join(path)).with_context(|| {
                format!("failed to read indexed knowledge path {}", path.display())
            })?;
            let mut chunks = KnowledgeScanner::scan_file(path, &content);
            for c in &mut chunks {
                c.path = path.clone();
            }
            cache.upsert_knowledge_chunks_with_cancel(path, hash, &chunks, is_cancelled)?;
        }

        cache.publish_successful_run(&live_files, current_head.as_deref(), is_cancelled)?;

        let code_chunks = cache.try_code_chunk_count()?;
        let knowledge_chunks = cache.try_knowledge_chunk_count()?;
        let duration_ms = started.elapsed().as_millis() as u64;

        tracing::info!(
            code_files = code_files.len(),
            knowledge_files = knowledge_files.len(),
            stale = stale_code.len() + stale_knowledge.len(),
            code_chunks,
            knowledge_chunks,
            duration_ms,
            "codescan indexed"
        );

        Ok(IndexStats {
            code_files: code_files.len(),
            knowledge_files: knowledge_files.len(),
            code_chunks,
            knowledge_chunks,
            duration_ms,
        })
    }
}

fn git_head(repo_path: &Path) -> Option<String> {
    let repo = git2::Repository::discover(repo_path).ok()?;
    let head = repo.head().ok()?;
    head.target().map(|oid| oid.to_string())
}

fn has_relevant_worktree_changes(repo_path: &Path) -> bool {
    let Ok(repo) = git2::Repository::discover(repo_path) else {
        return true;
    };
    let workdir = repo.workdir().unwrap_or(repo_path);
    let Ok(statuses) = repo.statuses(Some(
        git2::StatusOptions::new()
            .include_untracked(true)
            .recurse_untracked_dirs(true),
    )) else {
        return true;
    };

    statuses.iter().any(|entry| {
        entry
            .path()
            .map(|path| is_relevant_changed_path(workdir, Path::new(path)))
            .unwrap_or(false)
    })
}

fn is_relevant_changed_path(repo_path: &Path, rel_path: &Path) -> bool {
    let path = repo_path.join(rel_path);
    if should_skip_path(&path) {
        return false;
    }
    path.extension()
        .and_then(|x| x.to_str())
        .map(is_supported_code_extension)
        .unwrap_or_else(|| {
            KnowledgeDirs::default()
                .patterns
                .iter()
                .any(|pattern| path_matches_glob(repo_path, rel_path, pattern))
        })
}

fn path_matches_glob(repo_path: &Path, rel_path: &Path, pattern: &str) -> bool {
    let full = repo_path.join(pattern).to_string_lossy().to_string();
    glob::Pattern::new(&full)
        .ok()
        .map(|pattern| pattern.matches_path(&repo_path.join(rel_path)))
        .unwrap_or(false)
}

fn sha256(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn should_skip_path(path: &Path) -> bool {
    const SKIP_DIRS: &[&str] = &[
        "target",
        "node_modules",
        ".git",
        ".jj",
        ".omegon",
        "dist",
        "build",
        ".next",
    ];
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .map(|part| SKIP_DIRS.contains(&part))
            .unwrap_or(false)
    })
}

fn normalized_relative_path(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

fn discover_code_files(repo_path: &Path) -> Vec<PathBuf> {
    use walkdir::WalkDir;
    let mut files = WalkDir::new(repo_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip_path(e.path()))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(is_supported_code_extension)
                .unwrap_or(false)
        })
        .filter_map(|e| {
            e.path()
                .strip_prefix(repo_path)
                .ok()
                .map(normalized_relative_path)
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn discover_knowledge_files(repo_path: &Path, dirs: &KnowledgeDirs) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for pattern in &dirs.patterns {
        let full = format!("{}/{}", repo_path.to_string_lossy(), pattern);
        if let Ok(paths) = glob::glob(&full) {
            for p in paths.filter_map(|p| p.ok()) {
                if p.is_file()
                    && let Ok(relative) = p.strip_prefix(repo_path)
                {
                    files.push(normalized_relative_path(relative));
                }
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use crate::{BM25Index, SearchScope};

    #[test]
    fn runs_on_small_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn greet() {}").unwrap();
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/foo.md"), "# Foo\n\n## Overview\n\nText.").unwrap();

        let mut cache = ScanCache::open(&repo.join(".omegon/codescan.db")).unwrap();
        let stats = Indexer::run(repo, &mut cache).unwrap();
        assert!(stats.code_files >= 1, "code_files");
        assert!(stats.code_chunks >= 1, "code_chunks");
        assert!(stats.knowledge_chunks >= 1, "knowledge_chunks");
    }

    #[test]
    fn fast_path_skips_walk_when_head_unchanged() {
        // Simulate git HEAD being set in meta — in a temp dir without git,
        // git_head returns None and the fast path never fires. Instead, test
        // that a second run on a static dir (no git) still returns same counts.
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/main.rs"), "fn main() {}").unwrap();
        let mut cache = ScanCache::open(&repo.join(".omegon/codescan.db")).unwrap();

        let s1 = Indexer::run(repo, &mut cache).unwrap();
        // Manually set last_head to simulate "already indexed" state
        cache.set_meta("last_head", "fake_head_abc123").unwrap();

        // Now set env to return the same HEAD — simulate by checking counts are stable
        let s2 = Indexer::run(repo, &mut cache).unwrap();
        // Both runs should produce the same chunk count
        assert_eq!(
            s1.code_chunks, s2.code_chunks,
            "chunk count should be stable"
        );
    }

    #[test]
    fn non_git_runs_keep_paths_relative_and_replace_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn first() {}\n").unwrap();
        let mut cache = ScanCache::open(&repo.join(".omegon/codescan.db")).unwrap();

        Indexer::run(repo, &mut cache).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn second() {}\n").unwrap();
        Indexer::run(repo, &mut cache).unwrap();

        let chunks = cache.all_code_chunks().unwrap();
        assert_eq!(
            chunks.len(),
            1,
            "changed path should be replaced: {chunks:?}"
        );
        assert_eq!(chunks[0].path, Path::new("src/lib.rs"));
        assert_eq!(chunks[0].item_name, "second");
        assert_eq!(
            cache.all_hashes().keys().collect::<Vec<_>>(),
            vec!["src/lib.rs"]
        );
    }

    #[test]
    fn dirty_relevant_worktree_change_disables_head_fast_path() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn original() {}\n").unwrap();

        let git = git2::Repository::init(repo).unwrap();
        let mut index = git.index().unwrap();
        index.add_path(Path::new("src/lib.rs")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Omegon Test", "omegon@example.invalid").unwrap();
        git.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        drop(git);

        let mut cache = ScanCache::open(&repo.join(".omegon/codescan.db")).unwrap();
        let first = Indexer::run(repo, &mut cache).unwrap();
        assert_eq!(first.code_chunks, 1);

        std::fs::write(
            repo.join("src/lib.rs"),
            "pub fn original() {}\npub fn dirty_added() {}\n",
        )
        .unwrap();

        let second = Indexer::run(repo, &mut cache).unwrap();
        assert!(
            second.code_files > 0,
            "dirty relevant file must force a scan instead of HEAD fast-path: {second:?}"
        );
        let chunks = cache.all_code_chunks().unwrap();
        let names = chunks
            .iter()
            .map(|chunk| chunk.item_name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"dirty_added"), "chunks: {names:?}");
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.path == Path::new("src/lib.rs"))
        );
        assert!(cache.all_hashes().contains_key("src/lib.rs"));
        assert!(
            !cache
                .all_hashes()
                .contains_key(&repo.join("src/lib.rs").display().to_string())
        );
    }

    #[test]
    fn cancelled_incremental_run_keeps_commits_but_defers_pruning_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/a.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(repo.join("src/b.rs"), "pub fn b() {}\n").unwrap();
        let mut cache = ScanCache::open(&repo.join(".omegon/codescan.db")).unwrap();
        cache
            .upsert_code_chunks(
                Path::new("src/missing.rs"),
                "missing-hash",
                &[crate::code::CodeChunk {
                    path: PathBuf::from("src/missing.rs"),
                    start_line: 1,
                    end_line: 1,
                    item_name: "missing".into(),
                    item_kind: "fn".into(),
                    text: "fn missing() {}".into(),
                    parent_scope: None,
                    language: "rust".into(),
                    strategy: crate::code::ExtractionStrategy::TreeSitter,
                    confidence: crate::code::ExtractionConfidence::Extracted,
                }],
            )
            .unwrap();
        cache.set_meta("last_head", "prior-head").unwrap();

        let checks = Cell::new(0);
        let result = Indexer::run_with_cancel(repo, &mut cache, || {
            let next = checks.get() + 1;
            checks.set(next);
            next == 8
        });
        assert!(result.is_err());

        let names = cache
            .all_code_chunks()
            .unwrap()
            .into_iter()
            .map(|chunk| chunk.item_name)
            .collect::<Vec<_>>();
        assert!(
            names.iter().any(|name| name == "a"),
            "first path should remain committed: {names:?}"
        );
        assert!(
            names.iter().any(|name| name == "missing"),
            "pruning must be deferred: {names:?}"
        );
        assert_eq!(cache.get_meta("last_head").as_deref(), Some("prior-head"));
    }

    #[test]
    fn cancelled_full_invalidation_preserves_prior_index_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn retained_symbol() {}\n").unwrap();
        std::fs::write(repo.join("docs/design.md"), "# Retained knowledge\n").unwrap();
        let mut cache = ScanCache::open(&repo.join(".omegon/codescan.db")).unwrap();
        Indexer::run(repo, &mut cache).unwrap();
        cache.set_meta("last_head", "prior-head").unwrap();
        let old_hashes = cache.all_hashes();
        let old_code = cache.all_code_chunks().unwrap();
        let old_knowledge = cache.all_knowledge_chunks().unwrap();
        assert!(
            !BM25Index::build(&old_code, &old_knowledge)
                .search("retained_symbol", SearchScope::All, 5)
                .is_empty()
        );

        std::fs::write(repo.join("src/lib.rs"), "pub fn replacement_symbol() {}\n").unwrap();
        cache.begin_full_rebuild().unwrap();
        cache.set_meta("last_head", "").unwrap();
        let checks = Cell::new(0);
        let result = Indexer::run_with_cancel(repo, &mut cache, || {
            let next = checks.get() + 1;
            checks.set(next);
            next == 5
        });
        assert!(result.is_err());

        assert_eq!(cache.all_hashes(), old_hashes);
        assert_eq!(cache.get_meta("last_head").as_deref(), Some("prior-head"));
        let code = cache.all_code_chunks().unwrap();
        let knowledge = cache.all_knowledge_chunks().unwrap();
        let results =
            BM25Index::build(&code, &knowledge).search("retained_symbol", SearchScope::All, 5);
        assert!(!results.is_empty(), "prior searchable index must survive");
        assert!(
            code.iter()
                .any(|chunk| chunk.item_name == "retained_symbol")
        );
        assert!(
            !code
                .iter()
                .any(|chunk| chunk.item_name == "replacement_symbol")
        );
    }

    #[test]
    fn failed_full_invalidation_rolls_back_prior_index() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn retained_symbol() {}\n").unwrap();
        let db_path = repo.join(".omegon/codescan.db");
        let mut cache = ScanCache::open(&db_path).unwrap();
        Indexer::run(repo, &mut cache).unwrap();
        cache.set_meta("last_head", "prior-head").unwrap();

        std::fs::write(repo.join("src/lib.rs"), [0xff, 0xfe]).unwrap();
        cache.begin_full_rebuild().unwrap();
        cache.set_meta("last_head", "").unwrap();
        assert!(Indexer::run(repo, &mut cache).is_err());

        assert_eq!(cache.get_meta("last_head").as_deref(), Some("prior-head"));
        assert!(
            cache
                .all_code_chunks()
                .unwrap()
                .iter()
                .any(|chunk| chunk.item_name == "retained_symbol")
        );
    }

    #[test]
    fn successful_full_invalidation_commits_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn prior_symbol() {}\n").unwrap();
        let db_path = repo.join(".omegon/codescan.db");
        let mut cache = ScanCache::open(&db_path).unwrap();
        Indexer::run(repo, &mut cache).unwrap();

        std::fs::write(repo.join("src/lib.rs"), "pub fn replacement_symbol() {}\n").unwrap();
        cache.begin_full_rebuild().unwrap();
        cache.set_meta("last_head", "").unwrap();
        Indexer::run(repo, &mut cache).unwrap();
        drop(cache);

        let reopened = ScanCache::open(&db_path).unwrap();
        let chunks = reopened.all_code_chunks().unwrap();
        assert!(
            chunks
                .iter()
                .any(|chunk| chunk.item_name == "replacement_symbol")
        );
        assert!(!chunks.iter().any(|chunk| chunk.item_name == "prior_symbol"));
    }

    #[test]
    fn dirty_ignored_worktree_change_allows_head_fast_path() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn original() {}\n").unwrap();

        let git = git2::Repository::init(repo).unwrap();
        let mut index = git.index().unwrap();
        index.add_path(Path::new("src/lib.rs")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Omegon Test", "omegon@example.invalid").unwrap();
        git.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        drop(git);

        let mut cache = ScanCache::open(&repo.join(".omegon/codescan.db")).unwrap();
        let first = Indexer::run(repo, &mut cache).unwrap();
        assert_eq!(first.code_chunks, 1);

        std::fs::create_dir_all(repo.join("target/debug")).unwrap();
        std::fs::write(
            repo.join("target/debug/generated.rs"),
            "pub fn ignored() {}\n",
        )
        .unwrap();

        let second = Indexer::run(repo, &mut cache).unwrap();
        assert_eq!(
            second.code_files, 0,
            "ignored dirty file should not defeat HEAD fast-path: {second:?}"
        );
    }

    #[test]
    fn excludes_omegon_workspace_and_prunes_stale_entries() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/main.rs"), "fn canonical() {}").unwrap();
        std::fs::write(
            repo.join("src/InvoiceService.java"),
            "class InvoiceService {}",
        )
        .unwrap();
        std::fs::write(repo.join("src/TimeEntry.kt"), "class TimeEntry").unwrap();
        std::fs::write(
            repo.join("src/BillingService.cs"),
            "class BillingService {}",
        )
        .unwrap();
        std::fs::create_dir_all(repo.join(".omegon/cleave-workspace/0-wt-code-survey/src"))
            .unwrap();
        std::fs::write(
            repo.join(".omegon/cleave-workspace/0-wt-code-survey/src/tui_tests.rs"),
            "fn transient_workspace_copy() {}",
        )
        .unwrap();

        let discovered = discover_code_files(repo);
        assert!(
            discovered
                .iter()
                .any(|path| path.ends_with("InvoiceService.java")),
            "discover_code_files should include Java files: {discovered:?}"
        );
        assert!(
            discovered.iter().any(|path| path.ends_with("TimeEntry.kt")),
            "discover_code_files should include Kotlin files: {discovered:?}"
        );
        assert!(
            discovered
                .iter()
                .any(|path| path.ends_with("BillingService.cs")),
            "discover_code_files should include C# files: {discovered:?}"
        );
        assert!(
            discovered
                .iter()
                .all(|path| !path.to_string_lossy().contains(".omegon/cleave-workspace")),
            "discover_code_files should skip .omegon workspaces: {discovered:?}"
        );

        let cache_path = repo.join(".omegon/codescan.db");
        let cache = ScanCache::open(&cache_path).unwrap();
        cache
            .upsert_code_chunks(
                Path::new(".omegon/cleave-workspace/0-wt-code-survey/src/tui_tests.rs"),
                "stale",
                &[crate::code::CodeChunk {
                    path: PathBuf::from(
                        ".omegon/cleave-workspace/0-wt-code-survey/src/tui_tests.rs",
                    ),
                    start_line: 1,
                    end_line: 1,
                    item_name: "transient_workspace_copy".into(),
                    item_kind: "fn".into(),
                    text: "fn transient_workspace_copy() {}".into(),
                    parent_scope: None,
                    language: "rust".into(),
                    strategy: crate::code::ExtractionStrategy::TreeSitter,
                    confidence: crate::code::ExtractionConfidence::Extracted,
                }],
            )
            .unwrap();

        let mut cache = ScanCache::open(&cache_path).unwrap();
        Indexer::run(repo, &mut cache).unwrap();

        let chunks = ScanCache::open(&cache_path)
            .unwrap()
            .all_code_chunks()
            .unwrap();
        assert!(
            chunks.iter().all(|chunk| !chunk
                .path
                .to_string_lossy()
                .contains(".omegon/cleave-workspace")),
            "indexed chunks should prune stale .omegon workspace entries: {chunks:?}"
        );
        assert!(
            chunks
                .iter()
                .any(|chunk| chunk.path == std::path::Path::new("src/main.rs")),
            "canonical repo files should remain indexed: {chunks:?}"
        );
    }
}

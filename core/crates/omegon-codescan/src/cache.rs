//! SQLite-backed chunk cache at `.omegon/codescan.db`.
//!
//! Keyed by (path, content_hash). Incremental invalidation: only files
//! whose content_hash has changed since last index need re-chunking.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::code::CodeChunk;
use crate::knowledge::KnowledgeChunk;

pub struct ScanCache {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FileKind {
    Code,
    Knowledge,
}

impl FileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Knowledge => "knowledge",
        }
    }
}

impl ScanCache {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).context("failed to create codescan dir")?;
        }
        let conn = Connection::open(db_path).context("failed to open codescan.db")?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS code_chunks (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL,
                 start_line INTEGER NOT NULL,
                 end_line INTEGER NOT NULL,
                 item_name TEXT NOT NULL,
                 item_kind TEXT NOT NULL,
                 text TEXT NOT NULL,
                 content_hash TEXT NOT NULL,
                 parent_scope TEXT,
                 language TEXT NOT NULL DEFAULT '',
                 strategy TEXT NOT NULL DEFAULT 'regex',
                 confidence TEXT NOT NULL DEFAULT 'inferred'
             );
             CREATE INDEX IF NOT EXISTS idx_code_path ON code_chunks(path);
             CREATE TABLE IF NOT EXISTS knowledge_chunks (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL,
                 heading TEXT NOT NULL,
                 start_line INTEGER NOT NULL,
                 end_line INTEGER NOT NULL,
                 tags TEXT NOT NULL,
                 text TEXT NOT NULL,
                 content_hash TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_knowledge_path ON knowledge_chunks(path);
             CREATE TABLE IF NOT EXISTS file_state (
                 path TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 content_hash TEXT NOT NULL,
                 PRIMARY KEY (path, kind)
             );
             CREATE TABLE IF NOT EXISTS meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .context("failed to initialize codescan.db schema")?;
        let cache = Self { conn };
        cache.ensure_code_chunk_metadata_columns()?;
        cache.migrate_file_state()?;
        Ok(cache)
    }

    fn ensure_code_chunk_metadata_columns(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(code_chunks)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
        for (column, definition) in [
            ("language", "TEXT NOT NULL DEFAULT ''"),
            ("strategy", "TEXT NOT NULL DEFAULT 'regex'"),
            ("confidence", "TEXT NOT NULL DEFAULT 'inferred'"),
        ] {
            if !columns.contains(column) {
                self.conn.execute(
                    &format!("ALTER TABLE code_chunks ADD COLUMN {column} {definition}"),
                    [],
                )?;
            }
        }
        Ok(())
    }

    fn migrate_file_state(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.conn.execute_batch(
            "INSERT OR IGNORE INTO file_state (path, kind, content_hash)
             SELECT path, 'code', MIN(content_hash) FROM code_chunks
             GROUP BY path HAVING COUNT(DISTINCT content_hash) = 1;
             INSERT OR IGNORE INTO file_state (path, kind, content_hash)
             SELECT path, 'knowledge', MIN(content_hash) FROM knowledge_chunks
             GROUP BY path HAVING COUNT(DISTINCT content_hash) = 1;
             COMMIT;",
        );
        if let Err(error) = result {
            let _ = self.conn.execute_batch("ROLLBACK");
            return Err(error).context("failed to migrate codescan file state");
        }
        Ok(())
    }

    /// Return a (path → content_hash) map for ALL indexed files.
    ///
    /// Used by the indexer to batch-compare hashes without N individual queries.
    pub fn all_hashes(&self) -> HashMap<String, String> {
        self.try_all_hashes().unwrap_or_default()
    }

    fn try_all_hashes(&self) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT path, content_hash FROM file_state
                 ORDER BY CASE kind WHEN 'code' THEN 0 ELSE 1 END, path",
        )?;
        for row in stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (path, hash) = row?;
            map.entry(path).or_insert(hash);
        }
        Ok(map)
    }

    /// Return paths whose content_hash has changed (or are not yet indexed).
    ///
    /// Uses a single pair of batch DB queries instead of N individual queries.
    pub fn stale_paths(&self, paths: &[(PathBuf, String)]) -> Vec<PathBuf> {
        let cached = self.all_hashes();
        paths
            .iter()
            .filter(|(p, new_hash)| {
                cached
                    .get(p.to_string_lossy().as_ref())
                    .map(|h| h != new_hash)
                    .unwrap_or(true) // not cached → stale
            })
            .map(|(p, _)| p.clone())
            .collect()
    }

    pub(crate) fn stale_paths_for_kind(
        &self,
        kind: FileKind,
        paths: &[(PathBuf, String)],
    ) -> Result<Vec<PathBuf>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, content_hash FROM file_state WHERE kind = ?1")?;
        let cached = stmt
            .query_map(params![kind.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(paths
            .iter()
            .filter(|(path, hash)| {
                cached
                    .get(path.to_string_lossy().as_ref())
                    .map(|cached_hash| cached_hash != hash)
                    .unwrap_or(true)
            })
            .map(|(path, _)| path.clone())
            .collect())
    }

    pub fn upsert_code_chunks(&self, path: &Path, hash: &str, chunks: &[CodeChunk]) -> Result<()> {
        self.upsert_code_chunks_with_cancel(path, hash, chunks, || false)
    }

    pub(crate) fn upsert_code_chunks_with_cancel(
        &self,
        path: &Path,
        hash: &str,
        chunks: &[CodeChunk],
        is_cancelled: impl Fn() -> bool,
    ) -> Result<()> {
        let path_str = path.to_string_lossy();
        self.replace_path(FileKind::Code, &path_str, hash, &is_cancelled, || {
            self.conn.execute(
                "DELETE FROM code_chunks WHERE path = ?1",
                params![path_str],
            )?;
            for chunk in chunks {
                self.conn.execute(
                "INSERT INTO code_chunks (path, start_line, end_line, item_name, item_kind, text, content_hash, parent_scope, language, strategy, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    path_str,
                    chunk.start_line,
                    chunk.end_line,
                    chunk.item_name,
                    chunk.item_kind,
                    chunk.text,
                    hash,
                    chunk.parent_scope,
                    chunk.language,
                    chunk.strategy.as_str(),
                    chunk.confidence.as_str(),
                ],
                )?;
            }
            Ok(())
        })
    }

    pub fn upsert_knowledge_chunks(
        &self,
        path: &Path,
        hash: &str,
        chunks: &[KnowledgeChunk],
    ) -> Result<()> {
        self.upsert_knowledge_chunks_with_cancel(path, hash, chunks, || false)
    }

    pub(crate) fn upsert_knowledge_chunks_with_cancel(
        &self,
        path: &Path,
        hash: &str,
        chunks: &[KnowledgeChunk],
        is_cancelled: impl Fn() -> bool,
    ) -> Result<()> {
        let path_str = path.to_string_lossy();
        self.replace_path(FileKind::Knowledge, &path_str, hash, &is_cancelled, || {
            self.conn.execute(
                "DELETE FROM knowledge_chunks WHERE path = ?1",
                params![path_str],
            )?;
            for chunk in chunks {
                self.conn.execute(
                "INSERT INTO knowledge_chunks (path, heading, start_line, end_line, tags, text, content_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![path_str, chunk.heading, chunk.start_line, chunk.end_line, chunk.tags.join(","), chunk.text, hash],
                )?;
            }
            Ok(())
        })
    }

    fn replace_path(
        &self,
        kind: FileKind,
        path: &str,
        hash: &str,
        is_cancelled: &impl Fn() -> bool,
        replace_chunks: impl FnOnce() -> rusqlite::Result<()>,
    ) -> Result<()> {
        let owns_transaction = self.conn.is_autocommit();
        if owns_transaction {
            self.conn.execute_batch("BEGIN IMMEDIATE")?;
        }
        let result = (|| -> Result<()> {
            replace_chunks()?;
            self.conn.execute(
                "INSERT OR REPLACE INTO file_state (path, kind, content_hash) VALUES (?1, ?2, ?3)",
                params![path, kind.as_str(), hash],
            )?;
            if is_cancelled() {
                anyhow::bail!("codebase index cancelled");
            }
            if owns_transaction {
                self.conn.execute_batch("COMMIT")?;
            }
            Ok(())
        })();
        if result.is_err() && owns_transaction {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        result
    }

    pub fn prune_missing_paths(&self, live_paths: &HashSet<PathBuf>) -> Result<()> {
        let live_files = live_paths
            .iter()
            .flat_map(|path| {
                [
                    (path.clone(), FileKind::Code),
                    (path.clone(), FileKind::Knowledge),
                ]
            })
            .collect();
        self.prune_missing_files(&live_files)
    }

    pub(crate) fn prune_missing_files(
        &self,
        live_files: &HashSet<(PathBuf, FileKind)>,
    ) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT path, kind FROM file_state")?;
        let cached_files = stmt
            .query_map([], |row| {
                let kind = match row.get::<_, String>(1)?.as_str() {
                    "code" => FileKind::Code,
                    "knowledge" => FileKind::Knowledge,
                    value => {
                        return Err(rusqlite::Error::InvalidParameterName(format!(
                            "unknown file kind {value}"
                        )));
                    }
                };
                Ok((PathBuf::from(row.get::<_, String>(0)?), kind))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for (path, kind) in cached_files {
            if live_files.contains(&(path.clone(), kind)) {
                continue;
            }
            let path_str = path.to_string_lossy();
            let table = match kind {
                FileKind::Code => "code_chunks",
                FileKind::Knowledge => "knowledge_chunks",
            };
            self.conn.execute(
                &format!("DELETE FROM {table} WHERE path = ?1"),
                params![path_str.as_ref()],
            )?;
            self.conn.execute(
                "DELETE FROM file_state WHERE path = ?1 AND kind = ?2",
                params![path_str.as_ref(), kind.as_str()],
            )?;
        }
        Ok(())
    }

    pub(crate) fn publish_successful_run(
        &self,
        live_files: &HashSet<(PathBuf, FileKind)>,
        head: Option<&str>,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<()> {
        let owns_transaction = self.conn.is_autocommit();
        if owns_transaction {
            self.conn.execute_batch("BEGIN IMMEDIATE")?;
        }
        let result = (|| -> Result<()> {
            self.prune_missing_files(live_files)?;
            if let Some(head) = head {
                self.set_meta("last_head", head)?;
            }
            if is_cancelled() {
                anyhow::bail!("codebase index cancelled");
            }
            if owns_transaction {
                self.conn.execute_batch("COMMIT")?;
            }
            Ok(())
        })();
        if result.is_err() && owns_transaction {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        result
    }

    pub fn all_code_chunks(&self) -> Result<Vec<CodeChunk>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, start_line, end_line, item_name, item_kind, text, parent_scope, language, strategy, confidence FROM code_chunks",
        )?;
        let chunks = stmt
            .query_map([], |row| {
                Ok(CodeChunk {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    start_line: row.get(1)?,
                    end_line: row.get(2)?,
                    item_name: row.get(3)?,
                    item_kind: row.get(4)?,
                    text: row.get(5)?,
                    parent_scope: row.get(6)?,
                    language: row.get(7)?,
                    strategy: crate::code::ExtractionStrategy::parse(&row.get::<_, String>(8)?),
                    confidence: crate::code::ExtractionConfidence::parse(&row.get::<_, String>(9)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(chunks)
    }

    pub fn all_knowledge_chunks(&self) -> Result<Vec<KnowledgeChunk>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, heading, start_line, end_line, tags, text FROM knowledge_chunks",
        )?;
        let chunks = stmt
            .query_map([], |row| {
                let tags_str: String = row.get(4)?;
                Ok(KnowledgeChunk {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    heading: row.get(1)?,
                    start_line: row.get(2)?,
                    end_line: row.get(3)?,
                    tags: tags_str
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect(),
                    text: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(chunks)
    }

    pub fn get_meta(&self, key: &str) -> Option<String> {
        self.try_get_meta(key).ok().flatten()
    }

    pub(crate) fn try_get_meta(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<()> {
        if !self.conn.is_autocommit() {
            anyhow::bail!("codescan transaction already active");
        }
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.conn.execute_batch(
            "DELETE FROM code_chunks;
             DELETE FROM knowledge_chunks;
             DELETE FROM file_state;
             DELETE FROM meta WHERE key = 'last_head';",
        );
        if let Err(error) = result {
            let _ = self.conn.execute_batch("ROLLBACK");
            return Err(error.into());
        }
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn begin_full_rebuild(&self) -> Result<()> {
        if !self.conn.is_autocommit() {
            anyhow::bail!("codescan transaction already active");
        }
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.conn.execute_batch(
            "DELETE FROM code_chunks;
             DELETE FROM knowledge_chunks;
             DELETE FROM file_state;",
        );
        if let Err(error) = result {
            let _ = self.conn.execute_batch("ROLLBACK");
            return Err(error.into());
        }
        Ok(())
    }

    pub(crate) fn full_rebuild_active(&self) -> bool {
        !self.conn.is_autocommit()
    }

    pub(crate) fn commit_full_rebuild(&self) -> Result<()> {
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }

    pub(crate) fn rollback_full_rebuild(&self) -> Result<()> {
        self.conn.execute_batch("ROLLBACK")?;
        Ok(())
    }

    /// Count of indexed code chunks.
    pub fn code_chunk_count(&self) -> usize {
        self.try_code_chunk_count().unwrap_or(0)
    }

    pub(crate) fn try_code_chunk_count(&self) -> Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM code_chunks", [], |r| r.get(0))
            .map_err(Into::into)
    }

    /// Count of indexed knowledge chunks.
    pub fn knowledge_chunk_count(&self) -> usize {
        self.try_knowledge_chunk_count().unwrap_or(0)
    }

    pub(crate) fn try_knowledge_chunk_count(&self) -> Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM knowledge_chunks", [], |r| r.get(0))
            .map_err(Into::into)
    }

    pub(crate) fn try_file_state_count(&self) -> Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM file_state", [], |row| row.get(0))
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trip_code() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ScanCache::open(&dir.path().join("t.db")).unwrap();
        let path = Path::new("src/foo.rs");
        let chunk = CodeChunk {
            path: path.to_path_buf(),
            start_line: 1,
            end_line: 10,
            item_name: "foo".into(),
            item_kind: "fn".into(),
            text: "fn foo() {}".into(),
            parent_scope: None,
            language: "rust".into(),
            strategy: crate::code::ExtractionStrategy::TreeSitter,
            confidence: crate::code::ExtractionConfidence::Extracted,
        };
        cache.upsert_code_chunks(path, "h1", &[chunk]).unwrap();
        let loaded = cache.all_code_chunks().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].item_name, "foo");
        assert_eq!(loaded[0].language, "rust");
        assert_eq!(
            loaded[0].strategy,
            crate::code::ExtractionStrategy::TreeSitter
        );
        assert_eq!(
            loaded[0].confidence,
            crate::code::ExtractionConfidence::Extracted
        );
    }

    #[test]
    fn open_migrates_legacy_code_chunk_schema() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("legacy.db");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE code_chunks (
                    id INTEGER PRIMARY KEY,
                    path TEXT NOT NULL,
                    start_line INTEGER NOT NULL,
                    end_line INTEGER NOT NULL,
                    item_name TEXT NOT NULL,
                    item_kind TEXT NOT NULL,
                    text TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    parent_scope TEXT
                );
                INSERT INTO code_chunks
                    (path, start_line, end_line, item_name, item_kind, text, content_hash)
                VALUES
                    ('src/mixed.rs', 1, 1, 'old', 'fn', 'fn old() {}', 'old-hash'),
                    ('src/mixed.rs', 2, 2, 'new', 'fn', 'fn new() {}', 'new-hash');",
            )
            .unwrap();
        }

        let cache = ScanCache::open(&db_path).unwrap();
        let columns = {
            let mut stmt = cache
                .conn
                .prepare("PRAGMA table_info(code_chunks)")
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>()
        };
        assert!(columns.contains(&"language".to_string()));
        assert!(columns.contains(&"strategy".to_string()));
        assert!(columns.contains(&"confidence".to_string()));
        assert_eq!(
            cache
                .stale_paths_for_kind(
                    FileKind::Code,
                    &[(PathBuf::from("src/mixed.rs"), "old-hash".into())],
                )
                .unwrap(),
            vec![PathBuf::from("src/mixed.rs")],
            "inconsistent legacy rows must be reindexed instead of trusted"
        );
    }

    #[test]
    fn cache_round_trip_knowledge() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ScanCache::open(&dir.path().join("t.db")).unwrap();
        let path = Path::new("docs/foo.md");
        let chunk = KnowledgeChunk {
            path: path.to_path_buf(),
            heading: "Overview".into(),
            start_line: 3,
            end_line: 15,
            tags: vec!["arch".into()],
            text: "text".into(),
        };
        cache.upsert_knowledge_chunks(path, "h1", &[chunk]).unwrap();
        let loaded = cache.all_knowledge_chunks().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].heading, "Overview");
    }

    #[test]
    fn stale_paths_uses_batch_query() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ScanCache::open(&dir.path().join("t.db")).unwrap();
        let path_a = PathBuf::from("a.rs");
        let chunk = CodeChunk {
            path: path_a.clone(),
            start_line: 1,
            end_line: 1,
            item_name: "a".into(),
            item_kind: "fn".into(),
            text: "fn a(){}".into(),
            parent_scope: None,
            language: "rust".into(),
            strategy: crate::code::ExtractionStrategy::TreeSitter,
            confidence: crate::code::ExtractionConfidence::Extracted,
        };
        cache
            .upsert_code_chunks(&path_a, "hash_a", &[chunk])
            .unwrap();

        let stale = cache.stale_paths(&[
            (path_a.clone(), "hash_a".into()),     // not stale
            (path_a.clone(), "hash_new".into()),   // stale (changed)
            (PathBuf::from("b.rs"), "any".into()), // stale (new)
        ]);
        assert_eq!(stale.len(), 2, "should detect changed + new: {:?}", stale);
    }

    #[test]
    fn all_hashes_returns_correct_map() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ScanCache::open(&dir.path().join("t.db")).unwrap();
        let chunk = CodeChunk {
            path: PathBuf::from("x.rs"),
            start_line: 1,
            end_line: 1,
            item_name: "x".into(),
            item_kind: "fn".into(),
            text: "".into(),
            parent_scope: None,
            language: "rust".into(),
            strategy: crate::code::ExtractionStrategy::TreeSitter,
            confidence: crate::code::ExtractionConfidence::Extracted,
        };
        cache
            .upsert_code_chunks(Path::new("x.rs"), "abc123", &[chunk])
            .unwrap();
        let hashes = cache.all_hashes();
        assert_eq!(hashes.get("x.rs"), Some(&"abc123".to_string()));
    }

    #[test]
    fn zero_chunk_files_retain_typed_hash_state_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        {
            let cache = ScanCache::open(&db_path).unwrap();
            cache
                .upsert_code_chunks(Path::new("src/empty.rs"), "code-hash", &[])
                .unwrap();
            cache
                .upsert_knowledge_chunks(Path::new("docs/empty.md"), "knowledge-hash", &[])
                .unwrap();
            assert_eq!(cache.code_chunk_count(), 0);
            assert_eq!(cache.knowledge_chunk_count(), 0);
        }

        let cache = ScanCache::open(&db_path).unwrap();
        assert!(
            cache
                .stale_paths_for_kind(
                    FileKind::Code,
                    &[(PathBuf::from("src/empty.rs"), "code-hash".into())],
                )
                .unwrap()
                .is_empty()
        );
        assert!(
            cache
                .stale_paths_for_kind(
                    FileKind::Knowledge,
                    &[(PathBuf::from("docs/empty.md"), "knowledge-hash".into())],
                )
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn cancelled_publication_rolls_back_pruning_and_head() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ScanCache::open(&dir.path().join("t.db")).unwrap();
        cache
            .upsert_code_chunks(Path::new("src/prior.rs"), "prior-hash", &[])
            .unwrap();
        cache.set_meta("last_head", "prior-head").unwrap();

        let result =
            cache.publish_successful_run(&HashSet::new(), Some("replacement-head"), || true);

        assert!(result.is_err());
        assert_eq!(
            cache.all_hashes().get("src/prior.rs").map(String::as_str),
            Some("prior-hash")
        );
        assert_eq!(cache.get_meta("last_head").as_deref(), Some("prior-head"));
    }

    #[test]
    fn cancelled_code_and_knowledge_replacements_roll_back() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ScanCache::open(&dir.path().join("t.db")).unwrap();
        let code_path = Path::new("src/lib.rs");
        let knowledge_path = Path::new("docs/design.md");
        let code_chunk = |name: &str| CodeChunk {
            path: code_path.to_path_buf(),
            start_line: 1,
            end_line: 1,
            item_name: name.into(),
            item_kind: "fn".into(),
            text: format!("fn {name}() {{}}"),
            parent_scope: None,
            language: "rust".into(),
            strategy: crate::code::ExtractionStrategy::TreeSitter,
            confidence: crate::code::ExtractionConfidence::Extracted,
        };
        let knowledge_chunk = |heading: &str| KnowledgeChunk {
            path: knowledge_path.to_path_buf(),
            heading: heading.into(),
            start_line: 1,
            end_line: 1,
            tags: vec![],
            text: heading.into(),
        };
        cache
            .upsert_code_chunks(code_path, "old-code", &[code_chunk("old")])
            .unwrap();
        cache
            .upsert_knowledge_chunks(knowledge_path, "old-knowledge", &[knowledge_chunk("Old")])
            .unwrap();

        assert!(
            cache
                .upsert_code_chunks_with_cancel(code_path, "new-code", &[code_chunk("new")], || {
                    true
                },)
                .is_err()
        );
        assert!(
            cache
                .upsert_knowledge_chunks_with_cancel(
                    knowledge_path,
                    "new-knowledge",
                    &[knowledge_chunk("New")],
                    || true,
                )
                .is_err()
        );

        assert_eq!(cache.all_code_chunks().unwrap()[0].item_name, "old");
        assert_eq!(cache.all_knowledge_chunks().unwrap()[0].heading, "Old");
        assert!(
            cache
                .stale_paths_for_kind(
                    FileKind::Code,
                    &[(code_path.to_path_buf(), "old-code".into())],
                )
                .unwrap()
                .is_empty()
        );
        assert!(
            cache
                .stale_paths_for_kind(
                    FileKind::Knowledge,
                    &[(knowledge_path.to_path_buf(), "old-knowledge".into())],
                )
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn meta_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ScanCache::open(&dir.path().join("t.db")).unwrap();
        assert_eq!(cache.get_meta("k"), None);
        cache.set_meta("k", "v").unwrap();
        assert_eq!(cache.get_meta("k"), Some("v".into()));
    }

    #[test]
    fn prune_missing_paths_removes_absent_code_and_knowledge_rows() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ScanCache::open(&dir.path().join("t.db")).unwrap();

        cache
            .upsert_code_chunks(
                Path::new("src/keep.rs"),
                "h1",
                &[CodeChunk {
                    path: PathBuf::from("src/keep.rs"),
                    start_line: 1,
                    end_line: 1,
                    item_name: "keep".into(),
                    item_kind: "fn".into(),
                    text: "fn keep() {}".into(),
                    parent_scope: None,
                    language: "rust".into(),
                    strategy: crate::code::ExtractionStrategy::TreeSitter,
                    confidence: crate::code::ExtractionConfidence::Extracted,
                }],
            )
            .unwrap();
        cache
            .upsert_code_chunks(
                Path::new(".omegon/cleave-workspace/stale.rs"),
                "h2",
                &[CodeChunk {
                    path: PathBuf::from(".omegon/cleave-workspace/stale.rs"),
                    start_line: 1,
                    end_line: 1,
                    item_name: "stale".into(),
                    item_kind: "fn".into(),
                    text: "fn stale() {}".into(),
                    parent_scope: None,
                    language: "rust".into(),
                    strategy: crate::code::ExtractionStrategy::TreeSitter,
                    confidence: crate::code::ExtractionConfidence::Extracted,
                }],
            )
            .unwrap();
        cache
            .upsert_knowledge_chunks(
                Path::new("docs/keep.md"),
                "k1",
                &[KnowledgeChunk {
                    path: PathBuf::from("docs/keep.md"),
                    heading: "Keep".into(),
                    start_line: 1,
                    end_line: 1,
                    tags: vec![],
                    text: "keep".into(),
                }],
            )
            .unwrap();
        cache
            .upsert_knowledge_chunks(
                Path::new(".omegon/cleave-workspace/stale.md"),
                "k2",
                &[KnowledgeChunk {
                    path: PathBuf::from(".omegon/cleave-workspace/stale.md"),
                    heading: "Stale".into(),
                    start_line: 1,
                    end_line: 1,
                    tags: vec![],
                    text: "stale".into(),
                }],
            )
            .unwrap();

        let live_paths =
            HashSet::from([PathBuf::from("src/keep.rs"), PathBuf::from("docs/keep.md")]);
        cache.prune_missing_paths(&live_paths).unwrap();

        let code = cache.all_code_chunks().unwrap();
        let knowledge = cache.all_knowledge_chunks().unwrap();
        assert_eq!(code.len(), 1);
        assert_eq!(code[0].path, PathBuf::from("src/keep.rs"));
        assert_eq!(knowledge.len(), 1);
        assert_eq!(knowledge[0].path, PathBuf::from("docs/keep.md"));
    }
}

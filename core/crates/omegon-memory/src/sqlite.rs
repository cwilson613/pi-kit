//! SqliteBackend — production MemoryBackend backed by rusqlite.
//!
//! Schema matches the TypeScript factstore.ts (v5) exactly.
//! WAL mode for concurrent reads. FTS5 for full-text search.
//! Bundled sqlite via rusqlite's `bundled` feature.

use async_trait::async_trait;
use rusqlite::{Connection, DatabaseName, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MEMORY_SCHEMA_VERSION: i64 = 7;
pub const PRIMENSUS_MIND: &str = "primensus";
pub const LEGACY_MIND: &str = "legacy";
pub const LEGACY_MEMORY_SCHEMA_VERSIONS: std::ops::RangeInclusive<i64> = 5..=6;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryMigrationPlan {
    pub source: PathBuf,
    pub source_version: i64,
    pub target_version: i64,
    pub fact_count: u64,
    pub mind_count: u64,
    pub edge_count: u64,
    pub episode_count: u64,
    pub integrity_check: String,
    pub foreign_key_violations: u64,
    pub backup: PathBuf,
    pub statements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryMigrationResult {
    pub source: PathBuf,
    pub backup: PathBuf,
    pub source_version: i64,
    pub target_version: i64,
    pub fact_count: u64,
    pub mind_count: u64,
    pub edge_count: u64,
    pub episode_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryStoreStatus {
    pub path: PathBuf,
    pub schema_version: i64,
    pub fact_count: u64,
    pub mind_count: u64,
    pub edge_count: u64,
    pub episode_count: u64,
    pub integrity_check: String,
    pub foreign_key_violations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRollbackResult {
    pub source: PathBuf,
    pub restored_from: PathBuf,
    pub preserved_current: PathBuf,
    pub restored_version: i64,
    pub fact_count: u64,
    pub episode_count: u64,
}

impl MemoryMigrationPlan {
    pub fn is_applicable(&self) -> bool {
        self.integrity_check == "ok"
            && self.foreign_key_violations == 0
            && LEGACY_MEMORY_SCHEMA_VERSIONS.contains(&self.source_version)
    }
}

use crate::backend::*;
use crate::hash;
use crate::types::*;
use crate::util::{gen_id, now_iso};
use crate::vectors;

pub struct SqliteBackend {
    conn: Mutex<Connection>,
}

impl SqliteBackend {
    pub fn plan_migration(path: &Path) -> anyhow::Result<MemoryMigrationPlan> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let source_version = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )?;
        if !LEGACY_MEMORY_SCHEMA_VERSIONS.contains(&source_version) {
            anyhow::bail!(
                "memory migration only supports schema v5 or v6 sources; found v{source_version}"
            );
        }
        let integrity_check: String =
            conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let foreign_key_violations = {
            let mut statement = conn.prepare("PRAGMA foreign_key_check")?;
            let mut rows = statement.query([])?;
            let mut count = 0;
            while rows.next()?.is_some() {
                count += 1;
            }
            count
        };
        let count = |table: &str| -> anyhow::Result<u64> {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            Ok(conn.query_row(&sql, [], |row| row.get(0))?)
        };
        let backup = path.with_extension(format!("v{source_version}.backup.db"));
        Ok(MemoryMigrationPlan {
            source: path.to_path_buf(),
            source_version,
            target_version: MEMORY_SCHEMA_VERSION,
            fact_count: count("facts")?,
            mind_count: count("minds")?,
            edge_count: count("edges")?,
            episode_count: count("episodes")?,
            integrity_check,
            foreign_key_violations,
            backup,
            statements: vec![
                format!(
                    "INSERT OR IGNORE INTO minds (name, description, created_at) VALUES ('{PRIMENSUS_MIND}', 'Authoritative ambient memory', datetime('now'))"
                ),
                format!(
                    "INSERT OR IGNORE INTO minds (name, description, status, created_at) VALUES ('{LEGACY_MIND}', 'Quarantined pre-v7 memory', 'quarantined', datetime('now'))"
                ),
                format!("UPDATE facts SET mind = '{LEGACY_MIND}' WHERE mind = 'default'"),
                format!("UPDATE episodes SET mind = '{LEGACY_MIND}' WHERE mind = 'default'"),
                format!(
                    "INSERT INTO schema_version (version, applied_at) VALUES ({MEMORY_SCHEMA_VERSION}, datetime('now'))"
                ),
            ],
        })
    }

    pub fn status(path: &Path) -> anyhow::Result<MemoryStoreStatus> {
        let inspected = Self::inspect_current(path)?;
        Ok(MemoryStoreStatus {
            path: path.to_path_buf(),
            schema_version: inspected.source_version,
            fact_count: inspected.fact_count,
            mind_count: inspected.mind_count,
            edge_count: inspected.edge_count,
            episode_count: inspected.episode_count,
            integrity_check: inspected.integrity_check,
            foreign_key_violations: inspected.foreign_key_violations,
        })
    }

    pub fn rollback_migration(
        source_path: &Path,
        backup_path: &Path,
    ) -> anyhow::Result<MemoryRollbackResult> {
        if source_path == backup_path {
            anyhow::bail!("memory rollback backup path must differ from source");
        }
        let current = Self::inspect_current(source_path)?;
        if current.source_version != MEMORY_SCHEMA_VERSION {
            anyhow::bail!(
                "memory rollback requires a v{} source; found v{}",
                MEMORY_SCHEMA_VERSION,
                current.source_version
            );
        }
        let backup = Self::inspect_current(backup_path)?;
        if !LEGACY_MEMORY_SCHEMA_VERSIONS.contains(&backup.source_version) {
            anyhow::bail!(
                "memory rollback backup must be schema v5 or v6; found v{}",
                backup.source_version
            );
        }
        Self::verify_migration_counts(&current, &backup)?;

        let preserved_current =
            source_path.with_extension(format!("v{}.rollback-source.db", current.source_version));
        if preserved_current.exists() {
            anyhow::bail!(
                "memory rollback preservation path already exists at {}; refusing to overwrite it",
                preserved_current.display()
            );
        }

        let current_conn = Connection::open_with_flags(
            source_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        current_conn.backup(DatabaseName::Main, &preserved_current, None)?;
        drop(current_conn);

        let result = (|| -> anyhow::Result<MemoryRollbackResult> {
            let mut target = Connection::open(source_path)?;
            target.busy_timeout(std::time::Duration::from_secs(5))?;
            target.restore(
                DatabaseName::Main,
                backup_path,
                None::<fn(rusqlite::backup::Progress)>,
            )?;
            drop(target);

            let restored = Self::inspect_current(source_path)?;
            Self::verify_migration_counts(&backup, &restored)?;
            if restored.source_version != backup.source_version {
                anyhow::bail!(
                    "memory rollback verification expected v{}, found v{}",
                    backup.source_version,
                    restored.source_version
                );
            }
            Ok(MemoryRollbackResult {
                source: source_path.to_path_buf(),
                restored_from: backup_path.to_path_buf(),
                preserved_current: preserved_current.clone(),
                restored_version: restored.source_version,
                fact_count: restored.fact_count,
                episode_count: restored.episode_count,
            })
        })();

        if result.is_err() {
            let _ = std::fs::remove_file(&preserved_current);
        }
        result
    }

    pub fn apply_migration(plan: &MemoryMigrationPlan) -> anyhow::Result<MemoryMigrationResult> {
        if !plan.is_applicable() {
            anyhow::bail!("memory migration plan failed source verification");
        }
        if plan.source == plan.backup {
            anyhow::bail!("memory migration backup path must differ from source");
        }
        if plan.backup.exists() {
            anyhow::bail!(
                "memory migration backup already exists at {}; refusing to overwrite it",
                plan.backup.display()
            );
        }

        let source = Connection::open_with_flags(
            &plan.source,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        source.busy_timeout(std::time::Duration::from_secs(5))?;
        source.backup(DatabaseName::Main, &plan.backup, None)?;
        drop(source);

        let result = (|| -> anyhow::Result<MemoryMigrationResult> {
            let backup_plan = Self::plan_migration(&plan.backup)?;
            Self::verify_migration_counts(plan, &backup_plan)?;

            let mut target = Connection::open(&plan.source)?;
            target.busy_timeout(std::time::Duration::from_secs(5))?;
            let transaction =
                target.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let current: i64 = transaction.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )?;
            if current != plan.source_version {
                anyhow::bail!(
                    "memory schema changed after planning: expected v{}, found v{current}",
                    plan.source_version
                );
            }
            transaction.execute(
                "INSERT OR IGNORE INTO minds (name, description, created_at) VALUES (?1, 'Authoritative ambient memory', datetime('now'))",
                params![PRIMENSUS_MIND],
            )?;
            transaction.execute(
                "INSERT OR IGNORE INTO minds (name, description, status, created_at) VALUES (?1, 'Quarantined pre-v7 memory', 'quarantined', datetime('now'))",
                params![LEGACY_MIND],
            )?;
            transaction.execute(
                "UPDATE facts SET mind = ?1 WHERE mind = 'default'",
                params![LEGACY_MIND],
            )?;
            transaction.execute(
                "UPDATE episodes SET mind = ?1 WHERE mind = 'default'",
                params![LEGACY_MIND],
            )?;
            transaction.execute(
                "INSERT INTO schema_version (version, applied_at) VALUES (?1, datetime('now'))",
                params![MEMORY_SCHEMA_VERSION],
            )?;
            transaction.commit()?;

            let verified = Self::inspect_current(&plan.source)?;
            Self::verify_migration_counts(plan, &verified)?;
            if verified.source_version != MEMORY_SCHEMA_VERSION {
                anyhow::bail!(
                    "memory migration verification expected v{}, found v{}",
                    MEMORY_SCHEMA_VERSION,
                    verified.source_version
                );
            }
            let backend = Self::open(&plan.source)?;
            drop(backend);
            let admitted = Self::inspect_current(&plan.source)?;
            Self::verify_migration_counts(plan, &admitted)?;
            if admitted.source_version != MEMORY_SCHEMA_VERSION {
                anyhow::bail!(
                    "memory migration post-open verification expected v{}, found v{}",
                    MEMORY_SCHEMA_VERSION,
                    admitted.source_version
                );
            }

            Ok(MemoryMigrationResult {
                source: plan.source.clone(),
                backup: plan.backup.clone(),
                source_version: plan.source_version,
                target_version: MEMORY_SCHEMA_VERSION,
                fact_count: verified.fact_count,
                mind_count: verified.mind_count,
                edge_count: verified.edge_count,
                episode_count: verified.episode_count,
            })
        })();

        if result.is_err() {
            let _ = std::fs::remove_file(&plan.backup);
        }
        result
    }

    fn inspect_current(path: &Path) -> anyhow::Result<MemoryMigrationPlan> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let source_version = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )?;
        let integrity_check: String =
            conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let foreign_key_violations = {
            let mut statement = conn.prepare("PRAGMA foreign_key_check")?;
            let mut rows = statement.query([])?;
            let mut count = 0;
            while rows.next()?.is_some() {
                count += 1;
            }
            count
        };
        let count = |table: &str| -> anyhow::Result<u64> {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            Ok(conn.query_row(&sql, [], |row| row.get(0))?)
        };
        Ok(MemoryMigrationPlan {
            source: path.to_path_buf(),
            source_version,
            target_version: MEMORY_SCHEMA_VERSION,
            fact_count: count("facts")?,
            mind_count: count("minds")?,
            edge_count: count("edges")?,
            episode_count: count("episodes")?,
            integrity_check,
            foreign_key_violations,
            backup: path.with_extension(format!("v{source_version}.backup.db")),
            statements: Vec::new(),
        })
    }

    fn verify_migration_counts(
        expected: &MemoryMigrationPlan,
        actual: &MemoryMigrationPlan,
    ) -> anyhow::Result<()> {
        if actual.integrity_check != "ok" || actual.foreign_key_violations != 0 {
            anyhow::bail!(
                "memory migration verification failed: integrity={}, foreign_key_violations={}",
                actual.integrity_check,
                actual.foreign_key_violations
            );
        }
        let expected_counts = (
            expected.fact_count,
            expected.edge_count,
            expected.episode_count,
        );
        let actual_counts = (actual.fact_count, actual.edge_count, actual.episode_count);
        if expected_counts != actual_counts {
            anyhow::bail!(
                "memory migration fact/edge/episode counts changed: expected {expected_counts:?}, found {actual_counts:?}"
            );
        }
        Ok(())
    }

    /// Open or create a sqlite DB at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let existed = path.exists();
        let conn = Connection::open(path)?;
        let backend = Self {
            conn: Mutex::new(conn),
        };
        if let Err(error) = backend.init_schema(existed) {
            drop(backend);
            if !existed {
                let _ = std::fs::remove_file(path);
            }
            return Err(error);
        }
        Ok(backend)
    }

    /// Create an in-memory sqlite DB (for testing).
    pub fn in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let backend = Self {
            conn: Mutex::new(conn),
        };
        backend.init_schema(false)?;
        Ok(backend)
    }

    fn init_schema(&self, existed: bool) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        // busy_timeout: wait up to 5s for write lock instead of failing immediately.
        // Critical for multi-process access (cleave children share the same DB file).
        conn.execute_batch("PRAGMA busy_timeout=5000;")?;

        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS minds (
                name        TEXT PRIMARY KEY,
                description TEXT,
                status      TEXT NOT NULL DEFAULT 'active',
                origin_type TEXT,
                origin_path TEXT,
                origin_url  TEXT,
                readonly    INTEGER NOT NULL DEFAULT 0,
                parent      TEXT,
                created_at  TEXT NOT NULL,
                last_sync   TEXT
            );

            INSERT OR IGNORE INTO minds (name, description, created_at)
            VALUES ('primensus', 'Authoritative ambient memory', datetime('now'));

            CREATE TABLE IF NOT EXISTS facts (
                id                  TEXT PRIMARY KEY,
                mind                TEXT NOT NULL DEFAULT 'primensus',
                section             TEXT NOT NULL,
                content             TEXT NOT NULL,
                status              TEXT NOT NULL DEFAULT 'active',
                created_at          TEXT NOT NULL,
                created_session     TEXT,
                supersedes          TEXT,
                superseded_at       TEXT,
                archived_at         TEXT,
                source              TEXT NOT NULL DEFAULT 'manual',
                content_hash        TEXT NOT NULL,
                confidence          REAL NOT NULL DEFAULT 1.0,
                last_reinforced     TEXT NOT NULL,
                reinforcement_count INTEGER NOT NULL DEFAULT 1,
                decay_rate          REAL NOT NULL DEFAULT 0.05,
                decay_profile       TEXT NOT NULL DEFAULT 'standard',
                version             INTEGER NOT NULL DEFAULT 0,
                last_accessed       TEXT,
                jj_change_id        TEXT,
                persona_id          TEXT,
                layer               TEXT NOT NULL DEFAULT 'project',
                tags                TEXT,
                FOREIGN KEY (mind) REFERENCES minds(name) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_facts_active ON facts(mind, status) WHERE status = 'active';
            CREATE INDEX IF NOT EXISTS idx_facts_persona ON facts(persona_id) WHERE persona_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_facts_layer ON facts(mind, layer) WHERE status = 'active';
            CREATE INDEX IF NOT EXISTS idx_facts_hash ON facts(mind, content_hash);
            CREATE INDEX IF NOT EXISTS idx_facts_section ON facts(mind, section) WHERE status = 'active';
            CREATE INDEX IF NOT EXISTS idx_facts_session ON facts(created_session);
            CREATE INDEX IF NOT EXISTS idx_facts_version ON facts(version DESC);

            CREATE TABLE IF NOT EXISTS facts_vec (
                fact_id    TEXT PRIMARY KEY,
                embedding  BLOB NOT NULL,
                model_name TEXT NOT NULL DEFAULT '',
                dims       INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (fact_id) REFERENCES facts(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS embedding_metadata (
                model_name  TEXT PRIMARY KEY,
                dims        INTEGER NOT NULL,
                inserted_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS edges (
                id                  TEXT PRIMARY KEY,
                source_fact_id      TEXT NOT NULL,
                target_fact_id      TEXT NOT NULL,
                relation            TEXT NOT NULL,
                description         TEXT,
                confidence          REAL NOT NULL DEFAULT 1.0,
                last_reinforced     TEXT,
                reinforcement_count INTEGER NOT NULL DEFAULT 1,
                decay_rate          REAL NOT NULL DEFAULT 0.05,
                status              TEXT NOT NULL DEFAULT 'active',
                created_at          TEXT NOT NULL,
                created_session     TEXT,
                source_mind         TEXT,
                target_mind         TEXT,
                FOREIGN KEY (source_fact_id) REFERENCES facts(id) ON DELETE CASCADE,
                FOREIGN KEY (target_fact_id) REFERENCES facts(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS episodes (
                id          TEXT PRIMARY KEY,
                mind        TEXT NOT NULL DEFAULT 'primensus',
                title       TEXT NOT NULL,
                narrative   TEXT NOT NULL,
                date        TEXT NOT NULL,
                session_id  TEXT,
                created_at  TEXT NOT NULL,
                jj_change_id TEXT,
                FOREIGN KEY (mind) REFERENCES minds(name) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_episodes_mind ON episodes(mind, date DESC);

            CREATE TABLE IF NOT EXISTS episode_facts (
                episode_id TEXT NOT NULL,
                fact_id    TEXT NOT NULL,
                PRIMARY KEY (episode_id, fact_id),
                FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE,
                FOREIGN KEY (fact_id) REFERENCES facts(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS episodes_vec (
                episode_id TEXT PRIMARY KEY,
                embedding  BLOB NOT NULL,
                model_name TEXT NOT NULL DEFAULT '',
                dims       INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE
            );

            -- FTS5 for full-text search on facts
            CREATE VIRTUAL TABLE IF NOT EXISTS facts_fts USING fts5(
                id UNINDEXED, mind UNINDEXED, section UNINDEXED, content,
                content='facts', content_rowid='rowid'
            );

            -- FTS sync triggers
            CREATE TRIGGER IF NOT EXISTS facts_fts_insert AFTER INSERT ON facts BEGIN
                INSERT INTO facts_fts(rowid, id, mind, section, content)
                VALUES (NEW.rowid, NEW.id, NEW.mind, NEW.section, NEW.content);
            END;
            CREATE TRIGGER IF NOT EXISTS facts_fts_delete AFTER DELETE ON facts BEGIN
                INSERT INTO facts_fts(facts_fts, rowid, id, mind, section, content)
                VALUES ('delete', OLD.rowid, OLD.id, OLD.mind, OLD.section, OLD.content);
            END;
            CREATE TRIGGER IF NOT EXISTS facts_fts_update AFTER UPDATE ON facts BEGIN
                INSERT INTO facts_fts(facts_fts, rowid, id, mind, section, content)
                VALUES ('delete', OLD.rowid, OLD.id, OLD.mind, OLD.section, OLD.content);
                INSERT INTO facts_fts(rowid, id, mind, section, content)
                VALUES (NEW.rowid, NEW.id, NEW.mind, NEW.section, NEW.content);
            END;

            -- FTS5 for episodes
            CREATE VIRTUAL TABLE IF NOT EXISTS episodes_fts USING fts5(
                id UNINDEXED, mind UNINDEXED, title, narrative,
                content='episodes', content_rowid='rowid'
            );
            CREATE TRIGGER IF NOT EXISTS episodes_fts_insert AFTER INSERT ON episodes BEGIN
                INSERT INTO episodes_fts(rowid, id, mind, title, narrative)
                VALUES (NEW.rowid, NEW.id, NEW.mind, NEW.title, NEW.narrative);
            END;
            CREATE TRIGGER IF NOT EXISTS episodes_fts_delete AFTER DELETE ON episodes BEGIN
                INSERT INTO episodes_fts(episodes_fts, rowid, id, mind, title, narrative)
                VALUES ('delete', OLD.rowid, OLD.id, OLD.mind, OLD.title, OLD.narrative);
            END;

            -- Schema version tracking (TS compat — factstore.ts checks this)
            CREATE TABLE IF NOT EXISTS schema_version (
                version    INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
        ")?;

        let current: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )?;

        if current == 0 && !existed {
            conn.execute(
                "INSERT INTO schema_version (version, applied_at) VALUES (?1, datetime('now'))",
                params![MEMORY_SCHEMA_VERSION],
            )?;
        } else if current != MEMORY_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported memory schema version {current}; Omegon requires exactly v{MEMORY_SCHEMA_VERSION}. Run the documented memory migration workflow before opening this store"
            );
        }

        Ok(())
    }

    fn ensure_mind(&self, conn: &Connection, mind: &str) {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO minds (name, created_at) VALUES (?1, ?2)",
            params![mind, now_iso()],
        );
    }

    fn next_version(&self, conn: &Connection) -> u64 {
        Self::next_version_static(conn)
    }

    fn next_version_static(conn: &Connection) -> u64 {
        let max: u64 = conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM facts", [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        max + 1
    }

    fn row_to_fact(row: &rusqlite::Row<'_>) -> rusqlite::Result<Fact> {
        let section_str: String = row.get("section")?;
        let status_str: String = row.get("status")?;
        let profile_str: String = row.get("decay_profile")?;

        let section = serde_json::from_value::<Section>(serde_json::Value::String(section_str.clone()))
            .unwrap_or_else(|_| {
                tracing::warn!(section = %section_str, "unknown section in DB — defaulting to Architecture");
                Section::Architecture
            });
        let status = serde_json::from_value::<FactStatus>(serde_json::Value::String(
            status_str.clone(),
        ))
        .unwrap_or_else(|_| {
            tracing::warn!(status = %status_str, "unknown status in DB — defaulting to Active");
            FactStatus::Active
        });

        Ok(Fact {
            id: row.get("id")?,
            mind: row.get("mind")?,
            content: row.get("content")?,
            section,
            status,
            confidence: row.get("confidence")?,
            reinforcement_count: row.get::<_, u32>("reinforcement_count")?,
            decay_rate: row.get("decay_rate")?,
            decay_profile: serde_json::from_value::<DecayProfileName>(serde_json::Value::String(profile_str.clone()))
                .unwrap_or_else(|_| {
                    tracing::warn!(profile = %profile_str, "unknown decay_profile in DB — defaulting to Standard");
                    DecayProfileName::Standard
                }),
            last_reinforced: row.get("last_reinforced")?,
            created_at: row.get("created_at")?,
            version: row.get::<_, i64>("version")? as u64,
            superseded_by: row.get::<_, Option<String>>("supersedes")?,
            source: row.get("source")?,
            content_hash: Some(row.get::<_, String>("content_hash")?),
            last_accessed: row.get("last_accessed")?,
            created_session: row.get("created_session")?,
            superseded_at: row.get("superseded_at")?,
            archived_at: row.get("archived_at")?,
            jj_change_id: row.get("jj_change_id")?,
            persona_id: row.get("persona_id")?,
            layer: row.get::<_, Option<String>>("layer")?
                .unwrap_or_else(|| "project".into()),
            tags: row.get::<_, Option<String>>("tags")?
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
        })
    }
}

#[async_trait]
impl MemoryBackend for SqliteBackend {
    async fn store_fact(&self, req: StoreFact) -> Result<StoreResult> {
        let conn = self.conn.lock().unwrap();
        self.ensure_mind(&conn, &req.mind);
        let ch = hash::content_hash(&req.content);
        let section_str = serde_json::to_string(&req.section).unwrap_or_default();
        let section_str = section_str.trim_matches('"');
        let profile_str = serde_json::to_string(&req.decay_profile).unwrap_or_default();
        let profile_str = profile_str.trim_matches('"');

        // Check dedup
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM facts WHERE mind = ?1 AND content_hash = ?2 AND status = 'active'",
                params![req.mind, ch],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| MemoryError::Storage(e.into()))?;

        if let Some(id) = existing {
            let version = self.next_version(&conn);
            let ts = now_iso();
            conn.execute(
                "UPDATE facts SET reinforcement_count = reinforcement_count + 1, \
                 last_reinforced = ?1, version = ?2 WHERE id = ?3",
                params![ts, version as i64, id],
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;

            let fact = conn
                .query_row(
                    "SELECT * FROM facts WHERE id = ?1",
                    params![id],
                    Self::row_to_fact,
                )
                .map_err(|e| MemoryError::Storage(e.into()))?;
            return Ok(StoreResult {
                fact,
                action: StoreAction::Reinforced,
            });
        }

        let id = gen_id();
        let ts = now_iso();
        let version = self.next_version(&conn);
        conn.execute(
            "INSERT INTO facts (id, mind, section, content, status, created_at, source, \
             content_hash, confidence, last_reinforced, reinforcement_count, decay_rate, \
             decay_profile, version) VALUES (?1,?2,?3,?4,'active',?5,?6,?7,1.0,?5,1,0.05,?8,?9)",
            params![
                id,
                req.mind,
                section_str,
                req.content,
                ts,
                req.source.as_deref().unwrap_or("manual"),
                ch,
                profile_str,
                version as i64
            ],
        )
        .map_err(|e| MemoryError::Storage(e.into()))?;

        let fact = conn
            .query_row(
                "SELECT * FROM facts WHERE id = ?1",
                params![id],
                Self::row_to_fact,
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;
        Ok(StoreResult {
            fact,
            action: StoreAction::Stored,
        })
    }

    async fn get_fact(&self, id: &str) -> Result<Option<Fact>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT * FROM facts WHERE id = ?1 AND status = 'active'",
            params![id],
            Self::row_to_fact,
        )
        .optional()
        .map_err(|e| MemoryError::Storage(e.into()))
    }

    async fn list_facts(&self, mind: &str, filter: FactFilter) -> Result<Vec<Fact>> {
        let conn = self.conn.lock().unwrap();
        let status_str = filter
            .status
            .as_ref()
            .map(|s| {
                serde_json::to_string(s)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string()
            })
            .unwrap_or_else(|| "active".into());

        let (sql, section_param);
        if let Some(ref sec) = filter.section {
            section_param = serde_json::to_string(sec)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            sql = "SELECT * FROM facts WHERE mind = ?1 AND status = ?2 AND section = ?3 ORDER BY created_at DESC";
            let mut stmt = conn
                .prepare(sql)
                .map_err(|e| MemoryError::Storage(e.into()))?;
            let facts = stmt
                .query_map(params![mind, status_str, section_param], Self::row_to_fact)
                .map_err(|e| MemoryError::Storage(e.into()))?
                .filter_map(|r| r.map_err(|e| tracing::debug!("row deser error: {e}")).ok())
                .collect();
            Ok(facts)
        } else {
            sql = "SELECT * FROM facts WHERE mind = ?1 AND status = ?2 ORDER BY created_at DESC";
            let mut stmt = conn
                .prepare(sql)
                .map_err(|e| MemoryError::Storage(e.into()))?;
            let facts = stmt
                .query_map(params![mind, status_str], Self::row_to_fact)
                .map_err(|e| MemoryError::Storage(e.into()))?
                .filter_map(|r| r.map_err(|e| tracing::debug!("row deser error: {e}")).ok())
                .collect();
            Ok(facts)
        }
    }

    async fn reinforce_fact(&self, id: &str) -> Result<Fact> {
        let conn = self.conn.lock().unwrap();
        let version = self.next_version(&conn);
        let ts = now_iso();
        let updated = conn
            .execute(
                "UPDATE facts SET reinforcement_count = reinforcement_count + 1, \
             last_reinforced = ?1, version = ?2 WHERE id = ?3 AND status = 'active'",
                params![ts, version as i64, id],
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;
        if updated == 0 {
            return Err(MemoryError::FactNotFound(id.into()));
        }
        conn.query_row(
            "SELECT * FROM facts WHERE id = ?1",
            params![id],
            Self::row_to_fact,
        )
        .map_err(|e| MemoryError::Storage(e.into()))
    }

    async fn dormancy_facts(&self, ids: &[&str]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut transitioned = 0;
        for id in ids {
            transitioned += conn
                .execute(
                    "UPDATE facts SET status = 'dormant' WHERE id = ?1 AND status = 'active'",
                    params![id],
                )
                .map_err(|error| MemoryError::Storage(error.into()))?;
        }
        Ok(transitioned)
    }

    async fn archive_facts(&self, ids: &[&str]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut count = 0usize;
        for id in ids {
            let version = self.next_version(&conn);
            let n = conn.execute(
                "UPDATE facts SET status = 'archived', version = ?1 WHERE id = ?2 AND status = 'active'",
                params![version as i64, id],
            ).map_err(|e| MemoryError::Storage(e.into()))?;
            count += n;
        }
        Ok(count)
    }

    async fn supersede_fact(&self, id: &str, replacement: StoreFact) -> Result<Fact> {
        let mut conn = self.conn.lock().unwrap();
        self.ensure_mind(&conn, &replacement.mind);

        // Check original exists
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM facts WHERE id = ?1 AND status = 'active'",
                params![id],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| MemoryError::Storage(e.into()))?
            .unwrap_or(false);
        if !exists {
            return Err(MemoryError::FactNotFound(id.into()));
        }

        let new_id = gen_id();
        let version = Self::next_version_static(&conn);

        // Transaction: archive original + insert replacement atomically
        let tx = conn
            .transaction()
            .map_err(|e| MemoryError::Storage(e.into()))?;

        // Archive original — matches TS behavior
        tx.execute(
            "UPDATE facts SET status = 'superseded', version = ?1 WHERE id = ?2",
            params![version as i64, id],
        )
        .map_err(|e| MemoryError::Storage(e.into()))?;

        // Insert replacement
        let section_str = serde_json::to_string(&replacement.section).unwrap_or_default();
        let section_str = section_str.trim_matches('"');
        let profile_str = serde_json::to_string(&replacement.decay_profile).unwrap_or_default();
        let profile_str = profile_str.trim_matches('"');
        let ch = hash::content_hash(&replacement.content);
        let ts = now_iso();
        let version2 = version + 1;

        tx.execute(
            "INSERT INTO facts (id, mind, section, content, status, created_at, source, \
             content_hash, confidence, last_reinforced, reinforcement_count, decay_rate, \
             decay_profile, version, supersedes) VALUES (?1,?2,?3,?4,'active',?5,?6,?7,1.0,?5,1,0.05,?8,?9,?10)",
            params![new_id, replacement.mind, section_str, replacement.content, ts,
                    replacement.source.as_deref().unwrap_or("manual"), ch, profile_str, version2 as i64, id],
        ).map_err(|e| MemoryError::Storage(e.into()))?;

        let fact = tx
            .query_row(
                "SELECT * FROM facts WHERE id = ?1",
                params![new_id],
                Self::row_to_fact,
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;

        tx.commit().map_err(|e| MemoryError::Storage(e.into()))?;
        Ok(fact)
    }

    async fn fts_search(&self, mind: &str, query: &str, k: usize) -> Result<Vec<ScoredFact>> {
        let conn = self.conn.lock().unwrap();
        // Use FTS5 OR mode for broader matching
        let fts_query = query
            .split_whitespace()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(" OR ");

        let mut stmt = conn
            .prepare(
                "SELECT f.*, rank FROM facts_fts fts \
             JOIN facts f ON f.id = fts.id \
             WHERE facts_fts MATCH ?1 AND fts.mind = ?2 AND f.status = 'active' \
             ORDER BY rank LIMIT ?3",
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;

        let mut results: Vec<ScoredFact> = stmt
            .query_map(
                params![fts_query, mind, (k.saturating_mul(8).max(k)) as i64],
                |row| {
                    let fact = Self::row_to_fact(row)?;
                    let rank: f64 = row.get("rank")?;
                    Ok((fact, -rank))
                },
            )
            .map_err(|e| MemoryError::Storage(e.into()))?
            .filter_map(|r| r.map_err(|e| tracing::debug!("row deser: {e}")).ok())
            .filter_map(|(fact, relevance)| {
                let score = crate::decay::ambient_score(relevance, &fact)?;
                Some(ScoredFact {
                    fact,
                    similarity: relevance,
                    score,
                })
            })
            .collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(k);

        Ok(results)
    }

    async fn vector_search(
        &self,
        mind: &str,
        embedding: &[f32],
        k: usize,
        min_similarity: f32,
    ) -> Result<Vec<ScoredFact>> {
        let conn = self.conn.lock().unwrap();

        // Check if any embeddings exist for this mind
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM facts_vec fv JOIN facts f ON f.id = fv.fact_id WHERE f.mind = ?1",
            params![mind], |r| r.get(0),
        ).map_err(|e| MemoryError::Storage(e.into()))?;

        if count == 0 {
            return Err(MemoryError::NoEmbeddings);
        }

        // Check dimension match
        let stored_dims: u32 = conn.query_row(
            "SELECT dims FROM facts_vec fv JOIN facts f ON f.id = fv.fact_id WHERE f.mind = ?1 LIMIT 1",
            params![mind], |r| r.get(0),
        ).map_err(|e| MemoryError::Storage(e.into()))?;

        let query_dims = embedding.len() as u32;
        if stored_dims != query_dims {
            let model: String = conn.query_row(
                "SELECT model_name FROM facts_vec fv JOIN facts f ON f.id = fv.fact_id WHERE f.mind = ?1 LIMIT 1",
                params![mind], |r| r.get(0),
            ).map_err(|e| MemoryError::Storage(e.into()))?;
            return Err(MemoryError::EmbeddingDimensionMismatch {
                expected: stored_dims,
                got: query_dims,
                stored_model: model,
            });
        }

        // Linear scan — load all vectors and compute cosine similarity
        let mut stmt = conn
            .prepare(
                "SELECT fv.fact_id, fv.embedding, f.* FROM facts_vec fv \
             JOIN facts f ON f.id = fv.fact_id \
             WHERE f.mind = ?1 AND f.status = 'active'",
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;

        let mut results: Vec<ScoredFact> = stmt
            .query_map(params![mind], |row| {
                let blob: Vec<u8> = row.get("embedding")?;
                let fact = Self::row_to_fact(row)?;
                Ok((blob, fact))
            })
            .map_err(|e| MemoryError::Storage(e.into()))?
            .filter_map(|r| r.map_err(|e| tracing::debug!("row deser: {e}")).ok())
            .filter_map(|(blob, fact)| {
                let vec = vectors::blob_to_vector(&blob);
                let sim = vectors::cosine_similarity(&vec, embedding);
                if sim < min_similarity {
                    return None;
                }
                let score = crate::decay::ambient_score(sim as f64, &fact)?;
                Some(ScoredFact {
                    similarity: sim as f64,
                    score,
                    fact,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(k);
        Ok(results)
    }

    async fn store_embedding(
        &self,
        fact_id: &str,
        model_name: &str,
        embedding: &[f32],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let blob = vectors::vector_to_blob(embedding);
        let ts = now_iso();
        let dims = embedding.len() as i64;

        conn.execute(
            "INSERT OR REPLACE INTO facts_vec (fact_id, embedding, model_name, dims, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![fact_id, blob, model_name, dims, ts],
        )
        .map_err(|e| MemoryError::Storage(e.into()))?;

        conn.execute(
            "INSERT OR IGNORE INTO embedding_metadata (model_name, dims, inserted_at) VALUES (?1, ?2, ?3)",
            params![model_name, dims, ts],
        ).map_err(|e| MemoryError::Storage(e.into()))?;

        Ok(())
    }

    async fn embedding_metadata(&self, mind: &str) -> Result<Option<EmbeddingMetadata>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT em.model_name, em.dims, em.inserted_at FROM embedding_metadata em \
             JOIN facts_vec fv ON fv.model_name = em.model_name \
             JOIN facts f ON f.id = fv.fact_id \
             WHERE f.mind = ?1 LIMIT 1",
            params![mind],
            |row| {
                Ok(EmbeddingMetadata {
                    model_name: row.get(0)?,
                    dims: row.get(1)?,
                    inserted_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|e| MemoryError::Storage(e.into()))
    }

    async fn create_edge(&self, req: CreateEdge) -> Result<Edge> {
        let conn = self.conn.lock().unwrap();
        let id = gen_id();
        let ts = now_iso();
        conn.execute(
            "INSERT INTO edges (id, source_fact_id, target_fact_id, relation, description, confidence, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1.0, ?6)",
            params![id, req.source_id, req.target_id, req.relation, req.description, ts],
        ).map_err(|e| MemoryError::Storage(e.into()))?;

        Ok(Edge {
            id,
            source_id: req.source_id,
            target_id: req.target_id,
            relation: req.relation,
            description: req.description,
            confidence: 1.0,
            created_at: ts,
        })
    }

    async fn get_edges(&self, _mind: &str, fact_id: &str) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT * FROM edges WHERE source_fact_id = ?1 OR target_fact_id = ?1")
            .map_err(|e| MemoryError::Storage(e.into()))?;

        let edges = stmt
            .query_map(params![fact_id], |row| {
                Ok(Edge {
                    id: row.get("id")?,
                    source_id: row.get("source_fact_id")?,
                    target_id: row.get("target_fact_id")?,
                    relation: row.get("relation")?,
                    description: row.get("description")?,
                    confidence: row.get("confidence")?,
                    created_at: row.get("created_at")?,
                })
            })
            .map_err(|e| MemoryError::Storage(e.into()))?
            .filter_map(|r| r.map_err(|e| tracing::debug!("row deser: {e}")).ok())
            .collect();
        Ok(edges)
    }

    async fn store_episode(&self, req: StoreEpisode) -> Result<Episode> {
        let conn = self.conn.lock().unwrap();
        self.ensure_mind(&conn, &req.mind);
        let id = gen_id();
        let ts = now_iso();
        let date = req.date.unwrap_or_else(|| ts[..10].to_string());

        conn.execute(
            "INSERT INTO episodes (id, mind, title, narrative, date, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, req.mind, req.title, req.narrative, date, ts],
        ).map_err(|e| MemoryError::Storage(e.into()))?;

        Ok(Episode {
            id,
            mind: req.mind,
            date,
            title: req.title,
            narrative: req.narrative,
            created_at: ts,
            affected_nodes: req.affected_nodes,
            affected_changes: req.affected_changes,
            files_changed: req.files_changed,
            tags: req.tags,
            tool_calls_count: req.tool_calls_count,
            jj_change_id: None,
        })
    }

    async fn list_episodes(&self, mind: &str, k: usize) -> Result<Vec<Episode>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT * FROM episodes WHERE mind = ?1 ORDER BY date DESC, created_at DESC LIMIT ?2"
        ).map_err(|e| MemoryError::Storage(e.into()))?;

        let episodes = stmt
            .query_map(params![mind, k as i64], |row| {
                Ok(Episode {
                    id: row.get("id")?,
                    mind: row.get("mind")?,
                    date: row.get("date")?,
                    title: row.get("title")?,
                    narrative: row.get("narrative")?,
                    created_at: row.get("created_at")?,
                    affected_nodes: vec![],
                    affected_changes: vec![],
                    files_changed: vec![],
                    tags: vec![],
                    tool_calls_count: None,
                    jj_change_id: None,
                })
            })
            .map_err(|e| MemoryError::Storage(e.into()))?
            .filter_map(|r| r.map_err(|e| tracing::debug!("row deser: {e}")).ok())
            .collect();
        Ok(episodes)
    }

    async fn search_episodes(&self, mind: &str, query: &str, k: usize) -> Result<Vec<Episode>> {
        let conn = self.conn.lock().unwrap();
        let fts_query = query
            .split_whitespace()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(" OR ");

        let mut stmt = conn
            .prepare(
                "SELECT e.* FROM episodes_fts efts \
             JOIN episodes e ON e.id = efts.id \
             WHERE episodes_fts MATCH ?1 AND efts.mind = ?2 \
             ORDER BY rank LIMIT ?3",
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;

        let episodes = stmt
            .query_map(params![fts_query, mind, k as i64], |row| {
                Ok(Episode {
                    id: row.get("id")?,
                    mind: row.get("mind")?,
                    date: row.get("date")?,
                    title: row.get("title")?,
                    narrative: row.get("narrative")?,
                    created_at: row.get("created_at")?,
                    affected_nodes: vec![],
                    affected_changes: vec![],
                    files_changed: vec![],
                    tags: vec![],
                    tool_calls_count: None,
                    jj_change_id: None,
                })
            })
            .map_err(|e| MemoryError::Storage(e.into()))?
            .filter_map(|r| r.map_err(|e| tracing::debug!("row deser: {e}")).ok())
            .collect();
        Ok(episodes)
    }

    async fn export_jsonl(&self, mind: &str) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let mut lines = Vec::new();

        // Facts
        let mut stmt = conn.prepare(
            "SELECT * FROM facts WHERE mind = ?1 AND status = 'active' ORDER BY section, created_at, id"
        ).map_err(|e| MemoryError::Storage(e.into()))?;
        let facts: Vec<Fact> = stmt
            .query_map(params![mind], Self::row_to_fact)
            .map_err(|e| MemoryError::Storage(e.into()))?
            .filter_map(|r| r.map_err(|e| tracing::debug!("row deser: {e}")).ok())
            .collect();
        for f in &facts {
            let record = JsonlRecord::Fact(JsonlFact {
                id: f.id.clone(),
                mind: f.mind.clone(),
                content: f.content.clone(),
                section: f.section.clone(),
                status: f.status.clone(),
                created_at: f.created_at.clone(),
                source: f.source.clone(),
                content_hash: f.content_hash.clone(),
                supersedes: None,
                version: f.version,
                decay_profile: f.decay_profile.clone(),
                persona_id: f.persona_id.clone(),
                layer: f.layer.clone(),
                tags: f.tags.clone(),
            });
            lines.push(serde_json::to_string(&record).unwrap());
        }

        // Edges
        let mut stmt = conn.prepare(
            "SELECT * FROM edges WHERE source_fact_id IN (SELECT id FROM facts WHERE mind = ?1) ORDER BY id"
        ).map_err(|e| MemoryError::Storage(e.into()))?;
        let edges: Vec<Edge> = stmt
            .query_map(params![mind], |row| {
                Ok(Edge {
                    id: row.get("id")?,
                    source_id: row.get("source_fact_id")?,
                    target_id: row.get("target_fact_id")?,
                    relation: row.get("relation")?,
                    description: row.get("description")?,
                    confidence: row.get("confidence")?,
                    created_at: row.get("created_at")?,
                })
            })
            .map_err(|e| MemoryError::Storage(e.into()))?
            .filter_map(|r| r.map_err(|e| tracing::debug!("row deser: {e}")).ok())
            .collect();
        for e in &edges {
            lines.push(serde_json::to_string(&JsonlRecord::Edge(e.clone())).unwrap());
        }

        // Episodes
        let mut stmt = conn
            .prepare("SELECT * FROM episodes WHERE mind = ?1 ORDER BY id")
            .map_err(|e| MemoryError::Storage(e.into()))?;
        let episodes: Vec<Episode> = stmt
            .query_map(params![mind], |row| {
                Ok(Episode {
                    id: row.get("id")?,
                    mind: row.get("mind")?,
                    date: row.get("date")?,
                    title: row.get("title")?,
                    narrative: row.get("narrative")?,
                    created_at: row.get("created_at")?,
                    affected_nodes: vec![],
                    affected_changes: vec![],
                    files_changed: vec![],
                    tags: vec![],
                    tool_calls_count: None,
                    jj_change_id: None,
                })
            })
            .map_err(|e| MemoryError::Storage(e.into()))?
            .filter_map(|r| r.map_err(|e| tracing::debug!("row deser: {e}")).ok())
            .collect();
        for ep in &episodes {
            lines.push(serde_json::to_string(&JsonlRecord::Episode(ep.clone())).unwrap());
        }

        Ok(lines.join("\n"))
    }

    async fn import_jsonl(&self, jsonl: &str) -> Result<ImportStats> {
        let mut stats = ImportStats::default();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| MemoryError::Storage(e.into()))?;

        for line in jsonl.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<JsonlRecord>(trimmed) {
                Ok(JsonlRecord::Fact(jf)) => {
                    self.ensure_mind(&tx, &jf.mind);
                    let existing_version: Option<i64> = tx
                        .query_row(
                            "SELECT version FROM facts WHERE id = ?1",
                            params![jf.id],
                            |r| r.get(0),
                        )
                        .optional()
                        .map_err(|e| MemoryError::Storage(e.into()))?
                        .flatten();

                    if let Some(ev) = existing_version {
                        if (jf.version as i64) > ev {
                            let section_str =
                                serde_json::to_string(&jf.section).unwrap_or_default();
                            let section_str = section_str.trim_matches('"');
                            tx.execute(
                                "UPDATE facts SET content = ?1, section = ?2, version = ?3 WHERE id = ?4",
                                params![jf.content, section_str, jf.version as i64, jf.id],
                            ).map_err(|e| MemoryError::Storage(e.into()))?;
                            stats.reinforced += 1;
                        } else {
                            stats.skipped += 1;
                        }
                    } else {
                        let section_str = serde_json::to_string(&jf.section).unwrap_or_default();
                        let section_str = section_str.trim_matches('"');
                        let profile_str =
                            serde_json::to_string(&jf.decay_profile).unwrap_or_default();
                        let profile_str = profile_str.trim_matches('"');
                        let ch = jf
                            .content_hash
                            .unwrap_or_else(|| hash::content_hash(&jf.content));
                        tx.execute(
                            "INSERT INTO facts (id, mind, section, content, status, created_at, source, \
                             content_hash, confidence, last_reinforced, reinforcement_count, decay_rate, \
                             decay_profile, version) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1.0,?6,1,0.05,?9,?10)",
                            params![jf.id, jf.mind, section_str, jf.content,
                                    serde_json::to_string(&jf.status).unwrap_or_default().trim_matches('"'),
                                    jf.created_at, jf.source.as_deref().unwrap_or("manual"),
                                    ch, profile_str, jf.version as i64],
                        ).map_err(|e| MemoryError::Storage(e.into()))?;
                        stats.imported += 1;
                    }
                }
                Ok(JsonlRecord::Episode(ep)) => {
                    self.ensure_mind(&tx, &ep.mind);
                    tx.execute(
                        "INSERT OR IGNORE INTO episodes (id, mind, title, narrative, date, created_at) \
                         VALUES (?1,?2,?3,?4,?5,?6)",
                        params![ep.id, ep.mind, ep.title, ep.narrative, ep.date, ep.created_at],
                    ).map_err(|e| MemoryError::Storage(e.into()))?;
                    stats.imported += 1;
                }
                Ok(JsonlRecord::Edge(edge)) => {
                    tx.execute(
                        "INSERT OR IGNORE INTO edges (id, source_fact_id, target_fact_id, relation, description, confidence, created_at) \
                         VALUES (?1,?2,?3,?4,?5,?6,?7)",
                        params![edge.id, edge.source_id, edge.target_id, edge.relation,
                                edge.description, edge.confidence, edge.created_at],
                    ).map_err(|e| MemoryError::Storage(e.into()))?;
                    stats.imported += 1;
                }
                Ok(JsonlRecord::Mind(_)) => {
                    stats.skipped += 1;
                }
                Err(_) => {
                    stats.errors += 1;
                }
            }
        }
        tx.commit().map_err(|e| MemoryError::Storage(e.into()))?;
        Ok(stats)
    }

    async fn stats(&self, mind: &str) -> Result<MemoryStats> {
        let conn = self.conn.lock().unwrap();
        let total: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM facts WHERE mind = ?1",
                params![mind],
                |r| r.get(0),
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;
        let active: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM facts WHERE mind = ?1 AND status = 'active'",
                params![mind],
                |r| r.get(0),
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;
        let archived: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM facts WHERE mind = ?1 AND status = 'archived'",
                params![mind],
                |r| r.get(0),
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;
        let superseded: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM facts WHERE mind = ?1 AND status = 'superseded'",
                params![mind],
                |r| r.get(0),
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;
        let with_vecs: usize = conn.query_row(
            "SELECT COUNT(*) FROM facts_vec fv JOIN facts f ON f.id = fv.fact_id WHERE f.mind = ?1",
            params![mind], |r| r.get(0),
        ).map_err(|e| MemoryError::Storage(e.into()))?;
        let episodes: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE mind = ?1",
                params![mind],
                |r| r.get(0),
            )
            .map_err(|e| MemoryError::Storage(e.into()))?;
        let edges: usize = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE source_fact_id IN (SELECT id FROM facts WHERE mind = ?1)",
            params![mind], |r| r.get(0),
        ).map_err(|e| MemoryError::Storage(e.into()))?;
        let version_hwm: u64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM facts WHERE mind = ?1",
                params![mind],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| MemoryError::Storage(e.into()))? as u64;

        let meta: Option<(String, u32)> = conn
            .query_row(
                "SELECT em.model_name, em.dims FROM embedding_metadata em \
             JOIN facts_vec fv ON fv.model_name = em.model_name \
             JOIN facts f ON f.id = fv.fact_id \
             WHERE f.mind = ?1 LIMIT 1",
                params![mind],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| MemoryError::Storage(e.into()))?;

        Ok(MemoryStats {
            total_facts: total,
            active_facts: active,
            archived_facts: archived,
            superseded_facts: superseded,
            facts_with_vectors: with_vecs,
            embedding_model: meta.as_ref().map(|t: &(String, u32)| t.0.clone()),
            embedding_dims: meta.as_ref().map(|t: &(String, u32)| t.1),
            episodes,
            edges,
            version_hwm,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::run_backend_tests;

    #[tokio::test]
    async fn sqlite_backend_passes_all_tests() {
        let backend = SqliteBackend::in_memory().unwrap();
        run_backend_tests(&backend).await;
    }

    fn create_legacy_fixture(path: &Path, version: i64) {
        let backend = SqliteBackend::open(path).unwrap();
        drop(backend);
        let conn = Connection::open(path).unwrap();
        conn.execute(
            "INSERT INTO minds (name, description, created_at) VALUES ('default', 'Default legacy fixture', datetime('now')) ON CONFLICT(name) DO NOTHING",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO facts (id, mind, section, content, status, created_at, source, content_hash, confidence, last_reinforced, reinforcement_count, decay_rate, decay_profile, version, layer) VALUES ('legacy-fact', 'default', 'architecture', 'legacy content', 'active', datetime('now'), 'test', 'legacy-hash', 1.0, datetime('now'), 1, 0.05, 'standard', 1, 'project')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO episodes (id, mind, title, narrative, date, created_at) VALUES ('legacy-episode', 'default', 'legacy', 'legacy narrative', date('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM schema_version", []).unwrap();
        conn.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?1, datetime('now'))",
            params![version],
        )
        .unwrap();
    }

    #[test]
    fn migrates_representative_v5_and_v6_stores_with_rollback_backup() {
        for version in LEGACY_MEMORY_SCHEMA_VERSIONS {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(format!("facts-v{version}.db"));
            create_legacy_fixture(&path, version);

            let plan = SqliteBackend::plan_migration(&path).unwrap();
            assert!(plan.is_applicable());
            assert_eq!(plan.fact_count, 1);
            assert_eq!(plan.episode_count, 1);
            let result = SqliteBackend::apply_migration(&plan).unwrap();
            assert_eq!(result.source_version, version);
            assert_eq!(result.target_version, MEMORY_SCHEMA_VERSION);
            assert!(result.backup.exists());
            assert_eq!(
                SqliteBackend::inspect_current(&result.backup)
                    .unwrap()
                    .source_version,
                version
            );
            assert_eq!(
                SqliteBackend::inspect_current(&path)
                    .unwrap()
                    .source_version,
                MEMORY_SCHEMA_VERSION
            );
            drop(SqliteBackend::open(&path).unwrap());
        }
    }

    #[test]
    fn migration_rejects_unsupported_version_and_existing_backup() {
        let dir = tempfile::tempdir().unwrap();
        let unsupported = dir.path().join("unsupported.db");
        create_legacy_fixture(&unsupported, 4);
        assert!(SqliteBackend::plan_migration(&unsupported).is_err());

        let path = dir.path().join("facts-v6.db");
        create_legacy_fixture(&path, 6);
        let plan = SqliteBackend::plan_migration(&path).unwrap();
        std::fs::write(&plan.backup, b"do not overwrite").unwrap();
        let error = SqliteBackend::apply_migration(&plan).unwrap_err();
        assert!(error.to_string().contains("refusing to overwrite"));
    }

    #[test]
    fn migration_rejects_stale_plan_without_leaving_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("facts-v6.db");
        create_legacy_fixture(&path, 6);
        let plan = SqliteBackend::plan_migration(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO facts (id, mind, section, content, status, created_at, source, content_hash, confidence, last_reinforced, reinforcement_count, decay_rate, decay_profile, version, layer) VALUES ('late-fact', 'default', 'architecture', 'late content', 'active', datetime('now'), 'test', 'late-hash', 1.0, datetime('now'), 1, 0.05, 'standard', 2, 'project')",
            [],
        )
        .unwrap();
        drop(conn);

        let error = SqliteBackend::apply_migration(&plan).unwrap_err();
        assert!(error.to_string().contains("counts changed"));
        assert!(!plan.backup.exists());
        assert_eq!(
            SqliteBackend::inspect_current(&path)
                .unwrap()
                .source_version,
            6
        );
    }

    #[test]
    fn rollback_restores_legacy_snapshot_and_preserves_v7_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("facts-v6.db");
        create_legacy_fixture(&path, 6);
        let plan = SqliteBackend::plan_migration(&path).unwrap();
        let migrated = SqliteBackend::apply_migration(&plan).unwrap();

        let rolled_back = SqliteBackend::rollback_migration(&path, &migrated.backup).unwrap();
        assert_eq!(rolled_back.restored_version, 6);
        assert!(rolled_back.preserved_current.exists());
        assert_eq!(SqliteBackend::status(&path).unwrap().schema_version, 6);
        assert_eq!(
            SqliteBackend::status(&rolled_back.preserved_current)
                .unwrap()
                .schema_version,
            MEMORY_SCHEMA_VERSION
        );
    }

    /// Generate schema-contract.json from the actual Rust schema.
    /// This is the canonical contract — TS validates against it.
    /// If the Rust schema changes, re-run this test to update the contract:
    ///   cargo test -p omegon-memory schema_contract -- --ignored
    /// Then commit the updated schema-contract.json.
    #[test]
    #[ignore] // Run manually: cargo test -p omegon-memory schema_contract -- --ignored
    fn schema_contract_generate() {
        let backend = SqliteBackend::in_memory().unwrap();
        let conn = backend.conn.lock().unwrap();

        let contract = generate_schema_contract(&conn);
        let contract_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema-contract.json");
        std::fs::write(&contract_path, &contract)
            .unwrap_or_else(|e| panic!("Failed to write {}: {}", contract_path.display(), e));
        println!("Updated {}", contract_path.display());
    }

    /// Validate that schema-contract.json is up to date with the actual Rust schema.
    /// Fails CI if someone changed sqlite.rs without regenerating the contract.
    #[test]
    fn schema_contract_is_current() {
        let backend = SqliteBackend::in_memory().unwrap();
        let conn = backend.conn.lock().unwrap();

        let contract_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema-contract.json");
        let on_disk = std::fs::read_to_string(&contract_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}. Run: cargo test -p omegon-memory schema_contract_generate -- --ignored", contract_path.display(), e));
        let generated = generate_schema_contract(&conn);

        assert_eq!(
            on_disk.trim(),
            generated.trim(),
            "schema-contract.json is stale. Regenerate with:\n  cargo test -p omegon-memory schema_contract_generate -- --ignored"
        );
    }

    fn generate_schema_contract(conn: &Connection) -> String {
        use std::collections::BTreeMap;

        // Get all real tables (not sqlite_ internal, not FTS virtual tables)
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '%_fts%' ORDER BY name"
        ).unwrap();
        let table_names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let mut tables = BTreeMap::new();
        for table in &table_names {
            let mut col_stmt = conn
                .prepare(&format!("PRAGMA table_info({})", table))
                .unwrap();
            let cols: Vec<String> = col_stmt
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            tables.insert(table.clone(), cols);
        }

        let mut out = String::from("{\n");
        out.push_str("  \"description\": \"Canonical memory DB schema. Generated from Rust omegon-memory. Do not edit — regenerate with: cargo test -p omegon-memory schema_contract_generate -- --ignored\",\n");
        let version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        out.push_str(&format!("  \"schema_version\": {},\n", version));
        out.push_str("  \"tables\": {\n");
        let table_count = tables.len();
        for (i, (table, cols)) in tables.iter().enumerate() {
            out.push_str(&format!("    \"{}\": [", table));
            for (j, col) in cols.iter().enumerate() {
                out.push_str(&format!("\"{}\"", col));
                if j < cols.len() - 1 {
                    out.push_str(", ");
                }
            }
            out.push(']');
            if i < table_count - 1 {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  }\n");
        out.push_str("}\n");
        out
    }
}

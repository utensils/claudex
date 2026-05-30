use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime, Utc};
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, params, params_from_iter};

use crate::cli::ResolvedFilter;
use crate::parser::ModelSessionStats;
use crate::providers::Provider;
use crate::stats::percentile_sorted;
use crate::types::ModelPricing;
use crate::ui;

/// Convert an optional substring filter into a SQL `LIKE` value, NULL if absent.
fn opt_like(filter: Option<&str>) -> SqlValue {
    match filter {
        Some(f) => SqlValue::Text(format!("%{f}%")),
        None => SqlValue::Null,
    }
}

fn looks_like_session_id_prefix(selector: &str) -> bool {
    let compact = selector.replace('-', "");
    compact.len() >= 6 && compact.chars().all(|c| c.is_ascii_hexdigit())
}

const STALE_SECS: u64 = 300;
const SCHEMA_VERSION: i64 = 5;

/// Child tables whose rows hang off a single `sessions` row, paired with the
/// column that references `sessions(id)`. `messages_fts` is a virtual table
/// (no `ON DELETE CASCADE`), so we always clean it up explicitly; the rest are
/// listed here so re-indexing a session in place can clear its derived rows
/// without deleting (and thus losing the retained) parent row.
const DERIVED_TABLES: &[(&str, &str)] = &[
    ("token_usage", "session_id"),
    ("tool_calls", "session_id"),
    ("turn_durations", "session_rowid"),
    ("pr_links", "session_rowid"),
    ("file_modifications", "session_rowid"),
    ("thinking_usage", "session_rowid"),
    ("stop_reasons", "session_rowid"),
    ("attachments", "session_rowid"),
    ("permission_changes", "session_rowid"),
    ("messages_fts", "session_id"),
];

/// Delete every derived row for a session id across all child tables. Used when
/// re-indexing a changed file in place: the `sessions` row (and its retained
/// metadata) is kept and updated, but its derived data is rebuilt from scratch.
fn delete_session_derived(tx: &rusqlite::Transaction, session_id: i64) -> Result<()> {
    for (table, col) in DERIVED_TABLES {
        tx.execute(
            &format!("DELETE FROM {table} WHERE {col} = ?"),
            params![session_id],
        )?;
    }
    Ok(())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct IndexStore {
    conn: Connection,
}

// --- Public result types ---

#[derive(Clone)]
pub struct IndexedSession {
    pub provider: String,
    pub project_name: String,
    pub session_id: Option<String>,
    pub file_path: String,
    pub first_timestamp_ms: Option<i64>,
    pub message_count: i64,
    pub duration_ms: i64,
    pub model: Option<String>,
}

pub struct ProjectCostRow {
    pub project: String,
    pub session_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
    pub models: Vec<String>,
}

pub struct SessionCostRow {
    pub provider: String,
    pub project: String,
    pub session_id: Option<String>,
    pub first_timestamp_ms: Option<i64>,
    pub models: Vec<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
}

/// Unlimited grand-total aggregate for the `cost` views, computed independently
/// of `--limit` so the `TOTAL` row reflects every matching project/session, not
/// just the top-N that are displayed. Matches the model totals reported by
/// [`IndexStore::query_model_usage`] (same `SUM(token_usage.cost_usd)`).
pub struct CostSummary {
    pub session_count: i64,
    pub project_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
}

pub struct ToolRow {
    pub tool_name: String,
    pub count: i64,
}

pub struct SessionToolRow {
    pub project: String,
    pub session_id: Option<String>,
    pub first_timestamp_ms: Option<i64>,
    pub tools: HashMap<String, i64>,
}

pub struct SearchHit {
    pub provider: String,
    pub project_name: String,
    pub session_id: Option<String>,
    pub message_timestamp_ms: Option<i64>,
    pub message_type: String,
    pub snippet: String,
    pub rank: f64,
}

pub struct SummaryData {
    pub total_sessions: i64,
    pub sessions_today: i64,
    pub sessions_this_week: i64,
    pub total_cost: f64,
    pub week_cost: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cache_creation: i64,
    pub total_cache_read: i64,
    pub top_projects: Vec<(String, i64)>,
    pub top_tools: Vec<(String, i64)>,
    pub top_stop_reasons: Vec<(String, i64)>,
    pub most_recent: Option<MostRecentSession>,
    // Extended metric summary
    pub thinking_block_count: i64,
    pub avg_turn_duration_ms: Option<f64>,
    pub pr_count: i64,
    pub files_modified_count: i64,
    pub model_distribution: Vec<(String, i64, f64)>, // (model_family, sessions, cost)
}

pub struct MostRecentSession {
    pub project: String,
    pub session_id: String,
    pub first_timestamp_ms: i64,
    pub model: Option<String>,
    pub message_count: i64,
}

pub struct TurnStatsRow {
    pub project: String,
    pub turn_count: i64,
    pub avg_duration_ms: f64,
    pub p50_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub max_duration_ms: i64,
}

pub struct PrLinkRow {
    pub provider: String,
    pub project: String,
    pub session_id: Option<String>,
    pub pr_number: i64,
    pub pr_url: String,
    pub pr_repository: String,
    pub timestamp: String,
}

pub struct FileModRow {
    pub file_path: String,
    pub modification_count: i64,
    pub distinct_session_count: i64,
    pub last_touched_timestamp_ms: Option<i64>,
    pub top_project: Option<String>,
}

pub struct ModelUsageRow {
    pub model: String,
    pub session_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
    pub avg_cost_per_session_usd: f64,
    pub avg_tokens_per_session: f64,
    pub service_tiers: Vec<String>,
    pub inference_geos: Vec<String>,
    pub avg_speed: Option<f64>,
    pub total_iterations: i64,
}

pub struct SessionModelUsageRow {
    pub model: String,
    pub assistant_message_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
    pub inference_geos: Vec<String>,
    pub service_tiers: Vec<String>,
    pub avg_speed: Option<f64>,
    pub iterations: i64,
}

pub struct StopReasonRow {
    pub stop_reason: String,
    pub count: i64,
}

pub struct AttachmentRow {
    pub filename: String,
    pub mime_type: String,
}

pub struct PermissionChangeRow {
    pub mode: String,
    pub timestamp: String,
}

pub struct SessionDetail {
    pub project: String,
    pub file_path: String,
    pub session_id: Option<String>,
    pub first_timestamp_ms: Option<i64>,
    pub last_timestamp_ms: Option<i64>,
    pub duration_ms: i64,
    pub message_count: i64,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
    pub thinking_block_count: i64,
    pub files_modified: Vec<String>,
    pub tools: Vec<ToolRow>,
    pub pr_links: Vec<PrLinkRow>,
    pub turn_stats: Option<TurnStatsRow>,
    pub stop_reasons: Vec<StopReasonRow>,
    pub attachments: Vec<AttachmentRow>,
    pub permission_changes: Vec<PermissionChangeRow>,
    pub model_usage: Vec<SessionModelUsageRow>,
    pub subagent_files: Vec<String>,
}

type SessionRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
    Option<String>,
);

#[derive(Default)]
struct ModelUsageAccumulator {
    session_ids: HashSet<String>,
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    cost_usd: f64,
    service_tiers: BTreeSet<String>,
    inference_geos: BTreeSet<String>,
    speed_sum: f64,
    speed_samples: u64,
    total_iterations: i64,
}

// --- Internal sync types ---

/// A `sessions` row's identity and on-disk fingerprint, loaded at the start of
/// a sync so changed/unchanged/missing files can be reconciled in one pass.
struct KnownFile {
    id: i64,
    size: i64,
    mtime: i64,
    present: i64,
}

impl IndexStore {
    pub fn open() -> Result<Self> {
        let dir = crate::claudex_dir()?;
        Self::open_at(&dir.join("index.db"))
    }

    /// Open (or create) an index at an explicit path. Used by integration
    /// tests so they don't have to mutate `$HOME`.
    pub fn open_at(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.create_schema()?;
        Ok(store)
    }

    fn create_schema(&self) -> Result<()> {
        // Create meta first so we can read the stored schema version
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;

        let stored_version: Option<i64> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| s.parse().ok());

        // The index is no longer an expendable cache: retained sessions (those
        // archived or deleted from disk) cannot be rebuilt from source, so a
        // schema bump must MIGRATE rather than DROP. The forward-only ladder in
        // `migrate_schema` applies additive `ALTER TABLE`s; the only destructive
        // path left is the explicit `claudex index --force`.
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                id              INTEGER PRIMARY KEY,
                project_name    TEXT    NOT NULL,
                file_path       TEXT    NOT NULL UNIQUE,
                file_size       INTEGER NOT NULL,
                file_mtime      INTEGER NOT NULL,
                session_id      TEXT,
                parent_session_id TEXT,
                first_timestamp INTEGER,
                last_timestamp  INTEGER,
                duration_ms     INTEGER NOT NULL DEFAULT 0,
                message_count   INTEGER NOT NULL DEFAULT 0,
                model           TEXT,
                indexed_at      INTEGER NOT NULL,
                provider        TEXT    NOT NULL DEFAULT 'claude',
                present_on_disk INTEGER NOT NULL DEFAULT 1,
                archived_at     INTEGER,
                last_seen       INTEGER,
                extras          TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_project   ON sessions(project_name);
            CREATE INDEX IF NOT EXISTS idx_sessions_timestamp ON sessions(first_timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_sessions_parent    ON sessions(parent_session_id);
            CREATE TABLE IF NOT EXISTS token_usage (
                session_id            INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                model                 TEXT,
                assistant_message_count INTEGER NOT NULL DEFAULT 0,
                input_tokens          INTEGER NOT NULL DEFAULT 0,
                output_tokens         INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens     INTEGER NOT NULL DEFAULT 0,
                cost_usd              REAL    NOT NULL DEFAULT 0.0,
                inference_geo         TEXT,
                speed                 REAL,
                service_tier          TEXT,
                iterations            INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_token_usage_session ON token_usage(session_id);
            CREATE TABLE IF NOT EXISTS tool_calls (
                session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                tool_name  TEXT    NOT NULL,
                count      INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_tool_calls_session ON tool_calls(session_id);
            CREATE TABLE IF NOT EXISTS turn_durations (
                session_rowid INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                turn_number   INTEGER NOT NULL,
                duration_ms   INTEGER NOT NULL,
                timestamp     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_turn_durations_session ON turn_durations(session_rowid);
            CREATE TABLE IF NOT EXISTS pr_links (
                session_rowid  INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                pr_number      INTEGER NOT NULL DEFAULT 0,
                pr_url         TEXT    NOT NULL DEFAULT '',
                pr_repository  TEXT    NOT NULL DEFAULT '',
                timestamp      TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_pr_links_session ON pr_links(session_rowid);
            CREATE TABLE IF NOT EXISTS file_modifications (
                session_rowid      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                file_path          TEXT    NOT NULL,
                is_snapshot_update INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_file_mods_session ON file_modifications(session_rowid);
            CREATE INDEX IF NOT EXISTS idx_file_mods_path    ON file_modifications(file_path);
            CREATE TABLE IF NOT EXISTS thinking_usage (
                session_rowid   INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                thinking_blocks INTEGER NOT NULL DEFAULT 0,
                thinking_tokens INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_thinking_session ON thinking_usage(session_rowid);
            CREATE TABLE IF NOT EXISTS stop_reasons (
                session_rowid INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                stop_reason   TEXT    NOT NULL,
                count         INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_stop_reasons_session ON stop_reasons(session_rowid);
            CREATE TABLE IF NOT EXISTS attachments (
                session_rowid INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                filename      TEXT    NOT NULL,
                mime_type     TEXT    NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_attachments_session ON attachments(session_rowid);
            CREATE TABLE IF NOT EXISTS permission_changes (
                session_rowid INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                mode          TEXT    NOT NULL,
                timestamp     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_permission_session ON permission_changes(session_rowid);
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                session_id   UNINDEXED,
                message_type,
                content,
                timestamp    UNINDEXED,
                tokenize     = 'porter unicode61'
            );
            "#,
        )?;

        // Apply forward-only migrations for DBs created before the current
        // schema. Fresh DBs already have every column from the CREATE above, so
        // the guarded `ALTER`s no-op; older DBs gain the new columns in place.
        self.migrate_schema(stored_version)?;

        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?)",
            params![SCHEMA_VERSION.to_string()],
        )?;

        Ok(())
    }

    /// Returns true if `table` has a column named `col`. Table/column names are
    /// hardcoded literals at every call site, so the formatted PRAGMA is safe.
    fn column_exists(&self, table: &str, col: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let found = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|name| name == col);
        Ok(found)
    }

    /// `ALTER TABLE ADD COLUMN` is not idempotent in SQLite (there is no
    /// `IF NOT EXISTS`), so guard it with a `PRAGMA table_info` check.
    fn add_column_if_missing(&self, table: &str, col: &str, decl: &str) -> Result<()> {
        if !self.column_exists(table, col)? {
            self.conn
                .execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {col} {decl};"))?;
        }
        Ok(())
    }

    /// Forward-only migration ladder keyed on the stored `schema_version`. Each
    /// step is additive and idempotent so retained data is never lost. `from`
    /// is `None` for a brand-new DB (every column already exists → all no-ops).
    fn migrate_schema(&self, from: Option<i64>) -> Result<()> {
        // v4 → v5: provider awareness + additive retention metadata.
        if from.unwrap_or(0) < 5 {
            self.add_column_if_missing("sessions", "provider", "TEXT NOT NULL DEFAULT 'claude'")?;
            self.add_column_if_missing(
                "sessions",
                "present_on_disk",
                "INTEGER NOT NULL DEFAULT 1",
            )?;
            self.add_column_if_missing("sessions", "archived_at", "INTEGER")?;
            self.add_column_if_missing("sessions", "last_seen", "INTEGER")?;
            self.add_column_if_missing("sessions", "extras", "TEXT")?;
            self.conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_sessions_provider ON sessions(provider);
                 CREATE INDEX IF NOT EXISTS idx_sessions_present  ON sessions(present_on_disk);",
            )?;
        }
        Ok(())
    }

    /// Sync any provider whose index is stale, showing a spinner on stderr
    /// while it runs (TTY-gated). Each provider has its own staleness window and
    /// data-root stamp, so they sync independently.
    pub fn ensure_fresh(&mut self, providers: &[Provider]) -> Result<()> {
        let stale: Vec<&Provider> = providers
            .iter()
            .filter(|p| self.provider_is_stale(p))
            .collect();
        if stale.is_empty() {
            return Ok(());
        }

        let message = if self.any_provider_synced() {
            "Syncing index..."
        } else {
            "Building index..."
        };
        let spinner = ui::Spinner::start(message);
        let mut result = Ok(());
        for provider in stale {
            if let Err(e) = self.sync_provider(provider) {
                result = Err(e);
                break;
            }
        }
        spinner.finish();
        result
    }

    /// A provider is stale when it has never synced, its staleness window has
    /// elapsed, or its data root changed since the last sync — the last guards a
    /// `CLAUDEX_DIR` shared across different `$HOME` values from serving rows
    /// indexed under a previous home.
    fn provider_is_stale(&self, provider: &Provider) -> bool {
        let id = provider.id();
        let last_sync: Option<u64> = self
            .meta_get(&format!("last_sync:{id}"))
            .and_then(|s| s.parse().ok());
        let root = provider.root_dir().to_string_lossy().into_owned();
        let root_changed =
            self.meta_get(&format!("sessions_root:{id}")).as_deref() != Some(root.as_str());
        match last_sync {
            Some(ls) => now_unix_secs().saturating_sub(ls) >= STALE_SECS || root_changed,
            None => true,
        }
    }

    /// Whether any provider has ever completed a sync (controls spinner copy).
    fn any_provider_synced(&self) -> bool {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'last_sync:%'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    fn meta_get(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?", params![key], |r| {
                r.get::<_, String>(0)
            })
            .ok()
    }

    /// Force a full rebuild of every provider. This is the ONE destructive path
    /// — it discards retained/archived data — and only `claudex index --force`
    /// calls it.
    pub fn force_rebuild(&mut self, providers: &[Provider]) -> Result<usize> {
        self.conn
            .execute_batch("DELETE FROM messages_fts; DELETE FROM sessions;")?;
        self.conn.execute(
            "DELETE FROM meta WHERE key LIKE 'last_sync:%' OR key LIKE 'sessions_root:%'",
            [],
        )?;
        self.sync(providers)
    }

    /// Run an incremental sync of every provider now (bypass staleness check).
    pub fn sync_now(&mut self, providers: &[Provider]) -> Result<usize> {
        self.sync(providers)
    }

    fn sync(&mut self, providers: &[Provider]) -> Result<usize> {
        let mut total = 0;
        for provider in providers {
            total += self.sync_provider(provider)?;
        }
        Ok(total)
    }

    /// Incrementally sync one provider's transcripts. Every reconciliation query
    /// is scoped to `provider.id()` so a provider's sync never archives another
    /// provider's rows just because they aren't in this provider's enumeration.
    fn sync_provider(&mut self, provider: &Provider) -> Result<usize> {
        let provider_id = provider.id();

        // Load known file states for THIS provider only. `id`/`present_on_disk`
        // let us re-index in place and un-archive a file that reappears.
        let mut known: HashMap<String, KnownFile> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT file_path, id, file_size, file_mtime, present_on_disk
                 FROM sessions WHERE provider = ?",
            )?;
            let rows = stmt.query_map(params![provider_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    KnownFile {
                        id: row.get::<_, i64>(1)?,
                        size: row.get::<_, i64>(2)?,
                        mtime: row.get::<_, i64>(3)?,
                        present: row.get::<_, i64>(4)?,
                    },
                ))
            })?;
            for row in rows {
                let (p, k) = row?;
                known.insert(p, k);
            }
        }

        let files = provider.enumerate()?;
        let mut seen: HashSet<String> = HashSet::new();
        let now_secs = now_unix_secs() as i64;
        let mut indexed_count = 0usize;

        let tx = self.conn.transaction()?;

        for discovered in &files {
            let file_path = &discovered.path;
            let path_str = file_path.to_string_lossy().into_owned();
            seen.insert(path_str.clone());

            let meta = match std::fs::metadata(file_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size = meta.len() as i64;
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let prior = known.get(&path_str);
            let reuse_id = match prior {
                Some(k) if k.size == size && k.mtime == mtime => {
                    // Unchanged on disk. If it was previously archived (file had
                    // disappeared and came back byte-identical), un-archive it.
                    if k.present == 0 {
                        tx.execute(
                            "UPDATE sessions
                             SET present_on_disk = 1, archived_at = NULL, last_seen = ?
                             WHERE id = ?",
                            params![now_secs, k.id],
                        )?;
                    }
                    continue;
                }
                // Changed: rebuild this session's derived rows in place, keeping
                // the stable `sessions` rowid (and any retained metadata).
                Some(k) => {
                    delete_session_derived(&tx, k.id)?;
                    Some(k.id)
                }
                None => None,
            };

            let parent_session_id = discovered.parent_session_id.clone();
            let mut entry = match provider.parse(discovered) {
                Ok(e) => e,
                Err(_) => continue,
            };

            // The enumerator's path-derived project is the default; a provider
            // that reads the project from the transcript itself (Codex stores
            // its cwd in `session_meta`) overrides it.
            let project_display = if entry.project_display.is_empty() {
                discovered.project_display.clone()
            } else {
                entry.project_display.clone()
            };

            // Fall back to file stem when the transcript lacks a session id
            if entry.session_id.is_none() {
                entry.session_id = file_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned());
            }

            // A transcript living in the provider's archive location is indexed
            // but stamped archived from the start.
            let archived_at: Option<i64> = if discovered.archived {
                Some(now_secs)
            } else {
                None
            };

            let first_ts = entry.first_timestamp.map(|d| d.timestamp_millis());
            let last_ts = entry.last_timestamp.map(|d| d.timestamp_millis());
            let session_model = session_model_label(entry.model.as_deref(), &entry.model_usage);

            let row_id = if let Some(old_id) = reuse_id {
                tx.execute(
                    r#"UPDATE sessions SET
                           project_name = ?, file_size = ?, file_mtime = ?, session_id = ?,
                           parent_session_id = ?, first_timestamp = ?, last_timestamp = ?,
                           duration_ms = ?, message_count = ?, model = ?, indexed_at = ?,
                           provider = ?, present_on_disk = 1, archived_at = ?, last_seen = ?, extras = ?
                       WHERE id = ?"#,
                    params![
                        project_display,
                        size,
                        mtime,
                        entry.session_id,
                        parent_session_id,
                        first_ts,
                        last_ts,
                        entry.duration_ms as i64,
                        entry.message_count as i64,
                        session_model,
                        now_secs,
                        provider_id,
                        archived_at,
                        now_secs,
                        entry.extras,
                        old_id,
                    ],
                )?;
                old_id
            } else {
                tx.execute(
                    r#"INSERT INTO sessions
                       (project_name, file_path, file_size, file_mtime, session_id, parent_session_id,
                        first_timestamp, last_timestamp, duration_ms, message_count, model, indexed_at,
                        provider, present_on_disk, archived_at, last_seen, extras)
                       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)"#,
                    params![
                        project_display,
                        path_str,
                        size,
                        mtime,
                        entry.session_id,
                        parent_session_id,
                        first_ts,
                        last_ts,
                        entry.duration_ms as i64,
                        entry.message_count as i64,
                        session_model,
                        now_secs,
                        provider_id,
                        archived_at,
                        now_secs,
                        entry.extras,
                    ],
                )?;
                tx.last_insert_rowid()
            };

            if entry.model_usage.is_empty() && entry.usage.total_tokens() > 0 {
                let cost = entry
                    .embedded_cost
                    .unwrap_or_else(|| entry.usage.cost_for_model(entry.model.as_deref()));
                tx.execute(
                    r#"INSERT INTO token_usage
                       (session_id, model, assistant_message_count, input_tokens, output_tokens,
                        cache_creation_tokens, cache_read_tokens, cost_usd,
                        inference_geo, speed, service_tier, iterations)
                       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                    params![
                        row_id,
                        entry.model,
                        0i64,
                        entry.usage.input_tokens as i64,
                        entry.usage.output_tokens as i64,
                        entry.usage.cache_creation_tokens as i64,
                        entry.usage.cache_read_tokens as i64,
                        cost,
                        entry.inference_geo,
                        entry.speed,
                        entry.service_tier,
                        entry.iterations as i64,
                    ],
                )?;
            } else if !entry.model_usage.is_empty() {
                for (model, usage) in &entry.model_usage {
                    if usage.usage.total_tokens() == 0 {
                        continue;
                    }
                    let model_opt = if model.is_empty() {
                        None
                    } else {
                        Some(model.as_str())
                    };
                    let cost = usage
                        .embedded_cost
                        .unwrap_or_else(|| usage.usage.cost_for_model(model_opt));
                    tx.execute(
                        r#"INSERT INTO token_usage
                           (session_id, model, assistant_message_count, input_tokens, output_tokens,
                            cache_creation_tokens, cache_read_tokens, cost_usd,
                            inference_geo, speed, service_tier, iterations)
                           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                        params![
                            row_id,
                            model_opt,
                            usage.assistant_message_count as i64,
                            usage.usage.input_tokens as i64,
                            usage.usage.output_tokens as i64,
                            usage.usage.cache_creation_tokens as i64,
                            usage.usage.cache_read_tokens as i64,
                            cost,
                            join_strings(&usage.inference_geos),
                            usage.avg_speed(),
                            join_strings(&usage.service_tiers),
                            usage.iterations as i64,
                        ],
                    )?;
                }
            }

            let mut tool_counts: HashMap<String, i64> = HashMap::new();
            for name in &entry.tool_names {
                *tool_counts.entry(name.clone()).or_insert(0) += 1;
            }
            for (tool_name, count) in &tool_counts {
                tx.execute(
                    "INSERT INTO tool_calls (session_id, tool_name, count) VALUES (?, ?, ?)",
                    params![row_id, tool_name, count],
                )?;
            }

            for (i, (dur, ts)) in entry.turn_durations.iter().enumerate() {
                tx.execute(
                    "INSERT INTO turn_durations (session_rowid, turn_number, duration_ms, timestamp) VALUES (?, ?, ?, ?)",
                    params![row_id, (i + 1) as i64, *dur as i64, ts],
                )?;
            }

            for (pr_num, url, repo, ts) in &entry.pr_links {
                tx.execute(
                    "INSERT INTO pr_links (session_rowid, pr_number, pr_url, pr_repository, timestamp) VALUES (?, ?, ?, ?, ?)",
                    params![row_id, pr_num, url, repo, ts],
                )?;
            }

            for fp in &entry.file_paths_modified {
                tx.execute(
                    "INSERT INTO file_modifications (session_rowid, file_path, is_snapshot_update) VALUES (?, ?, 1)",
                    params![row_id, fp],
                )?;
            }

            if entry.thinking_block_count > 0 {
                tx.execute(
                    "INSERT INTO thinking_usage (session_rowid, thinking_blocks, thinking_tokens) VALUES (?, ?, NULL)",
                    params![row_id, entry.thinking_block_count as i64],
                )?;
            }

            for (reason, count) in &entry.stop_reason_counts {
                tx.execute(
                    "INSERT INTO stop_reasons (session_rowid, stop_reason, count) VALUES (?, ?, ?)",
                    params![row_id, reason, *count as i64],
                )?;
            }

            for (filename, mime) in &entry.attachments {
                tx.execute(
                    "INSERT INTO attachments (session_rowid, filename, mime_type) VALUES (?, ?, ?)",
                    params![row_id, filename, mime],
                )?;
            }

            for (mode, ts) in &entry.permission_modes {
                tx.execute(
                    "INSERT INTO permission_changes (session_rowid, mode, timestamp) VALUES (?, ?, ?)",
                    params![row_id, mode, ts],
                )?;
            }

            for msg in &entry.messages {
                tx.execute(
                    "INSERT INTO messages_fts (session_id, message_type, content, timestamp) VALUES (?, ?, ?, ?)",
                    params![row_id, msg.msg_type, msg.content, msg.timestamp_ms],
                )?;
            }

            indexed_count += 1;
        }

        // Soft-delete entries whose source file is gone. The index is additive:
        // archived/deleted sessions are RETAINED (their derived rows and FTS
        // content stay) and merely flagged, so historical usage never vanishes
        // when a transcript is cleaned off disk. `claudex index --force` is the
        // only path that actually discards retained data.
        for (path, k) in &known {
            if !seen.contains(path) && k.present == 1 {
                tx.execute(
                    "UPDATE sessions
                     SET present_on_disk = 0, archived_at = COALESCE(archived_at, ?)
                     WHERE id = ?",
                    params![now_secs, k.id],
                )?;
            }
        }

        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)",
            params![
                format!("last_sync:{provider_id}"),
                now_unix_secs().to_string()
            ],
        )?;
        // Stamp the provider's data root so `provider_is_stale` can invalidate
        // the staleness shortcut when the root changes (e.g. a different $HOME
        // sharing one `CLAUDEX_DIR`).
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)",
            params![
                format!("sessions_root:{provider_id}"),
                provider.root_dir().to_string_lossy().into_owned()
            ],
        )?;
        tx.commit()?;

        Ok(indexed_count)
    }

    // --- Query methods ---

    pub fn query_sessions(
        &self,
        project_filter: Option<&str>,
        file_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<IndexedSession>> {
        let project = opt_like(project_filter);
        let file = opt_like(file_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"SELECT s.provider, s.project_name, s.session_id, s.file_path, s.first_timestamp,
                      s.message_count, s.duration_ms, s.model
               FROM sessions s
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?)
                 AND (? IS NULL OR EXISTS (
                       SELECT 1
                       FROM file_modifications fm
                       WHERE fm.session_rowid = s.id
                         AND fm.file_path LIKE ?
                 )){pred}
               ORDER BY s.first_timestamp DESC
               LIMIT ?"#
        );
        let mut binds: Vec<SqlValue> = vec![
            project.clone(),
            project.clone(),
            project,
            file.clone(),
            file,
        ];
        binds.extend(pred_params);
        binds.push(SqlValue::Integer(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok(IndexedSession {
                provider: row.get(0)?,
                project_name: row.get(1)?,
                session_id: row.get(2)?,
                file_path: row.get(3)?,
                first_timestamp_ms: row.get(4)?,
                message_count: row.get(5)?,
                duration_ms: row.get(6)?,
                model: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn query_session_matches(
        &self,
        selector: &str,
        project_filter: Option<&str>,
    ) -> Result<Vec<IndexedSession>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT provider, project_name, session_id, file_path, first_timestamp,
                      message_count, duration_ms, model
               FROM sessions
               ORDER BY first_timestamp DESC, file_path ASC"#,
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(IndexedSession {
                    provider: row.get(0)?,
                    project_name: row.get(1)?,
                    session_id: row.get(2)?,
                    file_path: row.get(3)?,
                    first_timestamp_ms: row.get(4)?,
                    message_count: row.get(5)?,
                    duration_ms: row.get(6)?,
                    model: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let selector = selector.to_lowercase();
        let project_filter = project_filter.map(str::to_lowercase);
        let rows: Vec<_> = rows
            .into_iter()
            .filter(|row| {
                project_filter
                    .as_deref()
                    .is_none_or(|p| row.project_name.to_lowercase().contains(p))
            })
            .collect();

        let id_matches: Vec<_> = rows
            .iter()
            .filter(|row| {
                row.session_id
                    .as_deref()
                    .is_some_and(|id| id.to_lowercase().starts_with(&selector))
                    || Path::new(&row.file_path)
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().to_lowercase().starts_with(&selector))
                        .unwrap_or(false)
            })
            .cloned()
            .collect();

        if !id_matches.is_empty() || looks_like_session_id_prefix(&selector) {
            return Ok(id_matches);
        }

        Ok(rows
            .into_iter()
            .filter(|row| row.project_name.to_lowercase().contains(&selector))
            .collect())
    }

    pub fn query_cost_by_project(
        &self,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<ProjectCostRow>> {
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"SELECT s.project_name,
                      COUNT(DISTINCT s.provider || char(31) || COALESCE(s.parent_session_id, s.session_id, s.file_path)),
                      COALESCE(SUM(t.input_tokens), 0),
                      COALESCE(SUM(t.output_tokens), 0),
                      COALESCE(SUM(t.cache_creation_tokens), 0),
                      COALESCE(SUM(t.cache_read_tokens), 0),
                      COALESCE(SUM(t.cost_usd), 0),
                      GROUP_CONCAT(DISTINCT t.model)
               FROM sessions s
               LEFT JOIN token_usage t ON t.session_id = s.id
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}
               GROUP BY s.project_name
               ORDER BY COALESCE(SUM(t.cost_usd), 0) DESC
               LIMIT ?"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        binds.push(SqlValue::Integer(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            let models_raw: Option<String> = row.get(7)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, f64>(6)?,
                models_raw,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (project, session_count, inp, out, cache_c, cache_r, cost, models_raw) = row?;
            let models = model_families_from_concat(models_raw.as_deref());
            result.push(ProjectCostRow {
                project,
                session_count,
                input_tokens: inp,
                output_tokens: out,
                cache_creation_tokens: cache_c,
                cache_read_tokens: cache_r,
                cost_usd: cost,
                models,
            });
        }
        Ok(result)
    }

    pub fn query_cost_per_session(
        &self,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<SessionCostRow>> {
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"SELECT s.provider, s.project_name,
                      COALESCE(s.parent_session_id, s.session_id, s.file_path) AS display_session_id,
                      MIN(s.first_timestamp),
                      GROUP_CONCAT(DISTINCT t.model),
                      COALESCE(SUM(t.input_tokens), 0),
                      COALESCE(SUM(t.output_tokens), 0),
                      COALESCE(SUM(t.cache_creation_tokens), 0),
                      COALESCE(SUM(t.cache_read_tokens), 0),
                      COALESCE(SUM(t.cost_usd), 0)
               FROM sessions s
               JOIN token_usage t ON t.session_id = s.id
               WHERE (t.input_tokens + t.output_tokens + t.cache_creation_tokens + t.cache_read_tokens) > 0
                 AND (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}
               GROUP BY s.project_name, s.provider || char(31) || COALESCE(s.parent_session_id, s.session_id, s.file_path)
               ORDER BY SUM(t.cost_usd) DESC
               LIMIT ?"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        binds.push(SqlValue::Integer(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok(SessionCostRow {
                provider: row.get(0)?,
                project: row.get(1)?,
                session_id: row.get(2)?,
                first_timestamp_ms: row.get(3)?,
                models: split_joined_values(row.get::<_, Option<String>>(4)?.as_deref()),
                input_tokens: row.get(5)?,
                output_tokens: row.get(6)?,
                cache_creation_tokens: row.get(7)?,
                cache_read_tokens: row.get(8)?,
                cost_usd: row.get(9)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Unlimited grand totals for the `cost` views. Mirrors the
    /// `query_cost_per_session` join/filter (inner `token_usage` join,
    /// non-zero-token rows) so the summed cost equals what `models` reports,
    /// but with no `GROUP BY`/`LIMIT` — one row covering every match. The
    /// `session_count`/`project_count` give the full population behind a
    /// possibly-truncated display so callers can caption "showing top N of M".
    pub fn query_cost_summary(
        &self,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
    ) -> Result<CostSummary> {
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"SELECT COUNT(DISTINCT s.provider || char(31) || COALESCE(s.parent_session_id, s.session_id, s.file_path)),
                      COUNT(DISTINCT s.project_name),
                      COALESCE(SUM(t.input_tokens), 0),
                      COALESCE(SUM(t.output_tokens), 0),
                      COALESCE(SUM(t.cache_creation_tokens), 0),
                      COALESCE(SUM(t.cache_read_tokens), 0),
                      COALESCE(SUM(t.cost_usd), 0)
               FROM sessions s
               JOIN token_usage t ON t.session_id = s.id
               WHERE (t.input_tokens + t.output_tokens + t.cache_creation_tokens + t.cache_read_tokens) > 0
                 AND (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        let mut stmt = self.conn.prepare(&sql)?;
        let summary = stmt.query_row(params_from_iter(binds), |row| {
            Ok(CostSummary {
                session_count: row.get(0)?,
                project_count: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                cache_creation_tokens: row.get(4)?,
                cache_read_tokens: row.get(5)?,
                cost_usd: row.get(6)?,
            })
        })?;
        Ok(summary)
    }

    pub fn query_tools_aggregate(
        &self,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<ToolRow>> {
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"SELECT tc.tool_name, SUM(tc.count) AS total
               FROM tool_calls tc
               JOIN sessions s ON s.id = tc.session_id
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}
               GROUP BY tc.tool_name
               ORDER BY total DESC
               LIMIT ?"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        binds.push(SqlValue::Integer(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok(ToolRow {
                tool_name: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn query_tools_per_session(
        &self,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<SessionToolRow>> {
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"SELECT s.provider || char(31) || s.project_name || char(31) || COALESCE(s.parent_session_id, s.session_id, s.file_path) AS group_key,
                      s.project_name,
                      COALESCE(s.parent_session_id, s.session_id, s.file_path) AS display_session_id,
                      MIN(s.first_timestamp),
                      tc.tool_name,
                      SUM(tc.count)
               FROM sessions s
               JOIN tool_calls tc ON tc.session_id = s.id
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}
               GROUP BY group_key, s.project_name, display_session_id, tc.tool_name
               ORDER BY MIN(s.first_timestamp) DESC"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        let mut stmt = self.conn.prepare(&sql)?;

        let mut order: Vec<String> = Vec::new();
        let mut map: HashMap<String, SessionToolRow> = HashMap::new();

        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        for row in rows {
            let (group_key, project, session_id, first_ts, tool_name, count) = row?;
            let slot = map.entry(group_key.clone()).or_insert_with(|| {
                order.push(group_key);
                SessionToolRow {
                    project,
                    session_id,
                    first_timestamp_ms: first_ts,
                    tools: HashMap::new(),
                }
            });
            *slot.tools.entry(tool_name).or_insert(0) += count;
        }

        let mut result: Vec<SessionToolRow> = order
            .into_iter()
            .filter_map(|id| map.remove(&id))
            .filter(|r| !r.tools.is_empty())
            .collect();
        result.truncate(limit);
        Ok(result)
    }

    pub fn search_fts(
        &self,
        query: &str,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        let fts_query = fts_escape(query);
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"SELECT s.provider, s.project_name, s.session_id, f.timestamp, f.message_type,
                      snippet(messages_fts, 2, '[[', ']]', '...', 20),
                      bm25(messages_fts)
               FROM messages_fts f
               JOIN sessions s ON s.id = f.session_id
               WHERE messages_fts MATCH ?
                 AND (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}
               ORDER BY bm25(messages_fts)
               LIMIT ?"#
        );
        let mut binds: Vec<SqlValue> = vec![SqlValue::Text(fts_query), fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        binds.push(SqlValue::Integer(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok(SearchHit {
                provider: row.get(0)?,
                project_name: row.get(1)?,
                session_id: row.get(2)?,
                message_timestamp_ms: row.get(3)?,
                message_type: row.get(4)?,
                snippet: row.get(5)?,
                rank: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn query_turn_stats(
        &self,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<TurnStatsRow>> {
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");

        // Fetch all (project, duration_ms) pairs already sorted by duration for percentile math
        let sql = format!(
            r#"SELECT s.project_name, td.duration_ms
               FROM turn_durations td
               JOIN sessions s ON s.id = td.session_rowid
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}
               ORDER BY s.project_name, td.duration_ms"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        let mut stmt = self.conn.prepare(&sql)?;

        let mut by_project: HashMap<String, Vec<i64>> = HashMap::new();

        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        for row in rows {
            let (project, dur) = row?;
            by_project.entry(project).or_default().push(dur);
        }

        let mut result: Vec<TurnStatsRow> = by_project
            .into_iter()
            .map(|(project, durations)| {
                let n = durations.len() as i64;
                let avg = durations.iter().sum::<i64>() as f64 / n as f64;
                let p50 = percentile_sorted(&durations, 50);
                let p95 = percentile_sorted(&durations, 95);
                let max = *durations.last().unwrap_or(&0);
                TurnStatsRow {
                    project,
                    turn_count: n,
                    avg_duration_ms: avg,
                    p50_duration_ms: p50,
                    p95_duration_ms: p95,
                    max_duration_ms: max,
                }
            })
            .collect();

        result.sort_by(|a, b| {
            b.avg_duration_ms
                .partial_cmp(&a.avg_duration_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        result.truncate(limit);
        Ok(result)
    }

    pub fn query_pr_links(
        &self,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<PrLinkRow>> {
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        // One row per unique PR URL. A single PR is often referenced from many
        // sessions, which would otherwise produce a wall of duplicates. We
        // surface the most recent mention (MAX(timestamp)) and the session
        // that produced it — SQLite's bare-columns rule pairs the bare
        // columns with the MAX() row.
        let sql = format!(
            r#"SELECT s.provider, s.project_name, s.session_id,
                      p.pr_number, p.pr_url, p.pr_repository,
                      MAX(p.timestamp) AS latest_ts
               FROM pr_links p
               JOIN sessions s ON s.id = p.session_rowid
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}
               GROUP BY p.pr_url
               ORDER BY latest_ts DESC
               LIMIT ?"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        binds.push(SqlValue::Integer(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok(PrLinkRow {
                provider: row.get(0)?,
                project: row.get(1)?,
                session_id: row.get(2)?,
                pr_number: row.get(3)?,
                pr_url: row.get(4)?,
                pr_repository: row.get(5)?,
                timestamp: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn query_file_mods(
        &self,
        project_filter: Option<&str>,
        path_filter: Option<&str>,
        filter: &ResolvedFilter,
        limit: usize,
    ) -> Result<Vec<FileModRow>> {
        let fp = opt_like(project_filter);
        let path_pat = opt_like(path_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"WITH filtered AS (
                   SELECT fm.file_path, fm.session_rowid, s.project_name, s.last_timestamp
                   FROM file_modifications fm
                   JOIN sessions s ON s.id = fm.session_rowid
                   WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?)
                     AND (? IS NULL OR fm.file_path LIKE ?){pred}
               ),
               ranked_projects AS (
                   SELECT file_path, project_name, COUNT(*) AS project_events,
                          ROW_NUMBER() OVER (
                              PARTITION BY file_path
                              ORDER BY COUNT(*) DESC, project_name ASC
                          ) AS rn
                   FROM filtered
                   GROUP BY file_path, project_name
               )
               SELECT f.file_path,
                      COUNT(*) AS cnt,
                      COUNT(DISTINCT f.session_rowid) AS distinct_sessions,
                      MAX(f.last_timestamp) AS last_touched,
                      rp.project_name
               FROM filtered f
               LEFT JOIN ranked_projects rp
                 ON rp.file_path = f.file_path AND rp.rn = 1
               GROUP BY f.file_path
               ORDER BY cnt DESC, f.file_path ASC
               LIMIT ?"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp, path_pat.clone(), path_pat];
        binds.extend(pred_params);
        binds.push(SqlValue::Integer(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok(FileModRow {
                file_path: row.get(0)?,
                modification_count: row.get(1)?,
                distinct_session_count: row.get(2)?,
                last_touched_timestamp_ms: row.get(3)?,
                top_project: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn query_model_usage(
        &self,
        project_filter: Option<&str>,
        filter: &ResolvedFilter,
    ) -> Result<Vec<ModelUsageRow>> {
        let fp = opt_like(project_filter);
        let (pred, pred_params) = filter.sql_predicates("s");
        let sql = format!(
            r#"SELECT t.model,
                      s.provider || char(31) || s.project_name || char(31) || COALESCE(s.parent_session_id, s.session_id, s.file_path),
                      COALESCE(t.input_tokens, 0),
                      COALESCE(t.output_tokens, 0),
                      COALESCE(t.cache_creation_tokens, 0),
                      COALESCE(t.cache_read_tokens, 0),
                      COALESCE(t.cost_usd, 0),
                      t.service_tier,
                      t.inference_geo,
                      t.speed,
                      COALESCE(t.iterations, 0)
               FROM token_usage t
               JOIN sessions s ON s.id = t.session_id
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?){pred}"#
        );
        let mut binds: Vec<SqlValue> = vec![fp.clone(), fp.clone(), fp];
        binds.extend(pred_params);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |row| {
            Ok((
                row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<f64>>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?;

        let mut aggregated: BTreeMap<String, ModelUsageAccumulator> = BTreeMap::new();
        for row in rows {
            let (
                model,
                session_id,
                input_tokens,
                output_tokens,
                cache_creation_tokens,
                cache_read_tokens,
                cost_usd,
                service_tiers,
                inference_geos,
                speed,
                iterations,
            ) = row?;
            let entry = aggregated.entry(model).or_default();
            entry.session_ids.insert(session_id);
            entry.input_tokens += input_tokens;
            entry.output_tokens += output_tokens;
            entry.cache_creation_tokens += cache_creation_tokens;
            entry.cache_read_tokens += cache_read_tokens;
            entry.cost_usd += cost_usd;
            entry.total_iterations += iterations;
            for tier in split_joined_values(service_tiers.as_deref()) {
                entry.service_tiers.insert(tier);
            }
            for geo in split_joined_values(inference_geos.as_deref()) {
                entry.inference_geos.insert(geo);
            }
            if let Some(speed) = speed {
                entry.speed_sum += speed;
                entry.speed_samples += 1;
            }
        }

        let mut result = aggregated
            .into_iter()
            .map(|(model, acc)| {
                let session_count = acc.session_ids.len() as i64;
                let total_tokens = acc.input_tokens
                    + acc.output_tokens
                    + acc.cache_creation_tokens
                    + acc.cache_read_tokens;
                ModelUsageRow {
                    model,
                    session_count,
                    input_tokens: acc.input_tokens,
                    output_tokens: acc.output_tokens,
                    cache_creation_tokens: acc.cache_creation_tokens,
                    cache_read_tokens: acc.cache_read_tokens,
                    cost_usd: acc.cost_usd,
                    avg_cost_per_session_usd: if session_count == 0 {
                        0.0
                    } else {
                        acc.cost_usd / session_count as f64
                    },
                    avg_tokens_per_session: if session_count == 0 {
                        0.0
                    } else {
                        total_tokens as f64 / session_count as f64
                    },
                    service_tiers: acc.service_tiers.into_iter().collect(),
                    inference_geos: acc.inference_geos.into_iter().collect(),
                    avg_speed: if acc.speed_samples == 0 {
                        None
                    } else {
                        Some(acc.speed_sum / acc.speed_samples as f64)
                    },
                    total_iterations: acc.total_iterations,
                }
            })
            .collect::<Vec<_>>();

        result.sort_by(|a, b| {
            b.cost_usd
                .total_cmp(&a.cost_usd)
                .then_with(|| a.model.cmp(&b.model))
        });
        Ok(result)
    }

    pub fn query_session_detail(&self, file_path: &str) -> Result<Option<SessionDetail>> {
        let session_row: Option<SessionRow> = self
            .conn
            .query_row(
                r#"SELECT id, project_name, file_path, session_id, parent_session_id,
                          first_timestamp, last_timestamp, duration_ms, message_count, model
                   FROM sessions
                   WHERE file_path = ?"#,
                params![file_path],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                    ))
                },
            )
            .ok();

        let Some((
            session_rowid,
            project,
            file_path,
            session_id,
            parent_session_id,
            _first_timestamp_ms,
            _last_timestamp_ms,
            _duration_ms,
            _message_count,
            model,
        )) = session_row
        else {
            return Ok(None);
        };

        let mut row_ids = vec![session_rowid];
        let mut subagent_files = Vec::new();
        if parent_session_id.is_none()
            && let Some(parent) = session_id.as_deref()
        {
            let mut stmt = self.conn.prepare(
                "SELECT id, file_path FROM sessions WHERE parent_session_id = ? ORDER BY first_timestamp, file_path",
            )?;
            let child_rows = stmt
                .query_map(params![parent], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, path) in child_rows {
                row_ids.push(id);
                subagent_files.push(path);
            }
        }
        let token_filter = id_filter(&row_ids, "session_id");
        let row_filter = id_filter(&row_ids, "session_rowid");

        let (first_timestamp_ms, last_timestamp_ms, duration_ms, message_count): (
            Option<i64>,
            Option<i64>,
            i64,
            i64,
        ) = self.conn.query_row(
            &format!(
                r#"SELECT MIN(first_timestamp), MAX(last_timestamp),
                          COALESCE(SUM(duration_ms), 0), COALESCE(SUM(message_count), 0)
                   FROM sessions
                   WHERE {}"#,
                id_filter(&row_ids, "id")
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;

        let (input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens, cost_usd): (
            i64,
            i64,
            i64,
            i64,
            f64,
        ) = self.conn.query_row(
            &format!(
                r#"SELECT COALESCE(SUM(input_tokens), 0),
                          COALESCE(SUM(output_tokens), 0),
                          COALESCE(SUM(cache_creation_tokens), 0),
                          COALESCE(SUM(cache_read_tokens), 0),
                          COALESCE(SUM(cost_usd), 0)
                   FROM token_usage
                   WHERE {token_filter}"#,
            ),
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;

        let thinking_block_count: i64 = self
            .conn
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(thinking_blocks), 0) FROM thinking_usage WHERE {row_filter}"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let tools = {
            let mut stmt = self.conn.prepare(&format!(
                r#"SELECT tool_name, SUM(count) AS total
                   FROM tool_calls
                   WHERE {token_filter}
                   GROUP BY tool_name
                   ORDER BY total DESC, tool_name ASC"#
            ))?;
            stmt.query_map([], |row| {
                Ok(ToolRow {
                    tool_name: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };

        let pr_links = {
            let mut stmt = self.conn.prepare(&format!(
                r#"SELECT ?, ?, pr_number, pr_url, pr_repository, COALESCE(timestamp, '')
                   FROM pr_links
                   WHERE {row_filter}
                   ORDER BY timestamp DESC, pr_url ASC"#
            ))?;
            stmt.query_map(params![project.clone(), session_id.clone()], |row| {
                Ok(PrLinkRow {
                    // Session-detail PRs are not rendered with a provider column.
                    provider: String::new(),
                    project: row.get(0)?,
                    session_id: row.get(1)?,
                    pr_number: row.get(2)?,
                    pr_url: row.get(3)?,
                    pr_repository: row.get(4)?,
                    timestamp: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };

        let files_modified = {
            let mut stmt = self.conn.prepare(&format!(
                r#"SELECT DISTINCT file_path
                   FROM file_modifications
                   WHERE {row_filter}
                   ORDER BY file_path ASC"#
            ))?;
            stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };

        let turn_stats = {
            let mut stmt = self.conn.prepare(&format!(
                r#"SELECT duration_ms
                   FROM turn_durations
                   WHERE {row_filter}
                   ORDER BY duration_ms ASC"#
            ))?;
            let durations = stmt
                .query_map([], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if durations.is_empty() {
                None
            } else {
                let turn_count = durations.len() as i64;
                let avg_duration_ms = durations.iter().sum::<i64>() as f64 / turn_count as f64;
                Some(TurnStatsRow {
                    project: project.clone(),
                    turn_count,
                    avg_duration_ms,
                    p50_duration_ms: percentile_sorted(&durations, 50),
                    p95_duration_ms: percentile_sorted(&durations, 95),
                    max_duration_ms: *durations.last().unwrap_or(&0),
                })
            }
        };

        let stop_reasons = {
            let mut stmt = self.conn.prepare(&format!(
                r#"SELECT stop_reason, SUM(count) AS total
                   FROM stop_reasons
                   WHERE {row_filter}
                   GROUP BY stop_reason
                   ORDER BY total DESC, stop_reason ASC"#
            ))?;
            stmt.query_map([], |row| {
                Ok(StopReasonRow {
                    stop_reason: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };

        let attachments = {
            let mut stmt = self.conn.prepare(&format!(
                r#"SELECT filename, mime_type
                   FROM attachments
                   WHERE {row_filter}
                   ORDER BY filename ASC, mime_type ASC"#
            ))?;
            stmt.query_map([], |row| {
                Ok(AttachmentRow {
                    filename: row.get(0)?,
                    mime_type: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };

        let permission_changes = {
            let mut stmt = self.conn.prepare(&format!(
                r#"SELECT mode, COALESCE(timestamp, '')
                   FROM permission_changes
                   WHERE {row_filter}
                   ORDER BY timestamp ASC, mode ASC"#
            ))?;
            stmt.query_map([], |row| {
                Ok(PermissionChangeRow {
                    mode: row.get(0)?,
                    timestamp: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };

        let model_usage = {
            let mut stmt = self.conn.prepare(&format!(
                r#"SELECT model,
                          SUM(assistant_message_count),
                          SUM(input_tokens),
                          SUM(output_tokens),
                          SUM(cache_creation_tokens),
                          SUM(cache_read_tokens),
                          SUM(cost_usd),
                          GROUP_CONCAT(DISTINCT inference_geo),
                          GROUP_CONCAT(DISTINCT service_tier),
                          AVG(speed),
                          SUM(iterations)
                   FROM token_usage
                   WHERE {token_filter}
                   GROUP BY model
                   ORDER BY SUM(cost_usd) DESC, model ASC"#
            ))?;
            stmt.query_map([], |row| {
                Ok(SessionModelUsageRow {
                    model: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    assistant_message_count: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    cache_creation_tokens: row.get(4)?,
                    cache_read_tokens: row.get(5)?,
                    cost_usd: row.get(6)?,
                    inference_geos: split_joined_values(
                        row.get::<_, Option<String>>(7)?.as_deref(),
                    ),
                    service_tiers: split_joined_values(row.get::<_, Option<String>>(8)?.as_deref()),
                    avg_speed: row.get(9)?,
                    iterations: row.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };

        Ok(Some(SessionDetail {
            project,
            file_path,
            session_id,
            first_timestamp_ms,
            last_timestamp_ms,
            duration_ms,
            message_count,
            model,
            input_tokens,
            output_tokens,
            cache_creation_tokens,
            cache_read_tokens,
            cost_usd,
            thinking_block_count,
            files_modified,
            tools,
            pr_links,
            turn_stats,
            stop_reasons,
            attachments,
            permission_changes,
            model_usage,
            subagent_files,
        }))
    }

    pub fn query_summary(&self) -> Result<SummaryData> {
        let today = Utc::now().date_naive();
        let days_since_monday = today.weekday().num_days_from_monday() as i64;
        let week_start = today - Duration::days(days_since_monday);

        let midnight = NaiveTime::from_hms_opt(0, 0, 0).expect("valid time");
        let today_start_ms = NaiveDateTime::new(today, midnight)
            .and_utc()
            .timestamp_millis();
        let week_start_ms = NaiveDateTime::new(week_start, midnight)
            .and_utc()
            .timestamp_millis();

        let (total_sessions, total_cost, total_in, total_out, total_cc, total_cr): (
            i64,
            f64,
            i64,
            i64,
            i64,
            i64,
        ) = self.conn.query_row(
            r#"SELECT COUNT(DISTINCT s.provider || char(31) || s.project_name || char(31) || COALESCE(s.parent_session_id, s.session_id, s.file_path)),
                      COALESCE(SUM(t.cost_usd), 0),
                      COALESCE(SUM(t.input_tokens), 0),
                      COALESCE(SUM(t.output_tokens), 0),
                      COALESCE(SUM(t.cache_creation_tokens), 0),
                      COALESCE(SUM(t.cache_read_tokens), 0)
               FROM sessions s
               LEFT JOIN token_usage t ON t.session_id = s.id"#,
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;

        let sessions_today: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT provider || char(31) || project_name || char(31) || COALESCE(parent_session_id, session_id, file_path)) FROM sessions WHERE first_timestamp >= ?",
            params![today_start_ms],
            |row| row.get(0),
        )?;

        let sessions_this_week: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT provider || char(31) || project_name || char(31) || COALESCE(parent_session_id, session_id, file_path)) FROM sessions WHERE first_timestamp >= ?",
            params![week_start_ms],
            |row| row.get(0),
        )?;

        let week_cost: f64 = self.conn.query_row(
            r#"SELECT COALESCE(SUM(t.cost_usd), 0)
               FROM sessions s JOIN token_usage t ON t.session_id = s.id
               WHERE s.first_timestamp >= ?"#,
            params![week_start_ms],
            |row| row.get(0),
        )?;

        let mut top_stmt = self.conn.prepare(
            r#"SELECT project_name, COUNT(DISTINCT provider || char(31) || COALESCE(parent_session_id, session_id, file_path)) AS cnt
               FROM sessions
               GROUP BY project_name
               ORDER BY cnt DESC
               LIMIT 5"#,
        )?;
        let top_projects: Vec<(String, i64)> = top_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut tools_stmt = self.conn.prepare(
            r#"SELECT tool_name, SUM(count) AS total
               FROM tool_calls
               GROUP BY tool_name
               ORDER BY total DESC
               LIMIT 5"#,
        )?;
        let top_tools: Vec<(String, i64)> = tools_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut stop_stmt = self.conn.prepare(
            r#"SELECT stop_reason, SUM(count) AS total
               FROM stop_reasons
               GROUP BY stop_reason
               ORDER BY total DESC
               LIMIT 5"#,
        )?;
        let top_stop_reasons: Vec<(String, i64)> = stop_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let most_recent: Option<MostRecentSession> = self
            .conn
            .query_row(
                r#"SELECT project_name, session_id, first_timestamp, model, message_count
                   FROM sessions
                   WHERE first_timestamp IS NOT NULL
                   ORDER BY first_timestamp DESC
                   LIMIT 1"#,
                [],
                |row| {
                    Ok(MostRecentSession {
                        project: row.get(0)?,
                        session_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        first_timestamp_ms: row.get(2)?,
                        model: row.get(3)?,
                        message_count: row.get(4)?,
                    })
                },
            )
            .ok();

        // Extended metrics
        let thinking_block_count: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(thinking_blocks), 0) FROM thinking_usage",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let avg_turn_duration_ms: Option<f64> = self
            .conn
            .query_row(
                "SELECT AVG(CAST(duration_ms AS REAL)) FROM turn_durations",
                [],
                |row| row.get(0),
            )
            .ok();

        let pr_count: i64 = self
            .conn
            .query_row("SELECT COUNT(DISTINCT pr_url) FROM pr_links", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        let files_modified_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(DISTINCT file_path) FROM file_modifications",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let mut mdist_stmt = self.conn.prepare(
            r#"SELECT s.provider || char(31) || s.project_name || char(31) || COALESCE(s.parent_session_id, s.session_id, s.file_path), t.model, COALESCE(t.cost_usd, 0)
               FROM token_usage t
               JOIN sessions s ON s.id = t.session_id"#,
        )?;
        let raw_model_rows = mdist_stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut family_map: HashMap<String, (std::collections::HashSet<String>, f64)> =
            HashMap::new();
        for (session_id, model, cost) in raw_model_rows {
            let family = model
                .as_deref()
                .map(|m| ModelPricing::name(Some(m)).to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            let entry = family_map
                .entry(family)
                .or_insert_with(|| (std::collections::HashSet::new(), 0.0));
            entry.0.insert(session_id);
            entry.1 += cost;
        }
        let mut model_distribution: Vec<(String, i64, f64)> = family_map
            .into_iter()
            .map(|(family, (sessions, cost))| (family, sessions.len() as i64, cost))
            .collect();
        model_distribution
            .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        model_distribution.truncate(5);

        Ok(SummaryData {
            total_sessions,
            sessions_today,
            sessions_this_week,
            total_cost,
            week_cost,
            total_input_tokens: total_in,
            total_output_tokens: total_out,
            total_cache_creation: total_cc,
            total_cache_read: total_cr,
            top_projects,
            top_tools,
            top_stop_reasons,
            most_recent,
            thinking_block_count,
            avg_turn_duration_ms,
            pr_count,
            files_modified_count,
            model_distribution,
        })
    }
}

fn id_filter(row_ids: &[i64], column: &str) -> String {
    let ids = row_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("{column} IN ({ids})")
}

fn fts_escape(query: &str) -> String {
    let escaped = query.replace('"', "\"\"");
    if query.split_whitespace().count() > 1 {
        format!("\"{}\"", escaped)
    } else {
        escaped
    }
}

const MULTI_VALUE_SEPARATOR: &str = "\u{1f}";

fn split_joined_values(raw: Option<&str>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    raw.unwrap_or("")
        .split(['\u{1f}', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert((*s).to_string()))
        .map(str::to_string)
        .collect()
}

fn join_strings(values: &std::collections::BTreeSet<String>) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(
            values
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(MULTI_VALUE_SEPARATOR),
        )
    }
}

fn session_model_label(
    primary_model: Option<&str>,
    model_usage: &BTreeMap<String, ModelSessionStats>,
) -> Option<String> {
    let models = model_usage
        .keys()
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    match models.len() {
        0 => primary_model.map(ToOwned::to_owned),
        1 => Some(models[0].to_string()),
        _ => Some("mixed".to_string()),
    }
}

fn model_families_from_concat(raw: Option<&str>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.unwrap_or("")
        .split(',')
        .map(|m| ModelPricing::name(Some(m.trim())).to_string())
        .filter(|f| !f.is_empty() && seen.insert(f.clone()))
        .collect()
}

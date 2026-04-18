use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDateTime, NaiveTime, Utc};
use rusqlite::{Connection, params};

use crate::parser::stream_records;
use crate::store::{SessionStore, decode_project_name, display_project_name};
use crate::types::{ModelPricing, TokenUsage};

const STALE_SECS: u64 = 300;

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

pub struct IndexedSession {
    pub project_name: String,
    pub session_id: Option<String>,
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
    pub project: String,
    pub session_id: Option<String>,
    pub first_timestamp_ms: Option<i64>,
    pub model: Option<String>,
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
    pub project_name: String,
    pub session_id: Option<String>,
    pub first_timestamp_ms: Option<i64>,
    pub message_type: String,
    pub content: String,
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
    pub most_recent: Option<MostRecentSession>,
}

pub struct MostRecentSession {
    pub project: String,
    pub session_id: String,
    pub first_timestamp_ms: i64,
    pub model: Option<String>,
    pub message_count: i64,
}

// --- Internal parse types ---

struct MessageForFts {
    msg_type: String,
    content: String,
    timestamp_ms: Option<i64>,
}

struct ParseEntry {
    session_id: Option<String>,
    first_timestamp: Option<DateTime<Utc>>,
    last_timestamp: Option<DateTime<Utc>>,
    duration_ms: u64,
    message_count: usize,
    model: Option<String>,
    usage: TokenUsage,
    tool_names: Vec<String>,
    messages: Vec<MessageForFts>,
}

impl IndexStore {
    pub fn open() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        let dir = home.join(".claudex");
        std::fs::create_dir_all(&dir).context("creating ~/.claudex")?;
        let conn = Connection::open(dir.join("index.db"))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.create_schema()?;
        Ok(store)
    }

    fn create_schema(&self) -> Result<()> {
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
                first_timestamp INTEGER,
                last_timestamp  INTEGER,
                duration_ms     INTEGER NOT NULL DEFAULT 0,
                message_count   INTEGER NOT NULL DEFAULT 0,
                model           TEXT,
                indexed_at      INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_project   ON sessions(project_name);
            CREATE INDEX IF NOT EXISTS idx_sessions_timestamp ON sessions(first_timestamp DESC);
            CREATE TABLE IF NOT EXISTS token_usage (
                session_id            INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                model                 TEXT,
                input_tokens          INTEGER NOT NULL DEFAULT 0,
                output_tokens         INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens     INTEGER NOT NULL DEFAULT 0,
                cost_usd              REAL    NOT NULL DEFAULT 0.0
            );
            CREATE INDEX IF NOT EXISTS idx_token_usage_session ON token_usage(session_id);
            CREATE TABLE IF NOT EXISTS tool_calls (
                session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                tool_name  TEXT    NOT NULL,
                count      INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_tool_calls_session ON tool_calls(session_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                session_id   UNINDEXED,
                message_type,
                content,
                timestamp    UNINDEXED,
                tokenize     = 'porter unicode61'
            );
            "#,
        )?;
        Ok(())
    }

    /// Check staleness and sync if needed. Prints "Building index..." on first run.
    pub fn ensure_fresh(&mut self, store: &SessionStore) -> Result<()> {
        let last_sync: Option<u64> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'last_sync'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| s.parse().ok());

        if let Some(ls) = last_sync {
            if now_unix_secs().saturating_sub(ls) < STALE_SECS {
                return Ok(());
            }
        } else {
            eprintln!("Building index...");
        }

        self.sync(store)?;
        Ok(())
    }

    /// Force a full rebuild regardless of staleness.
    pub fn force_rebuild(&mut self, store: &SessionStore) -> Result<usize> {
        self.conn
            .execute_batch("DELETE FROM messages_fts; DELETE FROM sessions; DELETE FROM meta;")?;
        self.sync(store)
    }

    /// Run an incremental sync now (bypass staleness check).
    pub fn sync_now(&mut self, store: &SessionStore) -> Result<usize> {
        self.sync(store)
    }

    fn sync(&mut self, store: &SessionStore) -> Result<usize> {
        // Load known file states
        let mut known: HashMap<String, (i64, i64)> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT file_path, file_size, file_mtime FROM sessions")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (p, sz, mt) = row?;
                known.insert(p, (sz, mt));
            }
        }

        let all_files = store.all_session_files(None)?;
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let now_secs = now_unix_secs() as i64;
        let mut indexed_count = 0usize;

        let tx = self.conn.transaction()?;

        for (project_raw, file_path) in &all_files {
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

            if let Some(&(ksz, kmt)) = known.get(&path_str) {
                if ksz == size && kmt == mtime {
                    continue; // unchanged
                }
                // Changed: remove old FTS rows first (no CASCADE on virtual table)
                if let Ok(old_id) = tx.query_row(
                    "SELECT id FROM sessions WHERE file_path = ?",
                    params![path_str],
                    |row| row.get::<_, i64>(0),
                ) {
                    tx.execute(
                        "DELETE FROM messages_fts WHERE session_id = ?",
                        params![old_id],
                    )?;
                }
                tx.execute(
                    "DELETE FROM sessions WHERE file_path = ?",
                    params![path_str],
                )?;
            }

            let project_display = display_project_name(&decode_project_name(project_raw));
            let mut entry = match parse_session_for_index(file_path) {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Fall back to file stem when session JSON lacks a sessionId field
            if entry.session_id.is_none() {
                entry.session_id = file_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned());
            }

            let first_ts = entry.first_timestamp.map(|d| d.timestamp_millis());
            let last_ts = entry.last_timestamp.map(|d| d.timestamp_millis());
            let cost = entry.usage.cost_for_model(entry.model.as_deref());

            tx.execute(
                r#"INSERT INTO sessions
                   (project_name, file_path, file_size, file_mtime, session_id,
                    first_timestamp, last_timestamp, duration_ms, message_count, model, indexed_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                params![
                    project_display,
                    path_str,
                    size,
                    mtime,
                    entry.session_id,
                    first_ts,
                    last_ts,
                    entry.duration_ms as i64,
                    entry.message_count as i64,
                    entry.model,
                    now_secs,
                ],
            )?;

            let row_id = tx.last_insert_rowid();

            tx.execute(
                r#"INSERT INTO token_usage
                   (session_id, model, input_tokens, output_tokens,
                    cache_creation_tokens, cache_read_tokens, cost_usd)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"#,
                params![
                    row_id,
                    entry.model,
                    entry.usage.input_tokens as i64,
                    entry.usage.output_tokens as i64,
                    entry.usage.cache_creation_tokens as i64,
                    entry.usage.cache_read_tokens as i64,
                    cost,
                ],
            )?;

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

            for msg in &entry.messages {
                tx.execute(
                    "INSERT INTO messages_fts (session_id, message_type, content, timestamp) VALUES (?, ?, ?, ?)",
                    params![row_id, msg.msg_type, msg.content, msg.timestamp_ms],
                )?;
            }

            indexed_count += 1;
        }

        // Remove stale entries for deleted files
        for path in known.keys() {
            if !seen.contains(path) {
                if let Ok(id) = tx.query_row(
                    "SELECT id FROM sessions WHERE file_path = ?",
                    params![path],
                    |row| row.get::<_, i64>(0),
                ) {
                    tx.execute("DELETE FROM messages_fts WHERE session_id = ?", params![id])?;
                }
                tx.execute("DELETE FROM sessions WHERE file_path = ?", params![path])?;
            }
        }

        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_sync', ?)",
            params![now_unix_secs().to_string()],
        )?;
        tx.commit()?;

        Ok(indexed_count)
    }

    // --- Query methods ---

    pub fn query_sessions(
        &self,
        project_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<IndexedSession>> {
        let filter = project_filter.map(|f| format!("%{f}%"));
        let fp = filter.as_deref();
        let mut stmt = self.conn.prepare(
            r#"SELECT project_name, session_id, first_timestamp, message_count, duration_ms, model
               FROM sessions
               WHERE (? IS NULL OR project_name LIKE ? OR file_path LIKE ?)
               ORDER BY first_timestamp DESC
               LIMIT ?"#,
        )?;
        let rows = stmt.query_map(params![fp, fp, fp, limit as i64], |row| {
            Ok(IndexedSession {
                project_name: row.get(0)?,
                session_id: row.get(1)?,
                first_timestamp_ms: row.get(2)?,
                message_count: row.get(3)?,
                duration_ms: row.get(4)?,
                model: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn query_cost_by_project(
        &self,
        project_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ProjectCostRow>> {
        let filter = project_filter.map(|f| format!("%{f}%"));
        let fp = filter.as_deref();
        let mut stmt = self.conn.prepare(
            r#"SELECT s.project_name,
                      COUNT(DISTINCT s.id),
                      COALESCE(SUM(t.input_tokens), 0),
                      COALESCE(SUM(t.output_tokens), 0),
                      COALESCE(SUM(t.cache_creation_tokens), 0),
                      COALESCE(SUM(t.cache_read_tokens), 0),
                      COALESCE(SUM(t.cost_usd), 0),
                      GROUP_CONCAT(DISTINCT t.model)
               FROM sessions s
               LEFT JOIN token_usage t ON t.session_id = s.id
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?)
               GROUP BY s.project_name
               ORDER BY COALESCE(SUM(t.cost_usd), 0) DESC
               LIMIT ?"#,
        )?;
        let rows = stmt.query_map(params![fp, fp, fp, limit as i64], |row| {
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
        limit: usize,
    ) -> Result<Vec<SessionCostRow>> {
        let filter = project_filter.map(|f| format!("%{f}%"));
        let fp = filter.as_deref();
        let mut stmt = self.conn.prepare(
            r#"SELECT s.project_name, s.session_id, s.first_timestamp,
                      t.model, t.input_tokens, t.output_tokens,
                      t.cache_creation_tokens, t.cache_read_tokens, t.cost_usd
               FROM sessions s
               JOIN token_usage t ON t.session_id = s.id
               WHERE (t.input_tokens + t.output_tokens + t.cache_creation_tokens + t.cache_read_tokens) > 0
                 AND (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?)
               ORDER BY t.cost_usd DESC
               LIMIT ?"#,
        )?;
        let rows = stmt.query_map(params![fp, fp, fp, limit as i64], |row| {
            Ok(SessionCostRow {
                project: row.get(0)?,
                session_id: row.get(1)?,
                first_timestamp_ms: row.get(2)?,
                model: row.get(3)?,
                input_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                cache_creation_tokens: row.get(6)?,
                cache_read_tokens: row.get(7)?,
                cost_usd: row.get(8)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn query_tools_aggregate(
        &self,
        project_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ToolRow>> {
        let filter = project_filter.map(|f| format!("%{f}%"));
        let fp = filter.as_deref();
        let mut stmt = self.conn.prepare(
            r#"SELECT tc.tool_name, SUM(tc.count) AS total
               FROM tool_calls tc
               JOIN sessions s ON s.id = tc.session_id
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?)
               GROUP BY tc.tool_name
               ORDER BY total DESC
               LIMIT ?"#,
        )?;
        let rows = stmt.query_map(params![fp, fp, fp, limit as i64], |row| {
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
        limit: usize,
    ) -> Result<Vec<SessionToolRow>> {
        let filter = project_filter.map(|f| format!("%{f}%"));
        let fp = filter.as_deref();
        let mut stmt = self.conn.prepare(
            r#"SELECT s.id, s.project_name, s.session_id, s.first_timestamp,
                      tc.tool_name, tc.count
               FROM sessions s
               JOIN tool_calls tc ON tc.session_id = s.id
               WHERE (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?)
               ORDER BY s.first_timestamp DESC"#,
        )?;

        let mut order: Vec<i64> = Vec::new();
        let mut map: HashMap<i64, SessionToolRow> = HashMap::new();

        let rows = stmt.query_map(params![fp, fp, fp], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        for row in rows {
            let (db_id, project, session_id, first_ts, tool_name, count) = row?;
            let slot = map.entry(db_id).or_insert_with(|| {
                order.push(db_id);
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
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        let fts_query = fts_escape(query);
        let filter = project_filter.map(|f| format!("%{f}%"));
        let fp = filter.as_deref();
        let mut stmt = self.conn.prepare(
            r#"SELECT s.project_name, s.session_id, s.first_timestamp,
                      f.message_type, f.content
               FROM messages_fts f
               JOIN sessions s ON s.id = f.session_id
               WHERE messages_fts MATCH ?
                 AND (? IS NULL OR s.project_name LIKE ? OR s.file_path LIKE ?)
               ORDER BY rank
               LIMIT ?"#,
        )?;
        let rows = stmt.query_map(params![fts_query, fp, fp, fp, limit as i64], |row| {
            Ok(SearchHit {
                project_name: row.get(0)?,
                session_id: row.get(1)?,
                first_timestamp_ms: row.get(2)?,
                message_type: row.get(3)?,
                content: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
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
            r#"SELECT COUNT(DISTINCT s.id),
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
            "SELECT COUNT(*) FROM sessions WHERE first_timestamp >= ?",
            params![today_start_ms],
            |row| row.get(0),
        )?;

        let sessions_this_week: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE first_timestamp >= ?",
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
            r#"SELECT project_name, COUNT(*) AS cnt
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
            most_recent,
        })
    }
}

fn fts_escape(query: &str) -> String {
    // Phrase-quote the entire query so user input doesn't inject FTS syntax
    format!("\"{}\"", query.replace('"', "\"\""))
}

fn model_families_from_concat(raw: Option<&str>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.unwrap_or("")
        .split(',')
        .map(|m| ModelPricing::name(Some(m.trim())).to_string())
        .filter(|f| !f.is_empty() && seen.insert(f.clone()))
        .collect()
}

/// Parse a session file once, extracting both stats and FTS content.
fn parse_session_for_index(path: &Path) -> Result<ParseEntry> {
    let mut entry = ParseEntry {
        session_id: None,
        first_timestamp: None,
        last_timestamp: None,
        duration_ms: 0,
        message_count: 0,
        model: None,
        usage: TokenUsage::default(),
        tool_names: Vec::new(),
        messages: Vec::new(),
    };

    stream_records(path, |record| {
        if entry.session_id.is_none() {
            if let Some(sid) = record["sessionId"].as_str() {
                entry.session_id = Some(sid.to_string());
            }
        }

        let timestamp_ms = record["timestamp"].as_str().and_then(|ts| {
            DateTime::parse_from_rfc3339(ts)
                .ok()
                .map(|dt| dt.timestamp_millis())
        });

        if let Some(ts_str) = record["timestamp"].as_str() {
            if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str) {
                let dt = dt.with_timezone(&Utc);
                if entry.first_timestamp.is_none_or(|prev| dt < prev) {
                    entry.first_timestamp = Some(dt);
                }
                if entry.last_timestamp.is_none_or(|prev| dt > prev) {
                    entry.last_timestamp = Some(dt);
                }
            }
        }

        match record["type"].as_str().unwrap_or("") {
            "assistant" => {
                entry.message_count += 1;
                let msg = &record["message"];

                if entry.model.is_none() {
                    if let Some(m) = msg["model"].as_str() {
                        entry.model = Some(m.to_string());
                    }
                }

                let usage = &msg["usage"];
                entry.usage.input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
                entry.usage.output_tokens += usage["output_tokens"].as_u64().unwrap_or(0);
                entry.usage.cache_creation_tokens +=
                    usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                entry.usage.cache_read_tokens +=
                    usage["cache_read_input_tokens"].as_u64().unwrap_or(0);

                let mut text_parts: Vec<String> = Vec::new();
                if let Some(content) = msg["content"].as_array() {
                    for block in content {
                        match block["type"].as_str() {
                            Some("tool_use") => {
                                if let Some(name) = block["name"].as_str() {
                                    entry.tool_names.push(name.to_string());
                                }
                            }
                            Some("text") => {
                                if let Some(t) = block["text"].as_str() {
                                    if !t.is_empty() {
                                        text_parts.push(t.to_string());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if !text_parts.is_empty() {
                    entry.messages.push(MessageForFts {
                        msg_type: "assistant".to_string(),
                        content: text_parts.join(" "),
                        timestamp_ms,
                    });
                }
            }
            "user" => {
                entry.message_count += 1;
                let content_val = &record["message"]["content"];
                let content = if let Some(s) = content_val.as_str() {
                    s.to_string()
                } else if let Some(arr) = content_val.as_array() {
                    arr.iter()
                        .filter(|b| b["type"].as_str() == Some("text"))
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    String::new()
                };
                if !content.is_empty() {
                    entry.messages.push(MessageForFts {
                        msg_type: "user".to_string(),
                        content,
                        timestamp_ms,
                    });
                }
            }
            "system" => {
                if let Some(dur) = record["durationMs"].as_u64() {
                    entry.duration_ms += dur;
                }
            }
            _ => {}
        }
        true
    })?;

    // Fallback duration from timestamp range
    if entry.duration_ms == 0 {
        if let (Some(first), Some(last)) = (entry.first_timestamp, entry.last_timestamp) {
            entry.duration_ms = last.signed_duration_since(first).num_milliseconds().max(0) as u64;
        }
    }

    Ok(entry)
}

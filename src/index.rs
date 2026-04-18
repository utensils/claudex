use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::parser;
use crate::store::{SessionStore, decode_project_name, display_project_name};

const SCHEMA_VERSION: u32 = 1;
const STALE_SECS: u64 = 300;

pub struct IndexStore {
    conn: Connection,
}

impl IndexStore {
    pub fn open() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        let dir = home.join(".claudex");
        fs::create_dir_all(&dir)?;
        let db_path = dir.join("index.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
        )?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);
        if version < SCHEMA_VERSION {
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS sessions (
                    id INTEGER PRIMARY KEY,
                    project_name TEXT NOT NULL,
                    file_path TEXT NOT NULL UNIQUE,
                    file_size INTEGER NOT NULL,
                    file_mtime INTEGER NOT NULL,
                    session_id TEXT,
                    first_timestamp TEXT,
                    last_timestamp TEXT,
                    duration_ms INTEGER DEFAULT 0,
                    message_count INTEGER DEFAULT 0,
                    model TEXT,
                    indexed_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS token_usage (
                    session_rowid INTEGER PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
                    model TEXT,
                    input_tokens INTEGER DEFAULT 0,
                    output_tokens INTEGER DEFAULT 0,
                    cache_creation_tokens INTEGER DEFAULT 0,
                    cache_read_tokens INTEGER DEFAULT 0,
                    cost_usd REAL DEFAULT 0.0
                );
                CREATE TABLE IF NOT EXISTS tool_counts (
                    id INTEGER PRIMARY KEY,
                    session_rowid INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                    tool_name TEXT NOT NULL,
                    count INTEGER DEFAULT 1
                );
                CREATE INDEX IF NOT EXISTS idx_tool_counts_session ON tool_counts(session_rowid);
                CREATE INDEX IF NOT EXISTS idx_tool_counts_name ON tool_counts(tool_name);
                CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project_name);
                CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                    session_rowid UNINDEXED,
                    project_name,
                    message_type,
                    content,
                    timestamp
                );
                CREATE TABLE IF NOT EXISTS sync_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT
                );",
            )?;
            self.conn
                .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        Ok(())
    }

    pub fn ensure_fresh(&mut self) -> Result<bool> {
        if !self.is_stale()? {
            return Ok(false);
        }
        self.sync()
    }

    fn is_stale(&self) -> Result<bool> {
        let last: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM sync_meta WHERE key='last_sync'",
                [],
                |r| r.get(0),
            )
            .ok();
        match last {
            None => Ok(true),
            Some(ts) => {
                let last_ts: u64 = ts.parse().unwrap_or(0);
                let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
                Ok(now - last_ts > STALE_SECS)
            }
        }
    }

    pub fn sync(&mut self) -> Result<bool> {
        let store = SessionStore::new()?;
        let all_files = store.all_session_files(None)?;

        let mut known: std::collections::HashMap<String, (i64, u64)> =
            std::collections::HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT file_path, file_size, file_mtime FROM sessions")?;
            let rows = stmt.query_map([], |r| {
                let p: String = r.get(0)?;
                let s: i64 = r.get(1)?;
                let m: u64 = r.get(2)?;
                Ok((p, s, m))
            })?;
            for row in rows {
                let (p, s, m) = row?;
                known.insert(p, (s, m));
            }
        }

        let mut to_index = Vec::new();
        let mut current_paths: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for (project, path) in &all_files {
            let path_str = path.to_string_lossy().to_string();
            current_paths.insert(path_str.clone());
            let meta = fs::metadata(path).ok();
            let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
            let mtime = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if let Some(&(known_size, known_mtime)) = known.get(&path_str) {
                if size == known_size && mtime == known_mtime {
                    continue;
                }
            }
            to_index.push((project.clone(), path.clone(), size, mtime));
        }

        let to_remove: Vec<String> = known
            .keys()
            .filter(|k| !current_paths.contains(*k))
            .cloned()
            .collect();

        let changed = !to_index.is_empty() || !to_remove.is_empty();
        if !to_index.is_empty() {
            let count = to_index.len();
            if count > 10 {
                eprint!("Building index ({count} files)...");
            }
            let tx = self.conn.transaction()?;
            for (i, (project, path, size, mtime)) in to_index.iter().enumerate() {
                let path_str = path.to_string_lossy().to_string();
                if let Ok(rowid) = tx.query_row(
                    "SELECT id FROM sessions WHERE file_path=?1",
                    [&path_str],
                    |r| r.get::<_, i64>(0),
                ) {
                    tx.execute("DELETE FROM token_usage WHERE session_rowid=?1", [rowid])?;
                    tx.execute("DELETE FROM tool_counts WHERE session_rowid=?1", [rowid])?;
                    tx.execute(
                        "DELETE FROM messages_fts WHERE session_rowid=?1",
                        [rowid],
                    )?;
                    tx.execute("DELETE FROM sessions WHERE id=?1", [rowid])?;
                }
                if let Err(e) = index_file(&tx, project, path, *size, *mtime) {
                    eprintln!("\nwarning: skipping {}: {e}", path.display());
                    continue;
                }
                if count > 10 && (i + 1) % 500 == 0 {
                    eprint!("\rBuilding index ({}/{count})...", i + 1);
                }
            }
            tx.commit()?;
            if count > 10 {
                eprintln!("\rIndexed {count} files.          ");
            }
        }

        if !to_remove.is_empty() {
            let tx = self.conn.transaction()?;
            for path_str in &to_remove {
                if let Ok(rowid) = tx.query_row(
                    "SELECT id FROM sessions WHERE file_path=?1",
                    [path_str],
                    |r| r.get::<_, i64>(0),
                ) {
                    tx.execute("DELETE FROM token_usage WHERE session_rowid=?1", [rowid])?;
                    tx.execute("DELETE FROM tool_counts WHERE session_rowid=?1", [rowid])?;
                    tx.execute(
                        "DELETE FROM messages_fts WHERE session_rowid=?1",
                        [rowid],
                    )?;
                    tx.execute("DELETE FROM sessions WHERE id=?1", [rowid])?;
                }
            }
            tx.commit()?;
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('last_sync', ?1)",
            [now.to_string()],
        )?;
        Ok(changed)
    }

}

fn index_file(
        tx: &rusqlite::Transaction,
        project: &str,
        path: &Path,
        size: i64,
        mtime: u64,
    ) -> Result<()> {
        let stats = parser::parse_session(path)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let decoded = decode_project_name(project);
        let display = display_project_name(&decoded);

        tx.execute(
            "INSERT INTO sessions (project_name, file_path, file_size, file_mtime, session_id, first_timestamp, last_timestamp, duration_ms, message_count, model, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                display,
                path.to_string_lossy().as_ref(),
                size,
                mtime as i64,
                stats.session_id,
                stats.first_timestamp.map(|t| t.to_rfc3339()),
                stats.last_timestamp.map(|t| t.to_rfc3339()),
                stats.total_duration_ms as i64,
                stats.message_count as i64,
                stats.model,
                now,
            ],
        )?;
        let rowid = tx.last_insert_rowid();

        let cost = stats.usage.cost_for_model(stats.model.as_deref());
        tx.execute(
            "INSERT INTO token_usage (session_rowid, model, input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens, cost_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rowid,
                stats.model,
                stats.usage.input_tokens as i64,
                stats.usage.output_tokens as i64,
                stats.usage.cache_creation_tokens as i64,
                stats.usage.cache_read_tokens as i64,
                cost,
            ],
        )?;

        let mut tool_map: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for name in &stats.tool_names {
            *tool_map.entry(name.clone()).or_default() += 1;
        }
        for (name, count) in &tool_map {
            tx.execute(
                "INSERT INTO tool_counts (session_rowid, tool_name, count) VALUES (?1, ?2, ?3)",
                params![rowid, name, *count as i64],
            )?;
        }

        parser::stream_records(path, |record| {
            let msg_type = record["type"].as_str().unwrap_or("");
            if msg_type != "user" && msg_type != "assistant" {
                return true;
            }
            let ts = record["timestamp"].as_str().unwrap_or("");
            let content = extract_text_content(&record["message"]);
            if !content.is_empty() {
                let _ = tx.execute(
                    "INSERT INTO messages_fts (session_rowid, project_name, message_type, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![rowid, display, msg_type, content, ts],
                );
            }
            true
        })?;

        Ok(())
}

impl IndexStore {
    pub fn query_sessions(
        &self,
        project_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SessionRow>> {
        let (wc, fp) = project_where(project_filter);
        let sql = format!(
            "SELECT project_name, session_id, first_timestamp, message_count, duration_ms, model FROM sessions {wc} ORDER BY first_timestamp DESC LIMIT ?"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if let Some(ref f) = fp {
            stmt.query_map(params![f, limit as i64], map_session_row)?
        } else {
            stmt.query_map(params![limit as i64], map_session_row)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn query_cost_by_project(
        &self,
        project_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CostRow>> {
        let (wc, fp) = project_where(project_filter);
        let sql = format!(
            "SELECT s.project_name, COUNT(*) as sc, GROUP_CONCAT(DISTINCT t.model) as models,
                    SUM(t.input_tokens), SUM(t.output_tokens), SUM(t.cache_creation_tokens), SUM(t.cache_read_tokens), SUM(t.cost_usd)
             FROM sessions s JOIN token_usage t ON t.session_rowid=s.id {wc} GROUP BY s.project_name ORDER BY SUM(t.cost_usd) DESC LIMIT ?"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if let Some(ref f) = fp {
            stmt.query_map(params![f, limit as i64], map_cost_row)?
        } else {
            stmt.query_map(params![limit as i64], map_cost_row)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn query_tools(
        &self,
        project_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ToolRow>> {
        let (wc, fp) = project_where(project_filter);
        let sql = format!(
            "SELECT tc.tool_name, SUM(tc.count) as total FROM tool_counts tc JOIN sessions s ON s.id=tc.session_rowid {wc} GROUP BY tc.tool_name ORDER BY total DESC LIMIT ?"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut result = Vec::new();
        if let Some(ref f) = fp {
            let rows = stmt.query_map(params![f, limit as i64], map_tool_row)?;
            for row in rows { result.push(row?); }
        } else {
            let rows = stmt.query_map(params![limit as i64], map_tool_row)?;
            for row in rows { result.push(row?); }
        };
        Ok(result)
    }

    pub fn search_fts(
        &self,
        query: &str,
        project_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchRow>> {
        let extra = match project_filter {
            Some(f) => format!("AND project_name MATCH '\"{}\"'", f.replace('\'', "''")),
            None => String::new(),
        };
        let sql = format!(
            "SELECT project_name, session_rowid, message_type, snippet(messages_fts, 3, '<<', '>>', '...', 64), timestamp
             FROM messages_fts WHERE content MATCH ?1 {extra} ORDER BY rank LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![query, limit as i64], map_search_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn query_summary(&self) -> Result<SummaryData> {
        let total_sessions: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        let total_cost: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd),0) FROM token_usage",
            [],
            |r| r.get(0),
        )?;
        let now_utc = chrono::Utc::now();
        let today_start = now_utc.format("%Y-%m-%dT00:00:00").to_string();
        let week_start = (now_utc - chrono::Duration::days(7)).to_rfc3339();
        let today: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE first_timestamp >= ?1",
            [&today_start],
            |r| r.get(0),
        )?;
        let this_week: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE first_timestamp >= ?1",
            [&week_start],
            |r| r.get(0),
        )?;
        let week_cost: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(t.cost_usd),0) FROM token_usage t JOIN sessions s ON s.id=t.session_rowid WHERE s.first_timestamp >= ?1",
            [&week_start],
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT project_name, COUNT(*) as c FROM sessions GROUP BY project_name ORDER BY c DESC LIMIT 5",
        )?;
        let top_projects: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = self.conn.prepare(
            "SELECT tool_name, SUM(count) as c FROM tool_counts GROUP BY tool_name ORDER BY c DESC LIMIT 5",
        )?;
        let top_tools: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let recent: Option<SessionRow> = self
            .conn
            .query_row(
                "SELECT project_name, session_id, first_timestamp, message_count, duration_ms, model FROM sessions ORDER BY first_timestamp DESC LIMIT 1",
                [],
                map_session_row,
            )
            .ok();
        Ok(SummaryData {
            total_sessions,
            today,
            this_week,
            total_cost,
            week_cost,
            top_projects,
            top_tools,
            recent,
        })
    }

    pub fn force_rebuild(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM messages_fts; DELETE FROM tool_counts; DELETE FROM token_usage; DELETE FROM sessions; DELETE FROM sync_meta;",
        )?;
        self.sync()?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct SessionRow {
    pub project: String,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub message_count: i64,
    pub duration_ms: i64,
    pub model: Option<String>,
}

#[derive(Debug)]
pub struct CostRow {
    pub project: String,
    pub sessions: i64,
    pub models: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation: i64,
    pub cache_read: i64,
    pub cost_usd: f64,
}

#[derive(Debug)]
pub struct ToolRow {
    pub tool_name: String,
    pub count: i64,
}

#[derive(Debug)]
pub struct SearchRow {
    pub project: String,
    pub session_rowid: i64,
    pub message_type: String,
    pub snippet: String,
    pub timestamp: Option<String>,
}

#[derive(Debug)]
pub struct SummaryData {
    pub total_sessions: i64,
    pub today: i64,
    pub this_week: i64,
    pub total_cost: f64,
    pub week_cost: f64,
    pub top_projects: Vec<(String, i64)>,
    pub top_tools: Vec<(String, i64)>,
    pub recent: Option<SessionRow>,
}

fn project_where(filter: Option<&str>) -> (String, Option<String>) {
    match filter {
        Some(f) => (
            "WHERE s.project_name LIKE ?".to_string(),
            Some(format!("%{f}%")),
        ),
        None => (String::new(), None),
    }
}

fn map_session_row(r: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        project: r.get(0)?,
        session_id: r.get(1)?,
        timestamp: r.get(2)?,
        message_count: r.get(3)?,
        duration_ms: r.get(4)?,
        model: r.get(5)?,
    })
}

fn map_cost_row(r: &rusqlite::Row) -> rusqlite::Result<CostRow> {
    Ok(CostRow {
        project: r.get(0)?,
        sessions: r.get(1)?,
        models: r.get(2)?,
        input_tokens: r.get(3)?,
        output_tokens: r.get(4)?,
        cache_creation: r.get(5)?,
        cache_read: r.get(6)?,
        cost_usd: r.get(7)?,
    })
}

fn map_tool_row(r: &rusqlite::Row) -> rusqlite::Result<ToolRow> {
    Ok(ToolRow {
        tool_name: r.get(0)?,
        count: r.get(1)?,
    })
}

fn map_search_row(r: &rusqlite::Row) -> rusqlite::Result<SearchRow> {
    Ok(SearchRow {
        project: r.get(0)?,
        session_rowid: r.get(1)?,
        message_type: r.get(2)?,
        snippet: r.get(3)?,
        timestamp: r.get(4)?,
    })
}

fn extract_text_content(message: &serde_json::Value) -> String {
    if let Some(content) = message["content"].as_str() {
        return content.to_string();
    }
    if let Some(content) = message["content"].as_array() {
        let mut text = String::new();
        for block in content {
            if block["type"].as_str() == Some("text") {
                if let Some(t) = block["text"].as_str() {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(t);
                }
            }
        }
        return text;
    }
    String::new()
}

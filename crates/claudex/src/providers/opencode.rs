//! OpenCode's SQLite database and pre-SQLite JSON storage.
//! Database connections are strictly read-only, including when OpenCode is live.
//! Upstream schemas: anomalyco/opencode/packages/schema/src/session-message.ts
//! and packages/core/src/session (legacy message-v2 data is also supported).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::Value;

use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider};

pub struct OpenCodeProvider {
    base_dir: PathBuf,
}

impl OpenCodeProvider {
    pub fn new() -> Result<Self> {
        let data = match std::env::var("XDG_DATA_HOME") {
            Ok(value) => super::expand_home(&value)?,
            Err(_) => dirs::home_dir()
                .context("could not find home directory")?
                .join(".local/share"),
        };
        Ok(Self::at(data.join("opencode")))
    }

    /// A data root containing opencode.db / storage, or a database file itself.
    pub fn at(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn database(&self) -> PathBuf {
        if self.base_dir.extension().is_some_and(|e| e == "db") {
            self.base_dir.clone()
        } else {
            self.base_dir.join("opencode.db")
        }
    }

    fn storage(&self) -> PathBuf {
        if self.base_dir.join("storage").is_dir() {
            self.base_dir.join("storage")
        } else {
            self.base_dir.clone()
        }
    }
}

fn open_readonly(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("opening OpenCode database {} read-only", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

impl SessionProvider for OpenCodeProvider {
    fn id(&self) -> &'static str {
        "opencode"
    }
    fn root_dir(&self) -> &Path {
        &self.base_dir
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let database = self.database();
        let database = std::fs::canonicalize(&database).unwrap_or(database);
        let mut files = Vec::new();
        let mut ids = BTreeSet::new();
        if database.is_file() {
            let conn = open_readonly(&database)?;
            for table in ["session_v2", "session"] {
                if !has_table(&conn, table)? {
                    continue;
                }
                let mut query = conn.prepare(&format!(
                    "SELECT id, directory, parent_id, time_archived FROM {table} ORDER BY id"
                ))?;
                for row in query.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                })? {
                    let (id, project, parent, archived) = row?;
                    if !ids.insert(id.clone()) {
                        continue;
                    }
                    files.push(DiscoveredFile {
                        path: database.join(&id),
                        project_display: project,
                        parent_session_id: parent,
                        archived: archived.is_some(),
                        source_key: Some(format!("{}#{id}", database.display())),
                    });
                }
            }
        }
        let storage = self.storage();
        let sessions = storage.join("session");
        if sessions.is_dir() {
            for path in recursive_files(&sessions)? {
                if path.extension().is_some_and(|e| e == "json") {
                    if path
                        .file_stem()
                        .is_some_and(|id| ids.contains(id.to_string_lossy().as_ref()))
                    {
                        continue;
                    }
                    files.push(DiscoveredFile {
                        path: path.to_path_buf(),
                        project_display: String::new(),
                        parent_session_id: None,
                        archived: false,
                        source_key: None,
                    });
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn source_fingerprint(&self, file: &DiscoveredFile) -> Result<(i64, i64)> {
        if file
            .path
            .parent()
            .is_some_and(|p| p.extension().is_some_and(|e| e == "db"))
        {
            // WAL commits may not touch the main database. Include both files,
            // with nanosecond precision, so updates and deletions refresh sessions.
            let db = file
                .path
                .parent()
                .context("missing OpenCode database path")?;
            let mut size = 0i64;
            let mut fingerprint = 0i64;
            for path in [
                db.to_path_buf(),
                PathBuf::from(format!("{}-wal", db.display())),
            ] {
                if let Ok(meta) = std::fs::metadata(path) {
                    size = size.wrapping_add(meta.len() as i64);
                    fingerprint = fingerprint.wrapping_mul(31).wrapping_add(
                        meta.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64,
                    );
                }
            }
            Ok((size, fingerprint))
        } else {
            // Legacy session, message and part files are updated independently.
            // Fingerprint the entire session's subtree, preserving correctness.
            let metadata: Value = serde_json::from_slice(&std::fs::read(&file.path)?)?;
            let id = metadata["id"]
                .as_str()
                .context("OpenCode session missing id")?;
            let storage = legacy_storage(&file.path).unwrap_or_else(|| self.storage());
            let mut paths = vec![file.path.clone()];
            let message_root = storage.join("message").join(id);
            if message_root.is_dir() {
                for path in recursive_files(&message_root)? {
                    paths.push(path.clone());
                    if let Some(msg) = path.file_stem() {
                        let parts = storage.join("part").join(msg);
                        if parts.is_dir() {
                            paths.extend(recursive_files(&parts)?);
                        }
                    }
                }
            }
            paths.sort();
            let mut size = 0i64;
            let mut fingerprint = 0i64;
            for path in paths {
                let metadata = std::fs::metadata(path)?;
                size = size.wrapping_add(metadata.len() as i64);
                fingerprint = fingerprint.wrapping_mul(31).wrapping_add(
                    metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64,
                );
            }
            Ok((size, fingerprint))
        }
    }

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        if file
            .path
            .parent()
            .is_some_and(|p| p.extension().is_some_and(|e| e == "db"))
        {
            parse_database(file)
        } else {
            parse_legacy(
                file,
                &legacy_storage(&file.path).unwrap_or_else(|| self.storage()),
            )
        }
    }
}

fn parse_database(file: &DiscoveredFile) -> Result<ProviderRecord> {
    let database = file
        .path
        .parent()
        .context("missing OpenCode database path")?;
    let id = file
        .path
        .file_name()
        .and_then(|s| s.to_str())
        .context("missing OpenCode session id")?;
    let mut conn = open_readonly(database)?;
    // One consistent snapshot across message and part queries while a live harness writes.
    let tx = conn.transaction()?;
    let modern = has_table(&tx, "session_v2")?
        && tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM session_v2 WHERE id=?1)",
            params![id],
            |r| r.get::<_, bool>(0),
        )?;
    let table = if modern { "session_v2" } else { "session" };
    let mut record = tx.query_row(
        &format!(
            "SELECT id,directory,parent_id,time_created,time_updated FROM {table} WHERE id=?1"
        ),
        params![id],
        |row| {
            Ok(ProviderRecord {
                session_id: Some(row.get(0)?),
                project_display: row.get(1)?,
                parent_session_id: row.get(2)?,
                first_timestamp: DateTime::from_timestamp_millis(row.get(3)?),
                last_timestamp: DateTime::from_timestamp_millis(row.get(4)?),
                source_key: file.source_key.clone(),
                ..Default::default()
            })
        },
    )?;
    let message_table = if modern { "session_message" } else { "message" };
    let mut query = tx.prepare(&format!("SELECT id,data,time_created FROM {message_table} WHERE session_id=?1 ORDER BY time_created,id"))?;
    let rows = query.query_map(params![id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (message_id, data, ts) = row?;
        let msg: Value = serde_json::from_str(&data)?;
        let mut parts = Vec::new();
        if !modern {
            let mut query =
                tx.prepare("SELECT data FROM part WHERE message_id=?1 ORDER BY time_created,id")?;
            for part in query.query_map(params![message_id], |row| row.get::<_, String>(0))? {
                parts.push(serde_json::from_str::<Value>(&part?)?);
            }
        }
        accumulate(&mut record, &msg, &parts, Some(ts));
    }
    finish(&mut record);
    Ok(record)
}

fn json_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for item in std::fs::read_dir(root)? {
        let path = item?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "json") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn parse_legacy(file: &DiscoveredFile, storage: &Path) -> Result<ProviderRecord> {
    let session: Value = serde_json::from_slice(&std::fs::read(&file.path)?)?;
    let id = session["id"]
        .as_str()
        .context("OpenCode session missing id")?;
    let mut record = ProviderRecord {
        session_id: Some(id.to_string()),
        project_display: session["directory"].as_str().unwrap_or("").to_string(),
        parent_session_id: session["parentID"].as_str().map(str::to_string),
        first_timestamp: millis(&session["time"]["created"]),
        last_timestamp: millis(&session["time"]["updated"]),
        ..Default::default()
    };
    for path in json_files(&storage.join("message").join(id))? {
        let msg: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        let msg_id = msg["id"]
            .as_str()
            .or_else(|| path.file_stem().and_then(|s| s.to_str()))
            .unwrap_or("");
        let mut parts = Vec::new();
        for part in json_files(&storage.join("part").join(msg_id))? {
            parts.push(serde_json::from_slice::<Value>(&std::fs::read(part)?)?);
        }
        accumulate(&mut record, &msg, &parts, msg["time"]["created"].as_i64());
    }
    finish(&mut record);
    Ok(record)
}

fn millis(v: &Value) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(v.as_i64()?)
}

fn accumulate(
    record: &mut ProviderRecord,
    msg: &Value,
    parts: &[Value],
    timestamp_ms: Option<i64>,
) {
    let role = msg["role"]
        .as_str()
        .or_else(|| msg["type"].as_str())
        .unwrap_or("");
    if role != "user" && role != "assistant" {
        return;
    }
    record.message_count += 1;
    if role == "assistant" {
        let model = msg["modelID"]
            .as_str()
            .or_else(|| msg["model"]["modelID"].as_str())
            .or_else(|| msg["model"]["id"].as_str())
            .unwrap_or("unknown");
        let provider = msg["providerID"]
            .as_str()
            .or_else(|| msg["model"]["providerID"].as_str())
            .or_else(|| msg["model"]["provider"].as_str())
            .unwrap_or("");
        let key = if provider.is_empty() {
            model.to_string()
        } else {
            format!("{provider}/{model}")
        };
        record.model = Some(key.clone());
        let stats = record.model_usage.entry(key).or_default();
        stats.assistant_message_count += 1;
        stats.usage.input_tokens += msg["tokens"]["input"].as_u64().unwrap_or(0);
        stats.usage.output_tokens += msg["tokens"]["output"].as_u64().unwrap_or(0)
            + msg["tokens"]["reasoning"].as_u64().unwrap_or(0);
        stats.usage.cache_read_tokens += msg["tokens"]["cache"]["read"].as_u64().unwrap_or(0);
        stats.usage.cache_creation_tokens += msg["tokens"]["cache"]["write"].as_u64().unwrap_or(0);
        if let Some(cost) = msg["cost"].as_f64().filter(|c| c.is_finite() && *c >= 0.0) {
            stats.embedded_cost = Some(stats.embedded_cost.unwrap_or(0.0) + cost);
            record.embedded_cost = Some(record.embedded_cost.unwrap_or(0.0) + cost);
        }
        if let Some(reason) = msg["finish"].as_str() {
            *record
                .stop_reason_counts
                .entry(reason.to_string())
                .or_default() += 1;
        }
        if let Some(cwd) = msg["path"]["cwd"].as_str().filter(|s| !s.is_empty()) {
            record.project_display = cwd.to_string();
        }
    }
    let content = msg["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(parts);
    let mut texts = Vec::new();
    for part in content {
        match part["type"].as_str() {
            Some("text") => {
                if let Some(text) = part["text"].as_str() {
                    texts.push(text);
                }
            }
            Some("reasoning") => record.thinking_block_count += 1,
            Some("tool") => {
                if let Some(tool) = part["tool"].as_str().or_else(|| part["name"].as_str()) {
                    record.tool_names.push(tool.to_string());
                }
                for field in ["filePath", "path"] {
                    if let Some(path) = part["state"]["input"][field].as_str().filter(|_| {
                        matches!(
                            part["tool"].as_str().or_else(|| part["name"].as_str()),
                            Some("edit" | "write" | "apply_patch")
                        )
                    }) {
                        record.file_paths_modified.push(path.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if !texts.is_empty() {
        record.messages.push(MessageForFts {
            msg_type: role.to_string(),
            content: texts.join("\n"),
            timestamp_ms,
        });
    }
    if let Some(ts) = timestamp_ms.and_then(DateTime::from_timestamp_millis) {
        record.first_timestamp = Some(record.first_timestamp.map_or(ts, |p| p.min(ts)));
        record.last_timestamp = Some(record.last_timestamp.map_or(ts, |p| p.max(ts)));
    }
    if role == "assistant"
        && let (Some(start), Some(end)) = (
            msg["time"]["created"].as_i64(),
            msg["time"]["completed"].as_i64(),
        )
        && end >= start
    {
        record.turn_durations.push((
            (end - start) as u64,
            record.model.clone().unwrap_or_default(),
        ));
    }
}

fn finish(record: &mut ProviderRecord) {
    if let (Some(start), Some(end)) = (record.first_timestamp, record.last_timestamp) {
        record.duration_ms = (end - start).num_milliseconds().max(0) as u64;
    }
}

fn recursive_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(dir) = directories.pop() {
        for item in std::fs::read_dir(dir)? {
            let item = item?;
            let kind = item.file_type()?;
            if kind.is_dir() {
                directories.push(item.path());
            } else if kind.is_file() {
                files.push(item.path());
            }
        }
    }
    Ok(files)
}

fn has_table(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        params![table],
        |r| r.get(0),
    )?)
}

// A legacy session lives at storage/session/<project>/<id>.json. Deriving its
// root from the source also makes export work for configured additional roots.
fn legacy_storage(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .skip(1)
        .find(|p| p.file_name().is_some_and(|n| n == "session"))?
        .parent()
        .map(Path::to_path_buf)
}

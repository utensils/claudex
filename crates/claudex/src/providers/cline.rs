//! Cline SDK artifacts and legacy Cline / Roo Code VS Code task directories.
//! Official schemas: cline/cline sdk/packages/shared/src/llms/messages.ts,
//! sdk/packages/core/src/services/session-data.ts, apps/vscode/src/shared/
//! {getApiMetrics,combineApiRequests,HistoryItem}.ts; RooCodeInc/Roo-Code
//! src/core/task-persistence/{apiMessages,taskMessages,taskMetadata}.ts.

use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub struct ClineProvider {
    roots: Vec<PathBuf>,
}
pub struct RooProvider {
    inner: ClineProvider,
}

impl ClineProvider {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        let mut roots = vec![home.join(".cline/data/sessions")];
        roots.extend(extension_roots(&home, "saoudrizwan.claude-dev"));
        Ok(Self { roots })
    }
    /// Accepts a task directory, tasks tree, SDK sessions tree, or data parent.
    pub fn at(root: PathBuf) -> Self {
        Self { roots: vec![root] }
    }
}

impl RooProvider {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        Ok(Self {
            inner: ClineProvider {
                roots: extension_roots(&home, "rooveterinaryinc.roo-cline"),
            },
        })
    }
    pub fn at(root: PathBuf) -> Self {
        Self {
            inner: ClineProvider::at(root),
        }
    }
}

fn extension_roots(home: &Path, id: &str) -> Vec<PathBuf> {
    let config = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
    };
    ["Code", "Code - Insiders", "VSCodium"]
        .into_iter()
        .map(|editor| config.join(editor).join("User/globalStorage").join(id))
        .collect()
}

impl SessionProvider for ClineProvider {
    fn id(&self) -> &'static str {
        "cline"
    }
    fn root_dir(&self) -> &Path {
        self.roots
            .iter()
            .find(|root| root.exists())
            .unwrap_or(&self.roots[0])
    }
    fn root_dirs(&self) -> Vec<PathBuf> {
        self.roots.clone()
    }
    fn enabled(&self) -> bool {
        self.roots.iter().any(|p| p.exists())
    }
    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        enumerate_roots(&self.roots, true)
    }
    fn source_fingerprint(&self, file: &DiscoveredFile) -> Result<(i64, i64)> {
        fingerprint(file)
    }
    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        parse(file)
    }
}
impl SessionProvider for RooProvider {
    fn id(&self) -> &'static str {
        "roo"
    }
    fn root_dir(&self) -> &Path {
        self.inner.root_dir()
    }
    fn root_dirs(&self) -> Vec<PathBuf> {
        self.inner.roots.clone()
    }
    fn enabled(&self) -> bool {
        self.inner.enabled()
    }
    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        enumerate_roots(&self.inner.roots, false)
    }
    fn source_fingerprint(&self, file: &DiscoveredFile) -> Result<(i64, i64)> {
        fingerprint(file)
    }
    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        parse(file)
    }
}

fn enumerate_roots(roots: &[PathBuf], sdk: bool) -> Result<Vec<DiscoveredFile>> {
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut directories = roots
        .iter()
        .filter(|p| p.is_dir())
        .cloned()
        .collect::<Vec<_>>();
    while let Some(dir) = directories.pop() {
        let canonical = std::fs::canonicalize(&dir)?;
        if !seen.insert(canonical) {
            continue;
        }
        for item in std::fs::read_dir(dir)? {
            let item = item?;
            let kind = item.file_type()?;
            if kind.is_dir() {
                directories.push(item.path());
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let path = item.path();
            let name = item.file_name();
            let name = name.to_string_lossy();
            if name == "api_conversation_history.json"
                || (name == "ui_messages.json"
                    && !path
                        .with_file_name("api_conversation_history.json")
                        .is_file())
                || (sdk && name.ends_with(".messages.json"))
            {
                files.push(DiscoveredFile {
                    path: std::fs::canonicalize(&path)?,
                    project_display: String::new(),
                    parent_session_id: None,
                    archived: false,
                    source_key: if sdk_path(&path) {
                        None
                    } else {
                        path.parent()
                            .map(|dir| {
                                std::fs::canonicalize(dir)
                                    .map(|dir| dir.to_string_lossy().into_owned())
                            })
                            .transpose()?
                    },
                });
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn read(path: &Path) -> Result<Value> {
    serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("reading transcript {}", path.display()))?,
    )
    .with_context(|| format!("parsing transcript {}", path.display()))
}
fn optional(path: &Path) -> Result<Value> {
    if path.is_file() {
        read(path)
    } else {
        Ok(Value::Null)
    }
}
fn sdk_path(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|s| s.to_string_lossy().ends_with(".messages.json"))
}
fn history_path(path: &Path) -> Option<PathBuf> {
    let task = path.parent()?;
    let tasks = task.parent()?;
    if tasks.file_name().is_some_and(|n| n == "tasks") {
        Some(tasks.parent()?.join("state/taskHistory.json"))
    } else {
        None
    }
}
fn root_manifest(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let id = dir.file_name()?.to_str()?;
    Some(dir.join(format!("{id}.json")))
}
fn fingerprint(file: &DiscoveredFile) -> Result<(i64, i64)> {
    let mut paths = vec![file.path.clone()];
    if sdk_path(&file.path) {
        if let Some(manifest) = root_manifest(&file.path) {
            paths.push(manifest);
        }
    } else {
        for name in [
            "api_conversation_history.json",
            "ui_messages.json",
            "task_metadata.json",
            "history_item.json",
        ] {
            paths.push(file.path.with_file_name(name));
        }
        if let Some(history) = history_path(&file.path) {
            paths.push(history);
        }
    }
    paths.sort();
    paths.dedup();
    let (mut size, mut stamp) = (0i64, 0i64);
    for path in paths {
        if !path.exists() {
            continue;
        }
        let meta = std::fs::metadata(path)?;
        size = size.wrapping_add(meta.len() as i64);
        stamp = stamp
            .wrapping_mul(31)
            .wrapping_add(meta.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64);
    }
    Ok((size, stamp))
}
fn parse(file: &DiscoveredFile) -> Result<ProviderRecord> {
    if sdk_path(&file.path) {
        parse_sdk(file)
    } else {
        parse_legacy(file)
    }
}
fn ts(value: &Value) -> Option<DateTime<Utc>> {
    value
        .as_i64()
        .and_then(DateTime::from_timestamp_millis)
        .or_else(|| {
            value
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&Utc))
        })
}
fn observe(record: &mut ProviderRecord, value: &Value) {
    if let Some(timestamp) = ts(value) {
        record.first_timestamp = Some(
            record
                .first_timestamp
                .map_or(timestamp, |p| p.min(timestamp)),
        );
        record.last_timestamp = Some(
            record
                .last_timestamp
                .map_or(timestamp, |p| p.max(timestamp)),
        );
    }
}
fn finish(record: &mut ProviderRecord) {
    if let (Some(first), Some(last)) = (record.first_timestamp, record.last_timestamp) {
        record.duration_ms = (last - first).num_milliseconds().max(0) as u64;
    }
}
fn key(provider: &str, model: &str) -> String {
    if provider.is_empty() {
        model.to_string()
    } else {
        format!("{provider}/{model}")
    }
}
fn usage(record: &mut ProviderRecord, model: &str, v: &Value, sdk: bool) {
    let fields = if sdk {
        [
            "inputTokens",
            "outputTokens",
            "cacheReadTokens",
            "cacheWriteTokens",
        ]
    } else {
        ["tokensIn", "tokensOut", "cacheReads", "cacheWrites"]
    };
    let stats = record.model_usage.entry(model.to_string()).or_default();
    stats.usage.input_tokens += v[fields[0]].as_u64().unwrap_or(0);
    stats.usage.output_tokens += v[fields[1]].as_u64().unwrap_or(0);
    stats.usage.cache_read_tokens += v[fields[2]].as_u64().unwrap_or(0);
    stats.usage.cache_creation_tokens += v[fields[3]].as_u64().unwrap_or(0);
    if let Some(cost) = v["cost"].as_f64().filter(|c| c.is_finite() && *c >= 0.0) {
        stats.embedded_cost = Some(stats.embedded_cost.unwrap_or(0.0) + cost);
        record.embedded_cost = Some(record.embedded_cost.unwrap_or(0.0) + cost);
    }
}
fn message(record: &mut ProviderRecord, msg: &Value, timestamp: Option<i64>) {
    let Some(role) = msg["role"]
        .as_str()
        .filter(|r| matches!(*r, "user" | "assistant"))
    else {
        return;
    };
    record.message_count += 1;
    observe(record, &msg["ts"]);
    let mut texts = Vec::new();
    if let Some(text) = msg["content"].as_str() {
        texts.push(text.to_string());
    }
    if let Some(parts) = msg["content"].as_array() {
        for part in parts {
            match part["type"].as_str() {
                Some("text") => {
                    if let Some(text) = part["text"].as_str() {
                        texts.push(text.to_string());
                    }
                }
                Some("tool_use") => {
                    if let Some(tool) = part["name"].as_str() {
                        record.tool_names.push(tool.to_string());
                        if matches!(
                            tool,
                            "write_to_file"
                                | "replace_in_file"
                                | "apply_diff"
                                | "edit_file"
                                | "write_file"
                        ) && let Some(path) = part["input"]["path"].as_str()
                        {
                            record.file_paths_modified.push(path.to_string());
                        }
                    }
                }
                Some("thinking" | "redacted_thinking") => record.thinking_block_count += 1,
                _ => {}
            }
        }
    }
    if !texts.is_empty() {
        let content = texts.join("\n");
        if role == "assistant" {
            for name in [
                "read_file",
                "write_to_file",
                "replace_in_file",
                "execute_command",
                "search_files",
                "list_files",
                "apply_diff",
                "attempt_completion",
                "ask_followup_question",
            ] {
                record.tool_names.extend(std::iter::repeat_n(
                    name.to_string(),
                    content.matches(&format!("<{name}>")).count(),
                ));
            }
        }
        record.messages.push(MessageForFts {
            msg_type: role.to_string(),
            content,
            timestamp_ms: ts(&msg["ts"]).map(|t| t.timestamp_millis()).or(timestamp),
        });
    }
}
fn parse_sdk(file: &DiscoveredFile) -> Result<ProviderRecord> {
    let document = read(&file.path)?;
    let messages = document["messages"]
        .as_array()
        .context("Cline SDK artifact missing messages array")?;
    let manifest = root_manifest(&file.path)
        .map(|p| optional(&p))
        .transpose()?
        .unwrap_or(Value::Null);
    let id = document["sessionId"]
        .as_str()
        .context("Cline SDK artifact missing sessionId")?;
    let mut record = ProviderRecord {
        session_id: Some(id.to_string()),
        parent_session_id: document["origin"]["parentThreadId"]
            .as_str()
            .map(str::to_string),
        project_display: manifest["cwd"]
            .as_str()
            .or_else(|| manifest["workspace_root"].as_str())
            .unwrap_or("")
            .to_string(),
        ..Default::default()
    };
    observe(&mut record, &manifest["started_at"]);
    observe(&mut record, &document["updated_at"]);
    observe(&mut record, &manifest["ended_at"]);
    for msg in messages {
        message(&mut record, msg, None);
        if msg["role"].as_str() == Some("assistant") {
            let model = key(
                msg["modelInfo"]["provider"]
                    .as_str()
                    .or_else(|| msg["providerId"].as_str())
                    .or_else(|| manifest["provider"].as_str())
                    .unwrap_or(""),
                msg["modelInfo"]["id"]
                    .as_str()
                    .or_else(|| msg["modelId"].as_str())
                    .or_else(|| manifest["model"].as_str())
                    .unwrap_or("unknown"),
            );
            record.model = Some(model.clone());
            record
                .model_usage
                .entry(model.clone())
                .or_default()
                .assistant_message_count += 1;
            usage(&mut record, &model, &msg["metrics"], true);
        }
    }
    finish(&mut record);
    Ok(record)
}
fn legacy_model(metadata: &Value, history: &Value, timestamp: i64) -> String {
    let entry = metadata["model_usage"].as_array().and_then(|models| {
        models
            .iter()
            .filter(|v| v["ts"].as_i64().unwrap_or(0) <= timestamp)
            .max_by_key(|v| v["ts"].as_i64().unwrap_or(0))
            .or_else(|| models.first())
    });
    if let Some(entry) = entry {
        key(
            entry["model_provider_id"].as_str().unwrap_or(""),
            entry["model_id"].as_str().unwrap_or("unknown"),
        )
    } else {
        key(
            history["apiProvider"].as_str().unwrap_or(""),
            history["modelId"].as_str().unwrap_or("unknown"),
        )
    }
}
fn parse_legacy(file: &DiscoveredFile) -> Result<ProviderRecord> {
    let dir = file
        .path
        .parent()
        .context("Cline task has no parent directory")?;
    let id = dir
        .file_name()
        .and_then(|s| s.to_str())
        .context("Cline task has no id")?;
    let api = optional(&dir.join("api_conversation_history.json"))?;
    let ui = optional(&dir.join("ui_messages.json"))?;
    let metadata = optional(&dir.join("task_metadata.json"))?;
    let history = history_path(&file.path)
        .map(|p| optional(&p))
        .transpose()?
        .unwrap_or(Value::Null);
    let history = history
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"].as_str() == Some(id)))
        .cloned()
        .unwrap_or(Value::Null);
    let local_history = optional(&dir.join("history_item.json"))?;
    let history = if local_history.is_object() {
        local_history
    } else {
        history
    };
    let mut record = ProviderRecord {
        session_id: Some(id.to_string()),
        parent_session_id: history["parentTaskId"].as_str().map(str::to_string),
        project_display: history["cwdOnTaskInitialization"]
            .as_str()
            .or_else(|| history["workspace"].as_str())
            .unwrap_or("")
            .to_string(),
        ..Default::default()
    };
    let ui = ui.as_array().cloned().unwrap_or_default();
    for row in &ui {
        observe(&mut record, &row["ts"]);
    }
    if let Some(messages) = api.as_array() {
        for msg in messages {
            message(&mut record, msg, None);
        }
    } else {
        // UI-only tasks still retain useful text without treating status rows as messages.
        for row in &ui {
            let role = match (
                row["type"].as_str(),
                row["say"].as_str(),
                row["ask"].as_str(),
            ) {
                (_, Some("task" | "user_feedback"), _) => Some("user"),
                (_, Some("text" | "completion_result"), _) => Some("assistant"),
                (_, _, Some("followup" | "completion_result")) => Some("assistant"),
                _ => None,
            };
            if let Some(role) = role {
                message(
                    &mut record,
                    &serde_json::json!({"role":role,"content":row["text"],"ts":row["ts"]}),
                    None,
                );
            }
        }
    }
    let mut saw_usage = false;
    for (i, row) in ui.iter().enumerate() {
        if row["type"].as_str() != Some("say") {
            continue;
        }
        let kind = row["say"].as_str().unwrap_or("");
        if !matches!(
            kind,
            "api_req_started" | "deleted_api_reqs" | "subagent_usage"
        ) {
            continue;
        }
        let Ok(mut value) = serde_json::from_str::<Value>(row["text"].as_str().unwrap_or("{}"))
        else {
            continue;
        };
        if kind == "api_req_started"
            && let Some(finish) = ui[i + 1..]
                .iter()
                .take_while(|r| r["say"].as_str() != Some("api_req_started"))
                .find(|r| r["say"].as_str() == Some("api_req_finished"))
            && let Ok(finish) =
                serde_json::from_str::<Value>(finish["text"].as_str().unwrap_or("{}"))
            && let (Some(target), Some(fields)) = (value.as_object_mut(), finish.as_object())
        {
            target.extend(fields.clone());
        }
        let model = legacy_model(&metadata, &history, row["ts"].as_i64().unwrap_or(i64::MAX));
        record.model = Some(model.clone());
        if kind == "api_req_started" {
            record
                .model_usage
                .entry(model.clone())
                .or_default()
                .assistant_message_count += 1;
        }
        usage(&mut record, &model, &value, false);
        saw_usage = true;
    }
    if !saw_usage && history.is_object() {
        let model = legacy_model(&metadata, &history, i64::MAX);
        record.model = Some(model.clone());
        let mut totals = history.clone();
        if let Some(fields) = totals.as_object_mut() {
            fields.insert("cost".to_string(), history["totalCost"].clone());
        }
        usage(&mut record, &model, &totals, false);
    }
    if record.model_usage.is_empty() {
        let model = legacy_model(&metadata, &history, i64::MAX);
        record.model = Some(model.clone());
        record
            .model_usage
            .entry(model)
            .or_default()
            .assistant_message_count = record
            .messages
            .iter()
            .filter(|m| m.msg_type == "assistant")
            .count() as u64;
    }
    if let Some(files) = metadata["files_in_context"].as_array() {
        record.file_paths_modified.extend(
            files
                .iter()
                .filter(|f| {
                    f["record_source"].as_str() == Some("cline_edited")
                        || f["cline_edit_date"].is_number()
                })
                .filter_map(|f| f["path"].as_str())
                .map(str::to_string),
        );
    }
    finish(&mut record);
    Ok(record)
}

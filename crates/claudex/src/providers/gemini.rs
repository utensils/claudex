//! Gemini CLI session recordings (legacy JSON and current append-only JSONL).
//! Schema: google-gemini/gemini-cli/packages/core/src/services/chatRecordingTypes.ts.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider};

pub struct GeminiProvider {
    base_dir: PathBuf,
}

impl GeminiProvider {
    pub fn new() -> Result<Self> {
        Ok(Self::at(
            dirs::home_dir()
                .context("could not find home directory")?
                .join(".gemini"),
        ))
    }

    /// Accepts a Gemini home, project temporary directory, or chats directory.
    pub fn at(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SessionProvider for GeminiProvider {
    fn id(&self) -> &'static str {
        "gemini"
    }
    fn root_dir(&self) -> &Path {
        &self.base_dir
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let root = if self.base_dir.join("tmp").is_dir() {
            self.base_dir.join("tmp")
        } else {
            self.base_dir.clone()
        };
        let mut files = Vec::new();
        if root.exists() {
            for path in recursive_files(&root)? {
                if path.is_file()
                    && path
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("session-"))
                    && path
                        .extension()
                        .is_some_and(|e| e == "json" || e == "jsonl")
                {
                    let project = path
                        .parent()
                        .and_then(Path::parent)
                        .map(|p| p.join(".project_root"))
                        .and_then(|p| std::fs::read_to_string(p).ok())
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    files.push(DiscoveredFile {
                        path: path.clone(),
                        project_display: project,
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

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        let text = std::fs::read_to_string(&file.path)?;
        let (metadata, messages) = if file.path.extension().is_some_and(|e| e == "json") {
            let v: Value = serde_json::from_str(&text)?;
            let messages = v["messages"].as_array().cloned().unwrap_or_default();
            (v, messages)
        } else {
            load_jsonl(&text)?
        };
        let mut record = ProviderRecord {
            session_id: metadata["sessionId"].as_str().map(str::to_string),
            first_timestamp: timestamp(&metadata["startTime"]),
            last_timestamp: timestamp(&metadata["lastUpdated"]),
            project_display: file.project_display.clone(),
            ..Default::default()
        };
        if record.project_display.is_empty() {
            record.project_display = metadata["directories"]
                .as_array()
                .and_then(|d| d.first())
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
        }
        for msg in messages {
            let role = match msg["type"].as_str() {
                Some("user") => "user",
                Some("gemini") => "assistant",
                _ => continue,
            };
            record.message_count += 1;
            let ts = timestamp(&msg["timestamp"]);
            if let Some(ts) = ts {
                record.first_timestamp = Some(record.first_timestamp.map_or(ts, |p| p.min(ts)));
                record.last_timestamp = Some(record.last_timestamp.map_or(ts, |p| p.max(ts)));
            }
            let content = content_text(&msg["content"]);
            if !content.is_empty() {
                record.messages.push(MessageForFts {
                    msg_type: role.to_string(),
                    content,
                    timestamp_ms: ts.map(|t| t.timestamp_millis()),
                });
            }
            if role == "assistant" {
                let model = msg["model"].as_str().unwrap_or("unknown").to_string();
                record.model = Some(model.clone());
                let usage = &msg["tokens"];
                let stats = record.model_usage.entry(model).or_default();
                let cached = usage["cached"].as_u64().unwrap_or(0);
                stats.usage.input_tokens +=
                    usage["input"].as_u64().unwrap_or(0).saturating_sub(cached);
                stats.usage.cache_read_tokens += cached;
                stats.usage.output_tokens +=
                    usage["output"].as_u64().unwrap_or(0) + usage["thoughts"].as_u64().unwrap_or(0);
                stats.assistant_message_count += 1;
                record.thinking_block_count +=
                    msg["thoughts"].as_array().map_or(0, |t| t.len() as u64);
                if let Some(calls) = msg["toolCalls"].as_array() {
                    record.tool_names.extend(
                        calls
                            .iter()
                            .filter_map(|c| c["name"].as_str())
                            .map(str::to_string),
                    );
                }
            }
        }
        if let (Some(first), Some(last)) = (record.first_timestamp, record.last_timestamp) {
            record.duration_ms = (last - first).num_milliseconds().max(0) as u64;
        }
        Ok(record)
    }
}

fn timestamp(v: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v.as_str()?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn content_text(v: &Value) -> String {
    if let Some(text) = v.as_str() {
        return text.to_string();
    }
    v.as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn merge(target: &mut Value, patch: &Value) {
    if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
        for (k, v) in patch {
            target.insert(k.clone(), v.clone());
        }
    }
}

fn load_jsonl(text: &str) -> Result<(Value, Vec<Value>)> {
    let mut metadata = serde_json::json!({});
    let mut messages: Vec<Value> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        // A live writer may leave its final line incomplete.
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(id) = v["$rewindTo"].as_str() {
            let pos = messages
                .iter()
                .position(|m| m["id"].as_str() == Some(id))
                .unwrap_or(0);
            messages.truncate(pos);
        } else if v["$set"].is_object() {
            if let Some(replacement) = v["$set"]["messages"].as_array() {
                messages = replacement.clone();
            }
            merge(&mut metadata, &v["$set"]);
        } else if v["$patch"].is_object() {
            let patch = &v["$patch"];
            let updates = patch["updates"]
                .as_array()
                .cloned()
                .unwrap_or_else(|| vec![patch.clone()]);
            for update in updates {
                if let Some(message) = messages
                    .iter_mut()
                    .find(|m| m["id"] == update["id"] && update["id"].is_string())
                {
                    // Tool-call patches update existing calls by id rather than replacing them.
                    if let (Some(calls), Some(changes)) = (
                        message["toolCalls"].as_array_mut(),
                        update["toolCalls"].as_array(),
                    ) {
                        for change in changes {
                            if let Some(call) = calls.iter_mut().find(|c| c["id"] == change["id"]) {
                                merge(call, change);
                            }
                        }
                    }
                    let mut fields = update.clone();
                    if let Some(fields) = fields.as_object_mut() {
                        fields.remove("toolCalls");
                    }
                    merge(message, &fields);
                }
            }
            if let Some(ids) = patch["removeIds"].as_array() {
                messages.retain(|m| !ids.contains(&m["id"]));
            }
            if let Some(ids) = patch["orderIds"].as_array() {
                let mut ordered = Vec::new();
                for id in ids {
                    if let Some(pos) = messages.iter().position(|m| &m["id"] == id) {
                        ordered.push(messages.remove(pos));
                    }
                }
                messages.extend(ordered);
            }
        } else if v["id"].is_string() && v["type"].is_string() {
            if let Some(old) = messages.iter_mut().find(|m| m["id"] == v["id"]) {
                *old = v;
            } else {
                messages.push(v);
            }
        } else {
            merge(&mut metadata, &v);
        }
    }
    anyhow::ensure!(
        metadata["sessionId"].is_string(),
        "Gemini recording has no sessionId"
    );
    Ok((metadata, messages))
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

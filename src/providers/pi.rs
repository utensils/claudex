//! Pi provider: indexes the JSONL transcripts under `~/.pi/agent/sessions/`.
//! Pi is multi-provider (OpenAI, Anthropic, Ollama, …) and reports a computed
//! cost per assistant message, so [`PiProvider::parse`] accumulates per-model
//! token usage and trusts Pi's own cost (`embedded_cost`) instead of a pricing
//! table — which also makes local/free Ollama sessions correctly report $0.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider};
use crate::parser::stream_records;

pub struct PiProvider {
    base_dir: PathBuf,
}

impl PiProvider {
    /// Provider rooted at the user's `~/.pi/agent`.
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        Ok(Self {
            base_dir: home.join(".pi").join("agent"),
        })
    }

    /// Provider rooted at an explicit `agent` directory (tests).
    pub fn at(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SessionProvider for PiProvider {
    fn id(&self) -> &'static str {
        "pi"
    }

    fn root_dir(&self) -> &Path {
        &self.base_dir
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let mut files = Vec::new();
        let sessions = self.base_dir.join("sessions");
        if sessions.exists() {
            // sessions/<cwd-encoded>/<ts>_<uuid>.jsonl
            for cwd_dir in std::fs::read_dir(&sessions)
                .with_context(|| format!("reading {}", sessions.display()))?
            {
                let cwd_dir = cwd_dir?;
                if !cwd_dir.path().is_dir() {
                    continue;
                }
                let project = decode_pi_cwd(&cwd_dir.file_name().to_string_lossy());
                for file in std::fs::read_dir(cwd_dir.path())? {
                    let path = file?.path();
                    if path.extension().is_some_and(|e| e == "jsonl") {
                        files.push(DiscoveredFile {
                            path,
                            project_display: project.clone(),
                            parent_session_id: None,
                            archived: false,
                        });
                    }
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        parse_pi_session(&file.path)
    }
}

/// Decode Pi's cwd-encoded directory name (`--Users-me-Projects-foo--`) into a
/// filesystem path (`/Users/me/Projects/foo`).
fn decode_pi_cwd(encoded: &str) -> String {
    let trimmed = encoded.trim_matches('-');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    format!("/{}", trimmed.replace('-', "/"))
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn parse_pi_session(path: &Path) -> Result<ProviderRecord> {
    let mut entry = ProviderRecord::default();
    let mut session_cwd: Option<String> = None;
    let mut current_model: Option<String> = None;
    let mut total_cost = 0.0f64;
    let mut saw_cost = false;

    stream_records(path, |record| {
        if let Some(ts) = record["timestamp"].as_str().and_then(parse_ts) {
            if entry.first_timestamp.is_none_or(|p| ts < p) {
                entry.first_timestamp = Some(ts);
            }
            if entry.last_timestamp.is_none_or(|p| ts > p) {
                entry.last_timestamp = Some(ts);
            }
        }

        match record["type"].as_str() {
            Some("session") => {
                if let Some(id) = record["id"].as_str() {
                    entry.session_id = Some(id.to_string());
                }
                if let Some(cwd) = record["cwd"].as_str()
                    && !cwd.is_empty()
                {
                    session_cwd = Some(cwd.to_string());
                }
            }
            Some("model_change") => {
                let provider = record["provider"].as_str().unwrap_or("");
                let model = record["modelId"].as_str().unwrap_or("");
                if !model.is_empty() {
                    current_model = Some(model_key(provider, model));
                }
            }
            Some("message") => {
                let msg = &record["message"];
                let ts_ms = record["timestamp"].as_str().and_then(|t| {
                    DateTime::parse_from_rfc3339(t)
                        .ok()
                        .map(|d| d.timestamp_millis())
                });
                match msg["role"].as_str() {
                    Some("user") => {
                        entry.message_count += 1;
                        if let Some(text) = content_text(&msg["content"])
                            && !text.is_empty()
                        {
                            entry.messages.push(MessageForFts {
                                msg_type: "user".to_string(),
                                content: text,
                                timestamp_ms: ts_ms,
                            });
                        }
                    }
                    Some("assistant") => {
                        entry.message_count += 1;
                        accumulate_assistant(
                            &mut entry,
                            msg,
                            current_model.as_deref(),
                            &mut total_cost,
                            &mut saw_cost,
                            ts_ms,
                        );
                    }
                    _ => {} // toolResult etc. — the call was already counted
                }
            }
            _ => {}
        }
        true
    })?;

    if let Some(cwd) = session_cwd {
        entry.project_display = cwd;
    }
    if saw_cost {
        entry.embedded_cost = Some(total_cost);
    }

    let mut extras = serde_json::Map::new();
    if let Some(model) = &current_model {
        extras.insert("last_model".to_string(), json!(model));
    }
    if !extras.is_empty() {
        entry.extras = Some(Value::Object(extras).to_string());
    }

    if entry.duration_ms == 0
        && let (Some(first), Some(last)) = (entry.first_timestamp, entry.last_timestamp)
    {
        entry.duration_ms = last.signed_duration_since(first).num_milliseconds().max(0) as u64;
    }

    Ok(entry)
}

fn accumulate_assistant(
    entry: &mut ProviderRecord,
    msg: &Value,
    fallback_model: Option<&str>,
    total_cost: &mut f64,
    saw_cost: &mut bool,
    ts_ms: Option<i64>,
) {
    // Tool calls, thinking blocks, and assistant text live in the content array.
    if let Some(content) = msg["content"].as_array() {
        let mut text_parts = Vec::new();
        for block in content {
            match block["type"].as_str() {
                Some("toolCall") => {
                    if let Some(name) = block["name"].as_str()
                        && !name.is_empty()
                    {
                        entry.tool_names.push(name.to_string());
                    }
                }
                Some("thinking") => entry.thinking_block_count += 1,
                Some("text") => {
                    if let Some(t) = block["text"].as_str().filter(|t| !t.is_empty()) {
                        text_parts.push(t.to_string());
                    }
                }
                _ => {}
            }
        }
        if !text_parts.is_empty() {
            entry.messages.push(MessageForFts {
                msg_type: "assistant".to_string(),
                content: text_parts.join(" "),
                timestamp_ms: ts_ms,
            });
        }
    }

    if let Some(stop) = msg["stopReason"].as_str()
        && !stop.is_empty()
    {
        *entry
            .stop_reason_counts
            .entry(stop.to_string())
            .or_insert(0) += 1;
    }

    // Per-model token usage keyed by "provider/model", with Pi's own cost.
    let provider = msg["provider"].as_str().unwrap_or("");
    let model = msg["model"].as_str().unwrap_or("");
    let key = if model.is_empty() {
        fallback_model.unwrap_or("").to_string()
    } else {
        model_key(provider, model)
    };

    let usage = &msg["usage"];
    let stats = entry.model_usage.entry(key).or_default();
    stats.usage.input_tokens += usage["input"].as_u64().unwrap_or(0);
    stats.usage.output_tokens += usage["output"].as_u64().unwrap_or(0);
    stats.usage.cache_read_tokens += usage["cacheRead"].as_u64().unwrap_or(0);
    stats.usage.cache_creation_tokens += usage["cacheWrite"].as_u64().unwrap_or(0);
    stats.assistant_message_count += 1;

    if let Some(cost) = usage["cost"]["total"].as_f64() {
        *total_cost += cost;
        *saw_cost = true;
        stats.embedded_cost = Some(stats.embedded_cost.unwrap_or(0.0) + cost);
    }
}

/// `provider/model`, or just `model` when the provider is unknown.
fn model_key(provider: &str, model: &str) -> String {
    if provider.is_empty() {
        model.to_string()
    } else {
        format!("{provider}/{model}")
    }
}

/// Join the text blocks of a Pi message `content` array.
fn content_text(content: &Value) -> Option<String> {
    let arr = content.as_array()?;
    let parts: Vec<&str> = arr
        .iter()
        .filter(|b| b["type"].as_str() == Some("text"))
        .filter_map(|b| b["text"].as_str().filter(|t| !t.is_empty()))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

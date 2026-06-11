//! GitHub Copilot CLI provider: indexes the per-session event logs under
//! `~/.copilot/session-state/<uuid>/events.jsonl` (override the base directory
//! with `CLAUDEX_COPILOT_DIR`). The event log carries the project (`cwd`),
//! models, messages, tool calls, and — in the final `session.shutdown` event —
//! cumulative per-model token metrics. Copilot bills by premium requests, not
//! USD, so cost is computed from the rate card and the premium-request count is
//! preserved in `sessions.extras`.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::pr::{append_github_pr_links, looks_like_final_pr_text};
use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider, expand_home};
use crate::parser::{ModelSessionStats, stream_records};
use crate::types::TokenUsage;

pub struct CopilotProvider {
    base_dir: PathBuf,
}

impl CopilotProvider {
    /// Provider rooted at `$CLAUDEX_COPILOT_DIR`, default `~/.copilot`.
    pub fn new() -> Result<Self> {
        let base_dir = match std::env::var("CLAUDEX_COPILOT_DIR") {
            Ok(v) if !v.trim().is_empty() => expand_home(v.trim())?,
            _ => {
                let home = dirs::home_dir().context("could not find home directory")?;
                home.join(".copilot")
            }
        };
        Ok(Self { base_dir })
    }

    /// Provider rooted at an explicit `.copilot` directory (tests).
    pub fn at(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SessionProvider for CopilotProvider {
    fn id(&self) -> &'static str {
        "copilot"
    }

    fn root_dir(&self) -> &Path {
        &self.base_dir
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let mut files = Vec::new();
        let sessions_dir = self.base_dir.join("session-state");
        if !sessions_dir.exists() {
            return Ok(files);
        }
        for entry in std::fs::read_dir(&sessions_dir)
            .with_context(|| format!("reading {}", sessions_dir.display()))?
        {
            let entry = entry?;
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            // Each session directory also holds checkpoints/, files/, research/
            // and a session.db; events.jsonl is the authoritative transcript.
            // Directories without one (created but never run) are skipped.
            let events = dir.join("events.jsonl");
            if events.is_file() {
                files.push(DiscoveredFile {
                    path: events,
                    // The transcript records its cwd in session.start; `parse`
                    // fills the real project, this is only a fallback.
                    project_display: String::new(),
                    parent_session_id: None,
                    archived: false,
                    source_key: None,
                });
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        parse_copilot_session(&file.path)
    }
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn parse_copilot_session(path: &Path) -> Result<ProviderRecord> {
    let mut entry = ProviderRecord::default();
    let mut cwd: Option<String> = None;
    let mut copilot_version: Option<String> = None;
    let mut repository: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut current_model: Option<String> = None;
    // Shutdown metrics are cumulative, so only the last event counts (a
    // resumed session appends a fresh shutdown to the same log).
    let mut shutdown: Option<Value> = None;
    let mut turn_starts: HashMap<String, i64> = HashMap::new();
    let mut pr_links_seen = BTreeSet::new();

    stream_records(path, |record| {
        let timestamp = record["timestamp"].as_str();
        let parsed_ts = timestamp.and_then(parse_ts);
        let timestamp_ms = parsed_ts.map(|dt| dt.timestamp_millis());
        if let Some(ts) = parsed_ts {
            if entry.first_timestamp.is_none_or(|p| ts < p) {
                entry.first_timestamp = Some(ts);
            }
            if entry.last_timestamp.is_none_or(|p| ts > p) {
                entry.last_timestamp = Some(ts);
            }
        }
        let data = &record["data"];

        match record["type"].as_str() {
            Some("session.start") => {
                if let Some(id) = data["sessionId"].as_str() {
                    entry.session_id = Some(id.to_string());
                }
                set_if_present(&mut cwd, data["context"]["cwd"].as_str());
                set_if_present(&mut copilot_version, data["copilotVersion"].as_str());
                set_if_present(&mut repository, data["context"]["repository"].as_str());
                set_if_present(&mut branch, data["context"]["branch"].as_str());
                set_if_present(&mut current_model, data["selectedModel"].as_str());
            }
            Some("session.model_change") => {
                set_if_present(&mut current_model, data["newModel"].as_str());
            }
            Some("user.message") => {
                entry.message_count += 1;
                // `content` is what the user typed; `transformedContent` has
                // injected reminders/context and would pollute search.
                if let Some(text) = data["content"].as_str()
                    && !text.is_empty()
                {
                    entry.messages.push(MessageForFts {
                        msg_type: "user".to_string(),
                        content: text.to_string(),
                        timestamp_ms,
                    });
                }
            }
            Some("assistant.message") => {
                entry.message_count += 1;
                set_if_present(&mut current_model, data["model"].as_str());
                if let Some(text) = data["content"].as_str()
                    && !text.is_empty()
                {
                    entry.messages.push(MessageForFts {
                        msg_type: "assistant".to_string(),
                        content: text.to_string(),
                        timestamp_ms,
                    });
                    if looks_like_final_pr_text(text) {
                        append_github_pr_links(&mut entry, &mut pr_links_seen, text, timestamp);
                    }
                }
                if data["reasoningText"]
                    .as_str()
                    .is_some_and(|t| !t.is_empty())
                {
                    entry.thinking_block_count += 1;
                }
                if let Some(requests) = data["toolRequests"].as_array() {
                    for request in requests {
                        if let Some(name) = request["name"].as_str()
                            && !name.is_empty()
                        {
                            entry.tool_names.push(name.to_string());
                        }
                    }
                }
            }
            Some("assistant.turn_start") => {
                if let (Some(turn_id), Some(ms)) = (data["turnId"].as_str(), timestamp_ms) {
                    turn_starts.insert(turn_id.to_string(), ms);
                }
            }
            Some("assistant.turn_end") => {
                if let (Some(start_ms), Some(end_ms), Some(ts)) = (
                    data["turnId"]
                        .as_str()
                        .and_then(|id| turn_starts.remove(id)),
                    timestamp_ms,
                    timestamp,
                ) {
                    entry.turn_durations.push((
                        end_ms.saturating_sub(start_ms).max(0) as u64,
                        ts.to_string(),
                    ));
                }
            }
            Some("abort") => {
                *entry
                    .stop_reason_counts
                    .entry("abort".to_string())
                    .or_insert(0) += 1;
            }
            Some("session.compaction_complete") => {
                *entry
                    .stop_reason_counts
                    .entry("context_compacted".to_string())
                    .or_insert(0) += 1;
            }
            Some("session.shutdown") if data.is_object() => {
                shutdown = Some(data.clone());
            }
            _ => {}
        }
        true
    })?;

    entry.model = current_model;

    let mut premium_requests: Option<u64> = None;
    let mut api_duration_ms: Option<u64> = None;
    let mut lines_added: Option<u64> = None;
    let mut lines_removed: Option<u64> = None;
    if let Some(shutdown) = shutdown {
        apply_shutdown_metrics(&mut entry, &shutdown);
        premium_requests = shutdown["totalPremiumRequests"].as_u64();
        api_duration_ms = shutdown["totalApiDurationMs"].as_u64();
        lines_added = shutdown["codeChanges"]["linesAdded"].as_u64();
        lines_removed = shutdown["codeChanges"]["linesRemoved"].as_u64();
        if let Some(files) = shutdown["codeChanges"]["filesModified"].as_array() {
            entry.file_paths_modified = files
                .iter()
                .filter_map(|f| f.as_str())
                .filter(|f| !f.is_empty())
                .map(str::to_string)
                .collect();
        }
    }

    // workspace.yaml is the sidecar metadata file; the transcript is preferred
    // for everything it carries, the sidecar fills cwd for malformed logs and
    // contributes the session title / client name.
    let sidecar = path
        .parent()
        .map(|dir| read_workspace_sidecar(&dir.join("workspace.yaml")))
        .unwrap_or_default();
    if cwd.is_none() {
        cwd = sidecar.get("cwd").cloned();
    }
    if repository.is_none() {
        repository = sidecar.get("repository").cloned();
    }
    if branch.is_none() {
        branch = sidecar.get("branch").cloned();
    }

    if entry.session_id.is_none() {
        // Sessions live in `session-state/<uuid>/events.jsonl`.
        entry.session_id = path
            .parent()
            .and_then(|dir| dir.file_name())
            .map(|name| name.to_string_lossy().into_owned());
    }
    entry.project_display = cwd.unwrap_or_else(|| "unknown".to_string());

    let mut extras = serde_json::Map::new();
    for (k, v) in [
        ("copilot_version", copilot_version),
        ("repository", repository),
        ("branch", branch),
        ("name", sidecar.get("name").cloned()),
        ("client_name", sidecar.get("client_name").cloned()),
    ] {
        if let Some(v) = v {
            extras.insert(k.to_string(), json!(v));
        }
    }
    for (k, v) in [
        ("premium_requests", premium_requests),
        ("api_duration_ms", api_duration_ms),
        ("lines_added", lines_added),
        ("lines_removed", lines_removed),
    ] {
        if let Some(v) = v {
            extras.insert(k.to_string(), json!(v));
        }
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

/// Map the last `session.shutdown`'s cumulative per-model metrics onto the
/// record. Copilot's `usage.inputTokens` INCLUDES the cached prompt portion;
/// the non-cache input is `tokenDetails.input.tokenCount` when present (newer
/// CLIs), else derived by subtracting the cache counters.
fn apply_shutdown_metrics(entry: &mut ProviderRecord, shutdown: &Value) {
    let Some(models) = shutdown["modelMetrics"].as_object() else {
        return;
    };
    entry.model_usage.clear();
    for (model, metrics) in models {
        if model.is_empty() {
            continue;
        }
        let usage = &metrics["usage"];
        let cache_read = usage["cacheReadTokens"].as_u64().unwrap_or(0);
        let cache_write = usage["cacheWriteTokens"].as_u64().unwrap_or(0);
        let input = match metrics["tokenDetails"]["input"]["tokenCount"].as_u64() {
            Some(n) => n,
            None => usage["inputTokens"]
                .as_u64()
                .unwrap_or(0)
                .saturating_sub(cache_read)
                .saturating_sub(cache_write),
        };
        let stats = ModelSessionStats {
            usage: TokenUsage {
                input_tokens: input,
                output_tokens: usage["outputTokens"].as_u64().unwrap_or(0),
                cache_creation_tokens: cache_write,
                cache_read_tokens: cache_read,
            },
            assistant_message_count: metrics["requests"]["count"].as_u64().unwrap_or(0),
            ..Default::default()
        };
        entry.usage.input_tokens += stats.usage.input_tokens;
        entry.usage.output_tokens += stats.usage.output_tokens;
        entry.usage.cache_creation_tokens += stats.usage.cache_creation_tokens;
        entry.usage.cache_read_tokens += stats.usage.cache_read_tokens;
        entry.model_usage.insert(model.clone(), stats);
    }
}

/// Pull the handful of flat `key: value` lines claudex needs out of
/// `workspace.yaml`. The file is machine-written with scalar values only, so a
/// real YAML parser would be a dependency for nothing; values are optionally
/// double-quoted (titles with special characters).
fn read_workspace_sidecar(path: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if !matches!(
            key,
            "cwd" | "name" | "repository" | "branch" | "client_name"
        ) {
            continue;
        }
        let value = unquote_yaml_scalar(value.trim());
        if !value.is_empty() {
            out.insert(key.to_string(), value);
        }
    }
    out
}

fn unquote_yaml_scalar(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        value.to_string()
    }
}

fn set_if_present(slot: &mut Option<String>, value: Option<&str>) {
    if let Some(v) = value
        && !v.is_empty()
    {
        *slot = Some(v.to_string());
    }
}

//! OpenAI Codex CLI provider: indexes the JSONL rollout transcripts under
//! `~/.codex/sessions/` and `~/.codex/archived_sessions/`. Codex stores its
//! project (`cwd`), model, and cumulative token counts inside the transcript,
//! so [`CodexProvider::parse`] reads them from the file rather than the path.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::pr::{append_github_pr_links, looks_like_final_pr_text, looks_like_gh_pr_command};
use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider};
use crate::parser::stream_records;

pub struct CodexProvider {
    base_dir: PathBuf,
}

impl CodexProvider {
    /// Provider rooted at the user's `~/.codex`.
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        Ok(Self {
            base_dir: home.join(".codex"),
        })
    }

    /// Provider rooted at an explicit `.codex` directory (tests).
    pub fn at(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SessionProvider for CodexProvider {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn root_dir(&self) -> &Path {
        &self.base_dir
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let mut files = Vec::new();
        collect_jsonl(&self.base_dir.join("sessions"), false, &mut files)?;
        collect_jsonl(&self.base_dir.join("archived_sessions"), true, &mut files)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        parse_codex_session(&file.path)
    }
}

fn collect_jsonl(dir: &Path, archived: bool, out: &mut Vec<DiscoveredFile>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, archived, out)?;
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(DiscoveredFile {
                path,
                // Codex records its cwd inside the transcript; `parse` fills the
                // real project, this is only a fallback for malformed files.
                project_display: String::new(),
                parent_session_id: None,
                archived,
            });
        }
    }
    Ok(())
}

/// Recover the session UUID from a `rollout-<ts>-<uuid>.jsonl` filename.
pub fn session_id_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    if stem.starts_with("rollout-") && stem.len() >= 36 {
        return Some(stem[stem.len() - 36..].to_string());
    }
    stem.rsplit_once('-').map(|(_, id)| id.to_string())
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

/// Codex's `total_token_usage` is a running cumulative total, so we keep only
/// the last one observed and map it onto claudex's token model: the cached
/// portion of the prompt is billed as a cache read, the rest as fresh input.
struct CodexTokens {
    input: u64,
    cached_input: u64,
    output: u64,
}

fn parse_codex_session(path: &Path) -> Result<ProviderRecord> {
    let mut entry = ProviderRecord::default();
    let mut cwd: Option<String> = None;
    let mut cli_version: Option<String> = None;
    let mut originator: Option<String> = None;
    let mut source: Option<String> = None;
    let mut model_provider: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut last_tokens: Option<CodexTokens> = None;
    let mut pr_links_seen = BTreeSet::new();
    let mut gh_pr_call_ids = BTreeSet::new();

    stream_records(path, |record| {
        let timestamp = record["timestamp"].as_str();
        let timestamp_ms = timestamp.and_then(|ts| parse_ts(ts).map(|dt| dt.timestamp_millis()));
        if let Some(ts) = timestamp.and_then(parse_ts) {
            if entry.first_timestamp.is_none_or(|p| ts < p) {
                entry.first_timestamp = Some(ts);
            }
            if entry.last_timestamp.is_none_or(|p| ts > p) {
                entry.last_timestamp = Some(ts);
            }
        }

        match record["type"].as_str() {
            Some("session_meta") => {
                let p = &record["payload"];
                if let Some(id) = p["id"].as_str() {
                    entry.session_id = Some(id.to_string());
                }
                set_if_present(&mut cwd, p["cwd"].as_str());
                set_if_present(&mut cli_version, p["cli_version"].as_str());
                set_if_present(&mut originator, p["originator"].as_str());
                set_if_present(&mut source, p["source"].as_str());
                set_if_present(&mut model_provider, p["model_provider"].as_str());
                set_if_present(&mut git_branch, p["git"]["branch"].as_str());
            }
            Some("turn_context") => {
                if entry.model.is_none()
                    && let Some(m) = record["payload"]["model"].as_str()
                    && !m.is_empty()
                {
                    entry.model = Some(m.to_string());
                }
                set_if_present(&mut cwd, record["payload"]["cwd"].as_str());
            }
            Some("response_item") | Some("event_msg") => {
                handle_payload(
                    &mut entry,
                    &mut last_tokens,
                    &mut pr_links_seen,
                    &mut gh_pr_call_ids,
                    &record["payload"],
                    timestamp,
                    timestamp_ms,
                );
            }
            Some("message") => {
                handle_payload(
                    &mut entry,
                    &mut last_tokens,
                    &mut pr_links_seen,
                    &mut gh_pr_call_ids,
                    record,
                    timestamp,
                    timestamp_ms,
                );
            }
            Some("compacted") => {
                *entry
                    .stop_reason_counts
                    .entry("context_compacted".to_string())
                    .or_insert(0) += 1;
            }
            Some("reasoning") => entry.thinking_block_count += 1,
            _ => {}
        }
        true
    })?;

    if entry.session_id.is_none() {
        entry.session_id = session_id_from_path(path);
    }
    entry.project_display = cwd.clone().unwrap_or_else(|| "unknown".to_string());

    if let Some(t) = last_tokens {
        entry.usage.input_tokens = t.input.saturating_sub(t.cached_input);
        entry.usage.cache_read_tokens = t.cached_input;
        entry.usage.output_tokens = t.output;
        entry.usage.cache_creation_tokens = 0;
    }

    // Stash provider-specific metadata for `sessions.extras`.
    let mut extras = serde_json::Map::new();
    for (k, v) in [
        ("cli_version", cli_version),
        ("originator", originator),
        ("source", source),
        ("model_provider", model_provider),
        ("git_branch", git_branch),
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

fn handle_payload(
    entry: &mut ProviderRecord,
    last_tokens: &mut Option<CodexTokens>,
    pr_links_seen: &mut BTreeSet<String>,
    gh_pr_call_ids: &mut BTreeSet<String>,
    payload: &Value,
    timestamp: Option<&str>,
    timestamp_ms: Option<i64>,
) {
    match payload["type"].as_str() {
        Some("token_count") => {
            let usage = &payload["info"]["total_token_usage"];
            if usage.is_object() {
                *last_tokens = Some(CodexTokens {
                    input: usage["input_tokens"].as_u64().unwrap_or(0),
                    cached_input: usage["cached_input_tokens"].as_u64().unwrap_or(0),
                    output: usage["output_tokens"].as_u64().unwrap_or(0),
                });
            }
        }
        Some("message") => {
            let role = payload["role"].as_str().unwrap_or("");
            if role == "user" || role == "assistant" {
                entry.message_count += 1;
                if let Some(text) = message_text(payload)
                    && !text.is_empty()
                {
                    entry.messages.push(MessageForFts {
                        msg_type: role.to_string(),
                        content: text.clone(),
                        timestamp_ms,
                    });
                    if role == "assistant" && looks_like_final_pr_text(&text) {
                        append_github_pr_links(entry, pr_links_seen, &text, timestamp);
                    }
                }
            }
        }
        Some("user_message") => {
            entry.message_count += 1;
            if let Some(text) = payload["message"].as_str()
                && !text.is_empty()
            {
                entry.messages.push(MessageForFts {
                    msg_type: "user".to_string(),
                    content: text.to_string(),
                    timestamp_ms,
                });
            }
        }
        Some("agent_message") => {
            entry.message_count += 1;
            if let Some(text) = payload["message"].as_str()
                && !text.is_empty()
            {
                entry.messages.push(MessageForFts {
                    msg_type: "assistant".to_string(),
                    content: text.to_string(),
                    timestamp_ms,
                });
                if looks_like_final_pr_text(text) {
                    append_github_pr_links(entry, pr_links_seen, text, timestamp);
                }
            }
        }
        Some("reasoning") | Some("agent_reasoning") => entry.thinking_block_count += 1,
        Some("function_call") | Some("custom_tool_call") => {
            push_tool(entry, payload["name"].as_str().unwrap_or("tool"));
            if payload_command_mentions_gh_pr(payload)
                && let Some(call_id) = payload["call_id"]
                    .as_str()
                    .or_else(|| payload["id"].as_str())
            {
                gh_pr_call_ids.insert(call_id.to_string());
            }
        }
        Some("web_search_call") => push_tool(entry, "web_search"),
        Some("exec_command_end") => {
            push_tool(entry, "exec");
            if payload_command_mentions_gh_pr(payload) {
                append_prs_from_fields(entry, pr_links_seen, payload, timestamp);
            }
        }
        Some("function_call_output") | Some("custom_tool_call_output") => {
            let call_id = payload["call_id"]
                .as_str()
                .or_else(|| payload["id"].as_str());
            let output = output_text(payload);
            let trusted = call_id.is_some_and(|id| gh_pr_call_ids.contains(id));
            if trusted && let Some(text) = output {
                append_github_pr_links(entry, pr_links_seen, &text, timestamp);
            }
        }
        Some("patch_apply_end") => push_tool(entry, "apply_patch"),
        Some("turn_aborted") => {
            *entry
                .stop_reason_counts
                .entry("turn_aborted".to_string())
                .or_insert(0) += 1;
        }
        Some("context_compacted") => {
            *entry
                .stop_reason_counts
                .entry("context_compacted".to_string())
                .or_insert(0) += 1;
        }
        Some("entered_review_mode") | Some("exited_review_mode") => {
            *entry
                .stop_reason_counts
                .entry("review".to_string())
                .or_insert(0) += 1;
        }
        _ => {}
    }
}

fn push_tool(entry: &mut ProviderRecord, name: &str) {
    if !name.is_empty() {
        entry.tool_names.push(name.to_string());
    }
}

/// Concatenate the text of a Codex `message` payload's content blocks.
fn message_text(payload: &Value) -> Option<String> {
    if let Some(s) = payload["message"].as_str() {
        return Some(s.to_string());
    }
    let content = payload["content"].as_array()?;
    let parts: Vec<&str> = content
        .iter()
        .filter_map(|b| b["text"].as_str().filter(|t| !t.is_empty()))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn set_if_present(slot: &mut Option<String>, value: Option<&str>) {
    if let Some(v) = value
        && !v.is_empty()
    {
        *slot = Some(v.to_string());
    }
}

fn append_prs_from_fields(
    entry: &mut ProviderRecord,
    pr_links_seen: &mut BTreeSet<String>,
    payload: &Value,
    timestamp: Option<&str>,
) {
    for field in ["stdout", "stderr", "aggregated_output", "output"] {
        if let Some(text) = payload[field].as_str() {
            append_github_pr_links(entry, pr_links_seen, text, timestamp);
        }
    }
}

fn output_text(payload: &Value) -> Option<String> {
    for field in ["output", "stdout", "stderr", "aggregated_output"] {
        if let Some(text) = payload[field].as_str()
            && !text.is_empty()
        {
            return Some(text.to_string());
        }
    }
    None
}

fn payload_command_mentions_gh_pr(payload: &Value) -> bool {
    ["command", "cmd", "parsed_cmd", "arguments", "args", "input"]
        .iter()
        .any(|field| value_mentions_gh_pr(&payload[*field]))
}

fn value_mentions_gh_pr(value: &Value) -> bool {
    match value {
        Value::String(s) => text_mentions_gh_pr(s),
        Value::Array(items) => {
            let joined = items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            text_mentions_gh_pr(&joined) || items.iter().any(value_mentions_gh_pr)
        }
        Value::Object(map) => map.values().any(value_mentions_gh_pr),
        _ => false,
    }
}

fn text_mentions_gh_pr(text: &str) -> bool {
    looks_like_gh_pr_command(text)
}

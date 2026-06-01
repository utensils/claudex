//! OpenClaw provider: indexes session transcripts under
//! `$OPENCLAW_STATE_DIR/agents/*/sessions/` (default `~/.openclaw`) and merges
//! runtime trajectory sidecars when present.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::pr::{append_github_pr_links, looks_like_final_pr_text, looks_like_gh_pr_command};
use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider};
use crate::parser::stream_records;

pub struct OpenClawProvider {
    state_dir: PathBuf,
    trajectory_dir: Option<PathBuf>,
}

impl OpenClawProvider {
    pub fn new() -> Result<Self> {
        let state_dir = match std::env::var("OPENCLAW_STATE_DIR") {
            Ok(v) if !v.trim().is_empty() => expand_home(v.trim())?,
            _ => {
                let home = dirs::home_dir().context("could not find home directory")?;
                home.join(".openclaw")
            }
        };
        let trajectory_dir = std::env::var("OPENCLAW_TRAJECTORY_DIR")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| expand_home(v.trim()))
            .transpose()?;
        Ok(Self {
            state_dir,
            trajectory_dir,
        })
    }

    pub fn at(state_dir: PathBuf) -> Self {
        Self {
            state_dir,
            trajectory_dir: None,
        }
    }

    pub fn at_with_trajectory_dir(state_dir: PathBuf, trajectory_dir: PathBuf) -> Self {
        Self {
            state_dir,
            trajectory_dir: Some(trajectory_dir),
        }
    }
}

impl SessionProvider for OpenClawProvider {
    fn id(&self) -> &'static str {
        "openclaw"
    }

    fn root_dir(&self) -> &Path {
        &self.state_dir
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let mut files = Vec::new();
        let agents_dir = self.state_dir.join("agents");
        if !agents_dir.exists() {
            return Ok(files);
        }

        for agent in std::fs::read_dir(&agents_dir)
            .with_context(|| format!("reading {}", agents_dir.display()))?
        {
            let agent = agent?;
            let agent_dir = agent.path();
            if !agent_dir.is_dir() {
                continue;
            }
            let agent_id = agent.file_name().to_string_lossy().into_owned();
            let sessions_dir = agent_dir.join("sessions");
            if !sessions_dir.exists() {
                continue;
            }
            let store = read_sessions_store(&sessions_dir.join("sessions.json"));
            let mut classic_ids = BTreeSet::new();
            let mut dir_entries = Vec::new();
            for entry in std::fs::read_dir(&sessions_dir)
                .with_context(|| format!("reading {}", sessions_dir.display()))?
            {
                let path = entry?.path();
                if path.is_file() {
                    dir_entries.push(path);
                }
            }
            dir_entries.sort();

            for path in &dir_entries {
                let name = file_name(path);
                if let Some((session_id, archived)) = usage_counted_session_id(&name) {
                    classic_ids.insert(session_id.clone());
                    files.push(discovered(path.clone(), &agent_id, &session_id, archived));
                }
            }

            for path in &dir_entries {
                let name = file_name(path);
                if let Some(session_id) = trajectory_session_id(&name)
                    && !classic_ids.contains(&session_id)
                {
                    files.push(discovered(path.clone(), &agent_id, &session_id, false));
                }
            }

            if let Some(trajectory_dir) = &self.trajectory_dir
                && trajectory_dir.exists()
            {
                for (session_key, entry) in &store.entries {
                    let Some(session_id) = entry["sessionId"].as_str() else {
                        continue;
                    };
                    if classic_ids.contains(session_id) {
                        continue;
                    }
                    let path = trajectory_dir
                        .join(format!("{}.jsonl", safe_session_file_name(session_id)));
                    if path.exists() {
                        let mut f = discovered(path, &agent_id, session_id, false);
                        f.project_display = entry["workspaceDir"]
                            .as_str()
                            .or_else(|| entry["cwd"].as_str())
                            .unwrap_or(session_key)
                            .to_string();
                        files.push(f);
                    }
                }
            }
        }

        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        parse_openclaw_session(&file.path, &self.state_dir, self.trajectory_dir.as_deref())
    }
}

fn expand_home(value: &str) -> Result<PathBuf> {
    if let Some(rest) = value.strip_prefix("~/") {
        let home = dirs::home_dir().context("could not find home directory")?;
        Ok(home.join(rest))
    } else if value == "~" {
        dirs::home_dir().context("could not find home directory")
    } else {
        Ok(PathBuf::from(value))
    }
}

fn discovered(path: PathBuf, agent_id: &str, session_id: &str, archived: bool) -> DiscoveredFile {
    DiscoveredFile {
        path,
        project_display: String::new(),
        parent_session_id: None,
        archived,
        source_key: Some(source_key(agent_id, session_id)),
    }
}

fn source_key(agent_id: &str, session_id: &str) -> String {
    format!("agent:{agent_id}:session:{session_id}")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn usage_counted_session_id(name: &str) -> Option<(String, bool)> {
    if name == "sessions.json"
        || !name.contains(".jsonl")
        || name.ends_with(".trajectory.jsonl")
        || name.ends_with(".trajectory-path.json")
        || name.contains(".checkpoint.")
        || name.contains(".bak.")
    {
        return None;
    }
    if let Some(id) = name.strip_suffix(".jsonl") {
        return valid_session_id(id).then(|| (id.to_string(), false));
    }
    for marker in [".jsonl.deleted.", ".jsonl.reset."] {
        if let Some((id, ts)) = name.split_once(marker)
            && valid_session_id(id)
            && looks_like_archive_timestamp(ts)
        {
            return Some((id.to_string(), true));
        }
    }
    None
}

fn trajectory_session_id(name: &str) -> Option<String> {
    let id = name.strip_suffix(".trajectory.jsonl")?;
    valid_session_id(id).then(|| id.to_string())
}

fn valid_session_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn looks_like_archive_timestamp(ts: &str) -> bool {
    let bytes = ts.as_bytes();
    bytes.len() >= 20
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && ts.ends_with('Z')
}

fn safe_session_file_name(session_id: &str) -> String {
    let safe: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    if safe.chars().any(|c| c.is_ascii_alphanumeric()) {
        safe
    } else {
        "session".to_string()
    }
}

#[derive(Default)]
struct SessionStoreMeta {
    entries: BTreeMap<String, Value>,
}

fn read_sessions_store(path: &Path) -> SessionStoreMeta {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return SessionStoreMeta::default();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) else {
        return SessionStoreMeta::default();
    };
    SessionStoreMeta {
        entries: map.into_iter().collect(),
    }
}

fn parse_openclaw_session(
    path: &Path,
    state_dir: &Path,
    trajectory_dir: Option<&Path>,
) -> Result<ProviderRecord> {
    let context = FileContext::from_path(path, state_dir);
    let store = context
        .sessions_dir
        .as_ref()
        .map(|dir| read_sessions_store(&dir.join("sessions.json")))
        .unwrap_or_default();
    let is_classic = usage_counted_session_id(&file_name(path)).is_some();

    let mut entry = if is_classic {
        parse_classic(path)?
    } else {
        parse_trajectory(path)?
    };

    let session_id = entry.session_id.clone().or(context.session_id);
    if let Some(session_id) = &session_id {
        entry.session_id = Some(session_id.clone());
        if let Some(agent_id) = &context.agent_id {
            entry.source_key = Some(source_key(agent_id, session_id));
        }
    }

    enrich_from_store(&mut entry, &store, context.agent_id.as_deref());

    if is_classic && let Some(session_id) = entry.session_id.as_deref() {
        for sidecar in trajectory_paths(path, session_id, trajectory_dir) {
            if sidecar.exists() {
                let trajectory = parse_trajectory(&sidecar)?;
                merge_trajectory_metadata(&mut entry, trajectory, &sidecar);
            }
        }
    }

    if entry.duration_ms == 0
        && let (Some(first), Some(last)) = (entry.first_timestamp, entry.last_timestamp)
    {
        entry.duration_ms = last.signed_duration_since(first).num_milliseconds().max(0) as u64;
    }

    Ok(entry)
}

struct FileContext {
    agent_id: Option<String>,
    sessions_dir: Option<PathBuf>,
    session_id: Option<String>,
}

impl FileContext {
    fn from_path(path: &Path, state_dir: &Path) -> Self {
        let name = file_name(path);
        let session_id = usage_counted_session_id(&name)
            .map(|(id, _)| id)
            .or_else(|| trajectory_session_id(&name));
        let parts: Vec<String> = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let mut agent_id = None;
        let mut sessions_dir = None;
        for window in parts.windows(3) {
            if window[0] == "agents" && window[2] == "sessions" {
                agent_id = Some(window[1].clone());
                sessions_dir = path.parent().map(Path::to_path_buf);
                break;
            }
        }
        if sessions_dir.is_none() && path.starts_with(state_dir) {
            sessions_dir = path.parent().map(Path::to_path_buf);
        }
        Self {
            agent_id,
            sessions_dir,
            session_id,
        }
    }
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

#[derive(Default)]
struct ParseState {
    total_cost: f64,
    saw_cost: bool,
    pr_links_seen: BTreeSet<String>,
    gh_pr_tool_call_ids: BTreeSet<String>,
}

fn parse_classic(path: &Path) -> Result<ProviderRecord> {
    let mut entry = ProviderRecord::default();
    let mut current_model: Option<String> = None;
    let mut state = ParseState::default();

    stream_records(path, |record| {
        update_timestamps(&mut entry, record["timestamp"].as_str());
        let ts_ms = record["timestamp"]
            .as_str()
            .and_then(|t| parse_ts(t).map(|d| d.timestamp_millis()));

        match record["type"].as_str() {
            Some("session") => {
                set_session_fields(&mut entry, record);
            }
            Some("model_change") => {
                let provider = record["provider"].as_str().unwrap_or("");
                let model = record["modelId"].as_str().unwrap_or("");
                if !model.is_empty() {
                    current_model = Some(model_key(provider, model));
                    entry.model = current_model.clone();
                }
            }
            Some("message") => {
                handle_classic_message(
                    &mut entry,
                    &record["message"],
                    current_model.as_deref(),
                    &mut state,
                    record["timestamp"].as_str(),
                    ts_ms,
                );
            }
            _ => {}
        }
        true
    })?;

    if state.saw_cost {
        entry.embedded_cost = Some(state.total_cost);
    }
    Ok(entry)
}

fn set_session_fields(entry: &mut ProviderRecord, record: &Value) {
    if let Some(id) = record["id"].as_str() {
        entry.session_id = Some(id.to_string());
    }
    for key in ["cwd", "workspaceDir", "workspace"] {
        if let Some(cwd) = record[key].as_str().filter(|s| !s.is_empty()) {
            entry.project_display = cwd.to_string();
            break;
        }
    }
}

fn update_timestamps(entry: &mut ProviderRecord, timestamp: Option<&str>) {
    if let Some(ts) = timestamp.and_then(parse_ts) {
        if entry.first_timestamp.is_none_or(|p| ts < p) {
            entry.first_timestamp = Some(ts);
        }
        if entry.last_timestamp.is_none_or(|p| ts > p) {
            entry.last_timestamp = Some(ts);
        }
    }
}

fn handle_classic_message(
    entry: &mut ProviderRecord,
    msg: &Value,
    fallback_model: Option<&str>,
    state: &mut ParseState,
    timestamp: Option<&str>,
    ts_ms: Option<i64>,
) {
    match msg["role"].as_str() {
        Some("user") => {
            entry.message_count += 1;
            push_message(entry, "user", content_any_text(&msg["content"]), ts_ms);
        }
        Some("assistant") => {
            entry.message_count += 1;
            accumulate_assistant(entry, msg, fallback_model, state, timestamp, ts_ms);
        }
        Some("toolResult") => {
            if tool_result_matches(&state.gh_pr_tool_call_ids, msg)
                && let Some(text) = content_any_text(&msg["content"])
            {
                append_github_pr_links(entry, &mut state.pr_links_seen, &text, timestamp);
            }
        }
        Some("bashExecution") => {
            if message_command_mentions_gh_pr(msg)
                && let Some(text) = content_any_text(&msg["content"]).or_else(|| output_text(msg))
            {
                append_github_pr_links(entry, &mut state.pr_links_seen, &text, timestamp);
            }
        }
        _ => {}
    }
}

fn accumulate_assistant(
    entry: &mut ProviderRecord,
    msg: &Value,
    fallback_model: Option<&str>,
    state: &mut ParseState,
    timestamp: Option<&str>,
    ts_ms: Option<i64>,
) {
    if let Some(content) = msg["content"].as_array() {
        let mut text_parts = Vec::new();
        for block in content {
            match block["type"].as_str() {
                Some("toolCall") => {
                    if let Some(name) = block["name"].as_str().filter(|s| !s.is_empty()) {
                        entry.tool_names.push(name.to_string());
                    }
                    if block["name"].as_str().is_some_and(is_shell_tool)
                        && value_mentions_gh_pr(&block["arguments"])
                        && let Some(id) = block["id"].as_str()
                    {
                        state.gh_pr_tool_call_ids.insert(id.to_string());
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
            let text = text_parts.join(" ");
            push_message(entry, "assistant", Some(text.clone()), ts_ms);
            if looks_like_final_pr_text(&text) {
                append_github_pr_links(entry, &mut state.pr_links_seen, &text, timestamp);
            }
        }
    }

    if let Some(stop) = msg["stopReason"].as_str().filter(|s| !s.is_empty()) {
        *entry
            .stop_reason_counts
            .entry(stop.to_string())
            .or_insert(0) += 1;
    }

    let provider = msg["provider"].as_str().unwrap_or("");
    let model = msg["model"].as_str().unwrap_or("");
    let key = if model.is_empty() {
        fallback_model.unwrap_or("unknown").to_string()
    } else {
        model_key(provider, model)
    };
    entry.model = Some(key.clone());
    let usage = &msg["usage"];
    if usage.is_object() {
        let stats = entry.model_usage.entry(key).or_default();
        stats.usage.input_tokens += usage["input"].as_u64().unwrap_or(0);
        stats.usage.output_tokens += usage["output"].as_u64().unwrap_or(0);
        stats.usage.cache_read_tokens += usage["cacheRead"].as_u64().unwrap_or(0);
        stats.usage.cache_creation_tokens += usage["cacheWrite"].as_u64().unwrap_or(0);
        stats.assistant_message_count += 1;
        if let Some(cost) = usage["cost"]["total"].as_f64() {
            state.total_cost += cost;
            state.saw_cost = true;
            stats.embedded_cost = Some(stats.embedded_cost.unwrap_or(0.0) + cost);
        }
    }
}

fn parse_trajectory(path: &Path) -> Result<ProviderRecord> {
    let mut entry = ProviderRecord::default();
    let mut state = ParseState::default();

    stream_records(path, |event| {
        update_timestamps(&mut entry, event["ts"].as_str());
        let ts_ms = event["ts"]
            .as_str()
            .and_then(|t| parse_ts(t).map(|d| d.timestamp_millis()));

        if let Some(session_id) = event["sessionId"].as_str().filter(|s| !s.is_empty()) {
            entry.session_id = Some(session_id.to_string());
        }
        if let Some(workspace) = event["workspaceDir"].as_str().filter(|s| !s.is_empty()) {
            entry.project_display = workspace.to_string();
        }
        if let Some(model) = trajectory_model_key(event).filter(|s| !s.is_empty()) {
            entry.model = Some(model);
        }

        match event["type"].as_str() {
            Some("prompt.submitted") => {
                entry.message_count += 1;
                push_message(
                    &mut entry,
                    "user",
                    text_from_fields(&event["data"], &["prompt"]),
                    ts_ms,
                );
            }
            Some("tool.call") => {
                if let Some(name) = event["data"]["name"].as_str().filter(|s| !s.is_empty()) {
                    entry.tool_names.push(name.to_string());
                    if is_shell_tool(name)
                        && value_mentions_gh_pr(&event["data"])
                        && let Some(id) = event["entryId"]
                            .as_str()
                            .or_else(|| event["data"]["id"].as_str())
                    {
                        state.gh_pr_tool_call_ids.insert(id.to_string());
                    }
                }
            }
            Some("tool.result") => {
                if trajectory_tool_result_matches(&state.gh_pr_tool_call_ids, event)
                    && let Some(text) =
                        text_from_fields(&event["data"], &["output", "text", "result"])
                {
                    append_github_pr_links(
                        &mut entry,
                        &mut state.pr_links_seen,
                        &text,
                        event["ts"].as_str(),
                    );
                }
            }
            Some("model.completed") | Some("trace.artifacts") => {
                handle_trajectory_completion(&mut entry, event, &mut state, ts_ms);
            }
            Some("model.fallback_step") => {
                *entry
                    .stop_reason_counts
                    .entry("model_fallback_step".to_string())
                    .or_insert(0) += 1;
            }
            Some("session.ended") => {
                if let Some(status) = event["data"]["status"].as_str().filter(|s| !s.is_empty()) {
                    *entry
                        .stop_reason_counts
                        .entry(status.to_string())
                        .or_insert(0) += 1;
                }
                for key in ["promptError", "terminalError"] {
                    if event["data"][key].as_str().is_some_and(|s| !s.is_empty()) {
                        *entry.stop_reason_counts.entry(key.to_string()).or_insert(0) += 1;
                    }
                }
            }
            Some("compaction") | Some("context.compacted") => {
                *entry
                    .stop_reason_counts
                    .entry("context_compacted".to_string())
                    .or_insert(0) += 1;
            }
            _ => {}
        }
        true
    })?;

    if state.saw_cost {
        entry.embedded_cost = Some(state.total_cost);
    }
    Ok(entry)
}

fn handle_trajectory_completion(
    entry: &mut ProviderRecord,
    event: &Value,
    state: &mut ParseState,
    ts_ms: Option<i64>,
) {
    let data = &event["data"];
    if let Some(text) = text_from_fields(data, &["assistantText", "assistant_text", "text"]) {
        entry.message_count += 1;
        push_message(entry, "assistant", Some(text.clone()), ts_ms);
        if looks_like_final_pr_text(&text) {
            append_github_pr_links(entry, &mut state.pr_links_seen, &text, event["ts"].as_str());
        }
    } else if let Some(parts) = data["assistantTexts"].as_array() {
        let text = parts
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            entry.message_count += 1;
            push_message(entry, "assistant", Some(text.clone()), ts_ms);
            if looks_like_final_pr_text(&text) {
                append_github_pr_links(
                    entry,
                    &mut state.pr_links_seen,
                    &text,
                    event["ts"].as_str(),
                );
            }
        }
    }

    if let Some(usage) = data.get("usage") {
        let model = trajectory_model_key(event).unwrap_or_else(|| "unknown".to_string());
        let stats = entry.model_usage.entry(model.clone()).or_default();
        stats.usage.input_tokens += usage["input"].as_u64().unwrap_or(0);
        stats.usage.output_tokens += usage["output"].as_u64().unwrap_or(0);
        stats.usage.cache_read_tokens += usage["cacheRead"].as_u64().unwrap_or(0);
        stats.usage.cache_creation_tokens += usage["cacheWrite"].as_u64().unwrap_or(0);
        stats.assistant_message_count += 1;
        if let Some(cost) = usage["cost"]["total"]
            .as_f64()
            .or_else(|| data["estimatedCostUsd"].as_f64())
        {
            state.total_cost += cost;
            state.saw_cost = true;
            stats.embedded_cost = Some(stats.embedded_cost.unwrap_or(0.0) + cost);
        }
        entry.model = Some(model);
    }
}

fn trajectory_model_key(event: &Value) -> Option<String> {
    let model = event["modelId"].as_str().filter(|s| !s.is_empty())?;
    let provider = event["provider"].as_str().unwrap_or("");
    Some(model_key(provider, model))
}

fn merge_trajectory_metadata(
    entry: &mut ProviderRecord,
    mut trajectory: ProviderRecord,
    path: &Path,
) {
    if entry.project_display.is_empty() {
        entry.project_display = std::mem::take(&mut trajectory.project_display);
    }
    if entry.model.is_none() {
        entry.model = trajectory.model.take();
    }
    if entry.first_timestamp.is_none_or(|first| {
        trajectory
            .first_timestamp
            .is_some_and(|trajectory_first| trajectory_first < first)
    }) {
        entry.first_timestamp = trajectory.first_timestamp;
    }
    if entry.last_timestamp.is_none_or(|last| {
        trajectory
            .last_timestamp
            .is_some_and(|trajectory_last| trajectory_last > last)
    }) {
        entry.last_timestamp = trajectory.last_timestamp;
    }
    if entry.messages.is_empty() {
        entry.messages = std::mem::take(&mut trajectory.messages);
    }
    if entry.message_count == 0 {
        entry.message_count = trajectory.message_count;
    }
    if entry.embedded_cost.is_none() && trajectory.embedded_cost.is_some() {
        entry.embedded_cost = trajectory.embedded_cost;
    }
    if entry.model_usage.is_empty() {
        entry.model_usage = std::mem::take(&mut trajectory.model_usage);
        entry.usage = std::mem::take(&mut trajectory.usage);
    } else {
        for (model, trajectory_stats) in &trajectory.model_usage {
            let Some(cost) = trajectory_stats.embedded_cost else {
                continue;
            };
            if let Some(stats) = entry.model_usage.get_mut(model)
                && stats.embedded_cost.is_none()
            {
                stats.embedded_cost = Some(cost);
            }
        }
    }
    if entry.tool_names.is_empty() {
        entry.tool_names = std::mem::take(&mut trajectory.tool_names);
    }
    if entry.pr_links.is_empty() {
        entry.pr_links = std::mem::take(&mut trajectory.pr_links);
    }
    if entry.file_paths_modified.is_empty() {
        entry.file_paths_modified = std::mem::take(&mut trajectory.file_paths_modified);
    }
    if entry.attachments.is_empty() {
        entry.attachments = std::mem::take(&mut trajectory.attachments);
    }
    if entry.permission_modes.is_empty() {
        entry.permission_modes = std::mem::take(&mut trajectory.permission_modes);
    }
    if entry.inference_geo.is_none() {
        entry.inference_geo = trajectory.inference_geo.take();
    }
    if entry.speed.is_none() {
        entry.speed = trajectory.speed;
    }
    if entry.service_tier.is_none() {
        entry.service_tier = trajectory.service_tier.take();
    }
    if entry.iterations == 0 {
        entry.iterations = trajectory.iterations;
    }
    for (reason, count) in trajectory.stop_reason_counts {
        *entry.stop_reason_counts.entry(reason).or_insert(0) += count;
    }
    if trajectory.thinking_block_count > 0 {
        entry.thinking_block_count += trajectory.thinking_block_count;
    }
    merge_extras(
        entry,
        "trajectory_path",
        json!(path.to_string_lossy().into_owned()),
    );
}

fn enrich_from_store(entry: &mut ProviderRecord, store: &SessionStoreMeta, agent_id: Option<&str>) {
    if let Some(agent_id) = agent_id {
        merge_extras(entry, "agent_id", json!(agent_id));
    }
    let Some(session_id) = entry.session_id.as_deref() else {
        return;
    };
    let mut session_keys = Vec::new();
    let mut matched_entries = Vec::new();
    for (key, value) in &store.entries {
        let matches_session = value["sessionId"].as_str() == Some(session_id)
            || value["usageFamilySessionIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(session_id)));
        if matches_session {
            session_keys.push(key.clone());
            matched_entries.push(value);
        }
    }
    if session_keys.is_empty() {
        return;
    }
    merge_extras(entry, "session_keys", json!(session_keys));
    if entry.project_display.is_empty() {
        for field in ["workspaceDir", "cwd", "workspace"] {
            if let Some(v) = matched_entries
                .iter()
                .find_map(|e| e[field].as_str().filter(|s| !s.is_empty()))
            {
                entry.project_display = v.to_string();
                break;
            }
        }
    }
    for field in [
        "status",
        "modelProvider",
        "modelOverride",
        "messageProvider",
        "messageChannel",
        "route",
        "origin",
        "fastMode",
        "compactionCount",
        "sessionFile",
        "usageFamilySessionIds",
    ] {
        if let Some(v) = matched_entries.iter().find_map(|e| e.get(field).cloned()) {
            merge_extras(entry, field, v);
        }
    }
}

fn merge_extras(entry: &mut ProviderRecord, key: &str, value: Value) {
    let mut map = entry
        .extras
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    map.insert(key.to_string(), value);
    entry.extras = Some(Value::Object(map).to_string());
}

fn trajectory_paths(
    classic_path: &Path,
    session_id: &str,
    trajectory_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let sibling = classic_path.with_file_name(format!("{session_id}.trajectory.jsonl"));
    paths.push(sibling);
    let pointer = classic_path.with_file_name(format!("{session_id}.trajectory-path.json"));
    if let Ok(raw) = std::fs::read_to_string(pointer)
        && let Ok(value) = serde_json::from_str::<Value>(&raw)
        && let Some(runtime_file) = value["runtimeFile"].as_str().filter(|s| !s.is_empty())
    {
        paths.push(PathBuf::from(runtime_file));
    }
    if let Some(dir) = trajectory_dir {
        paths.push(dir.join(format!("{}.jsonl", safe_session_file_name(session_id))));
    }
    paths.sort();
    paths.dedup();
    paths
}

fn push_message(
    entry: &mut ProviderRecord,
    msg_type: &str,
    text: Option<String>,
    ts_ms: Option<i64>,
) {
    if let Some(content) = text.filter(|s| !s.is_empty()) {
        entry.messages.push(MessageForFts {
            msg_type: msg_type.to_string(),
            content,
            timestamp_ms: ts_ms,
        });
    }
}

fn content_any_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str().filter(|t| !t.is_empty()) {
        return Some(text.to_string());
    }
    let arr = content.as_array()?;
    let parts: Vec<&str> = arr
        .iter()
        .filter(|b| matches!(b["type"].as_str(), Some("text") | Some("output_text")))
        .filter_map(|b| b["text"].as_str().filter(|t| !t.is_empty()))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn text_from_fields(value: &Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| value[*field].as_str().filter(|s| !s.is_empty()))
        .map(str::to_string)
}

fn output_text(msg: &Value) -> Option<String> {
    text_from_fields(msg, &["output", "stdout", "stderr", "text"])
}

fn tool_result_matches(gh_pr_tool_call_ids: &BTreeSet<String>, msg: &Value) -> bool {
    msg["toolName"].as_str().is_some_and(is_shell_tool)
        && ["toolCallId", "tool_call_id", "callId", "id"]
            .iter()
            .filter_map(|field| msg[*field].as_str())
            .any(|id| gh_pr_tool_call_ids.contains(id))
}

fn trajectory_tool_result_matches(gh_pr_tool_call_ids: &BTreeSet<String>, event: &Value) -> bool {
    event["data"]["name"].as_str().is_some_and(is_shell_tool)
        && ["entryId", "parentEntryId"]
            .iter()
            .filter_map(|field| event[*field].as_str())
            .any(|id| gh_pr_tool_call_ids.contains(id))
}

fn message_command_mentions_gh_pr(msg: &Value) -> bool {
    ["command", "cmd", "arguments", "args", "input"]
        .iter()
        .any(|field| value_mentions_gh_pr(&msg[*field]))
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

fn is_shell_tool(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "bash" | "shell" | "exec" | "exec_command"
    )
}

fn model_key(provider: &str, model: &str) -> String {
    if provider.is_empty() {
        model.to_string()
    } else {
        format!("{provider}/{model}")
    }
}

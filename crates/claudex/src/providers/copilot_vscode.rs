//! VS Code GitHub Copilot Chat provider: indexes the chat sessions VS Code
//! persists under `<config>/Code/User/workspaceStorage/<hash>/chatSessions/`
//! (plus `globalStorage/emptyWindowChatSessions/`), for both stable and
//! Insiders. Override the `User` directory with `CLAUDEX_VSCODE_USER_DIR`.
//!
//! Two encodings exist side by side: older sessions are one monolithic v3
//! JSON object; current VS Code writes a delta log (`.jsonl`) whose first line
//! snapshots the session and whose subsequent lines patch it (set-at-path /
//! array-insert ops). [`load_session_value`] replays either into the same v3
//! object before extraction.
//!
//! VS Code stores no token counts locally (they live server-side), so these
//! sessions index with zero usage and $0 computed cost — they still count for
//! activity, search, models, and message reporting.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::pr::{append_github_pr_links, looks_like_final_pr_text};
use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider, expand_home};
use crate::parser::ModelSessionStats;

/// Project label for chats started in a window with no folder open.
const EMPTY_WINDOW_PROJECT: &str = "(empty window)";

pub struct CopilotVscodeProvider {
    user_dirs: Vec<PathBuf>,
}

impl CopilotVscodeProvider {
    /// Provider over `$CLAUDEX_VSCODE_USER_DIR`, default the `Code/User` and
    /// `Code - Insiders/User` directories under the platform config root
    /// (`~/Library/Application Support` on macOS, `~/.config` on Linux).
    pub fn new() -> Result<Self> {
        let user_dirs = match std::env::var("CLAUDEX_VSCODE_USER_DIR") {
            Ok(v) if !v.trim().is_empty() => vec![expand_home(v.trim())?],
            _ => {
                let config = dirs::config_dir().context("could not find config directory")?;
                vec![
                    config.join("Code").join("User"),
                    config.join("Code - Insiders").join("User"),
                ]
            }
        };
        Ok(Self { user_dirs })
    }

    /// Provider over an explicit VS Code `User` directory (tests).
    pub fn at(user_dir: PathBuf) -> Self {
        Self {
            user_dirs: vec![user_dir],
        }
    }
}

impl SessionProvider for CopilotVscodeProvider {
    fn id(&self) -> &'static str {
        "copilot-vscode"
    }

    fn root_dir(&self) -> &Path {
        // Prefer the first User directory that exists (e.g. only Insiders is
        // installed) so `claudex providers` and the index's root stamp report
        // the directory actually being read.
        self.user_dirs
            .iter()
            .find(|d| d.exists())
            .unwrap_or(&self.user_dirs[0])
    }

    fn enabled(&self) -> bool {
        self.user_dirs.iter().any(|d| d.exists())
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let mut files = Vec::new();
        for user_dir in &self.user_dirs {
            let storage_root = user_dir.join("workspaceStorage");
            if storage_root.exists() {
                for entry in std::fs::read_dir(&storage_root)
                    .with_context(|| format!("reading {}", storage_root.display()))?
                {
                    let workspace_dir = entry?.path();
                    if !workspace_dir.is_dir() {
                        continue;
                    }
                    let chat_dir = workspace_dir.join("chatSessions");
                    if !chat_dir.exists() {
                        continue;
                    }
                    let project = workspace_folder(&workspace_dir.join("workspace.json"))
                        .unwrap_or_else(|| "(unknown workspace)".to_string());
                    collect_sessions(&chat_dir, &project, &mut files)?;
                }
            }
            let empty_dir = user_dir
                .join("globalStorage")
                .join("emptyWindowChatSessions");
            collect_sessions(&empty_dir, EMPTY_WINDOW_PROJECT, &mut files)?;
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        let session = load_session_value(&file.path)?;
        let format = if file.path.extension().is_some_and(|e| e == "jsonl") {
            "jsonl"
        } else {
            "json"
        };
        Ok(extract_session(&session, format))
    }
}

fn collect_sessions(dir: &Path, project: &str, out: &mut Vec<DiscoveredFile>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|e| e == "json" || e == "jsonl")
        {
            out.push(DiscoveredFile {
                path,
                // The session file itself has no workspace folder; the label
                // comes from the sibling workspace.json mapping.
                project_display: project.to_string(),
                parent_session_id: None,
                archived: false,
                source_key: None,
            });
        }
    }
    Ok(())
}

/// Resolve the workspace folder a `workspaceStorage/<hash>` belongs to from
/// its `workspace.json` (`{"folder": "file:///..."}`, or `{"workspace": ...}`
/// for `.code-workspace` setups).
fn workspace_folder(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let uri = value["folder"]
        .as_str()
        .or_else(|| value["workspace"].as_str())?;
    let decoded = percent_decode(uri.strip_prefix("file://")?);
    (!decoded.is_empty()).then_some(decoded)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Load a chat session file into its v3 session object: monolithic `.json`
/// parses directly, the `.jsonl` delta log is replayed op by op. Unknown op
/// kinds and unwalkable paths are ignored — a future VS Code change degrades
/// to partial extraction, never a parse failure.
pub fn load_session_value(path: &Path) -> Result<Value> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if path.extension().is_some_and(|e| e == "jsonl") {
        let mut root = Value::Null;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(op) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            apply_op(&mut root, &op);
        }
        Ok(root)
    } else {
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }
}

fn apply_op(root: &mut Value, op: &Value) {
    match op["kind"].as_u64() {
        // Full snapshot.
        Some(0) => *root = op["v"].clone(),
        // Set value at path.
        Some(1) => {
            if let Some(path) = op["k"].as_array()
                && let Some(slot) = resolve_path(root, path)
            {
                *slot = op["v"].clone();
            }
        }
        // Insert into (or append to) the array at path.
        Some(2) => {
            let Some(path) = op["k"].as_array() else {
                return;
            };
            let Some(slot) = resolve_path(root, path) else {
                return;
            };
            if slot.is_null() {
                *slot = Value::Array(Vec::new());
            }
            let Some(arr) = slot.as_array_mut() else {
                return;
            };
            let items = match &op["v"] {
                Value::Array(items) => items.clone(),
                other => vec![other.clone()],
            };
            let at = op["i"]
                .as_u64()
                .map(|i| (i as usize).min(arr.len()))
                .unwrap_or(arr.len());
            arr.splice(at..at, items);
        }
        _ => {}
    }
}

/// Walk `segments` (object keys / array indices) to a mutable slot, creating
/// missing object keys along the way so appends to not-yet-present arrays work.
fn resolve_path<'a>(root: &'a mut Value, segments: &[Value]) -> Option<&'a mut Value> {
    let mut current = root;
    for segment in segments {
        current = if let Some(key) = segment.as_str() {
            if current.is_null() {
                *current = Value::Object(serde_json::Map::new());
            }
            current
                .as_object_mut()?
                .entry(key.to_string())
                .or_insert(Value::Null)
        } else if let Some(idx) = segment.as_u64() {
            current.as_array_mut()?.get_mut(idx as usize)?
        } else {
            return None;
        };
    }
    Some(current)
}

/// One normalized chat message, shared with `claudex export`.
pub struct ExtractedMessage {
    pub role: &'static str,
    pub timestamp_ms: Option<i64>,
    pub text: String,
}

/// Flatten a v3 session object into user/assistant messages in request order.
pub fn session_messages(session: &Value) -> Vec<ExtractedMessage> {
    let mut messages = Vec::new();
    let Some(requests) = session["requests"].as_array() else {
        return messages;
    };
    for request in requests {
        let timestamp_ms = request["timestamp"].as_i64();
        if let Some(text) = request["message"]["text"]
            .as_str()
            .filter(|t| !t.is_empty())
        {
            messages.push(ExtractedMessage {
                role: "user",
                timestamp_ms,
                text: text.to_string(),
            });
        }
        let text = assistant_text(request);
        if !text.is_empty() {
            messages.push(ExtractedMessage {
                role: "assistant",
                timestamp_ms: request["modelState"]["completedAt"]
                    .as_i64()
                    .or(timestamp_ms),
                text,
            });
        }
    }
    messages
}

/// Concatenate the markdown parts of a request's response. Text parts carry a
/// `value` string and no `kind` discriminator; everything tagged with a kind
/// (tool invocations, thinking, references, edits) is rendered elsewhere.
fn assistant_text(request: &Value) -> String {
    let Some(parts) = request["response"].as_array() else {
        return String::new();
    };
    let texts: Vec<&str> = parts
        .iter()
        .filter(|part| part["kind"].is_null())
        .filter_map(|part| part["value"].as_str())
        .filter(|t| !t.is_empty())
        .collect();
    texts.join("")
}

fn extract_session(session: &Value, format: &str) -> ProviderRecord {
    let mut entry = ProviderRecord::default();
    let mut pr_links_seen = BTreeSet::new();

    if let Some(id) = session["sessionId"].as_str() {
        entry.session_id = Some(id.to_string());
    }
    entry.first_timestamp = session["creationDate"].as_i64().and_then(ts_from_ms);

    let mut last_activity_ms = session["lastMessageDate"].as_i64();
    let empty = Vec::new();
    let requests = session["requests"].as_array().unwrap_or(&empty);
    for request in requests {
        let timestamp_ms = request["timestamp"].as_i64();
        let completed_ms = request["modelState"]["completedAt"].as_i64();
        for ms in [timestamp_ms, completed_ms].into_iter().flatten() {
            // jsonl snapshots frequently lack lastMessageDate; the newest
            // request activity stands in for it.
            if last_activity_ms.is_none_or(|prev| ms > prev) {
                last_activity_ms = Some(ms);
            }
        }
        let timestamp_str = timestamp_ms
            .and_then(ts_from_ms)
            .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));

        if let Some(text) = request["message"]["text"]
            .as_str()
            .filter(|t| !t.is_empty())
        {
            entry.message_count += 1;
            entry.messages.push(MessageForFts {
                msg_type: "user".to_string(),
                content: text.to_string(),
                timestamp_ms,
            });
        }

        let model = request["modelId"]
            .as_str()
            .map(|m| m.strip_prefix("copilot/").unwrap_or(m))
            .filter(|m| !m.is_empty());
        if let Some(model) = model {
            entry.model = Some(model.to_string());
        }

        let mut tool_rounds_seen = false;
        if let Some(rounds) = request["result"]["metadata"]["toolCallRounds"].as_array() {
            for round in rounds {
                for call in round["toolCalls"].as_array().unwrap_or(&empty) {
                    if let Some(name) = call["name"].as_str().filter(|n| !n.is_empty()) {
                        entry.tool_names.push(name.to_string());
                        tool_rounds_seen = true;
                    }
                }
            }
        }
        for part in request["response"].as_array().unwrap_or(&empty) {
            match part["kind"].as_str() {
                Some("thinking") => entry.thinking_block_count += 1,
                Some("toolInvocationSerialized") => {
                    // toolCallRounds is the authoritative list when present;
                    // the serialized parts are its rendering.
                    if !tool_rounds_seen
                        && let Some(id) = part["toolId"].as_str().filter(|t| !t.is_empty())
                    {
                        entry.tool_names.push(id.to_string());
                    }
                }
                Some("textEditGroup") => {
                    if let Some(path) = part["uri"]["path"]
                        .as_str()
                        .or_else(|| part["uri"]["fsPath"].as_str())
                        .filter(|p| !p.is_empty())
                    {
                        entry.file_paths_modified.push(path.to_string());
                    }
                }
                _ => {}
            }
        }

        let text = assistant_text(request);
        let responded =
            !text.is_empty() || !request["response"].as_array().unwrap_or(&empty).is_empty();
        if responded {
            entry.message_count += 1;
            if let Some(model) = model {
                entry
                    .model_usage
                    .entry(model.to_string())
                    .or_insert_with(ModelSessionStats::default)
                    .assistant_message_count += 1;
            }
        }
        if !text.is_empty() {
            if looks_like_final_pr_text(&text) {
                append_github_pr_links(
                    &mut entry,
                    &mut pr_links_seen,
                    &text,
                    timestamp_str.as_deref(),
                );
            }
            entry.messages.push(MessageForFts {
                msg_type: "assistant".to_string(),
                content: text,
                timestamp_ms: completed_ms.or(timestamp_ms),
            });
        }

        if let (Some(elapsed), Some(ts)) = (
            request["result"]["timings"]["totalElapsed"].as_u64(),
            timestamp_str,
        ) {
            entry.turn_durations.push((elapsed, ts));
        }
    }

    entry.last_timestamp = last_activity_ms
        .and_then(ts_from_ms)
        .or(entry.first_timestamp);
    if entry.first_timestamp.is_none() {
        entry.first_timestamp = entry.last_timestamp;
    }
    if let (Some(first), Some(last)) = (entry.first_timestamp, entry.last_timestamp) {
        entry.duration_ms = last.signed_duration_since(first).num_milliseconds().max(0) as u64;
    }

    let mut extras = serde_json::Map::new();
    if let Some(title) = session["customTitle"].as_str().filter(|t| !t.is_empty()) {
        extras.insert("custom_title".to_string(), json!(title));
    }
    if let Some(location) = session["initialLocation"]
        .as_str()
        .filter(|l| !l.is_empty())
    {
        extras.insert("initial_location".to_string(), json!(location));
    }
    extras.insert("format".to_string(), json!(format));
    entry.extras = Some(Value::Object(extras).to_string());

    entry
}

fn ts_from_ms(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms)
}

//! Claude Code provider: indexes the JSONL transcripts under
//! `~/.claude/projects/`. This wraps the existing [`SessionStore`] discovery
//! (project-name decoding, worktree canonicalization, subagent rollup) and owns
//! the canonical Claude transcript parser.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::{DiscoveredFile, MessageForFts, ProviderRecord, SessionProvider};
use crate::parser::stream_records;
use crate::store::{
    SessionStore, canonical_project_path, decode_project_name, parent_session_id_for_path,
};

pub struct ClaudeProvider {
    store: SessionStore,
}

impl ClaudeProvider {
    /// Construct a provider rooted at the user's `~/.claude/projects`.
    pub fn new() -> Result<Self> {
        Ok(Self {
            store: SessionStore::new()?,
        })
    }

    /// Construct a provider over an explicit `projects` directory (tests).
    pub fn at(store: SessionStore) -> Self {
        Self { store }
    }
}

impl SessionProvider for ClaudeProvider {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn root_dir(&self) -> &Path {
        &self.store.base_dir
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let mut files = Vec::new();
        let mut decoded_projects = HashMap::new();
        for (project_raw, path) in self.store.all_session_files(None)? {
            let decoded = decoded_projects
                .entry(project_raw.clone())
                .or_insert_with(|| decode_project_name(&project_raw));
            let project_display = canonical_project_path(decoded.as_str()).to_string();
            let project_dir = self.store.base_dir.join(&project_raw);
            let parent_session_id = parent_session_id_for_path(&project_dir, &path);
            files.push(DiscoveredFile {
                path,
                project_display,
                parent_session_id,
                archived: false,
                source_key: None,
            });
        }
        Ok(files)
    }

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        parse_claude_session(&file.path)
    }
}

/// Stream a Claude Code transcript into a normalized [`ProviderRecord`]. Reads
/// one JSONL record at a time so large sessions don't balloon memory.
pub fn parse_claude_session(path: &Path) -> Result<ProviderRecord> {
    let mut entry = ProviderRecord::default();

    stream_records(path, |record| {
        if entry.project_display.is_empty()
            && let Some(cwd) = record["cwd"].as_str()
            && !cwd.is_empty()
        {
            entry.project_display = canonical_project_path(cwd).to_string();
        }
        if entry.session_id.is_none()
            && let Some(sid) = record["sessionId"].as_str()
        {
            entry.session_id = Some(sid.to_string());
        }

        let timestamp_str = record["timestamp"].as_str();
        let timestamp_ms = timestamp_str.and_then(|ts| {
            DateTime::parse_from_rfc3339(ts)
                .ok()
                .map(|dt| dt.timestamp_millis())
        });

        if let Some(ts_str) = timestamp_str
            && let Ok(dt) = DateTime::parse_from_rfc3339(ts_str)
        {
            let dt = dt.with_timezone(&Utc);
            if entry.first_timestamp.is_none_or(|prev| dt < prev) {
                entry.first_timestamp = Some(dt);
            }
            if entry.last_timestamp.is_none_or(|prev| dt > prev) {
                entry.last_timestamp = Some(dt);
            }
        }

        match record["type"].as_str().unwrap_or("") {
            "assistant" => {
                entry.message_count += 1;
                let msg = &record["message"];

                if entry.model.is_none()
                    && let Some(m) = msg["model"].as_str()
                {
                    entry.model = Some(m.to_string());
                }

                let usage = &msg["usage"];
                let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
                let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);
                let cache_creation_tokens =
                    usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                let cache_read_tokens = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);

                entry.usage.input_tokens += input_tokens;
                entry.usage.output_tokens += output_tokens;
                entry.usage.cache_creation_tokens += cache_creation_tokens;
                entry.usage.cache_read_tokens += cache_read_tokens;

                if entry.inference_geo.is_none() {
                    entry.inference_geo = usage["inference_geo"].as_str().map(|s| s.to_string());
                }
                if entry.speed.is_none() {
                    entry.speed = usage["speed"].as_f64();
                }
                if entry.service_tier.is_none() {
                    entry.service_tier = usage["service_tier"].as_str().map(|s| s.to_string());
                }
                entry.iterations += usage["iterations"].as_u64().unwrap_or(0);

                let model_key = msg["model"].as_str().unwrap_or("").to_string();
                let model_stats = entry.model_usage.entry(model_key).or_default();
                model_stats.usage.input_tokens += input_tokens;
                model_stats.usage.output_tokens += output_tokens;
                model_stats.usage.cache_creation_tokens += cache_creation_tokens;
                model_stats.usage.cache_read_tokens += cache_read_tokens;
                model_stats.assistant_message_count += 1;
                if let Some(geo) = usage["inference_geo"].as_str()
                    && !geo.is_empty()
                {
                    model_stats.inference_geos.insert(geo.to_string());
                }
                if let Some(tier) = usage["service_tier"].as_str()
                    && !tier.is_empty()
                {
                    model_stats.service_tiers.insert(tier.to_string());
                }
                if let Some(speed) = usage["speed"].as_f64() {
                    model_stats.speed_sum += speed;
                    model_stats.speed_samples += 1;
                }
                model_stats.iterations += usage["iterations"].as_u64().unwrap_or(0);

                if let Some(stop) = msg["stop_reason"].as_str() {
                    *entry
                        .stop_reason_counts
                        .entry(stop.to_string())
                        .or_insert(0) += 1;
                }

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
                                if let Some(t) = block["text"].as_str()
                                    && !t.is_empty()
                                {
                                    text_parts.push(t.to_string());
                                }
                            }
                            Some("thinking") => {
                                entry.thinking_block_count += 1;
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
                    if record["subtype"].as_str() == Some("turn_duration") {
                        let ts = timestamp_str.unwrap_or("").to_string();
                        entry.turn_durations.push((dur, ts));
                    }
                }
            }
            "pr-link" => {
                let number = record["prNumber"].as_i64().unwrap_or(0);
                let url = record["prUrl"].as_str().unwrap_or("").to_string();
                let repo = record["prRepository"].as_str().unwrap_or("").to_string();
                let ts = timestamp_str.unwrap_or("").to_string();
                entry.pr_links.push((number, url, repo, ts));
            }
            "file-history-snapshot" => {
                if let Some(backups) = record["snapshot"]["trackedFileBackups"].as_object() {
                    for key in backups.keys() {
                        if !entry.file_paths_modified.contains(key) {
                            entry.file_paths_modified.push(key.clone());
                        }
                    }
                }
            }
            "attachment" => {
                let filename = record["filename"].as_str().unwrap_or("").to_string();
                let mime = record["mimeType"].as_str().unwrap_or("").to_string();
                if !filename.is_empty() {
                    entry.attachments.push((filename, mime));
                }
            }
            "permission-mode" => {
                let mode = record["mode"].as_str().unwrap_or("").to_string();
                let ts = timestamp_str.unwrap_or("").to_string();
                if !mode.is_empty() {
                    entry.permission_modes.push((mode, ts));
                }
            }
            _ => {}
        }
        true
    })?;

    // Fallback duration from timestamp range
    if entry.duration_ms == 0
        && let (Some(first), Some(last)) = (entry.first_timestamp, entry.last_timestamp)
    {
        entry.duration_ms = last.signed_duration_since(first).num_milliseconds().max(0) as u64;
    }

    Ok(entry)
}

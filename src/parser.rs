use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::types::TokenUsage;

/// Aggregated statistics extracted from a single session JSONL file.
#[derive(Debug, Default)]
pub struct SessionStats {
    pub session_id: Option<String>,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub message_count: usize,
    pub total_duration_ms: u64,
    pub model: Option<String>,
    pub usage: TokenUsage,
    pub tool_names: Vec<String>,
    // Extended metric fields
    pub turn_durations: Vec<(u64, String)>, // (duration_ms, timestamp)
    pub pr_links: Vec<(i64, String, String, String)>, // (pr_number, url, repo, timestamp)
    pub file_paths_modified: Vec<String>,
    pub thinking_block_count: u64,
    pub stop_reason_counts: HashMap<String, u64>,
    pub attachments: Vec<(String, String)>, // (filename, mime_type)
    pub permission_modes: Vec<(String, String)>, // (mode, timestamp)
    pub inference_geo: Option<String>,
    pub speed: Option<f64>,
    pub service_tier: Option<String>,
    pub iterations: u64,
}

/// Parse a JSONL session file line-by-line, accumulating stats without loading
/// the entire file into memory.
pub fn parse_session(path: &Path) -> Result<SessionStats> {
    let mut stats = SessionStats::default();

    stream_records(path, |record| {
        if stats.session_id.is_none() {
            if let Some(sid) = record["sessionId"].as_str() {
                stats.session_id = Some(sid.to_string());
            }
        }

        let timestamp_str = record["timestamp"].as_str();

        if let Some(ts) = timestamp_str {
            if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                let dt = dt.with_timezone(&Utc);
                if stats.first_timestamp.is_none_or(|prev| dt < prev) {
                    stats.first_timestamp = Some(dt);
                }
                if stats.last_timestamp.is_none_or(|prev| dt > prev) {
                    stats.last_timestamp = Some(dt);
                }
            }
        }

        match record["type"].as_str().unwrap_or("") {
            "assistant" => {
                stats.message_count += 1;
                let msg = &record["message"];

                if stats.model.is_none() {
                    if let Some(m) = msg["model"].as_str() {
                        stats.model = Some(m.to_string());
                    }
                }

                let usage = &msg["usage"];
                stats.usage.input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
                stats.usage.output_tokens += usage["output_tokens"].as_u64().unwrap_or(0);
                stats.usage.cache_creation_tokens +=
                    usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                stats.usage.cache_read_tokens +=
                    usage["cache_read_input_tokens"].as_u64().unwrap_or(0);

                if stats.inference_geo.is_none() {
                    stats.inference_geo = usage["inference_geo"].as_str().map(|s| s.to_string());
                }
                if stats.speed.is_none() {
                    stats.speed = usage["speed"].as_f64();
                }
                if stats.service_tier.is_none() {
                    stats.service_tier = usage["service_tier"].as_str().map(|s| s.to_string());
                }
                stats.iterations += usage["iterations"].as_u64().unwrap_or(0);

                if let Some(stop) = msg["stop_reason"].as_str() {
                    *stats
                        .stop_reason_counts
                        .entry(stop.to_string())
                        .or_insert(0) += 1;
                }

                if let Some(content) = msg["content"].as_array() {
                    for block in content {
                        match block["type"].as_str() {
                            Some("tool_use") => {
                                if let Some(name) = block["name"].as_str() {
                                    stats.tool_names.push(name.to_string());
                                }
                            }
                            Some("thinking") => {
                                stats.thinking_block_count += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
            "user" => {
                stats.message_count += 1;
            }
            "system" => {
                if let Some(dur) = record["durationMs"].as_u64() {
                    stats.total_duration_ms += dur;
                    if record["subtype"].as_str() == Some("turn_duration") {
                        let ts = timestamp_str.unwrap_or("").to_string();
                        stats.turn_durations.push((dur, ts));
                    }
                }
            }
            "pr-link" => {
                let number = record["prNumber"].as_i64().unwrap_or(0);
                let url = record["prUrl"].as_str().unwrap_or("").to_string();
                let repo = record["repository"].as_str().unwrap_or("").to_string();
                let ts = timestamp_str.unwrap_or("").to_string();
                stats.pr_links.push((number, url, repo, ts));
            }
            "file-history-snapshot" => {
                if let Some(snapshot) = record["snapshot"].as_object() {
                    for key in snapshot.keys() {
                        if !stats.file_paths_modified.contains(key) {
                            stats.file_paths_modified.push(key.clone());
                        }
                    }
                }
            }
            "attachment" => {
                let filename = record["filename"].as_str().unwrap_or("").to_string();
                let mime = record["mimeType"].as_str().unwrap_or("").to_string();
                if !filename.is_empty() {
                    stats.attachments.push((filename, mime));
                }
            }
            "permission-mode" => {
                let mode = record["mode"].as_str().unwrap_or("").to_string();
                let ts = timestamp_str.unwrap_or("").to_string();
                if !mode.is_empty() {
                    stats.permission_modes.push((mode, ts));
                }
            }
            _ => {}
        }
        true
    })?;

    // Fallback: derive duration from timestamp range when system records are absent
    if stats.total_duration_ms == 0 {
        if let (Some(first), Some(last)) = (stats.first_timestamp, stats.last_timestamp) {
            let diff = last.signed_duration_since(first);
            stats.total_duration_ms = diff.num_milliseconds().max(0) as u64;
        }
    }

    Ok(stats)
}

/// Stream records from a JSONL file, calling `callback` for each parsed JSON
/// object.  Return `false` from the callback to stop early.
pub fn stream_records<F>(path: &Path, mut callback: F) -> Result<()>
where
    F: FnMut(&Value) -> bool,
{
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(record) => {
                if !callback(&record) {
                    break;
                }
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
        f.flush().unwrap();
        f
    }

    #[test]
    fn parse_empty_file() {
        let f = write_jsonl(&[]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.message_count, 0);
        assert!(stats.session_id.is_none());
        assert!(stats.model.is_none());
    }

    #[test]
    fn parse_user_and_assistant_messages() {
        let f = write_jsonl(&[
            r#"{"type":"user","sessionId":"abc-123","timestamp":"2026-04-18T00:00:00Z","message":{"content":"hello"}}"#,
            r#"{"type":"assistant","sessionId":"abc-123","timestamp":"2026-04-18T00:01:00Z","message":{"model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":200},"content":[{"type":"text","text":"hi"}]}}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.message_count, 2);
        assert_eq!(stats.session_id.as_deref(), Some("abc-123"));
        assert_eq!(stats.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(stats.usage.input_tokens, 100);
        assert_eq!(stats.usage.output_tokens, 50);
        assert_eq!(stats.usage.cache_read_tokens, 200);
        assert_eq!(*stats.stop_reason_counts.get("end_turn").unwrap_or(&0), 1);
    }

    #[test]
    fn parse_tool_use_and_thinking() {
        let f = write_jsonl(&[
            r#"{"type":"assistant","timestamp":"2026-04-18T00:00:00Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":0,"output_tokens":0},"content":[{"type":"thinking","text":"..."},{"type":"tool_use","name":"Bash","id":"t1","input":{}}]}}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.thinking_block_count, 1);
        assert_eq!(stats.tool_names, vec!["Bash"]);
    }

    #[test]
    fn parse_pr_link() {
        let f = write_jsonl(&[
            r#"{"type":"pr-link","prNumber":42,"prUrl":"https://github.com/org/repo/pull/42","repository":"org/repo","timestamp":"2026-04-18T00:00:00Z","sessionId":"s1"}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.pr_links.len(), 1);
        assert_eq!(stats.pr_links[0].0, 42);
    }

    #[test]
    fn parse_file_history_snapshot() {
        let f = write_jsonl(&[
            r#"{"type":"file-history-snapshot","snapshot":{"src/main.rs":{},"src/lib.rs":{}}}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.file_paths_modified.len(), 2);
        assert!(
            stats
                .file_paths_modified
                .contains(&"src/main.rs".to_string())
        );
    }

    #[test]
    fn parse_system_turn_duration() {
        let f = write_jsonl(&[
            r#"{"type":"system","subtype":"turn_duration","durationMs":1500,"timestamp":"2026-04-18T00:00:00Z"}"#,
            r#"{"type":"system","subtype":"turn_duration","durationMs":2500,"timestamp":"2026-04-18T00:01:00Z"}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.turn_durations.len(), 2);
        assert_eq!(stats.total_duration_ms, 4000);
    }

    #[test]
    fn parse_permission_mode() {
        let f = write_jsonl(&[
            r#"{"type":"permission-mode","mode":"bypassPermissions","timestamp":"2026-04-18T00:00:00Z","sessionId":"s1"}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.permission_modes.len(), 1);
        assert_eq!(stats.permission_modes[0].0, "bypassPermissions");
    }

    #[test]
    fn parse_usage_extended_fields() {
        let f = write_jsonl(&[
            r#"{"type":"assistant","timestamp":"2026-04-18T00:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":10,"output_tokens":5,"inference_geo":"us-east-1","speed":42.5,"service_tier":"default","iterations":3},"content":[]}}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.inference_geo.as_deref(), Some("us-east-1"));
        assert_eq!(stats.speed, Some(42.5));
        assert_eq!(stats.service_tier.as_deref(), Some("default"));
        assert_eq!(stats.iterations, 3);
    }

    #[test]
    fn parse_duration_fallback_from_timestamps() {
        let f = write_jsonl(&[
            r#"{"type":"user","timestamp":"2026-04-18T00:00:00Z","message":{"content":"start"}}"#,
            r#"{"type":"user","timestamp":"2026-04-18T00:05:00Z","message":{"content":"end"}}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.total_duration_ms, 300_000); // 5 minutes
    }

    #[test]
    fn stream_records_early_exit() {
        let f = write_jsonl(&[
            r#"{"type":"user","n":1}"#,
            r#"{"type":"user","n":2}"#,
            r#"{"type":"user","n":3}"#,
        ]);
        let mut count = 0;
        stream_records(f.path(), |_| {
            count += 1;
            count < 2 // stop after 2
        })
        .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn parse_skips_malformed_lines() {
        let f = write_jsonl(&[
            r#"{"type":"user","timestamp":"2026-04-18T00:00:00Z","message":{"content":"ok"}}"#,
            "not json at all",
            r#"{"type":"user","timestamp":"2026-04-18T00:01:00Z","message":{"content":"also ok"}}"#,
        ]);
        let stats = parse_session(f.path()).unwrap();
        assert_eq!(stats.message_count, 2);
    }
}

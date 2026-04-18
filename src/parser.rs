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
    pub cwd: Option<String>,
    pub usage: TokenUsage,
    pub tool_names: Vec<String>,
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

        if let Some(ts) = record["timestamp"].as_str() {
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

                if let Some(content) = msg["content"].as_array() {
                    for block in content {
                        if block["type"].as_str() == Some("tool_use") {
                            if let Some(name) = block["name"].as_str() {
                                stats.tool_names.push(name.to_string());
                            }
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
                }
                if stats.cwd.is_none() {
                    if let Some(cwd) = record["cwd"].as_str() {
                        stats.cwd = Some(cwd.to_string());
                    }
                }
            }
            _ => {}
        }
        true
    })?;

    // Fall back to timestamp span when no durationMs system records exist.
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

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::Value;

pub fn run(raw: bool) -> Result<()> {
    let home = dirs::home_dir().context("could not find home directory")?;
    let link = home.join(".claude").join("debug").join("latest");

    if !link.exists() {
        anyhow::bail!(
            "~/.claude/debug/latest not found\n\
             Enable debug logging in Claude Code settings (or set CLAUDE_DEBUG=1)"
        );
    }

    let mut resolved = resolve_link(&link)?;
    // Start from the current end of file so we only show new output
    let mut pos: u64 = file_len(&resolved);
    let mut leftover = String::new();

    eprintln!(
        "Watching {} (Ctrl-C to exit)",
        link.display().to_string().dimmed()
    );

    loop {
        // Detect symlink target changes (new session started)
        if let Ok(new_resolved) = resolve_link(&link) {
            if new_resolved != resolved {
                eprintln!(
                    "\n{}  {}",
                    "─── new session".bright_yellow(),
                    new_resolved.display().to_string().dimmed()
                );
                resolved = new_resolved;
                pos = 0;
                leftover.clear();
            }
        }

        let len = file_len(&resolved);
        if len > pos {
            if let Ok(mut f) = fs::File::open(&resolved) {
                if f.seek(SeekFrom::Start(pos)).is_ok() {
                    let mut buf = Vec::new();
                    if f.read_to_end(&mut buf).is_ok() {
                        pos += buf.len() as u64;
                        let chunk = String::from_utf8_lossy(&buf).to_string();
                        let combined = format!("{}{}", leftover, chunk);
                        let ends_with_newline = combined.ends_with('\n');
                        let mut parts: Vec<&str> = combined.split('\n').collect();

                        if !ends_with_newline {
                            leftover = parts.pop().unwrap_or("").to_string();
                        } else {
                            leftover.clear();
                            if parts.last() == Some(&"") {
                                parts.pop();
                            }
                        }

                        for line in parts {
                            if !line.trim().is_empty() {
                                if raw {
                                    println!("{line}");
                                } else {
                                    println!("{}", format_line(line));
                                }
                            }
                        }
                    }
                }
            }
        } else if len < pos {
            // File was truncated; restart from beginning
            pos = 0;
            leftover.clear();
        }

        thread::sleep(Duration::from_millis(500));
    }
}

fn resolve_link(link: &std::path::Path) -> Result<PathBuf> {
    fs::canonicalize(link).context("could not resolve ~/.claude/debug/latest")
}

fn file_len(path: &std::path::Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn format_line(line: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(line) {
        format_json_line(&v, line)
    } else {
        format_text_line(line)
    }
}

fn format_json_line(v: &Value, raw_line: &str) -> String {
    let ts = v["timestamp"]
        .as_str()
        .or_else(|| v["ts"].as_str())
        .unwrap_or("");
    let ts_short = shorten_ts(ts);

    // Session JSONL record style (has a "type" field)
    if let Some(record_type) = v["type"].as_str() {
        let type_colored = match record_type {
            "user" => record_type.bright_green().bold().to_string(),
            "assistant" => record_type.bright_blue().bold().to_string(),
            "system" => {
                let dur = v["durationMs"].as_u64().unwrap_or(0);
                let suffix = if dur > 0 {
                    format!(" {}ms", dur)
                } else {
                    String::new()
                };
                return format!(
                    "{} [{}]{}",
                    ts_short.dimmed(),
                    "system".dimmed(),
                    suffix.dimmed()
                );
            }
            _ => record_type.bright_yellow().to_string(),
        };
        return format!("{} [{}]", ts_short.dimmed(), type_colored);
    }

    // Structured log style (level + message)
    let level = v["level"]
        .as_str()
        .or_else(|| v["severity"].as_str())
        .unwrap_or("info");
    let msg = v["message"]
        .as_str()
        .or_else(|| v["msg"].as_str())
        .unwrap_or(raw_line);

    let (level_s, msg_s) = match level.to_lowercase().as_str() {
        "error" | "fatal" | "critical" => (level.red().bold().to_string(), msg.red().to_string()),
        "warn" | "warning" => (level.yellow().to_string(), msg.yellow().to_string()),
        "debug" | "trace" => (level.dimmed().to_string(), msg.dimmed().to_string()),
        _ => (level.to_string(), msg.to_string()),
    };

    if ts_short.is_empty() {
        format!("[{}] {}", level_s, msg_s)
    } else {
        format!("{} [{}] {}", ts_short.dimmed(), level_s, msg_s)
    }
}

fn format_text_line(line: &str) -> String {
    let lower = line.to_lowercase();
    if lower.contains("error") || lower.contains("fatal") {
        line.red().to_string()
    } else if lower.contains("warn") {
        line.yellow().to_string()
    } else if lower.contains("tool_call") || lower.contains("tool_use") {
        line.cyan().to_string()
    } else if lower.contains("debug") || lower.contains("trace") {
        line.dimmed().to_string()
    } else {
        line.to_string()
    }
}

fn shorten_ts(ts: &str) -> &str {
    // Extract HH:MM:SS from "YYYY-MM-DDTHH:MM:SS..." or "YYYY-MM-DD HH:MM:SS..."
    if ts.len() >= 19 {
        &ts[11..19]
    } else if ts.len() >= 8 {
        &ts[..8]
    } else {
        ts
    }
}

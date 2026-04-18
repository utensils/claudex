use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::parser::{parse_session, stream_records};
use crate::store::{SessionStore, decode_project_name, display_project_name};

pub fn run(
    selector: &str,
    format: &str,
    output: Option<&str>,
    project_filter: Option<&str>,
) -> Result<()> {
    if !["markdown", "json"].contains(&format) {
        anyhow::bail!("unknown format {:?}; expected markdown or json", format);
    }

    let store = SessionStore::new()?;
    let all_files = store.all_session_files(project_filter)?;
    let matching = find_matching(&all_files, selector);

    if matching.is_empty() {
        anyhow::bail!("no sessions found matching {:?}", selector);
    }

    let mut out: Box<dyn Write> = match output {
        Some(path) => Box::new(
            fs::File::create(path).with_context(|| format!("creating output file {path}"))?,
        ),
        None => Box::new(io::stdout()),
    };

    for (project_raw, path) in &matching {
        let project = display_project_name(&decode_project_name(project_raw));
        let buf = if format == "json" {
            build_json(&project, path)?
        } else {
            build_markdown(&project, path)?
        };
        out.write_all(buf.as_bytes())?;
    }

    Ok(())
}

/// Return sessions that match `selector` as a session-ID prefix OR project-name substring.
fn find_matching<'a>(files: &'a [(String, PathBuf)], selector: &str) -> Vec<&'a (String, PathBuf)> {
    let sel = selector.to_lowercase();

    // First try: session ID match via filename stem (most common — Claude Code names
    // session files after the session UUID).
    let id_matches: Vec<_> = files
        .iter()
        .filter(|(_, path)| {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            stem.starts_with(&sel) || stem.contains(&sel)
        })
        .collect();

    if !id_matches.is_empty() {
        return id_matches;
    }

    // Fallback: project name match
    files
        .iter()
        .filter(|(project_raw, _)| {
            let decoded = decode_project_name(project_raw).to_lowercase();
            project_raw.to_lowercase().contains(&sel) || decoded.contains(&sel)
        })
        .collect()
}

fn build_markdown(project: &str, path: &Path) -> Result<String> {
    let stats = parse_session(path)?;
    let mut buf = String::new();

    let sid: String = stats
        .session_id
        .as_deref()
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        })
        .chars()
        .take(8)
        .collect();

    buf.push_str(&format!("# Session: {}\n\n", sid));
    buf.push_str(&format!("**Project:** {}\n", project));
    if let Some(dt) = stats.first_timestamp {
        buf.push_str(&format!("**Date:** {}\n", dt.format("%Y-%m-%d %H:%M UTC")));
    }
    if let Some(m) = &stats.model {
        buf.push_str(&format!("**Model:** {}\n", m.trim_start_matches("claude-")));
    }
    buf.push('\n');
    buf.push_str("---\n\n");

    stream_records(path, |record| {
        let ts = record["timestamp"]
            .as_str()
            .map(|s| &s[..19.min(s.len())])
            .unwrap_or("");

        match record["type"].as_str().unwrap_or("") {
            "user" => {
                buf.push_str("## User\n");
                if !ts.is_empty() {
                    buf.push_str(&format!("*{}*\n\n", ts));
                }
                push_user_content(&mut buf, &record["message"]["content"]);
                buf.push_str("\n---\n\n");
            }
            "assistant" => {
                buf.push_str("## Assistant\n");
                if !ts.is_empty() {
                    buf.push_str(&format!("*{}*\n\n", ts));
                }
                push_assistant_content(&mut buf, &record["message"]["content"]);
                buf.push_str("\n---\n\n");
            }
            _ => {}
        }
        true
    })?;

    Ok(buf)
}

fn push_user_content(buf: &mut String, content: &Value) {
    if let Some(text) = content.as_str() {
        buf.push_str(text);
        buf.push('\n');
    } else if let Some(arr) = content.as_array() {
        for block in arr {
            match block["type"].as_str().unwrap_or("") {
                "text" => {
                    if let Some(text) = block["text"].as_str() {
                        buf.push_str(text);
                        buf.push('\n');
                    }
                }
                "tool_result" => {
                    let id = block["tool_use_id"].as_str().unwrap_or("");
                    buf.push_str(&format!("\n**Tool result** ({})\n", id));
                    match &block["content"] {
                        Value::Array(arr) => {
                            for c in arr {
                                if let Some(text) = c["text"].as_str() {
                                    buf.push_str("```\n");
                                    buf.push_str(text);
                                    buf.push_str("\n```\n");
                                }
                            }
                        }
                        Value::String(s) => {
                            buf.push_str("```\n");
                            buf.push_str(s);
                            buf.push_str("\n```\n");
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

fn push_assistant_content(buf: &mut String, content: &Value) {
    if let Some(arr) = content.as_array() {
        for block in arr {
            match block["type"].as_str().unwrap_or("") {
                "text" => {
                    if let Some(text) = block["text"].as_str() {
                        buf.push_str(text);
                        buf.push('\n');
                    }
                }
                "tool_use" => {
                    let name = block["name"].as_str().unwrap_or("unknown");
                    buf.push_str(&format!("\n**Tool: {}**\n", name));
                    if !block["input"].is_null() {
                        buf.push_str("```json\n");
                        if let Ok(json) = serde_json::to_string_pretty(&block["input"]) {
                            buf.push_str(&json);
                            buf.push('\n');
                        }
                        buf.push_str("```\n");
                    }
                }
                _ => {}
            }
        }
    }
}

fn build_json(project: &str, path: &Path) -> Result<String> {
    let stats = parse_session(path)?;
    let mut messages: Vec<Value> = Vec::new();

    stream_records(path, |record| {
        if matches!(record["type"].as_str(), Some("user") | Some("assistant")) {
            messages.push(record.clone());
        }
        true
    })?;

    let output = serde_json::json!({
        "project": project,
        "session_id": stats.session_id,
        "date": stats.first_timestamp.map(|d| d.to_rfc3339()),
        "model": stats.model,
        "message_count": stats.message_count,
        "messages": messages,
    });

    Ok(format!("{}\n", serde_json::to_string_pretty(&output)?))
}

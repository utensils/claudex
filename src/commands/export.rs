use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::DateTime;
use serde_json::Value;

use crate::parser::stream_records;
use crate::store::{SessionStore, decode_project_name, display_project_name};

pub fn run(
    target: &str,
    format: &str,
    output_path: Option<&str>,
    project_filter: Option<&str>,
) -> Result<()> {
    let store = SessionStore::new()?;
    let files = store.all_session_files(project_filter)?;

    let matched: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter(|(project_raw, path)| {
            let decoded = decode_project_name(project_raw);
            if decoded.contains(target) || project_raw.contains(target) {
                return true;
            }
            if let Some(stem) = path.file_stem() {
                let s = stem.to_string_lossy();
                if s.starts_with(target) || s.contains(target) {
                    return true;
                }
            }
            false
        })
        .collect();

    if matched.is_empty() {
        anyhow::bail!("no sessions found matching {:?}", target);
    }

    let mut out: Box<dyn Write> = if let Some(p) = output_path {
        Box::new(std::fs::File::create(p).with_context(|| format!("creating output file {p}"))?)
    } else {
        Box::new(std::io::stdout())
    };

    for (project_raw, path) in &matched {
        let project = display_project_name(project_raw);
        match format {
            "json" => export_json(&mut *out, &project, path)?,
            _ => export_markdown(&mut *out, &project, path)?,
        }
    }

    Ok(())
}

fn export_markdown(out: &mut dyn Write, project: &str, path: &Path) -> Result<()> {
    let mut session_id: Option<String> = None;
    let mut model: Option<String> = None;
    let mut first_ts: Option<chrono::DateTime<chrono::Utc>> = None;
    let mut records: Vec<Value> = Vec::new();

    stream_records(path, |r| {
        if session_id.is_none() {
            if let Some(sid) = r["sessionId"].as_str() {
                session_id = Some(sid.to_string());
            }
        }
        if model.is_none() {
            if let Some(m) = r["message"]["model"].as_str() {
                model = Some(m.to_string());
            }
        }
        if first_ts.is_none() {
            if let Some(ts) = r["timestamp"].as_str() {
                first_ts = DateTime::parse_from_rfc3339(ts)
                    .ok()
                    .map(|d| d.with_timezone(&chrono::Utc));
            }
        }
        records.push(r.clone());
        true
    })?;

    let sid = session_id.as_deref().unwrap_or("-");
    let date = first_ts
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_default();
    let model_str = model.as_deref().unwrap_or("unknown");

    writeln!(out, "# Session: {sid}")?;
    writeln!(out, "**Project:** {project}")?;
    writeln!(out, "**Date:** {date}")?;
    writeln!(out, "**Model:** {model_str}")?;
    writeln!(out)?;

    for record in &records {
        let ts = record["timestamp"].as_str().unwrap_or("");

        match record["type"].as_str().unwrap_or("") {
            "user" => {
                let content = record["message"]["content"].as_str().unwrap_or("");
                if content.is_empty() {
                    continue;
                }
                writeln!(out, "## User")?;
                if !ts.is_empty() {
                    writeln!(out, "*{ts}*")?;
                    writeln!(out)?;
                }
                writeln!(out, "{content}")?;
                writeln!(out)?;
            }
            "assistant" => {
                let blocks = match record["message"]["content"].as_array() {
                    Some(b) if !b.is_empty() => b,
                    _ => continue,
                };
                writeln!(out, "## Assistant")?;
                if !ts.is_empty() {
                    writeln!(out, "*{ts}*")?;
                    writeln!(out)?;
                }
                for block in blocks {
                    match block["type"].as_str().unwrap_or("") {
                        "text" => {
                            let text = block["text"].as_str().unwrap_or("");
                            if !text.is_empty() {
                                writeln!(out, "{text}")?;
                            }
                        }
                        "tool_use" => {
                            let name = block["name"].as_str().unwrap_or("unknown");
                            let input =
                                serde_json::to_string_pretty(&block["input"]).unwrap_or_default();
                            writeln!(out, "```tool:{name}")?;
                            writeln!(out, "{input}")?;
                            writeln!(out, "```")?;
                        }
                        _ => {}
                    }
                }
                writeln!(out)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn export_json(out: &mut dyn Write, project: &str, path: &Path) -> Result<()> {
    let mut records: Vec<Value> = Vec::new();
    stream_records(path, |r| {
        records.push(r.clone());
        true
    })?;
    let output = serde_json::json!({
        "project": project,
        "records": records,
    });
    writeln!(out, "{}", serde_json::to_string_pretty(&output)?)?;
    Ok(())
}

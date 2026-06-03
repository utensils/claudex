use std::fs;
use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::DateTime;
use serde_json::Value;

use claudex::index::{IndexStore, IndexedSession};
use claudex::parser::{parse_session, stream_records};
use claudex::providers::enabled_default;
use claudex::store::{
    SessionStore, decode_project_name, display_project_name, find_matching_sessions,
};

pub fn run(
    selector: &str,
    format: &str,
    output: Option<&str>,
    project_filter: Option<&str>,
) -> Result<()> {
    if !["markdown", "json"].contains(&format) {
        anyhow::bail!("unknown format {:?}; expected markdown or json", format);
    }

    let indexed = resolve_indexed(selector, project_filter).unwrap_or_default();
    if !indexed.is_empty() {
        let mut buf = Vec::new();
        if format == "json" {
            let mut payload = Vec::new();
            for row in &indexed {
                payload.push(build_indexed_json_value(row)?);
            }
            let output = if payload.len() == 1 {
                payload.into_iter().next().unwrap_or(Value::Null)
            } else {
                Value::Array(payload)
            };
            writeln!(buf, "{}", serde_json::to_string_pretty(&output)?)?;
        } else {
            for row in &indexed {
                buf.write_all(build_indexed_markdown(row)?.as_bytes())?;
            }
        }
        write_output(output, &buf)?;
        return Ok(());
    }

    let store = SessionStore::new()?;
    let all_files = store.all_session_files(project_filter)?;
    let matching = find_matching_sessions(&all_files, selector);

    if matching.is_empty() {
        anyhow::bail!("no sessions found matching {:?}", selector);
    }

    let mut buf = Vec::new();
    if format == "json" {
        let mut payload = Vec::new();
        for (project_raw, path) in &matching {
            let project = display_project_name(&decode_project_name(project_raw));
            payload.push(build_json_value(&project, path)?);
        }
        let output = if payload.len() == 1 {
            payload.into_iter().next().unwrap_or(Value::Null)
        } else {
            Value::Array(payload)
        };
        writeln!(buf, "{}", serde_json::to_string_pretty(&output)?)?;
    } else {
        for (project_raw, path) in &matching {
            let project = display_project_name(&decode_project_name(project_raw));
            buf.write_all(build_markdown(&project, path)?.as_bytes())?;
        }
    }
    write_output(output, &buf)?;

    Ok(())
}

fn write_output(output: Option<&str>, bytes: &[u8]) -> Result<()> {
    match output {
        Some(path) => {
            fs::write(path, bytes).with_context(|| format!("creating output file {path}"))
        }
        None => io::stdout().write_all(bytes).map_err(Into::into),
    }
}

fn resolve_indexed(selector: &str, project_filter: Option<&str>) -> Result<Vec<IndexedSession>> {
    let providers = enabled_default()?;
    if providers.is_empty() {
        return Ok(Vec::new());
    }
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&providers)?;
    Ok(idx
        .query_session_matches(selector, project_filter)?
        .into_iter()
        .filter(|row| row.present_on_disk && Path::new(&row.file_path).exists())
        .collect())
}

fn build_indexed_markdown(row: &IndexedSession) -> Result<String> {
    if row.provider == "claude" {
        return build_markdown(&row.project_name, Path::new(&row.file_path));
    }

    let mut buf = String::new();
    let sid = row
        .session_id
        .as_deref()
        .or_else(|| {
            Path::new(&row.file_path)
                .file_stem()
                .and_then(|s| s.to_str())
        })
        .unwrap_or("unknown");
    buf.push_str(&format!(
        "# Session: {}\n\n",
        sid.chars().take(8).collect::<String>()
    ));
    buf.push_str(&format!("**Provider:** {}\n", row.provider));
    buf.push_str(&format!("**Project:** {}\n", row.project_name));
    buf.push_str(&format!("**File:** {}\n", row.file_path));
    if let Some(dt) = row
        .first_timestamp_ms
        .and_then(DateTime::from_timestamp_millis)
    {
        buf.push_str(&format!("**Date:** {}\n", dt.format("%Y-%m-%d %H:%M UTC")));
    }
    if let Some(model) = &row.model {
        buf.push_str(&format!(
            "**Model:** {}\n",
            model.trim_start_matches("claude-")
        ));
    }
    if let Some(extras) = &row.extras {
        buf.push_str(&format!("**Metadata:** `{}`\n", extras));
    }
    buf.push_str("\n---\n\n");

    stream_records(Path::new(&row.file_path), |record| {
        if let Some((role, text)) = generic_message_text(&row.provider, record) {
            let ts = record["timestamp"]
                .as_str()
                .map(|s| &s[..19.min(s.len())])
                .unwrap_or("");
            buf.push_str(&format!("## {}\n", title_case(&role)));
            if !ts.is_empty() {
                buf.push_str(&format!("*{}*\n\n", ts));
            }
            buf.push_str(&text);
            buf.push_str("\n\n---\n\n");
        }
        true
    })?;
    Ok(buf)
}

fn build_indexed_json_value(row: &IndexedSession) -> Result<Value> {
    let mut records = Vec::new();
    let mut messages = Vec::new();
    stream_records(Path::new(&row.file_path), |record| {
        if let Some((role, text)) = generic_message_text(&row.provider, record) {
            messages.push(serde_json::json!({
                "role": role,
                "timestamp": record["timestamp"].as_str(),
                "text": text,
            }));
        }
        records.push(record.clone());
        true
    })?;
    Ok(serde_json::json!({
        "provider": row.provider,
        "project": row.project_name,
        "session_id": row.session_id,
        "file_path": row.file_path,
        "date": row.first_timestamp_ms.and_then(DateTime::from_timestamp_millis).map(|d| d.to_rfc3339()),
        "last_activity": row.last_timestamp_ms.and_then(DateTime::from_timestamp_millis).map(|d| d.to_rfc3339()),
        "model": row.model,
        "message_count": row.message_count,
        "duration_ms": row.duration_ms,
        "present_on_disk": row.present_on_disk,
        "archived_at": row.archived_at.and_then(|s| DateTime::from_timestamp(s, 0)).map(|d| d.to_rfc3339()),
        "extras": row.extras.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
        "messages": records.clone(),
        "normalized_messages": messages,
        "records": records,
    }))
}

fn generic_message_text(provider: &str, record: &Value) -> Option<(String, String)> {
    match provider {
        "claude" => match record["type"].as_str()? {
            "user" => {
                text_from_value(&record["message"]["content"]).map(|t| ("user".to_string(), t))
            }
            "assistant" => {
                text_from_value(&record["message"]["content"]).map(|t| ("assistant".to_string(), t))
            }
            _ => None,
        },
        "codex" => {
            let payload = if matches!(record["type"].as_str(), Some("response_item" | "event_msg"))
            {
                &record["payload"]
            } else {
                record
            };
            match payload["type"].as_str()? {
                "message" => {
                    let role = payload["role"].as_str()?.to_string();
                    text_from_value(&payload["content"])
                        .or_else(|| payload["message"].as_str().map(str::to_string))
                        .map(|t| (role, t))
                }
                "user_message" => payload["message"]
                    .as_str()
                    .map(|t| ("user".to_string(), t.to_string())),
                "agent_message" => payload["message"]
                    .as_str()
                    .map(|t| ("assistant".to_string(), t.to_string())),
                _ => None,
            }
        }
        "pi" | "openclaw" => {
            let msg = if record["type"].as_str() == Some("message") {
                &record["message"]
            } else {
                record
            };
            let role = msg["role"].as_str().or_else(|| record["role"].as_str())?;
            if role == "user" || role == "assistant" {
                text_from_value(&msg["content"])
                    .or_else(|| text_from_value(&record["content"]))
                    .map(|t| (role.to_string(), t))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn text_from_value(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str().filter(|s| !s.is_empty()) {
        return Some(s.to_string());
    }
    let parts: Vec<String> = value
        .as_array()?
        .iter()
        .filter_map(|b| {
            b["text"]
                .as_str()
                .or_else(|| b["content"].as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn title_case(role: &str) -> String {
    let mut chars = role.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "Message".to_string(),
    }
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

fn build_json_value(project: &str, path: &Path) -> Result<Value> {
    let stats = parse_session(path)?;
    let mut messages: Vec<Value> = Vec::new();

    stream_records(path, |record| {
        if matches!(record["type"].as_str(), Some("user") | Some("assistant")) {
            messages.push(record.clone());
        }
        true
    })?;

    Ok(serde_json::json!({
        "project": project,
        "session_id": stats.session_id,
        "date": stats.first_timestamp.map(|d| d.to_rfc3339()),
        "model": stats.model,
        "message_count": stats.message_count,
        "messages": messages,
    }))
}

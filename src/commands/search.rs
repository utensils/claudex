use anyhow::Result;
use chrono::DateTime;

use crate::index::IndexStore;
use crate::parser::stream_records;
use crate::store::{SessionStore, decode_project_name, short_name};
use crate::ui;

pub fn run(
    query: &str,
    project: Option<&str>,
    limit: usize,
    json: bool,
    case_sensitive: bool,
    no_index: bool,
) -> Result<()> {
    // FTS5 is case-insensitive; fall back to file scan for case-sensitive queries
    if !no_index
        && !case_sensitive
        && let Ok(()) = run_indexed(query, project, limit, json)
    {
        return Ok(());
    }
    run_from_files(query, project, limit, json, case_sensitive)
}

fn run_indexed(query: &str, project: Option<&str>, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;

    let hits = idx.search_fts(query, project, limit)?;

    if json {
        let output: Vec<_> = hits
            .iter()
            .map(|hit| {
                let message_timestamp = hit
                    .message_timestamp_ms
                    .and_then(DateTime::from_timestamp_millis)
                    .map(|d| d.to_rfc3339());
                serde_json::json!({
                    "project": hit.project_name,
                    "session_id": hit.session_id,
                    "message_timestamp": message_timestamp,
                    "message_type": hit.message_type,
                    "snippet": strip_search_markers(&hit.snippet),
                    "rank": hit.rank,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if hits.is_empty() {
        println!("No matches found for {query:?}");
        return Ok(());
    }

    for hit in &hits {
        let date_str = hit
            .message_timestamp_ms
            .and_then(DateTime::from_timestamp_millis)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string());
        let sid: String = hit
            .session_id
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(8)
            .collect();
        let project_display = short_name(&hit.project_name);

        println!(
            "{} {} [{}] {}",
            ui::project_headline(&project_display),
            ui::session_id(&sid),
            ui::timestamp(&date_str),
            ui::role(&hit.message_type),
        );
        println!("  {}", render_indexed_snippet(&hit.snippet));
        println!();
    }
    Ok(())
}

fn run_from_files(
    query: &str,
    project: Option<&str>,
    limit: usize,
    json: bool,
    case_sensitive: bool,
) -> Result<()> {
    let store = SessionStore::new()?;
    let files = store.all_session_files(project)?;

    let query_cmp = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut found = 0usize;
    let mut json_hits = Vec::new();

    'outer: for (project_raw, path) in &files {
        let project_display = short_name(&decode_project_name(project_raw));
        let mut session_date = None;
        let mut session_id: Option<String> = None;
        let mut stop = false;

        stream_records(path, |record| {
            if session_id.is_none()
                && let Some(sid) = record["sessionId"].as_str()
            {
                session_id = Some(sid.to_string());
            }
            if session_date.is_none()
                && let Some(ts) = record["timestamp"].as_str()
            {
                session_date = DateTime::parse_from_rfc3339(ts)
                    .ok()
                    .map(|d| d.with_timezone(&chrono::Utc));
            }

            let (role, text) = match record["type"].as_str().unwrap_or("") {
                "user" => {
                    let content = record["message"]["content"].as_str().unwrap_or("");
                    ("user", content.to_string())
                }
                "assistant" => {
                    let blocks = record["message"]["content"].as_array();
                    let text = blocks
                        .map(|arr| {
                            arr.iter()
                                .filter(|b| b["type"].as_str() == Some("text"))
                                .map(|b| b["text"].as_str().unwrap_or("").to_string())
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_default();
                    ("assistant", text)
                }
                _ => return true,
            };

            if text.is_empty() {
                return true;
            }

            let haystack = if case_sensitive {
                text.as_str().to_string()
            } else {
                text.to_lowercase()
            };

            if !haystack.contains(&query_cmp) {
                return true;
            }

            let date_str = session_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "-".to_string());
            let sid: String = session_id
                .as_deref()
                .unwrap_or("-")
                .chars()
                .take(8)
                .collect();

            if !json {
                println!(
                    "{} {} [{}] {}",
                    ui::project_headline(&project_display),
                    ui::session_id(&sid),
                    ui::timestamp(&date_str),
                    ui::role(role),
                );
            }

            for line in text.lines() {
                let line_cmp = if case_sensitive {
                    line.to_string()
                } else {
                    line.to_lowercase()
                };
                if line_cmp.contains(&query_cmp) {
                    let snippet = build_file_scan_snippet(line, query, case_sensitive);
                    if json {
                        json_hits.push(serde_json::json!({
                            "project": decode_project_name(project_raw),
                            "session_id": session_id,
                            "message_timestamp": session_date.map(|d| d.to_rfc3339()),
                            "message_type": role,
                            "snippet": snippet,
                            "rank": serde_json::Value::Null,
                        }));
                    } else {
                        print_highlighted(line, query, case_sensitive);
                        println!();
                    }
                    found += 1;
                    if found >= limit {
                        stop = true;
                        return false;
                    }
                }
            }
            true
        })?;

        if stop {
            break 'outer;
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&json_hits)?);
        return Ok(());
    }

    if found == 0 {
        println!("No matches found for {query:?}");
    }
    Ok(())
}

fn print_highlighted(line: &str, query: &str, case_sensitive: bool) {
    const MAX_LINE: usize = 300;
    let display = if line.len() > MAX_LINE {
        let mut end = MAX_LINE;
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        &line[..end]
    } else {
        line
    };

    let haystack = if case_sensitive {
        display.to_string()
    } else {
        display.to_lowercase()
    };
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut result = String::new();
    let mut last = 0usize;
    let mut search_from = 0usize;

    while let Some(rel) = haystack[search_from..].find(&needle) {
        let pos = search_from + rel;
        let end = pos + needle.len();

        if !display.is_char_boundary(pos) || !display.is_char_boundary(end) {
            search_from = pos + 1;
            continue;
        }

        result.push_str(&display[last..pos]);
        let matched = &display[pos..end];
        result.push_str(&ui::match_highlight(matched));
        last = end;
        search_from = end;
    }
    result.push_str(&display[last..]);
    println!("  {}", result);
}

fn render_indexed_snippet(snippet: &str) -> String {
    let mut out = String::new();
    let mut rest = snippet;
    while let Some(start) = rest.find("[[") {
        let (before, after_start) = rest.split_at(start);
        out.push_str(before);
        let after_start = &after_start[2..];
        if let Some(end) = after_start.find("]]") {
            let (matched, after_end) = after_start.split_at(end);
            out.push_str(&ui::match_highlight(matched));
            rest = &after_end[2..];
        } else {
            out.push_str(after_start);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn strip_search_markers(snippet: &str) -> String {
    snippet.replace("[[", "").replace("]]", "")
}

fn build_file_scan_snippet(line: &str, query: &str, case_sensitive: bool) -> String {
    const CONTEXT: usize = 80;
    let haystack = if case_sensitive {
        line.to_string()
    } else {
        line.to_lowercase()
    };
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    if let Some(pos) = haystack.find(&needle) {
        let mut start = pos.saturating_sub(CONTEXT);
        while start > 0 && !line.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = (pos + needle.len() + CONTEXT).min(line.len());
        while end < line.len() && !line.is_char_boundary(end) {
            end += 1;
        }
        let prefix = if start > 0 { "..." } else { "" };
        let suffix = if end < line.len() { "..." } else { "" };
        format!("{prefix}{}{suffix}", &line[start..end])
    } else {
        line.to_string()
    }
}

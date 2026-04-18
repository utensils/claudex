use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::parser::parse_session;
use crate::store::{SessionStore, decode_project_name, display_project_name, short_name};

pub fn run(project: Option<&str>, per_session: bool, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let files = store.all_session_files(project)?;
    if per_session {
        run_per_session(files, limit, json)
    } else {
        run_aggregate(files, limit, json)
    }
}

fn run_aggregate(files: Vec<(String, PathBuf)>, limit: usize, json: bool) -> Result<()> {
    let mut counts: HashMap<String, u64> = HashMap::new();

    for (_, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for name in stats.tool_names {
            *counts.entry(name).or_insert(0) += 1;
        }
    }

    let mut rows: Vec<(String, u64)> = counts.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    rows.truncate(limit);

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|(name, count)| serde_json::json!({"tool": name, "count": count}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Tool", "Calls"]);
    for (name, count) in &rows {
        table.add_row([name.as_str(), &count.to_string()]);
    }
    println!("{table}");
    Ok(())
}

fn run_per_session(files: Vec<(String, PathBuf)>, limit: usize, json: bool) -> Result<()> {
    let mut rows = Vec::new();
    for (project_raw, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if stats.tool_names.is_empty() {
            continue;
        }
        let mut counts: HashMap<String, u64> = HashMap::new();
        for name in &stats.tool_names {
            *counts.entry(name.clone()).or_insert(0) += 1;
        }
        rows.push((
            display_project_name(&decode_project_name(project_raw)),
            stats.session_id,
            stats.first_timestamp,
            counts,
        ));
    }
    rows.truncate(limit);

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|(project, session_id, date, counts)| {
                serde_json::json!({
                    "project": project,
                    "session_id": session_id,
                    "date": date.map(|d| d.to_rfc3339()),
                    "tools": counts,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Project", "Session", "Top Tools", "Total Calls"]);

    for (project, session_id, _, counts) in &rows {
        let sid: String = session_id
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(8)
            .collect();
        let total: u64 = counts.values().sum();
        let mut sorted: Vec<_> = counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        let top: Vec<_> = sorted
            .iter()
            .take(3)
            .map(|(k, v)| format!("{}({})", k, v))
            .collect();
        table.add_row([short_name(project), sid, top.join(", "), total.to_string()]);
    }
    println!("{table}");
    Ok(())
}

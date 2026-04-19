use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use chrono::DateTime;

use crate::index::IndexStore;
use crate::parser::parse_session;
use crate::store::{SessionStore, decode_project_name, display_project_name, short_name};
use crate::ui;

pub fn run(
    project: Option<&str>,
    per_session: bool,
    limit: usize,
    json: bool,
    no_index: bool,
) -> Result<()> {
    if !no_index && let Ok(()) = run_indexed(project, per_session, limit, json) {
        return Ok(());
    }
    run_from_files(project, per_session, limit, json)
}

fn run_indexed(project: Option<&str>, per_session: bool, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;

    if per_session {
        let rows = idx.query_tools_per_session(project, limit)?;

        if json {
            let output: Vec<_> = rows
                .iter()
                .map(|r| {
                    let date = r
                        .first_timestamp_ms
                        .and_then(DateTime::from_timestamp_millis)
                        .map(|d| d.to_rfc3339());
                    serde_json::json!({
                        "project": r.project,
                        "session_id": r.session_id,
                        "date": date,
                        "tools": r.tools,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        let mut table = ui::table();
        table.set_header(ui::header([
            "Project",
            "Session",
            "Date",
            "Top Tools",
            "Total Calls",
        ]));
        ui::right_align(&mut table, &[4]);

        for r in &rows {
            let sid: String = r
                .session_id
                .as_deref()
                .unwrap_or("-")
                .chars()
                .take(8)
                .collect();
            let total: i64 = r.tools.values().sum();
            let mut sorted: Vec<_> = r.tools.iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(a.1));
            let date = r
                .first_timestamp_ms
                .and_then(DateTime::from_timestamp_millis)
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "-".to_string());
            let top: Vec<_> = sorted
                .iter()
                .take(3)
                .map(|(k, v)| format!("{}({})", k, ui::fmt_count(**v as u64)))
                .collect();
            table.add_row([
                ui::cell_project(&short_name(&r.project)),
                ui::cell_dim(&sid),
                ui::cell_dim(&date),
                ui::cell_plain(top.join(", ")),
                ui::cell_count(total as u64),
            ]);
        }
        println!("{table}");
        return Ok(());
    }

    let rows = idx.query_tools_aggregate(project, limit)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| serde_json::json!({"tool": r.tool_name, "count": r.count}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut table = ui::table();
    table.set_header(ui::header(["Tool", "Calls"]));
    ui::right_align(&mut table, &[1]);
    for r in &rows {
        table.add_row([ui::cell_tool(&r.tool_name), ui::cell_count(r.count as u64)]);
    }
    println!("{table}");
    Ok(())
}

fn run_from_files(
    project: Option<&str>,
    per_session: bool,
    limit: usize,
    json: bool,
) -> Result<()> {
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

    let mut table = ui::table();
    table.set_header(ui::header(["Tool", "Calls"]));
    ui::right_align(&mut table, &[1]);
    for (name, count) in &rows {
        table.add_row([ui::cell_tool(name), ui::cell_count(*count)]);
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
    rows.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
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

    let mut table = ui::table();
    table.set_header(ui::header([
        "Project",
        "Session",
        "Date",
        "Top Tools",
        "Total Calls",
    ]));
    ui::right_align(&mut table, &[4]);

    for (project, session_id, date, counts) in &rows {
        let sid: String = session_id
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(8)
            .collect();
        let date = date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string());
        let total: u64 = counts.values().sum();
        let mut sorted: Vec<_> = counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        let top: Vec<_> = sorted
            .iter()
            .take(3)
            .map(|(k, v)| format!("{}({})", k, ui::fmt_count(**v)))
            .collect();
        table.add_row([
            ui::cell_project(&short_name(project)),
            ui::cell_dim(&sid),
            ui::cell_dim(&date),
            ui::cell_plain(top.join(", ")),
            ui::cell_count(total),
        ]);
    }
    println!("{table}");
    Ok(())
}

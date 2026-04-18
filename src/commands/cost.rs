use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::parser::parse_session;
use crate::store::{SessionStore, decode_project_name, short_name};
use crate::types::TokenUsage;

pub fn run(project: Option<&str>, per_session: bool, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let files = store.all_session_files(project)?;
    if per_session {
        run_per_session(files, limit, json)
    } else {
        run_by_project(files, limit, json)
    }
}

struct ProjectCost {
    project: String,
    usage: TokenUsage,
    session_count: usize,
}

fn run_by_project(files: Vec<(String, PathBuf)>, limit: usize, json: bool) -> Result<()> {
    let mut projects: HashMap<String, ProjectCost> = HashMap::new();

    for (project_raw, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let entry = projects
            .entry(project_raw.clone())
            .or_insert_with(|| ProjectCost {
                project: decode_project_name(project_raw),
                usage: TokenUsage::default(),
                session_count: 0,
            });
        entry.usage.add(&stats.usage);
        entry.session_count += 1;
    }

    let mut rows: Vec<ProjectCost> = projects.into_values().collect();
    rows.sort_by(|a, b| {
        b.usage
            .approx_cost_usd()
            .partial_cmp(&a.usage.approx_cost_usd())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(limit);

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "project": r.project,
                    "sessions": r.session_count,
                    "input_tokens": r.usage.input_tokens,
                    "output_tokens": r.usage.output_tokens,
                    "cache_creation_tokens": r.usage.cache_creation_tokens,
                    "cache_read_tokens": r.usage.cache_read_tokens,
                    "cost_usd": r.usage.approx_cost_usd(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header([
        "Project",
        "Sessions",
        "Input",
        "Output",
        "Cache Read",
        "Cost (USD)",
    ]);

    let mut total = TokenUsage::default();
    let mut total_sessions = 0usize;

    for r in &rows {
        table.add_row([
            short_name(&r.project),
            r.session_count.to_string(),
            fmt_tokens(r.usage.input_tokens),
            fmt_tokens(r.usage.output_tokens),
            fmt_tokens(r.usage.cache_read_tokens),
            format!("${:.4}", r.usage.approx_cost_usd()),
        ]);
        total.add(&r.usage);
        total_sessions += r.session_count;
    }
    table.add_row([
        "TOTAL".to_string(),
        total_sessions.to_string(),
        fmt_tokens(total.input_tokens),
        fmt_tokens(total.output_tokens),
        fmt_tokens(total.cache_read_tokens),
        format!("${:.4}", total.approx_cost_usd()),
    ]);

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
        if stats.usage.total_tokens() == 0 {
            continue;
        }
        rows.push((decode_project_name(project_raw), stats));
    }
    rows.sort_by(|a, b| {
        b.1.usage
            .approx_cost_usd()
            .partial_cmp(&a.1.usage.approx_cost_usd())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(limit);

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|(project, stats)| {
                serde_json::json!({
                    "project": project,
                    "session_id": stats.session_id,
                    "date": stats.first_timestamp.map(|d| d.to_rfc3339()),
                    "input_tokens": stats.usage.input_tokens,
                    "output_tokens": stats.usage.output_tokens,
                    "cache_creation_tokens": stats.usage.cache_creation_tokens,
                    "cache_read_tokens": stats.usage.cache_read_tokens,
                    "cost_usd": stats.usage.approx_cost_usd(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header([
        "Project",
        "Session",
        "Date",
        "Input",
        "Output",
        "Cost (USD)",
    ]);

    for (project, stats) in &rows {
        let sid: String = stats
            .session_id
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(8)
            .collect();
        let date = stats
            .first_timestamp
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string());
        table.add_row([
            short_name(project),
            sid,
            date,
            fmt_tokens(stats.usage.input_tokens),
            fmt_tokens(stats.usage.output_tokens),
            format!("${:.4}", stats.usage.approx_cost_usd()),
        ]);
    }
    println!("{table}");
    Ok(())
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

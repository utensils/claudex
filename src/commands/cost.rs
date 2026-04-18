use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::index::IndexStore;
use crate::parser::parse_session;
use crate::store::{SessionStore, decode_project_name, display_project_name, short_name};
use crate::types::{ModelPricing, TokenUsage};

pub fn run_indexed(store: &IndexStore, project: Option<&str>, limit: usize, json: bool) -> Result<()> {
    let rows = store.query_cost_by_project(project, limit)?;
    if json {
        let output: Vec<_> = rows.iter().map(|r| serde_json::json!({
            "project": r.project, "sessions": r.sessions, "models": r.models,
            "input_tokens": r.input_tokens, "output_tokens": r.output_tokens,
            "cache_read_tokens": r.cache_read, "cost_usd": r.cost_usd,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Project", "Sessions", "Input", "Output", "Cache Read", "Model(s)", "Cost (USD)"]);
    let mut tc = 0.0f64;
    for r in &rows {
        table.add_row([
            short_name(&r.project), r.sessions.to_string(),
            fmt_tokens(r.input_tokens as u64), fmt_tokens(r.output_tokens as u64),
            fmt_tokens(r.cache_read as u64), r.models.as_deref().unwrap_or("-").to_string(),
            format!("${:.4}", r.cost_usd),
        ]);
        tc += r.cost_usd;
    }
    table.add_row(["TOTAL".into(), String::new(), String::new(), String::new(), String::new(), String::new(), format!("${tc:.4}")]);
    println!("{table}");
    Ok(())
}

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
    /// Accumulated model-accurate cost (sum of per-session costs).
    total_cost: f64,
    /// Distinct model families used in this project's sessions.
    models: Vec<String>,
}

fn run_by_project(files: Vec<(String, PathBuf)>, limit: usize, json: bool) -> Result<()> {
    let mut projects: HashMap<String, ProjectCost> = HashMap::new();

    for (project_raw, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let display = display_project_name(&decode_project_name(project_raw));
        let entry = projects
            .entry(project_raw.clone())
            .or_insert_with(|| ProjectCost {
                project: display,
                usage: TokenUsage::default(),
                session_count: 0,
                total_cost: 0.0,
                models: Vec::new(),
            });
        entry.total_cost += stats.usage.cost_for_model(stats.model.as_deref());
        entry.usage.add(&stats.usage);
        entry.session_count += 1;
        if let Some(m) = &stats.model {
            let family = ModelPricing::name(Some(m)).to_string();
            if !entry.models.contains(&family) {
                entry.models.push(family);
            }
        }
    }

    let mut rows: Vec<ProjectCost> = projects.into_values().collect();
    rows.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
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
                    "cost_usd": r.total_cost,
                    "models": r.models,
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
        "Model(s)",
        "Cost (USD)",
    ]);

    let mut total_cost = 0.0f64;
    let mut total_usage = TokenUsage::default();
    let mut total_sessions = 0usize;

    for r in &rows {
        let model_str = if r.models.is_empty() {
            "-".to_string()
        } else {
            r.models.join("/")
        };
        table.add_row([
            short_name(&r.project),
            r.session_count.to_string(),
            fmt_tokens(r.usage.input_tokens),
            fmt_tokens(r.usage.output_tokens),
            fmt_tokens(r.usage.cache_read_tokens),
            model_str,
            format!("${:.4}", r.total_cost),
        ]);
        total_cost += r.total_cost;
        total_usage.add(&r.usage);
        total_sessions += r.session_count;
    }
    table.add_row([
        "TOTAL".to_string(),
        total_sessions.to_string(),
        fmt_tokens(total_usage.input_tokens),
        fmt_tokens(total_usage.output_tokens),
        fmt_tokens(total_usage.cache_read_tokens),
        String::new(),
        format!("${:.4}", total_cost),
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
        let cost = stats.usage.cost_for_model(stats.model.as_deref());
        rows.push((display_project_name(&decode_project_name(project_raw)), stats, cost));
    }
    rows.sort_by(|a, b| {
        b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(limit);

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|(project, stats, cost)| {
                serde_json::json!({
                    "project": project,
                    "session_id": stats.session_id,
                    "date": stats.first_timestamp.map(|d| d.to_rfc3339()),
                    "model": stats.model,
                    "input_tokens": stats.usage.input_tokens,
                    "output_tokens": stats.usage.output_tokens,
                    "cache_creation_tokens": stats.usage.cache_creation_tokens,
                    "cache_read_tokens": stats.usage.cache_read_tokens,
                    "cost_usd": cost,
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
        "Model",
        "Input",
        "Output",
        "Cost (USD)",
    ]);

    for (project, stats, cost) in &rows {
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
        let model = stats
            .model
            .as_deref()
            .map(|m| ModelPricing::name(Some(m)).to_string())
            .unwrap_or_else(|| "-".to_string());
        table.add_row([
            short_name(project),
            sid,
            date,
            model,
            fmt_tokens(stats.usage.input_tokens),
            fmt_tokens(stats.usage.output_tokens),
            format!("${:.4}", cost),
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

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::parser::parse_session;
use crate::store::{SessionStore, display_project_name, short_name};
use crate::types::{TokenUsage, model_label};

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
    /// Accumulated model-aware cost (each session contributes with its detected model).
    total_cost_usd: f64,
    /// Dominant model (most sessions).
    models: HashMap<String, usize>,
}

impl ProjectCost {
    fn dominant_model(&self) -> Option<&str> {
        self.models
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(m, _)| m.as_str())
    }
}

fn run_by_project(files: Vec<(String, PathBuf)>, limit: usize, json: bool) -> Result<()> {
    let mut projects: HashMap<String, ProjectCost> = HashMap::new();

    for (project_raw, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let session_cost = stats.usage.cost_for_model(stats.model.as_deref());
        let entry = projects
            .entry(project_raw.clone())
            .or_insert_with(|| ProjectCost {
                project: display_project_name(project_raw),
                usage: TokenUsage::default(),
                session_count: 0,
                total_cost_usd: 0.0,
                models: HashMap::new(),
            });
        entry.usage.add(&stats.usage);
        entry.session_count += 1;
        entry.total_cost_usd += session_cost;
        if let Some(m) = stats.model {
            *entry.models.entry(m).or_insert(0) += 1;
        }
    }

    let mut rows: Vec<ProjectCost> = projects.into_values().collect();
    rows.sort_by(|a, b| {
        b.total_cost_usd
            .partial_cmp(&a.total_cost_usd)
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
                    "cost_usd": r.total_cost_usd,
                    "model": r.dominant_model(),
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
        "Model",
        "Cost (USD)",
    ]);

    let mut total_cost = 0f64;
    let mut total_sessions = 0usize;
    let mut total_usage = TokenUsage::default();

    for r in &rows {
        let model_str = model_label(r.dominant_model());
        table.add_row([
            short_name(&r.project),
            r.session_count.to_string(),
            fmt_tokens(r.usage.input_tokens),
            fmt_tokens(r.usage.output_tokens),
            fmt_tokens(r.usage.cache_read_tokens),
            model_str.to_string(),
            format!("${:.4}", r.total_cost_usd),
        ]);
        total_usage.add(&r.usage);
        total_sessions += r.session_count;
        total_cost += r.total_cost_usd;
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
        rows.push((display_project_name(project_raw), stats));
    }
    rows.sort_by(|a, b| {
        let cost_a = a.1.usage.cost_for_model(a.1.model.as_deref());
        let cost_b = b.1.usage.cost_for_model(b.1.model.as_deref());
        cost_b
            .partial_cmp(&cost_a)
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
                    "model": stats.model,
                    "cost_usd": stats.usage.cost_for_model(stats.model.as_deref()),
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
        "Model",
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
        let cost = stats.usage.cost_for_model(stats.model.as_deref());
        let model_str = model_label(stats.model.as_deref());
        table.add_row([
            short_name(project),
            sid,
            date,
            fmt_tokens(stats.usage.input_tokens),
            fmt_tokens(stats.usage.output_tokens),
            model_str.to_string(),
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

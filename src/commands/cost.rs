use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use chrono::DateTime;

use crate::index::IndexStore;
use crate::parser::parse_session;
use crate::store::{SessionStore, decode_project_name, display_project_name, short_name};
use crate::types::{ModelPricing, TokenUsage};
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
        let rows = idx.query_cost_per_session(project, limit)?;

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
                        "model": r.model,
                        "input_tokens": r.input_tokens,
                        "output_tokens": r.output_tokens,
                        "cache_creation_tokens": r.cache_creation_tokens,
                        "cache_read_tokens": r.cache_read_tokens,
                        "cost_usd": r.cost_usd,
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
            "Model",
            "Input",
            "Output",
            "Cost (USD)",
        ]));
        ui::right_align(&mut table, &[4, 5, 6]);
        for r in &rows {
            let sid: String = r
                .session_id
                .as_deref()
                .unwrap_or("-")
                .chars()
                .take(8)
                .collect();
            let date = r
                .first_timestamp_ms
                .and_then(DateTime::from_timestamp_millis)
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "-".to_string());
            let model = r
                .model
                .as_deref()
                .map(|m| ModelPricing::name(Some(m)).to_string())
                .unwrap_or_else(|| "-".to_string());
            table.add_row([
                ui::cell_project(&short_name(&r.project)),
                ui::cell_dim(&sid),
                ui::cell_dim(&date),
                ui::cell_model(&model),
                ui::cell_count(r.input_tokens as u64),
                ui::cell_count(r.output_tokens as u64),
                ui::cell_cost(r.cost_usd),
            ]);
        }
        println!("{table}");
        return Ok(());
    }

    let rows = idx.query_cost_by_project(project, limit)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "project": r.project,
                    "sessions": r.session_count,
                    "input_tokens": r.input_tokens,
                    "output_tokens": r.output_tokens,
                    "cache_creation_tokens": r.cache_creation_tokens,
                    "cache_read_tokens": r.cache_read_tokens,
                    "cost_usd": r.cost_usd,
                    "models": r.models,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut table = ui::table();
    table.set_header(ui::header([
        "Project",
        "Sessions",
        "Input",
        "Output",
        "Cache Read",
        "Model(s)",
        "Cost (USD)",
    ]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 6]);

    let mut total_cost = 0.0f64;
    let mut total_in = 0i64;
    let mut total_out = 0i64;
    let mut total_cr = 0i64;
    let mut total_sessions = 0i64;

    for r in &rows {
        let model_str = if r.models.is_empty() {
            "-".to_string()
        } else {
            r.models.join("/")
        };
        table.add_row([
            ui::cell_project(&short_name(&r.project)),
            ui::cell_count(r.session_count as u64),
            ui::cell_count(r.input_tokens as u64),
            ui::cell_count(r.output_tokens as u64),
            ui::cell_count(r.cache_read_tokens as u64),
            ui::cell_model(&model_str),
            ui::cell_cost(r.cost_usd),
        ]);
        total_cost += r.cost_usd;
        total_in += r.input_tokens;
        total_out += r.output_tokens;
        total_cr += r.cache_read_tokens;
        total_sessions += r.session_count;
    }
    table.add_row(ui::total_row([
        "TOTAL".to_string(),
        ui::fmt_count(total_sessions as u64),
        ui::fmt_count(total_in as u64),
        ui::fmt_count(total_out as u64),
        ui::fmt_count(total_cr as u64),
        String::new(),
        ui::fmt_cost(total_cost),
    ]));
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
        run_by_project(files, limit, json)
    }
}

struct ProjectCost {
    project: String,
    usage: TokenUsage,
    session_count: usize,
    total_cost: f64,
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

    let mut table = ui::table();
    table.set_header(ui::header([
        "Project",
        "Sessions",
        "Input",
        "Output",
        "Cache Read",
        "Model(s)",
        "Cost (USD)",
    ]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 6]);

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
            ui::cell_project(&short_name(&r.project)),
            ui::cell_count(r.session_count as u64),
            ui::cell_count(r.usage.input_tokens),
            ui::cell_count(r.usage.output_tokens),
            ui::cell_count(r.usage.cache_read_tokens),
            ui::cell_model(&model_str),
            ui::cell_cost(r.total_cost),
        ]);
        total_cost += r.total_cost;
        total_usage.add(&r.usage);
        total_sessions += r.session_count;
    }
    table.add_row(ui::total_row([
        "TOTAL".to_string(),
        ui::fmt_count(total_sessions as u64),
        ui::fmt_count(total_usage.input_tokens),
        ui::fmt_count(total_usage.output_tokens),
        ui::fmt_count(total_usage.cache_read_tokens),
        String::new(),
        ui::fmt_cost(total_cost),
    ]));

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
        rows.push((
            display_project_name(&decode_project_name(project_raw)),
            stats,
            cost,
        ));
    }
    rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
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

    let mut table = ui::table();
    table.set_header(ui::header([
        "Project",
        "Session",
        "Date",
        "Model",
        "Input",
        "Output",
        "Cost (USD)",
    ]));
    ui::right_align(&mut table, &[4, 5, 6]);

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
            ui::cell_project(&short_name(project)),
            ui::cell_dim(&sid),
            ui::cell_dim(&date),
            ui::cell_model(&model),
            ui::cell_count(stats.usage.input_tokens),
            ui::cell_count(stats.usage.output_tokens),
            ui::cell_cost(*cost),
        ]);
    }
    println!("{table}");
    Ok(())
}

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use chrono::DateTime;

use crate::cli::ResolvedFilter;
use crate::index::IndexStore;
use crate::parser::SessionStats;
use crate::parser::parse_session;
use crate::providers::enabled_default;
use crate::store::{SessionStore, decode_project_name, display_project_name, short_name};
use crate::types::{ModelPricing, TokenUsage};
use crate::ui;

pub fn run(
    project: Option<&str>,
    per_session: bool,
    limit: usize,
    json: bool,
    no_index: bool,
    filter: &ResolvedFilter,
) -> Result<()> {
    if !no_index && let Ok(()) = run_indexed(project, per_session, limit, json, filter) {
        return Ok(());
    }
    run_from_files(project, per_session, limit, json, filter)
}

/// When the displayed rows were truncated to `--limit`, print a dim caption
/// reconciling the (full) TOTAL with the (partial) rows above it. `unit` is
/// `"projects"` or `"sessions"`.
fn print_truncation_note(shown: usize, total: i64, unit: &str) {
    if total > shown as i64 {
        println!(
            "{}",
            ui::note(&format!(
                "Showing top {shown} of {total} {unit} · TOTAL is the full sum."
            ))
        );
    }
}

fn run_indexed(
    project: Option<&str>,
    per_session: bool,
    limit: usize,
    json: bool,
    filter: &ResolvedFilter,
) -> Result<()> {
    let providers = enabled_default()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&providers)?;

    if per_session {
        let rows = idx.query_cost_per_session(project, filter, limit)?;

        if json {
            let output: Vec<_> = rows
                .iter()
                .map(|r| {
                    let date = r
                        .first_timestamp_ms
                        .and_then(DateTime::from_timestamp_millis)
                        .map(|d| d.to_rfc3339());
                    let model = single_model(&r.models);
                    serde_json::json!({
                        "provider": r.provider,
                        "project": r.project,
                        "session_id": r.session_id,
                        "date": date,
                        "model": model,
                        "models": r.models,
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

        let show_provider = ui::spans_providers(rows.iter().map(|r| r.provider.as_str()));
        let mut table = ui::table();
        let mut headers = vec![
            "Project",
            "Session",
            "Date",
            "Model(s)",
            "Input",
            "Output",
            "Cache Write",
            "Cache Read",
            "Cost (USD)",
        ];
        if show_provider {
            headers.insert(0, "Provider");
        }
        table.set_header(ui::header(headers));
        ui::right_align(
            &mut table,
            if show_provider {
                &[5, 6, 7, 8, 9]
            } else {
                &[4, 5, 6, 7, 8]
            },
        );
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
            let model = display_models(&r.models);
            let mut cells = vec![
                ui::cell_project(&short_name(&r.project)),
                ui::cell_dim(&sid),
                ui::cell_dim(&date),
                ui::cell_model(&model),
                ui::cell_count(r.input_tokens as u64),
                ui::cell_count(r.output_tokens as u64),
                ui::cell_count(r.cache_creation_tokens as u64),
                ui::cell_count(r.cache_read_tokens as u64),
                ui::cell_cost(r.cost_usd),
            ];
            if show_provider {
                cells.insert(0, ui::cell_provider(&r.provider));
            }
            table.add_row(cells);
        }
        let summary = idx.query_cost_summary(project, filter)?;
        let mut total_cells = vec![
            "TOTAL".to_string(),
            String::new(),
            String::new(),
            String::new(),
            ui::fmt_count(summary.input_tokens as u64),
            ui::fmt_count(summary.output_tokens as u64),
            ui::fmt_count(summary.cache_creation_tokens as u64),
            ui::fmt_count(summary.cache_read_tokens as u64),
            ui::fmt_cost(summary.cost_usd),
        ];
        if show_provider {
            total_cells.insert(0, String::new());
        }
        table.add_row(ui::total_row(total_cells));
        println!("{table}");
        print_truncation_note(rows.len(), summary.session_count, "sessions");
        return Ok(());
    }

    let rows = idx.query_cost_by_project(project, filter, limit)?;

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
                    "avg_cost_per_session_usd": if r.session_count == 0 {
                        0.0
                    } else {
                        r.cost_usd / r.session_count as f64
                    },
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
        "Cache Write",
        "Cache Read",
        "Model(s)",
        "Cost (USD)",
    ]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 5, 7]);

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
            ui::cell_count(r.cache_creation_tokens as u64),
            ui::cell_count(r.cache_read_tokens as u64),
            ui::cell_model(&model_str),
            ui::cell_cost(r.cost_usd),
        ]);
    }
    // TOTAL is the grand total across *all* matching projects, independent of
    // `--limit`, so it agrees with `models` (see `query_cost_summary`).
    let summary = idx.query_cost_summary(project, filter)?;
    table.add_row(ui::total_row([
        "TOTAL".to_string(),
        ui::fmt_count(summary.session_count as u64),
        ui::fmt_count(summary.input_tokens as u64),
        ui::fmt_count(summary.output_tokens as u64),
        ui::fmt_count(summary.cache_creation_tokens as u64),
        ui::fmt_count(summary.cache_read_tokens as u64),
        String::new(),
        ui::fmt_cost(summary.cost_usd),
    ]));
    println!("{table}");
    print_truncation_note(rows.len(), summary.project_count, "projects");
    Ok(())
}

fn run_from_files(
    project: Option<&str>,
    per_session: bool,
    limit: usize,
    json: bool,
    filter: &ResolvedFilter,
) -> Result<()> {
    let store = SessionStore::new()?;
    let files = store.all_session_files(project)?;
    if per_session {
        run_per_session(files, limit, json, filter)
    } else {
        run_by_project(files, limit, json, filter)
    }
}

struct ProjectCost {
    project: String,
    usage: TokenUsage,
    session_count: usize,
    total_cost: f64,
    models: Vec<String>,
}

fn run_by_project(
    files: Vec<(String, PathBuf)>,
    limit: usize,
    json: bool,
    filter: &ResolvedFilter,
) -> Result<()> {
    let mut projects: HashMap<String, ProjectCost> = HashMap::new();

    for (project_raw, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !filter.matches("claude", &stats, false) {
            continue;
        }
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
        entry.total_cost += stats.cost_usd();
        entry.usage.add(&stats.usage);
        entry.session_count += 1;
        for family in session_model_families(&stats) {
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

    // Grand totals over *all* projects, computed before truncating to `--limit`
    // so the TOTAL row reflects every project (matches the indexed path).
    let project_count = rows.len();
    let mut grand_cost = 0.0f64;
    let mut grand_usage = TokenUsage::default();
    let mut grand_sessions = 0usize;
    for r in &rows {
        grand_cost += r.total_cost;
        grand_usage.add(&r.usage);
        grand_sessions += r.session_count;
    }

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
                    "avg_cost_per_session_usd": if r.session_count == 0 {
                        0.0
                    } else {
                        r.total_cost / r.session_count as f64
                    },
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
        "Cache Write",
        "Cache Read",
        "Model(s)",
        "Cost (USD)",
    ]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 5, 7]);

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
            ui::cell_count(r.usage.cache_creation_tokens),
            ui::cell_count(r.usage.cache_read_tokens),
            ui::cell_model(&model_str),
            ui::cell_cost(r.total_cost),
        ]);
    }
    table.add_row(ui::total_row([
        "TOTAL".to_string(),
        ui::fmt_count(grand_sessions as u64),
        ui::fmt_count(grand_usage.input_tokens),
        ui::fmt_count(grand_usage.output_tokens),
        ui::fmt_count(grand_usage.cache_creation_tokens),
        ui::fmt_count(grand_usage.cache_read_tokens),
        String::new(),
        ui::fmt_cost(grand_cost),
    ]));

    println!("{table}");
    print_truncation_note(rows.len(), project_count as i64, "projects");
    Ok(())
}

fn run_per_session(
    files: Vec<(String, PathBuf)>,
    limit: usize,
    json: bool,
    filter: &ResolvedFilter,
) -> Result<()> {
    let mut rows = Vec::new();
    for (project_raw, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if stats.usage.total_tokens() == 0 {
            continue;
        }
        if !filter.matches("claude", &stats, false) {
            continue;
        }
        let cost = stats.cost_usd();
        rows.push((
            display_project_name(&decode_project_name(project_raw)),
            stats,
            cost,
        ));
    }
    rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Grand totals over *all* sessions, before truncating to `--limit`.
    let session_count = rows.len();
    let mut grand_cost = 0.0f64;
    let mut grand_usage = TokenUsage::default();
    for (_, stats, cost) in &rows {
        grand_cost += cost;
        grand_usage.add(&stats.usage);
    }

    rows.truncate(limit);

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|(project, stats, cost)| {
                serde_json::json!({
                    "provider": "claude",
                    "project": project,
                    "session_id": stats.session_id,
                    "date": stats.first_timestamp.map(|d| d.to_rfc3339()),
                    "model": single_model(&stats.model_names()),
                    "models": stats.model_names(),
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
        "Model(s)",
        "Input",
        "Output",
        "Cache Write",
        "Cache Read",
        "Cost (USD)",
    ]));
    ui::right_align(&mut table, &[4, 5, 6, 7, 8]);

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
        let model = display_models(&stats.model_names());
        table.add_row([
            ui::cell_project(&short_name(project)),
            ui::cell_dim(&sid),
            ui::cell_dim(&date),
            ui::cell_model(&model),
            ui::cell_count(stats.usage.input_tokens),
            ui::cell_count(stats.usage.output_tokens),
            ui::cell_count(stats.usage.cache_creation_tokens),
            ui::cell_count(stats.usage.cache_read_tokens),
            ui::cell_cost(*cost),
        ]);
    }
    table.add_row(ui::total_row([
        "TOTAL".to_string(),
        String::new(),
        String::new(),
        String::new(),
        ui::fmt_count(grand_usage.input_tokens),
        ui::fmt_count(grand_usage.output_tokens),
        ui::fmt_count(grand_usage.cache_creation_tokens),
        ui::fmt_count(grand_usage.cache_read_tokens),
        ui::fmt_cost(grand_cost),
    ]));
    println!("{table}");
    print_truncation_note(rows.len(), session_count as i64, "sessions");
    Ok(())
}

fn session_model_families(stats: &SessionStats) -> Vec<String> {
    let mut families = Vec::new();
    for model in stats.model_names() {
        let family = ModelPricing::name(Some(&model)).to_string();
        if !families.contains(&family) {
            families.push(family);
        }
    }
    if families.is_empty()
        && let Some(model) = &stats.model
    {
        families.push(ModelPricing::name(Some(model)).to_string());
    }
    families
}

fn display_models(models: &[String]) -> String {
    match models {
        [] => "-".to_string(),
        [single] => ModelPricing::name(Some(single)).to_string(),
        _ => "Mixed".to_string(),
    }
}

fn single_model(models: &[String]) -> Option<String> {
    match models {
        [single] => Some(single.clone()),
        _ => None,
    }
}

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use chrono::DateTime;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::index::IndexStore;
use crate::parser::parse_session;
use crate::store::{SessionStore, decode_project_name, display_project_name, short_name};
use crate::types::{ModelPricing, TokenUsage};

pub fn run(
    project: Option<&str>,
    per_session: bool,
    limit: usize,
    json: bool,
    no_index: bool,
) -> Result<()> {
    if !no_index {
        if let Ok(()) = run_indexed(project, per_session, limit, json) {
            return Ok(());
        }
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
                short_name(&r.project),
                sid,
                date,
                model,
                fmt_tokens(r.input_tokens as u64),
                fmt_tokens(r.output_tokens as u64),
                format!("${:.4}", r.cost_usd),
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
            short_name(&r.project),
            r.session_count.to_string(),
            fmt_tokens(r.input_tokens as u64),
            fmt_tokens(r.output_tokens as u64),
            fmt_tokens(r.cache_read_tokens as u64),
            model_str,
            format!("${:.4}", r.cost_usd),
        ]);
        total_cost += r.cost_usd;
        total_in += r.input_tokens;
        total_out += r.output_tokens;
        total_cr += r.cache_read_tokens;
        total_sessions += r.session_count;
    }
    table.add_row([
        "TOTAL".to_string(),
        total_sessions.to_string(),
        fmt_tokens(total_in as u64),
        fmt_tokens(total_out as u64),
        fmt_tokens(total_cr as u64),
        String::new(),
        format!("${:.4}", total_cost),
    ]);
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

#[cfg(test)]
mod tests {
    use super::fmt_tokens;

    #[test]
    fn fmt_tokens_small() {
        assert_eq!(fmt_tokens(0), "0");
        assert_eq!(fmt_tokens(999), "999");
    }

    #[test]
    fn fmt_tokens_thousands() {
        assert_eq!(fmt_tokens(1_000), "1.0K");
        assert_eq!(fmt_tokens(1_500), "1.5K");
    }

    #[test]
    fn fmt_tokens_millions() {
        assert_eq!(fmt_tokens(1_000_000), "1.0M");
        assert_eq!(fmt_tokens(2_500_000), "2.5M");
    }
}

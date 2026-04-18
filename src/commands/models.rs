use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::index::IndexStore;
use crate::store::SessionStore;
use crate::types::ModelPricing;

pub fn run(project: Option<&str>, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;

    let rows = idx.query_model_usage(project)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "model": r.model,
                    "model_family": ModelPricing::name(Some(&r.model)),
                    "session_count": r.session_count,
                    "input_tokens": r.input_tokens,
                    "output_tokens": r.output_tokens,
                    "cost_usd": r.cost_usd,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No model usage data found.");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Model", "Sessions", "Input", "Output", "Cost (USD)"]);
    let mut total_sessions = 0i64;
    let mut total_cost = 0.0f64;
    for r in &rows {
        let family = ModelPricing::name(Some(&r.model));
        let display = if r.model.is_empty() {
            family.to_string()
        } else {
            format!("{} ({})", family, r.model.trim_start_matches("claude-"))
        };
        table.add_row([
            display,
            r.session_count.to_string(),
            fmt_tokens(r.input_tokens as u64),
            fmt_tokens(r.output_tokens as u64),
            format!("${:.4}", r.cost_usd),
        ]);
        total_sessions += r.session_count;
        total_cost += r.cost_usd;
    }
    table.add_row([
        "TOTAL".to_string(),
        total_sessions.to_string(),
        String::new(),
        String::new(),
        format!("${:.4}", total_cost),
    ]);
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

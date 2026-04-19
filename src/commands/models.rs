use anyhow::Result;

use crate::index::IndexStore;
use crate::store::SessionStore;
use crate::types::ModelPricing;
use crate::ui;

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
                    "cache_creation_tokens": r.cache_creation_tokens,
                    "cache_read_tokens": r.cache_read_tokens,
                    "avg_cost_per_session_usd": r.avg_cost_per_session_usd,
                    "avg_tokens_per_session": r.avg_tokens_per_session,
                    "service_tiers": r.service_tiers,
                    "inference_geos": r.inference_geos,
                    "avg_speed": r.avg_speed,
                    "total_iterations": r.total_iterations,
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

    let mut table = ui::table();
    table.set_header(ui::header([
        "Model",
        "Sessions",
        "Input",
        "Output",
        "Cache Write",
        "Cache Read",
        "Avg/Session",
        "Avg Tokens",
        "Cost (USD)",
    ]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 5, 6, 7, 8]);
    let mut total_sessions = 0i64;
    let mut total_input = 0i64;
    let mut total_output = 0i64;
    let mut total_cache_creation = 0i64;
    let mut total_cache_read = 0i64;
    let mut total_cost = 0.0f64;
    for r in &rows {
        let family = ModelPricing::name(Some(&r.model));
        let display = if r.model.is_empty() {
            family.to_string()
        } else {
            format!("{} ({})", family, r.model.trim_start_matches("claude-"))
        };
        table.add_row([
            ui::cell_model(&display),
            ui::cell_count(r.session_count as u64),
            ui::cell_count(r.input_tokens as u64),
            ui::cell_count(r.output_tokens as u64),
            ui::cell_count(r.cache_creation_tokens as u64),
            ui::cell_count(r.cache_read_tokens as u64),
            ui::cell_cost(r.avg_cost_per_session_usd),
            ui::cell_count(r.avg_tokens_per_session.round() as u64),
            ui::cell_cost(r.cost_usd),
        ]);
        total_sessions += r.session_count;
        total_input += r.input_tokens;
        total_output += r.output_tokens;
        total_cache_creation += r.cache_creation_tokens;
        total_cache_read += r.cache_read_tokens;
        total_cost += r.cost_usd;
    }
    table.add_row(ui::total_row([
        "TOTAL".to_string(),
        ui::fmt_count(total_sessions as u64),
        ui::fmt_count(total_input as u64),
        ui::fmt_count(total_output as u64),
        ui::fmt_count(total_cache_creation as u64),
        ui::fmt_count(total_cache_read as u64),
        String::new(),
        String::new(),
        ui::fmt_cost(total_cost),
    ]));
    println!("{table}");
    Ok(())
}

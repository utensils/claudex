use anyhow::Result;

use crate::cli::ResolvedFilter;
use crate::commands::sessions::format_duration;
use crate::ui;
use claudex::index::IndexStore;
use claudex::providers::enabled_default;

pub fn run(weekly: bool, limit: usize, json: bool, filter: &ResolvedFilter) -> Result<()> {
    let providers = enabled_default()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&providers)?;
    idx.ensure_pr_links_fresh(&providers)?;
    let rows = idx.query_timeline(filter, weekly, limit)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "bucket": r.bucket,
                    "sessions": r.session_count,
                    "cost_usd": r.cost_usd,
                    "input_tokens": r.input_tokens,
                    "output_tokens": r.output_tokens,
                    "cache_creation_tokens": r.cache_creation_tokens,
                    "cache_read_tokens": r.cache_read_tokens,
                    "tool_calls": r.tool_calls,
                    "pr_count": r.pr_count,
                    "avg_turn_duration_ms": r.avg_turn_duration_ms,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No timeline data found.");
        return Ok(());
    }

    let mut table = ui::table();
    table.set_header(ui::header([
        if weekly { "Week" } else { "Day" },
        "Sessions",
        "Cost",
        "Tokens",
        "Tools",
        "PRs",
        "Avg Turn",
    ]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 5, 6]);
    for r in &rows {
        let total_tokens =
            r.input_tokens + r.output_tokens + r.cache_creation_tokens + r.cache_read_tokens;
        let avg_turn = r
            .avg_turn_duration_ms
            .map(|ms| format_duration(ms.round() as u64))
            .unwrap_or_else(|| "-".to_string());
        table.add_row([
            ui::cell_dim(&r.bucket),
            ui::cell_count(r.session_count as u64),
            ui::cell_cost(r.cost_usd),
            ui::cell_count(total_tokens as u64),
            ui::cell_count(r.tool_calls as u64),
            ui::cell_count(r.pr_count as u64),
            ui::cell_plain(avg_turn),
        ]);
    }
    println!("{table}");
    Ok(())
}

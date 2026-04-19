use anyhow::Result;

use crate::commands::sessions::format_duration;
use crate::index::IndexStore;
use crate::store::{SessionStore, short_name};
use crate::ui;

pub fn run(project: Option<&str>, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;

    let rows = idx.query_turn_stats(project, limit)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "project": r.project,
                    "turn_count": r.turn_count,
                    "avg_duration_ms": r.avg_duration_ms,
                    "p50_duration_ms": r.p50_duration_ms,
                    "p95_duration_ms": r.p95_duration_ms,
                    "max_duration_ms": r.max_duration_ms,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No turn timing data found.");
        return Ok(());
    }

    let mut table = ui::table();
    table.set_header(ui::header(["Project", "Turns", "Avg", "P50", "P95", "Max"]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 5]);
    for r in &rows {
        table.add_row([
            ui::cell_project(&short_name(&r.project)),
            ui::cell_count(r.turn_count as u64),
            ui::cell_plain(format_duration(r.avg_duration_ms as u64)),
            ui::cell_plain(format_duration(r.p50_duration_ms as u64)),
            ui::cell_plain(format_duration(r.p95_duration_ms as u64)),
            ui::cell_plain(format_duration(r.max_duration_ms as u64)),
        ]);
    }
    println!("{table}");
    Ok(())
}

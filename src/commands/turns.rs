use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::commands::sessions::format_duration;
use crate::index::IndexStore;
use crate::store::{SessionStore, short_name};

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

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Project", "Turns", "Avg", "P50", "P95", "Max"]);
    for r in &rows {
        table.add_row([
            short_name(&r.project),
            r.turn_count.to_string(),
            format_duration(r.avg_duration_ms as u64),
            format_duration(r.p50_duration_ms as u64),
            format_duration(r.p95_duration_ms as u64),
            format_duration(r.max_duration_ms as u64),
        ]);
    }
    println!("{table}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::commands::sessions::format_duration;

    #[test]
    fn format_duration_values_used_in_turns_table() {
        assert_eq!(format_duration(0), "-");
        assert_eq!(format_duration(1_000), "1s");
        assert_eq!(format_duration(60_000), "1m0s");
        assert_eq!(format_duration(90_000), "1m30s");
        assert_eq!(format_duration(3_600_000), "1h0m");
    }
}

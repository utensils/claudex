use anyhow::Result;
use chrono::DateTime;

use crate::cli::ResolvedFilter;
use crate::ui;
use claudex::index::IndexStore;
use claudex::providers::enabled_default;

pub fn run(json: bool, deep: bool, filter: &ResolvedFilter) -> Result<()> {
    let providers: Vec<_> = enabled_default()?
        .into_iter()
        .filter(|p| filter.includes_provider(p.id()))
        .collect();
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&providers)?;
    let rows = idx.provider_status(&providers, deep, filter)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "provider": r.provider,
                    "root_dir": r.root_dir,
                    "enabled": r.enabled,
                    "discovered_files": r.discovered_files,
                    "parse_failures": r.parse_failures,
                    "indexed_sessions": r.indexed_sessions,
                    "live_sessions": r.live_sessions,
                    "retained_sessions": r.retained_sessions,
                    "archived_sessions": r.archived_sessions,
                    "last_sync": r.last_sync.and_then(|s| DateTime::from_timestamp(s, 0)).map(|d| d.to_rfc3339()),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No provider roots found.");
        return Ok(());
    }

    let mut table = ui::table();
    table.set_header(ui::header([
        "Provider",
        "Root",
        "Files",
        "Indexed",
        "Live",
        "Retained",
        "Archived",
        "Parse Failures",
        "Last Sync",
    ]));
    ui::right_align(&mut table, &[2, 3, 4, 5, 6, 7]);
    for r in &rows {
        let last_sync = r
            .last_sync
            .and_then(|s| DateTime::from_timestamp(s, 0))
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".to_string());
        table.add_row([
            ui::cell_provider(&r.provider),
            ui::cell_dim(&r.root_dir),
            ui::cell_count(r.discovered_files as u64),
            ui::cell_count(r.indexed_sessions as u64),
            ui::cell_count(r.live_sessions as u64),
            ui::cell_count(r.retained_sessions as u64),
            ui::cell_count(r.archived_sessions as u64),
            r.parse_failures
                .map(|n| ui::cell_count(n as u64))
                .unwrap_or_else(|| ui::cell_dim("-")),
            ui::cell_dim(&last_sync),
        ]);
    }
    println!("{table}");
    Ok(())
}

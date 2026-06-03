use anyhow::Result;

use crate::cli::ResolvedFilter;
use crate::ui;
use claudex::index::IndexStore;
use claudex::providers::enabled_default;

pub fn run(
    force: bool,
    status: bool,
    prune_retained_days: Option<u64>,
    vacuum: bool,
) -> Result<()> {
    let providers = enabled_default()?;
    let mut idx = IndexStore::open()?;

    if force {
        eprintln!("Rebuilding index (full)...");
        let count = idx.force_rebuild(&providers)?;
        println!("Indexed {count} sessions.");
    } else if let Some(days) = prune_retained_days {
        let secs = (days as i64).saturating_mul(86_400);
        let pruned = idx.prune_retained(secs, &ResolvedFilter::default())?;
        println!("Pruned {pruned} retained sessions older than {days} days.");
    } else {
        eprintln!("Updating index...");
        let count = idx.sync_now(&providers)?;
        println!("Updated {count} sessions.");
    }

    if vacuum {
        idx.vacuum()?;
        println!("Vacuumed index database.");
    }

    if status {
        let stats = idx.retention_stats(&ResolvedFilter::default())?;
        let mut table = ui::table();
        table.set_header(ui::header(["Total", "Live", "Retained", "Archived"]));
        ui::right_align(&mut table, &[0, 1, 2, 3]);
        table.add_row([
            ui::cell_count(stats.total_sessions as u64),
            ui::cell_count(stats.live_sessions as u64),
            ui::cell_count(stats.retained_sessions as u64),
            ui::cell_count(stats.archived_sessions as u64),
        ]);
        println!("{table}");
    }
    Ok(())
}

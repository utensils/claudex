use anyhow::Result;
use chrono::DateTime;

use crate::index::IndexStore;
use crate::store::{SessionStore, short_name};
use crate::ui;

pub fn run(project: Option<&str>, path: Option<&str>, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;

    let rows = idx.query_file_mods(project, path, limit)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                let last_touched_at = r
                    .last_touched_timestamp_ms
                    .and_then(DateTime::from_timestamp_millis)
                    .map(|d| d.to_rfc3339());
                serde_json::json!({
                    "file_path": r.file_path,
                    "modification_count": r.modification_count,
                    "distinct_session_count": r.distinct_session_count,
                    "last_touched_at": last_touched_at,
                    "top_project": r.top_project,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No file modification data found.");
        return Ok(());
    }

    let mut table = ui::table();
    table.set_header(ui::header([
        "File Path",
        "Modifications",
        "Sessions",
        "Last Touched",
        "Top Project",
    ]));
    ui::right_align(&mut table, &[1, 2]);
    for r in &rows {
        let last_touched = r
            .last_touched_timestamp_ms
            .and_then(DateTime::from_timestamp_millis)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string());
        table.add_row([
            ui::cell_project(&short_name(&r.file_path)),
            ui::cell_count(r.modification_count as u64),
            ui::cell_count(r.distinct_session_count as u64),
            ui::cell_dim(&last_touched),
            ui::cell_dim(r.top_project.as_deref().unwrap_or("-")),
        ]);
    }
    println!("{table}");
    Ok(())
}

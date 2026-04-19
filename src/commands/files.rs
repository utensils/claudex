use anyhow::Result;

use crate::index::IndexStore;
use crate::store::SessionStore;
use crate::ui;

pub fn run(project: Option<&str>, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;

    let rows = idx.query_file_mods(project, limit)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "file_path": r.file_path,
                    "modification_count": r.modification_count,
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
    table.set_header(ui::header(["File Path", "Sessions"]));
    ui::right_align(&mut table, &[1]);
    for r in &rows {
        table.add_row([
            ui::cell_project(&r.file_path),
            ui::cell_count(r.modification_count as u64),
        ]);
    }
    println!("{table}");
    Ok(())
}

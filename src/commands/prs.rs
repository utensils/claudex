use anyhow::Result;

use crate::index::IndexStore;
use crate::store::{SessionStore, short_name};
use crate::ui;

pub fn run(project: Option<&str>, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;

    let rows = idx.query_pr_links(project, limit)?;

    if json {
        let output: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "project": r.project,
                    "session_id": r.session_id,
                    "pr_number": r.pr_number,
                    "pr_url": r.pr_url,
                    "pr_repository": r.pr_repository,
                    "timestamp": r.timestamp,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No PR links found.");
        return Ok(());
    }

    let mut table = ui::table();
    table.set_header(ui::header(["Project", "PR #", "Repository", "URL"]));
    ui::right_align(&mut table, &[1]);
    for r in &rows {
        let sid: String = r
            .session_id
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(8)
            .collect();
        let repo = if r.pr_repository.is_empty() {
            sid
        } else {
            r.pr_repository.clone()
        };
        table.add_row([
            ui::cell_project(&short_name(&r.project)),
            ui::cell_plain(format!("#{}", ui::fmt_count(r.pr_number as u64))),
            ui::cell_dim(&repo),
            ui::cell_plain(r.pr_url.clone()),
        ]);
    }
    println!("{table}");
    Ok(())
}

use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::index::IndexStore;
use crate::store::{SessionStore, short_name};

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

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Project", "PR #", "Repository", "URL"]);
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
            short_name(&r.project),
            format!("#{}", r.pr_number),
            repo,
            r.pr_url.clone(),
        ]);
    }
    println!("{table}");
    Ok(())
}

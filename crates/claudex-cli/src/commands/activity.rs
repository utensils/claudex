use anyhow::Result;
use chrono::DateTime;

use crate::cli::ResolvedFilter;
use crate::commands::sessions::format_duration;
use crate::ui;
use claudex::index::IndexStore;
use claudex::providers::enabled_default;
use claudex::store::short_name;

pub fn run(limit: usize, json: bool, filter: &ResolvedFilter) -> Result<()> {
    let providers = enabled_default()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&providers)?;
    idx.ensure_pr_links_fresh(&providers)?;
    let data = idx.query_activity(filter, limit)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "summary": {
                    "sessions": data.summary.total_sessions,
                    "cost_usd": data.summary.total_cost,
                    "tokens": data.summary.total_input_tokens + data.summary.total_output_tokens + data.summary.total_cache_creation + data.summary.total_cache_read,
                    "pr_count": data.summary.pr_count,
                    "files_modified_count": data.summary.files_modified_count,
                    "avg_turn_duration_ms": data.summary.avg_turn_duration_ms,
                },
                "recent_sessions": data.recent_sessions.iter().map(|s| serde_json::json!({
                    "provider": s.provider,
                    "project": s.project_name,
                    "session_id": s.session_id,
                    "date": s.last_timestamp_ms.or(s.first_timestamp_ms).and_then(DateTime::from_timestamp_millis).map(|d| d.to_rfc3339()),
                    "model": s.model,
                    "cost_known": true,
                })).collect::<Vec<_>>(),
                "recent_prs": data.recent_prs.iter().map(|p| serde_json::json!({
                    "provider": p.provider,
                    "project": p.project,
                    "session_id": p.session_id,
                    "timestamp": p.timestamp,
                    "pr_number": p.pr_number,
                    "pr_repository": p.pr_repository,
                    "pr_url": p.pr_url,
                })).collect::<Vec<_>>(),
                "hot_files": data.hot_files.iter().map(|f| serde_json::json!({
                    "file_path": f.file_path,
                    "modification_count": f.modification_count,
                    "distinct_session_count": f.distinct_session_count,
                    "top_project": f.top_project,
                })).collect::<Vec<_>>(),
                "slow_projects": data.slow_projects.iter().map(|t| serde_json::json!({
                    "project": t.project,
                    "turn_count": t.turn_count,
                    "avg_duration_ms": t.avg_duration_ms,
                    "p95_duration_ms": t.p95_duration_ms,
                })).collect::<Vec<_>>(),
            }))?
        );
        return Ok(());
    }

    println!(
        "Sessions: {}",
        ui::fmt_count(data.summary.total_sessions as u64)
    );
    println!("Cost:     {}", ui::fmt_cost(data.summary.total_cost));
    println!(
        "Tokens:   {}",
        ui::fmt_count(
            (data.summary.total_input_tokens
                + data.summary.total_output_tokens
                + data.summary.total_cache_creation
                + data.summary.total_cache_read) as u64
        )
    );
    println!();

    print_sessions(&data.recent_sessions);
    print_prs(&data.recent_prs);
    print_files(&data.hot_files);
    print_slow(&data.slow_projects);
    Ok(())
}

fn print_sessions(rows: &[claudex::index::IndexedSession]) {
    if rows.is_empty() {
        return;
    }
    println!("{}", ui::emphasis("Recent sessions"));
    let mut table = ui::table();
    table.set_header(ui::header([
        "Provider", "Project", "Session", "When", "Model",
    ]));
    for s in rows {
        let when = s
            .last_timestamp_ms
            .or(s.first_timestamp_ms)
            .and_then(DateTime::from_timestamp_millis)
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".to_string());
        table.add_row([
            ui::cell_provider(&s.provider),
            ui::cell_project(&short_name(&s.project_name)),
            ui::cell_dim(s.session_id.as_deref().unwrap_or("-")),
            ui::cell_dim(&when),
            ui::cell_model(s.model.as_deref().unwrap_or("-")),
        ]);
    }
    println!("{table}");
}

fn print_prs(rows: &[claudex::index::PrLinkRow]) {
    if rows.is_empty() {
        return;
    }
    println!("{}", ui::emphasis("Recent PRs"));
    let mut table = ui::table();
    table.set_header(ui::header(["Provider", "Repo", "PR", "When"]));
    for p in rows {
        table.add_row([
            ui::cell_provider(&p.provider),
            ui::cell_project(&p.pr_repository),
            ui::cell_dim(&format!("#{}", p.pr_number)),
            ui::cell_dim(&p.timestamp),
        ]);
    }
    println!("{table}");
}

fn print_files(rows: &[claudex::index::FileModRow]) {
    if rows.is_empty() {
        return;
    }
    println!("{}", ui::emphasis("Hot files"));
    let mut table = ui::table();
    table.set_header(ui::header(["File", "Edits", "Sessions"]));
    ui::right_align(&mut table, &[1, 2]);
    for f in rows {
        table.add_row([
            ui::cell_project(&short_name(&f.file_path)),
            ui::cell_count(f.modification_count as u64),
            ui::cell_count(f.distinct_session_count as u64),
        ]);
    }
    println!("{table}");
}

fn print_slow(rows: &[claudex::index::TurnStatsRow]) {
    if rows.is_empty() {
        return;
    }
    println!("{}", ui::emphasis("Slow projects"));
    let mut table = ui::table();
    table.set_header(ui::header(["Project", "Turns", "Avg", "P95"]));
    ui::right_align(&mut table, &[1, 2, 3]);
    for t in rows {
        table.add_row([
            ui::cell_project(&short_name(&t.project)),
            ui::cell_count(t.turn_count as u64),
            ui::cell_plain(format_duration(t.avg_duration_ms.round() as u64)),
            ui::cell_plain(format_duration(t.p95_duration_ms.round() as u64)),
        ]);
    }
    println!("{table}");
}

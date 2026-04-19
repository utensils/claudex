use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::DateTime;

use crate::commands::sessions::format_duration;
use crate::index::{
    IndexStore, SessionDetail, SessionModelUsageRow, StopReasonRow, ToolRow, TurnStatsRow,
};
use crate::parser::{ModelSessionStats, SessionStats, parse_session};
use crate::store::{SessionStore, decode_project_name, display_project_name, short_name};
use crate::types::ModelPricing;
use crate::ui;

pub fn run(selector: &str, project_filter: Option<&str>, json: bool, no_index: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let (project_raw, path) = resolve_one_session(&store, selector, project_filter)?;
    let project = display_project_name(&decode_project_name(&project_raw));

    if !no_index {
        let mut idx = IndexStore::open()?;
        idx.ensure_fresh(&store)?;
        if let Some(detail) = idx.query_session_detail(&path.to_string_lossy())? {
            return render_indexed(detail, json);
        }
    }

    let stats = parse_session(&path)?;
    render_from_file(&project, &path, stats, json)
}

fn render_indexed(detail: SessionDetail, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&indexed_json(&detail))?);
        return Ok(());
    }

    section("Overview");
    println!("  Project:      {}", ui::project(&detail.project));
    println!("  File:         {}", detail.file_path);
    println!(
        "  Session:      {}",
        ui::session_id(short_session_id(detail.session_id.as_deref()))
    );
    if let Some(date) = detail
        .first_timestamp_ms
        .and_then(DateTime::from_timestamp_millis)
    {
        println!("  Started:      {}", date.format("%Y-%m-%d %H:%M UTC"));
    }
    if let Some(date) = detail
        .last_timestamp_ms
        .and_then(DateTime::from_timestamp_millis)
    {
        println!("  Last activity: {}", date.format("%Y-%m-%d %H:%M UTC"));
    }
    println!(
        "  Duration:     {}",
        format_duration(detail.duration_ms as u64)
    );
    println!(
        "  Messages:     {}",
        ui::fmt_count(detail.message_count as u64)
    );
    println!(
        "  Model:        {}",
        detail
            .model
            .as_deref()
            .map(display_session_model)
            .unwrap_or_else(|| "-".to_string())
    );
    println!("  Cost:         {}", ui::cost(detail.cost_usd));

    print_tokens(
        detail.input_tokens as u64,
        detail.output_tokens as u64,
        detail.cache_creation_tokens as u64,
        detail.cache_read_tokens as u64,
    );

    if !detail.model_usage.is_empty() {
        print_models_indexed(&detail.model_usage);
    }
    if let Some(turn_stats) = &detail.turn_stats {
        print_turn_stats(turn_stats);
    }
    if detail.thinking_block_count > 0 {
        section("Thinking");
        println!(
            "  Blocks: {}",
            ui::fmt_count(detail.thinking_block_count as u64)
        );
    }
    print_tools(&detail.tools);
    print_files(&detail.files_modified);
    print_prs(&detail.pr_links);
    print_stop_reasons(&detail.stop_reasons);
    print_attachments_indexed(&detail.attachments);
    print_permission_changes_indexed(&detail.permission_changes);

    println!();
    Ok(())
}

fn render_from_file(project: &str, path: &Path, stats: SessionStats, json: bool) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&file_json(project, path, &stats))?
        );
        return Ok(());
    }

    section("Overview");
    println!("  Project:      {}", ui::project(project));
    println!("  File:         {}", path.display());
    println!(
        "  Session:      {}",
        ui::session_id(short_session_id(
            stats
                .session_id
                .as_deref()
                .or_else(|| path.file_stem().and_then(|s| s.to_str()))
        ))
    );
    if let Some(date) = stats.first_timestamp {
        println!("  Started:      {}", date.format("%Y-%m-%d %H:%M UTC"));
    }
    if let Some(date) = stats.last_timestamp {
        println!("  Last activity: {}", date.format("%Y-%m-%d %H:%M UTC"));
    }
    println!(
        "  Duration:     {}",
        format_duration(stats.total_duration_ms)
    );
    println!(
        "  Messages:     {}",
        ui::fmt_count(stats.message_count as u64)
    );
    println!(
        "  Model:        {}",
        stats
            .model_names()
            .as_slice()
            .first()
            .map(|m| {
                if stats.model_names().len() == 1 {
                    display_session_model(m)
                } else {
                    "Mixed".to_string()
                }
            })
            .or_else(|| stats.model.as_ref().map(|m| display_session_model(m)))
            .unwrap_or_else(|| "-".to_string())
    );
    println!("  Cost:         {}", ui::cost(stats.cost_usd()));

    print_tokens(
        stats.usage.input_tokens,
        stats.usage.output_tokens,
        stats.usage.cache_creation_tokens,
        stats.usage.cache_read_tokens,
    );

    if !stats.model_usage.is_empty() {
        print_models_file(&stats.model_usage);
    }
    if let Some(turn_stats) = build_turn_stats(project, &stats.turn_durations) {
        print_turn_stats(&turn_stats);
    }
    if stats.thinking_block_count > 0 {
        section("Thinking");
        println!("  Blocks: {}", ui::fmt_count(stats.thinking_block_count));
    }

    let tools = tool_rows_from_names(&stats.tool_names);
    print_tools(&tools);
    print_files(&stats.file_paths_modified);
    print_prs_file(project, stats.session_id.as_deref(), &stats.pr_links);
    let stop_reasons = stop_reason_rows(&stats.stop_reason_counts);
    print_stop_reasons(&stop_reasons);
    print_attachments_file(&stats.attachments);
    print_permission_changes_file(&stats.permission_modes);

    println!();
    Ok(())
}

fn resolve_one_session(
    store: &SessionStore,
    selector: &str,
    project_filter: Option<&str>,
) -> Result<(String, PathBuf)> {
    let all_files = store.all_session_files(project_filter)?;
    let matches = find_matching(&all_files, selector);
    match matches.as_slice() {
        [] => bail!("no sessions found matching {:?}", selector),
        [single] => Ok((single.0.clone(), single.1.clone())),
        many => {
            let mut preview = Vec::new();
            for (project_raw, path) in many.iter().take(8) {
                let sid = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "?".to_string());
                preview.push(format!(
                    "{}  {}",
                    short_session_id(Some(&sid)),
                    short_name(&display_project_name(&decode_project_name(project_raw))),
                ));
            }
            bail!(
                "selector {:?} matched {} sessions; refine it:\n{}",
                selector,
                many.len(),
                preview.join("\n")
            )
        }
    }
}

fn find_matching<'a>(files: &'a [(String, PathBuf)], selector: &str) -> Vec<&'a (String, PathBuf)> {
    let sel = selector.to_lowercase();

    let id_matches: Vec<_> = files
        .iter()
        .filter(|(_, path)| {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            stem.starts_with(&sel) || stem.contains(&sel)
        })
        .collect();

    if !id_matches.is_empty() {
        return id_matches;
    }

    files
        .iter()
        .filter(|(project_raw, _)| {
            let decoded = decode_project_name(project_raw).to_lowercase();
            project_raw.to_lowercase().contains(&sel) || decoded.contains(&sel)
        })
        .collect()
}

fn indexed_json(detail: &SessionDetail) -> serde_json::Value {
    serde_json::json!({
        "project": detail.project,
        "file_path": detail.file_path,
        "session_id": detail.session_id,
        "date": detail.first_timestamp_ms.and_then(DateTime::from_timestamp_millis).map(|d| d.to_rfc3339()),
        "last_activity": detail.last_timestamp_ms.and_then(DateTime::from_timestamp_millis).map(|d| d.to_rfc3339()),
        "duration_ms": detail.duration_ms,
        "message_count": detail.message_count,
        "model": detail.model,
        "input_tokens": detail.input_tokens,
        "output_tokens": detail.output_tokens,
        "cache_creation_tokens": detail.cache_creation_tokens,
        "cache_read_tokens": detail.cache_read_tokens,
        "total_tokens": detail.input_tokens + detail.output_tokens + detail.cache_creation_tokens + detail.cache_read_tokens,
        "cost_usd": detail.cost_usd,
        "thinking_block_count": detail.thinking_block_count,
        "turn_stats": detail.turn_stats.as_ref().map(turn_stats_json),
        "models": detail.model_usage.iter().map(indexed_model_json).collect::<Vec<_>>(),
        "tools": detail.tools.iter().map(|t| serde_json::json!({"tool": t.tool_name, "count": t.count})).collect::<Vec<_>>(),
        "files_modified": detail.files_modified,
        "pr_links": detail.pr_links.iter().map(|p| serde_json::json!({
            "pr_number": p.pr_number,
            "pr_url": p.pr_url,
            "pr_repository": p.pr_repository,
            "timestamp": p.timestamp,
        })).collect::<Vec<_>>(),
        "stop_reasons": detail.stop_reasons.iter().map(|r| serde_json::json!({"stop_reason": r.stop_reason, "count": r.count})).collect::<Vec<_>>(),
        "attachments": detail.attachments.iter().map(|a| serde_json::json!({"filename": a.filename, "mime_type": a.mime_type})).collect::<Vec<_>>(),
        "permission_changes": detail.permission_changes.iter().map(|p| serde_json::json!({"mode": p.mode, "timestamp": p.timestamp})).collect::<Vec<_>>(),
    })
}

fn file_json(project: &str, path: &Path, stats: &SessionStats) -> serde_json::Value {
    let turn_stats = build_turn_stats(project, &stats.turn_durations);
    let stop_reasons = stop_reason_rows(&stats.stop_reason_counts);
    let tools = tool_rows_from_names(&stats.tool_names);
    serde_json::json!({
        "project": project,
        "file_path": path.to_string_lossy().into_owned(),
        "session_id": stats.session_id.clone().or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned())),
        "date": stats.first_timestamp.map(|d| d.to_rfc3339()),
        "last_activity": stats.last_timestamp.map(|d| d.to_rfc3339()),
        "duration_ms": stats.total_duration_ms,
        "message_count": stats.message_count,
        "model": if stats.model_names().len() == 1 { stats.model_names().first().cloned() } else { stats.model.clone() },
        "input_tokens": stats.usage.input_tokens,
        "output_tokens": stats.usage.output_tokens,
        "cache_creation_tokens": stats.usage.cache_creation_tokens,
        "cache_read_tokens": stats.usage.cache_read_tokens,
        "total_tokens": stats.usage.total_tokens(),
        "cost_usd": stats.cost_usd(),
        "thinking_block_count": stats.thinking_block_count,
        "turn_stats": turn_stats.as_ref().map(turn_stats_json),
        "models": model_stats_rows(stats).iter().map(file_model_json).collect::<Vec<_>>(),
        "tools": tools.iter().map(|t| serde_json::json!({"tool": t.tool_name, "count": t.count})).collect::<Vec<_>>(),
        "files_modified": stats.file_paths_modified,
        "pr_links": stats.pr_links.iter().map(|(pr_number, pr_url, pr_repository, timestamp)| serde_json::json!({
            "pr_number": pr_number,
            "pr_url": pr_url,
            "pr_repository": pr_repository,
            "timestamp": timestamp,
        })).collect::<Vec<_>>(),
        "stop_reasons": stop_reasons.iter().map(|r| serde_json::json!({"stop_reason": r.stop_reason, "count": r.count})).collect::<Vec<_>>(),
        "attachments": stats.attachments.iter().map(|(filename, mime_type)| serde_json::json!({"filename": filename, "mime_type": mime_type})).collect::<Vec<_>>(),
        "permission_changes": stats.permission_modes.iter().map(|(mode, timestamp)| serde_json::json!({"mode": mode, "timestamp": timestamp})).collect::<Vec<_>>(),
    })
}

fn indexed_model_json(row: &SessionModelUsageRow) -> serde_json::Value {
    serde_json::json!({
        "model": row.model,
        "model_family": ModelPricing::name(Some(&row.model)),
        "assistant_message_count": row.assistant_message_count,
        "input_tokens": row.input_tokens,
        "output_tokens": row.output_tokens,
        "cache_creation_tokens": row.cache_creation_tokens,
        "cache_read_tokens": row.cache_read_tokens,
        "cost_usd": row.cost_usd,
        "inference_geos": row.inference_geos,
        "service_tiers": row.service_tiers,
        "avg_speed": row.avg_speed,
        "iterations": row.iterations,
    })
}

fn file_model_json((model, stats): &(String, ModelSessionStats)) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "model_family": ModelPricing::name(Some(model)),
        "assistant_message_count": stats.assistant_message_count,
        "input_tokens": stats.usage.input_tokens,
        "output_tokens": stats.usage.output_tokens,
        "cache_creation_tokens": stats.usage.cache_creation_tokens,
        "cache_read_tokens": stats.usage.cache_read_tokens,
        "cost_usd": stats.usage.cost_for_model(Some(model)),
        "inference_geos": stats.inference_geos.iter().cloned().collect::<Vec<_>>(),
        "service_tiers": stats.service_tiers.iter().cloned().collect::<Vec<_>>(),
        "avg_speed": stats.avg_speed(),
        "iterations": stats.iterations,
    })
}

fn turn_stats_json(turn_stats: &TurnStatsRow) -> serde_json::Value {
    serde_json::json!({
        "turn_count": turn_stats.turn_count,
        "avg_duration_ms": turn_stats.avg_duration_ms,
        "p50_duration_ms": turn_stats.p50_duration_ms,
        "p95_duration_ms": turn_stats.p95_duration_ms,
        "max_duration_ms": turn_stats.max_duration_ms,
    })
}

fn model_stats_rows(stats: &SessionStats) -> Vec<(String, ModelSessionStats)> {
    let mut rows = stats
        .model_usage
        .iter()
        .map(|(model, detail)| (model.clone(), detail.clone()))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.1.usage
            .cost_for_model(Some(&b.0))
            .partial_cmp(&a.1.usage.cost_for_model(Some(&a.0)))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

fn tool_rows_from_names(names: &[String]) -> Vec<ToolRow> {
    let mut counts = HashMap::new();
    for name in names {
        *counts.entry(name.clone()).or_insert(0i64) += 1;
    }
    let mut rows = counts
        .into_iter()
        .map(|(tool_name, count)| ToolRow { tool_name, count })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.tool_name.cmp(&b.tool_name))
    });
    rows
}

fn stop_reason_rows(counts: &HashMap<String, u64>) -> Vec<StopReasonRow> {
    let mut rows = counts
        .iter()
        .map(|(stop_reason, count)| StopReasonRow {
            stop_reason: stop_reason.clone(),
            count: *count as i64,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.stop_reason.cmp(&b.stop_reason))
    });
    rows
}

fn build_turn_stats(project: &str, turns: &[(u64, String)]) -> Option<TurnStatsRow> {
    if turns.is_empty() {
        return None;
    }
    let mut durations = turns.iter().map(|(dur, _)| *dur as i64).collect::<Vec<_>>();
    durations.sort_unstable();
    let turn_count = durations.len() as i64;
    let avg_duration_ms = durations.iter().sum::<i64>() as f64 / turn_count as f64;
    Some(TurnStatsRow {
        project: project.to_string(),
        turn_count,
        avg_duration_ms,
        p50_duration_ms: percentile_sorted(&durations, 50),
        p95_duration_ms: percentile_sorted(&durations, 95),
        max_duration_ms: *durations.last().unwrap_or(&0),
    })
}

fn percentile_sorted(sorted: &[i64], p: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p * sorted.len()).saturating_sub(1) / 100;
    sorted[idx.min(sorted.len() - 1)] as f64
}

fn print_tokens(input: u64, output: u64, cache_write: u64, cache_read: u64) {
    section("Tokens");
    println!("  Input:       {}", ui::count(input));
    println!("  Output:      {}", ui::count(output));
    println!("  Cache write: {}", ui::count(cache_write));
    println!("  Cache read:  {}", ui::count(cache_read));
    println!(
        "  Total:       {}",
        ui::emphasis(&ui::count(input + output + cache_write + cache_read))
    );
}

fn print_models_indexed(rows: &[SessionModelUsageRow]) {
    section("Models");
    let mut table = ui::table();
    table.set_header(ui::header([
        "Model",
        "Msgs",
        "Input",
        "Output",
        "Cache Read",
        "Cost",
    ]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 5]);
    for row in rows {
        table.add_row([
            ui::cell_model(&display_session_model(&row.model)),
            ui::cell_count(row.assistant_message_count as u64),
            ui::cell_count(row.input_tokens as u64),
            ui::cell_count(row.output_tokens as u64),
            ui::cell_count(row.cache_read_tokens as u64),
            ui::cell_cost(row.cost_usd),
        ]);
    }
    println!("{table}");
}

fn print_models_file(rows: &std::collections::BTreeMap<String, ModelSessionStats>) {
    let rows = model_stats_rows(&SessionStats {
        model_usage: rows.clone(),
        ..SessionStats::default()
    });
    section("Models");
    let mut table = ui::table();
    table.set_header(ui::header([
        "Model",
        "Msgs",
        "Input",
        "Output",
        "Cache Read",
        "Cost",
    ]));
    ui::right_align(&mut table, &[1, 2, 3, 4, 5]);
    for (model, row) in rows {
        table.add_row([
            ui::cell_model(&display_session_model(&model)),
            ui::cell_count(row.assistant_message_count),
            ui::cell_count(row.usage.input_tokens),
            ui::cell_count(row.usage.output_tokens),
            ui::cell_count(row.usage.cache_read_tokens),
            ui::cell_cost(row.usage.cost_for_model(Some(&model))),
        ]);
    }
    println!("{table}");
}

fn print_turn_stats(turn_stats: &TurnStatsRow) {
    section("Turns");
    println!("  Turns: {}", ui::fmt_count(turn_stats.turn_count as u64));
    println!(
        "  Avg / P50 / P95 / Max: {} / {} / {} / {}",
        format_duration(turn_stats.avg_duration_ms as u64),
        format_duration(turn_stats.p50_duration_ms as u64),
        format_duration(turn_stats.p95_duration_ms as u64),
        format_duration(turn_stats.max_duration_ms as u64),
    );
}

fn print_tools(tools: &[ToolRow]) {
    section("Tools");
    if tools.is_empty() {
        println!("  (none)");
        return;
    }
    for row in tools {
        println!(
            "  {}  {}",
            ui::tool_name(&row.tool_name),
            ui::fmt_count(row.count as u64)
        );
    }
}

fn print_files(files: &[String]) {
    section("Files");
    if files.is_empty() {
        println!("  (none)");
        return;
    }
    for file in files {
        println!("  {}", file);
    }
}

fn print_prs(prs: &[crate::index::PrLinkRow]) {
    section("PR Links");
    if prs.is_empty() {
        println!("  (none)");
        return;
    }
    for pr in prs {
        let repo = if pr.pr_repository.is_empty() {
            "-".to_string()
        } else {
            pr.pr_repository.clone()
        };
        println!(
            "  #{}  {}  {}",
            pr.pr_number,
            ui::timestamp(&repo),
            pr.pr_url
        );
    }
}

fn print_prs_file(project: &str, session_id: Option<&str>, prs: &[(i64, String, String, String)]) {
    let rows = prs
        .iter()
        .map(
            |(pr_number, pr_url, pr_repository, timestamp)| crate::index::PrLinkRow {
                project: project.to_string(),
                session_id: session_id.map(|s| s.to_string()),
                pr_number: *pr_number,
                pr_url: pr_url.clone(),
                pr_repository: pr_repository.clone(),
                timestamp: timestamp.clone(),
            },
        )
        .collect::<Vec<_>>();
    print_prs(&rows);
}

fn print_stop_reasons(rows: &[StopReasonRow]) {
    section("Stop Reasons");
    if rows.is_empty() {
        println!("  (none)");
        return;
    }
    for row in rows {
        println!(
            "  {}  {}",
            ui::role(&row.stop_reason),
            ui::fmt_count(row.count as u64)
        );
    }
}

fn print_attachments_indexed(rows: &[crate::index::AttachmentRow]) {
    section("Attachments");
    if rows.is_empty() {
        println!("  (none)");
        return;
    }
    for row in rows {
        if row.mime_type.is_empty() {
            println!("  {}", row.filename);
        } else {
            println!("  {}  {}", row.filename, ui::timestamp(&row.mime_type));
        }
    }
}

fn print_attachments_file(rows: &[(String, String)]) {
    section("Attachments");
    if rows.is_empty() {
        println!("  (none)");
        return;
    }
    for (filename, mime) in rows {
        if mime.is_empty() {
            println!("  {}", filename);
        } else {
            println!("  {}  {}", filename, ui::timestamp(mime));
        }
    }
}

fn print_permission_changes_indexed(rows: &[crate::index::PermissionChangeRow]) {
    section("Permission Changes");
    if rows.is_empty() {
        println!("  (none)");
        return;
    }
    for row in rows {
        if row.timestamp.is_empty() {
            println!("  {}", row.mode);
        } else {
            println!("  {}  {}", row.mode, ui::timestamp(&row.timestamp));
        }
    }
}

fn print_permission_changes_file(rows: &[(String, String)]) {
    section("Permission Changes");
    if rows.is_empty() {
        println!("  (none)");
        return;
    }
    for (mode, timestamp) in rows {
        if timestamp.is_empty() {
            println!("  {}", mode);
        } else {
            println!("  {}  {}", mode, ui::timestamp(timestamp));
        }
    }
}

fn short_session_id(session_id: Option<&str>) -> &str {
    session_id.unwrap_or("-")
}

fn display_session_model(model: &str) -> String {
    if model == "mixed" {
        "Mixed".to_string()
    } else if model.is_empty() {
        "-".to_string()
    } else {
        model.trim_start_matches("claude-").to_string()
    }
}

fn section(title: &str) {
    println!("\n{}", ui::section_title(title));
    println!("{}", "─".repeat(title.len()));
}

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Utc};
use owo_colors::OwoColorize;

use crate::index::IndexStore;
use crate::parser::parse_session;
use crate::store::{SessionStore, decode_project_name, display_project_name};
use crate::types::TokenUsage;

pub fn run(json: bool, no_index: bool) -> Result<()> {
    if !no_index {
        if let Ok(()) = run_indexed(json) {
            return Ok(());
        }
    }
    run_from_files(json)
}

fn run_indexed(json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;
    let data = idx.query_summary()?;

    if json {
        let out = serde_json::json!({
            "total_sessions": data.total_sessions,
            "sessions_today": data.sessions_today,
            "sessions_this_week": data.sessions_this_week,
            "total_cost_usd": data.total_cost,
            "cost_this_week_usd": data.week_cost,
            "total_tokens": data.total_input_tokens + data.total_output_tokens
                            + data.total_cache_creation + data.total_cache_read,
            "thinking_block_count": data.thinking_block_count,
            "avg_turn_duration_ms": data.avg_turn_duration_ms,
            "pr_count": data.pr_count,
            "files_modified_count": data.files_modified_count,
            "top_projects": data.top_projects.iter()
                .map(|(p, c)| serde_json::json!({"project": p, "sessions": c}))
                .collect::<Vec<_>>(),
            "top_tools": data.top_tools.iter()
                .map(|(t, c)| serde_json::json!({"tool": t, "calls": c}))
                .collect::<Vec<_>>(),
            "model_distribution": data.model_distribution.iter()
                .map(|(m, s, c)| serde_json::json!({"model": m, "sessions": s, "cost_usd": c}))
                .collect::<Vec<_>>(),
            "most_recent": data.most_recent.as_ref().map(|r| {
                let date = DateTime::from_timestamp_millis(r.first_timestamp_ms)
                    .map(|d| d.to_rfc3339());
                serde_json::json!({
                    "project": r.project,
                    "session_id": r.session_id,
                    "date": date,
                    "model": r.model,
                    "message_count": r.message_count,
                })
            }),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    section("Sessions");
    println!("  Total:      {}", data.total_sessions.to_string().bold());
    println!("  Today:      {}", data.sessions_today);
    println!("  This week:  {}", data.sessions_this_week);

    section("Cost (estimated)");
    println!("  All time:   ${:.4}", data.total_cost);
    println!("  This week:  ${:.4}", data.week_cost);

    section("Top Projects");
    if data.top_projects.is_empty() {
        println!("  (none)");
    } else {
        for (i, (proj, count)) in data.top_projects.iter().enumerate() {
            println!("  {}. {}  {} sessions", i + 1, proj.bright_blue(), count);
        }
    }

    section("Top Tools");
    if data.top_tools.is_empty() {
        println!("  (none)");
    } else {
        for (i, (tool, count)) in data.top_tools.iter().enumerate() {
            println!(
                "  {}. {}  {} calls",
                i + 1,
                tool.cyan(),
                fmt_num(*count as u64)
            );
        }
    }

    section("Model Distribution");
    if data.model_distribution.is_empty() {
        println!("  (none)");
    } else {
        for (model, sessions, cost) in &data.model_distribution {
            println!("  {}  {} sessions  ${:.4}", model.yellow(), sessions, cost);
        }
    }

    section("Metrics");
    if data.thinking_block_count > 0 {
        println!("  Thinking blocks:    {}", fmt_num(data.thinking_block_count as u64));
    }
    if let Some(avg) = data.avg_turn_duration_ms {
        println!("  Avg turn duration:  {:.0}ms", avg);
    }
    if data.pr_count > 0 {
        println!("  PRs linked:         {}", data.pr_count);
    }
    if data.files_modified_count > 0 {
        println!("  Files modified:     {}", fmt_num(data.files_modified_count as u64));
    }

    if let Some(r) = &data.most_recent {
        section("Most Recent Session");
        println!("  Project:   {}", r.project.bright_blue());
        if let Some(dt) = DateTime::from_timestamp_millis(r.first_timestamp_ms) {
            println!("  Date:      {}", dt.format("%Y-%m-%d %H:%M UTC"));
        }
        let sid: String = r.session_id.chars().take(8).collect();
        println!("  Session:   {}", sid.dimmed());
        let model = r
            .model
            .as_deref()
            .map(|m| m.trim_start_matches("claude-").to_string())
            .unwrap_or_else(|| "-".to_string());
        println!("  Model:     {}", model);
        println!("  Messages:  {}", r.message_count);
    }

    println!();
    Ok(())
}

fn run_from_files(json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let files = store.all_session_files(None)?;

    let now = Utc::now();
    let today = now.date_naive();
    let days_since_monday = today.weekday().num_days_from_monday() as i64;
    let week_start = today - Duration::days(days_since_monday);

    let mut total_sessions = 0usize;
    let mut sessions_today = 0usize;
    let mut sessions_this_week = 0usize;
    let mut total_cost = 0.0f64;
    let mut week_cost = 0.0f64;
    let mut total_usage = TokenUsage::default();
    let mut project_counts: HashMap<String, usize> = HashMap::new();
    let mut tool_counts: HashMap<String, u64> = HashMap::new();

    struct RecentSession {
        date: DateTime<Utc>,
        project: String,
        session_id: String,
        model: Option<String>,
        message_count: usize,
    }
    let mut most_recent: Option<RecentSession> = None;

    for (project_raw, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        total_sessions += 1;
        let session_cost = stats.usage.cost_for_model(stats.model.as_deref());
        total_cost += session_cost;
        total_usage.add(&stats.usage);

        if let Some(dt) = stats.first_timestamp {
            let date = dt.date_naive();
            if date == today {
                sessions_today += 1;
            }
            if date >= week_start {
                sessions_this_week += 1;
                week_cost += session_cost;
            }

            let is_newer = most_recent.as_ref().map(|r| dt > r.date).unwrap_or(true);
            if is_newer {
                most_recent = Some(RecentSession {
                    date: dt,
                    project: display_project_name(&decode_project_name(project_raw)),
                    session_id: stats.session_id.unwrap_or_default(),
                    model: stats.model.clone(),
                    message_count: stats.message_count,
                });
            }
        }

        let proj = display_project_name(&decode_project_name(project_raw));
        *project_counts.entry(proj).or_insert(0) += 1;

        for name in &stats.tool_names {
            *tool_counts.entry(name.clone()).or_insert(0) += 1;
        }
    }

    let mut top_projects: Vec<(String, usize)> = project_counts.into_iter().collect();
    top_projects.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    top_projects.truncate(5);

    let mut top_tools: Vec<(String, u64)> = tool_counts.into_iter().collect();
    top_tools.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    top_tools.truncate(5);

    if json {
        let out = serde_json::json!({
            "total_sessions": total_sessions,
            "sessions_today": sessions_today,
            "sessions_this_week": sessions_this_week,
            "total_cost_usd": total_cost,
            "cost_this_week_usd": week_cost,
            "total_tokens": total_usage.total_tokens(),
            "top_projects": top_projects.iter().map(|(p, c)| serde_json::json!({"project": p, "sessions": c})).collect::<Vec<_>>(),
            "top_tools": top_tools.iter().map(|(t, c)| serde_json::json!({"tool": t, "calls": c})).collect::<Vec<_>>(),
            "most_recent": most_recent.as_ref().map(|r| serde_json::json!({
                "project": r.project,
                "session_id": r.session_id,
                "date": r.date.to_rfc3339(),
                "model": r.model,
                "message_count": r.message_count,
            })),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    section("Sessions");
    println!("  Total:      {}", total_sessions.to_string().bold());
    println!("  Today:      {}", sessions_today);
    println!("  This week:  {}", sessions_this_week);

    section("Cost (estimated)");
    println!("  All time:   ${:.4}", total_cost);
    println!("  This week:  ${:.4}", week_cost);

    section("Top Projects");
    if top_projects.is_empty() {
        println!("  (none)");
    } else {
        for (i, (proj, count)) in top_projects.iter().enumerate() {
            println!("  {}. {}  {} sessions", i + 1, proj.bright_blue(), count);
        }
    }

    section("Top Tools");
    if top_tools.is_empty() {
        println!("  (none)");
    } else {
        for (i, (tool, count)) in top_tools.iter().enumerate() {
            println!("  {}. {}  {} calls", i + 1, tool.cyan(), fmt_num(*count));
        }
    }

    if let Some(r) = &most_recent {
        section("Most Recent Session");
        println!("  Project:   {}", r.project.bright_blue());
        println!("  Date:      {}", r.date.format("%Y-%m-%d %H:%M UTC"));
        let sid: String = r.session_id.chars().take(8).collect();
        println!("  Session:   {}", sid.dimmed());
        let model = r
            .model
            .as_deref()
            .map(|m| m.trim_start_matches("claude-").to_string())
            .unwrap_or_else(|| "-".to_string());
        println!("  Model:     {}", model);
        println!("  Messages:  {}", r.message_count);
    }

    println!();
    Ok(())
}

fn section(title: &str) {
    println!("\n{}", title.bold());
    println!("{}", "─".repeat(title.len()));
}

fn fmt_num(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

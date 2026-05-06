use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Utc};

use crate::index::IndexStore;
use crate::parser::parse_session;
use crate::plan::Plan;
use crate::store::{SessionStore, decode_project_name, display_project_name};
use crate::types::{ModelPricing, TokenUsage};
use crate::ui;

pub fn run(json: bool, no_index: bool, plan: Plan) -> Result<()> {
    if !no_index && let Ok(()) = run_indexed(json, plan) {
        return Ok(());
    }
    run_from_files(json, plan)
}

fn run_indexed(json: bool, plan: Plan) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&store)?;
    let data = idx.query_summary()?;

    if json {
        // Plan-aware cost emission: Plan::Api preserves the historical
        // `total_cost_usd` / `cost_this_week_usd` keys (backward-compat);
        // Plan::FlatMonthly substitutes plan-relative fields.
        let cost_obj = plan.cost_fields(data.total_cost, data.week_cost);
        let mut out = serde_json::json!({
            "total_sessions": data.total_sessions,
            "sessions_today": data.sessions_today,
            "sessions_this_week": data.sessions_this_week,
            "total_input_tokens": data.total_input_tokens,
            "total_output_tokens": data.total_output_tokens,
            "total_cache_creation_tokens": data.total_cache_creation,
            "total_cache_read_tokens": data.total_cache_read,
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
            "top_stop_reasons": data.top_stop_reasons.iter()
                .map(|(reason, count)| serde_json::json!({"stop_reason": reason, "count": count}))
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
        // Merge plan-aware cost fields into the top-level object.
        // - Plan::Api  → adds `total_cost_usd`, `cost_this_week_usd` (historical keys)
        // - Plan::FlatMonthly → adds `actual_monthly_cost_usd`, `api_equivalent_*`,
        //   `leverage_*_multiple` (no `total_cost_usd` to avoid ambiguity)
        if let (Some(out_obj), Some(cost_obj)) = (out.as_object_mut(), cost_obj.as_object()) {
            for (k, v) in cost_obj {
                out_obj.insert(k.clone(), v.clone());
            }
        }
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    section("Sessions");
    println!(
        "  Total:      {}",
        ui::emphasis(&ui::fmt_count(data.total_sessions as u64))
    );
    println!(
        "  Today:      {}",
        ui::fmt_count(data.sessions_today as u64)
    );
    println!(
        "  This week:  {}",
        ui::fmt_count(data.sessions_this_week as u64)
    );

    section("Cost (estimated)");
    println!("  All time:   {}", ui::cost(data.total_cost));
    println!("  This week:  {}", ui::cost(data.week_cost));

    section("Tokens");
    println!(
        "  Input:       {}",
        ui::count(data.total_input_tokens as u64)
    );
    println!(
        "  Output:      {}",
        ui::count(data.total_output_tokens as u64)
    );
    println!(
        "  Cache write: {}",
        ui::count(data.total_cache_creation as u64)
    );
    println!("  Cache read:  {}", ui::count(data.total_cache_read as u64));
    println!(
        "  Total:       {}",
        ui::emphasis(&ui::count(
            (data.total_input_tokens
                + data.total_output_tokens
                + data.total_cache_creation
                + data.total_cache_read) as u64,
        ))
    );

    section("Top Projects");
    if data.top_projects.is_empty() {
        println!("  (none)");
    } else {
        for (i, (proj, count)) in data.top_projects.iter().enumerate() {
            println!(
                "  {}. {}  {} sessions",
                i + 1,
                ui::project(proj),
                ui::fmt_count(*count as u64)
            );
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
                ui::tool_name(tool),
                ui::fmt_count(*count as u64)
            );
        }
    }

    section("Top Stop Reasons");
    if data.top_stop_reasons.is_empty() {
        println!("  (none)");
    } else {
        for (i, (reason, count)) in data.top_stop_reasons.iter().enumerate() {
            println!(
                "  {}. {}  {}",
                i + 1,
                ui::role(reason),
                ui::fmt_count(*count as u64)
            );
        }
    }

    section("Model Distribution");
    if data.model_distribution.is_empty() {
        println!("  (none)");
    } else {
        for (model, sessions, c) in &data.model_distribution {
            println!(
                "  {}  {} sessions  {}",
                ui::model_name(model),
                ui::fmt_count(*sessions as u64),
                ui::cost(*c)
            );
        }
    }

    section("Metrics");
    if data.thinking_block_count > 0 {
        println!(
            "  Thinking blocks:    {}",
            ui::fmt_count(data.thinking_block_count as u64)
        );
    }
    if let Some(avg) = data.avg_turn_duration_ms {
        let secs = avg / 1000.0;
        if secs < 60.0 {
            println!("  Avg turn duration:  {secs:.1}s");
        } else {
            println!("  Avg turn duration:  {:.1}m", secs / 60.0);
        }
    }
    if data.pr_count > 0 {
        println!(
            "  PRs linked:         {}",
            ui::fmt_count(data.pr_count as u64)
        );
    }
    if data.files_modified_count > 0 {
        println!(
            "  Files modified:     {}",
            ui::fmt_count(data.files_modified_count as u64)
        );
    }

    if let Some(r) = &data.most_recent {
        section("Most Recent Session");
        println!("  Project:   {}", ui::project(&r.project));
        if let Some(dt) = DateTime::from_timestamp_millis(r.first_timestamp_ms) {
            println!("  Date:      {}", dt.format("%Y-%m-%d %H:%M UTC"));
        }
        let sid: String = r.session_id.chars().take(8).collect();
        println!("  Session:   {}", ui::session_id(&sid));
        let model = r
            .model
            .as_deref()
            .map(|m| m.trim_start_matches("claude-").to_string())
            .unwrap_or_else(|| "-".to_string());
        println!("  Model:     {}", model);
        println!("  Messages:  {}", ui::fmt_count(r.message_count as u64));
    }

    println!();
    Ok(())
}

fn run_from_files(json: bool, plan: Plan) -> Result<()> {
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
    let mut stop_reason_counts: HashMap<String, u64> = HashMap::new();
    let mut thinking_block_count = 0u64;
    let mut total_turn_duration_ms = 0u64;
    let mut total_turn_count = 0u64;
    let mut pr_urls = std::collections::BTreeSet::new();
    let mut files_modified = std::collections::BTreeSet::new();
    let mut model_distribution: HashMap<String, (u64, f64)> = HashMap::new();

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
        let session_cost = stats.cost_usd();
        total_cost += session_cost;
        total_usage.add(&stats.usage);
        thinking_block_count += stats.thinking_block_count;
        total_turn_duration_ms += stats
            .turn_durations
            .iter()
            .map(|(dur, _)| *dur)
            .sum::<u64>();
        total_turn_count += stats.turn_durations.len() as u64;

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
        for (reason, count) in &stats.stop_reason_counts {
            *stop_reason_counts.entry(reason.clone()).or_insert(0) += *count;
        }
        for (_, url, _, _) in &stats.pr_links {
            if !url.is_empty() {
                pr_urls.insert(url.clone());
            }
        }
        for file in &stats.file_paths_modified {
            files_modified.insert(file.clone());
        }
        let mut session_families = std::collections::BTreeSet::new();
        for (model, usage) in &stats.model_usage {
            let family = ModelPricing::name(Some(model)).to_string();
            session_families.insert(family.clone());
            let entry = model_distribution.entry(family).or_insert((0, 0.0));
            entry.1 += usage.usage.cost_for_model(Some(model));
        }
        if session_families.is_empty()
            && let Some(model) = &stats.model
        {
            let family = ModelPricing::name(Some(model)).to_string();
            session_families.insert(family.clone());
            let entry = model_distribution.entry(family).or_insert((0, 0.0));
            entry.1 += session_cost;
        }
        for family in session_families {
            let entry = model_distribution.entry(family).or_insert((0, 0.0));
            entry.0 += 1;
        }
    }

    let mut top_projects: Vec<(String, usize)> = project_counts.into_iter().collect();
    top_projects.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    top_projects.truncate(5);

    let mut top_tools: Vec<(String, u64)> = tool_counts.into_iter().collect();
    top_tools.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    top_tools.truncate(5);

    let mut top_stop_reasons: Vec<(String, u64)> = stop_reason_counts.into_iter().collect();
    top_stop_reasons.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    top_stop_reasons.truncate(5);

    let mut model_distribution: Vec<(String, u64, f64)> = model_distribution
        .into_iter()
        .map(|(model, (sessions, cost))| (model, sessions, cost))
        .collect();
    model_distribution.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    model_distribution.truncate(5);
    let avg_turn_duration_ms = if total_turn_count == 0 {
        None
    } else {
        Some(total_turn_duration_ms as f64 / total_turn_count as f64)
    };

    if json {
        let cost_obj = plan.cost_fields(total_cost, week_cost);
        let mut out = serde_json::json!({
            "total_sessions": total_sessions,
            "sessions_today": sessions_today,
            "sessions_this_week": sessions_this_week,
            "total_input_tokens": total_usage.input_tokens,
            "total_output_tokens": total_usage.output_tokens,
            "total_cache_creation_tokens": total_usage.cache_creation_tokens,
            "total_cache_read_tokens": total_usage.cache_read_tokens,
            "total_tokens": total_usage.total_tokens(),
            "thinking_block_count": thinking_block_count,
            "avg_turn_duration_ms": avg_turn_duration_ms,
            "pr_count": pr_urls.len(),
            "files_modified_count": files_modified.len(),
            "top_projects": top_projects.iter().map(|(p, c)| serde_json::json!({"project": p, "sessions": c})).collect::<Vec<_>>(),
            "top_tools": top_tools.iter().map(|(t, c)| serde_json::json!({"tool": t, "calls": c})).collect::<Vec<_>>(),
            "top_stop_reasons": top_stop_reasons.iter().map(|(reason, count)| serde_json::json!({"stop_reason": reason, "count": count})).collect::<Vec<_>>(),
            "model_distribution": model_distribution.iter().map(|(m, s, c)| serde_json::json!({"model": m, "sessions": s, "cost_usd": c})).collect::<Vec<_>>(),
            "most_recent": most_recent.as_ref().map(|r| serde_json::json!({
                "project": r.project,
                "session_id": r.session_id,
                "date": r.date.to_rfc3339(),
                "model": r.model,
                "message_count": r.message_count,
            })),
        });
        if let (Some(out_obj), Some(cost_obj)) = (out.as_object_mut(), cost_obj.as_object()) {
            for (k, v) in cost_obj {
                out_obj.insert(k.clone(), v.clone());
            }
        }
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    section("Sessions");
    println!(
        "  Total:      {}",
        ui::emphasis(&ui::fmt_count(total_sessions as u64))
    );
    println!("  Today:      {}", ui::fmt_count(sessions_today as u64));
    println!("  This week:  {}", ui::fmt_count(sessions_this_week as u64));

    section("Cost (estimated)");
    println!("  All time:   {}", ui::cost(total_cost));
    println!("  This week:  {}", ui::cost(week_cost));

    section("Tokens");
    println!("  Input:       {}", ui::count(total_usage.input_tokens));
    println!("  Output:      {}", ui::count(total_usage.output_tokens));
    println!(
        "  Cache write: {}",
        ui::count(total_usage.cache_creation_tokens)
    );
    println!(
        "  Cache read:  {}",
        ui::count(total_usage.cache_read_tokens)
    );
    println!(
        "  Total:       {}",
        ui::emphasis(&ui::count(total_usage.total_tokens()))
    );

    section("Top Projects");
    if top_projects.is_empty() {
        println!("  (none)");
    } else {
        for (i, (proj, count)) in top_projects.iter().enumerate() {
            println!(
                "  {}. {}  {} sessions",
                i + 1,
                ui::project(proj),
                ui::fmt_count(*count as u64)
            );
        }
    }

    section("Top Tools");
    if top_tools.is_empty() {
        println!("  (none)");
    } else {
        for (i, (tool, count)) in top_tools.iter().enumerate() {
            println!(
                "  {}. {}  {} calls",
                i + 1,
                ui::tool_name(tool),
                ui::fmt_count(*count)
            );
        }
    }

    section("Top Stop Reasons");
    if top_stop_reasons.is_empty() {
        println!("  (none)");
    } else {
        for (i, (reason, count)) in top_stop_reasons.iter().enumerate() {
            println!(
                "  {}. {}  {}",
                i + 1,
                ui::role(reason),
                ui::fmt_count(*count)
            );
        }
    }

    section("Model Distribution");
    if model_distribution.is_empty() {
        println!("  (none)");
    } else {
        for (model, sessions, c) in &model_distribution {
            println!(
                "  {}  {} sessions  {}",
                ui::model_name(model),
                ui::fmt_count(*sessions),
                ui::cost(*c)
            );
        }
    }

    section("Metrics");
    if thinking_block_count > 0 {
        println!(
            "  Thinking blocks:    {}",
            ui::fmt_count(thinking_block_count)
        );
    }
    if let Some(avg) = avg_turn_duration_ms {
        let secs = avg / 1000.0;
        if secs < 60.0 {
            println!("  Avg turn duration:  {secs:.1}s");
        } else {
            println!("  Avg turn duration:  {:.1}m", secs / 60.0);
        }
    }
    if !pr_urls.is_empty() {
        println!(
            "  PRs linked:         {}",
            ui::fmt_count(pr_urls.len() as u64)
        );
    }
    if !files_modified.is_empty() {
        println!(
            "  Files modified:     {}",
            ui::fmt_count(files_modified.len() as u64)
        );
    }

    if let Some(r) = &most_recent {
        section("Most Recent Session");
        println!("  Project:   {}", ui::project(&r.project));
        println!("  Date:      {}", r.date.format("%Y-%m-%d %H:%M UTC"));
        let sid: String = r.session_id.chars().take(8).collect();
        println!("  Session:   {}", ui::session_id(&sid));
        let model = r
            .model
            .as_deref()
            .map(|m| m.trim_start_matches("claude-").to_string())
            .unwrap_or_else(|| "-".to_string());
        println!("  Model:     {}", model);
        println!("  Messages:  {}", ui::fmt_count(r.message_count as u64));
    }

    println!();
    Ok(())
}

fn section(title: &str) {
    println!("\n{}", ui::section_title(title));
    println!("{}", "─".repeat(title.len()));
}

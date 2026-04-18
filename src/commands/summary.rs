use std::collections::HashMap;

use anyhow::Result;
use chrono::{Datelike, Duration, Utc};
use owo_colors::OwoColorize;

use crate::parser::parse_session;
use crate::store::{SessionStore, display_project_name};
use crate::types::TokenUsage;

pub fn run(json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let files = store.all_session_files(None)?;

    let now = Utc::now();
    let today = now.date_naive();
    let week_start = today - Duration::days(today.weekday().num_days_from_monday() as i64);

    let mut total_sessions = 0usize;
    let mut sessions_today = 0usize;
    let mut sessions_week = 0usize;
    let mut total_usage = TokenUsage::default();
    let mut week_usage = TokenUsage::default();
    let mut total_cost = 0f64;
    let mut week_cost = 0f64;
    let mut project_counts: HashMap<String, usize> = HashMap::new();
    let mut tool_counts: HashMap<String, u64> = HashMap::new();
    let mut most_recent: Option<MostRecent> = None;

    for (project_raw, path) in &files {
        let stats = match parse_session(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        total_sessions += 1;
        let session_cost = stats.usage.cost_for_model(stats.model.as_deref());
        total_usage.add(&stats.usage);
        total_cost += session_cost;

        let project_display = display_project_name(project_raw);
        *project_counts.entry(project_display.clone()).or_insert(0) += 1;

        for name in &stats.tool_names {
            *tool_counts.entry(name.clone()).or_insert(0) += 1;
        }

        if let Some(ts) = stats.first_timestamp {
            let date = ts.date_naive();
            if date == today {
                sessions_today += 1;
            }
            if date >= week_start {
                sessions_week += 1;
                week_usage.add(&stats.usage);
                week_cost += session_cost;
            }

            let update = most_recent
                .as_ref()
                .is_none_or(|r: &MostRecent| ts > r.date);
            if update {
                most_recent = Some(MostRecent {
                    project: project_display,
                    session_id: stats.session_id.unwrap_or_default(),
                    date: ts,
                    model: stats.model,
                });
            }
        }
    }

    if json {
        let mut top_projects: Vec<_> = project_counts.iter().collect();
        top_projects.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
        let mut top_tools: Vec<_> = tool_counts.iter().collect();
        top_tools.sort_by_key(|(_, c)| std::cmp::Reverse(**c));

        let output = serde_json::json!({
            "sessions": {
                "total": total_sessions,
                "today": sessions_today,
                "this_week": sessions_week,
            },
            "cost_usd": {
                "total": total_cost,
                "this_week": week_cost,
            },
            "top_projects": top_projects.iter().take(5).map(|(p, c)| {
                serde_json::json!({"project": p, "sessions": c})
            }).collect::<Vec<_>>(),
            "top_tools": top_tools.iter().take(5).map(|(t, c)| {
                serde_json::json!({"tool": t, "calls": c})
            }).collect::<Vec<_>>(),
            "most_recent": most_recent.as_ref().map(|r| {
                serde_json::json!({
                    "project": r.project,
                    "session_id": r.session_id,
                    "date": r.date.to_rfc3339(),
                    "model": r.model,
                })
            }),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("{}", "claudex summary".bold());
    println!();

    println!("{}", "Sessions".bright_cyan().bold());
    println!(
        "  {} total  |  {} today  |  {} this week",
        total_sessions, sessions_today, sessions_week
    );
    println!();

    println!("{}", "Cost (USD)".bright_cyan().bold());
    println!(
        "  ${:.4} all time  |  ${:.4} this week",
        total_cost, week_cost
    );
    println!();

    let mut top_projects: Vec<_> = project_counts.iter().collect();
    top_projects.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    println!("{}", "Top Projects".bright_cyan().bold());
    for (i, (project, count)) in top_projects.iter().take(5).enumerate() {
        println!(
            "  {}. {:<50} {}",
            i + 1,
            project,
            count.to_string().dimmed()
        );
    }
    println!();

    let mut top_tools: Vec<_> = tool_counts.iter().collect();
    top_tools.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    println!("{}", "Top Tools".bright_cyan().bold());
    for (i, (tool, count)) in top_tools.iter().take(5).enumerate() {
        println!("  {}. {:<30} {}", i + 1, tool, count.to_string().dimmed());
    }
    println!();

    if let Some(r) = &most_recent {
        println!("{}", "Most Recent Session".bright_cyan().bold());
        println!("  Project:  {}", r.project);
        println!(
            "  Session:  {}",
            r.session_id.chars().take(8).collect::<String>()
        );
        println!("  Date:     {}", r.date.format("%Y-%m-%d %H:%M UTC"));
        println!(
            "  Model:    {}",
            r.model
                .as_deref()
                .unwrap_or("-")
                .trim_start_matches("claude-")
        );
    }

    Ok(())
}

struct MostRecent {
    project: String,
    session_id: String,
    date: chrono::DateTime<Utc>,
    model: Option<String>,
}

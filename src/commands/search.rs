use anyhow::Result;
use chrono::DateTime;

use crate::cli::ResolvedFilter;
use crate::index::{IndexStore, SearchFtsOptions};
use crate::parser::{parse_session, stream_records};
use crate::providers::enabled_default;
use crate::store::{SessionStore, decode_project_name, short_name};
use crate::ui;

pub struct SearchCommand<'a> {
    pub query: &'a str,
    pub project: Option<&'a str>,
    pub limit: usize,
    pub json: bool,
    pub case_sensitive: bool,
    pub role: Option<&'a str>,
    pub tool: Option<&'a str>,
    pub file: Option<&'a str>,
    pub pr: Option<&'a str>,
    pub context: usize,
    pub no_index: bool,
    pub filter: &'a ResolvedFilter,
}

pub fn run(opts: SearchCommand<'_>) -> Result<()> {
    // FTS5 is case-insensitive; fall back to file scan for case-sensitive queries
    if !opts.no_index
        && !opts.case_sensitive
        && let Ok(()) = run_indexed(&opts)
    {
        return Ok(());
    }
    run_from_files(&opts)
}

fn run_indexed(opts: &SearchCommand<'_>) -> Result<()> {
    let providers = enabled_default()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&providers)?;
    if opts.pr.is_some() {
        idx.ensure_pr_links_fresh(&providers)?;
    }

    let hits = idx.search_fts(SearchFtsOptions {
        query: opts.query,
        project_filter: opts.project,
        filter: opts.filter,
        role_filter: opts.role,
        tool_filter: opts.tool,
        file_filter: opts.file,
        pr_filter: opts.pr,
        context: opts.context,
        limit: opts.limit,
    })?;

    if opts.json {
        let output: Vec<_> = hits
            .iter()
            .map(|hit| {
                let message_timestamp = hit
                    .message_timestamp_ms
                    .and_then(DateTime::from_timestamp_millis)
                    .map(|d| d.to_rfc3339());
                serde_json::json!({
                    "provider": hit.provider,
                    "project": hit.project_name,
                    "session_id": hit.session_id,
                    "message_timestamp": message_timestamp,
                    "message_type": hit.message_type,
                    "snippet": hit.snippet,
                    "rank": hit.rank,
                    "context_before": hit.context_before.iter().map(context_json).collect::<Vec<_>>(),
                    "context_after": hit.context_after.iter().map(context_json).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if hits.is_empty() {
        println!("No matches found for {:?}", opts.query);
        return Ok(());
    }

    let show_provider = ui::spans_providers(hits.iter().map(|h| h.provider.as_str()));
    for hit in &hits {
        let date_str = hit
            .message_timestamp_ms
            .and_then(DateTime::from_timestamp_millis)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string());
        let sid: String = hit
            .session_id
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(8)
            .collect();
        let project_display = short_name(&hit.project_name);

        let prefix = if show_provider {
            format!("{} ", ui::record_type(&hit.provider))
        } else {
            String::new()
        };
        println!(
            "{prefix}{} {} [{}] {}",
            ui::project_headline(&project_display),
            ui::session_id(&sid),
            ui::timestamp(&date_str),
            ui::role(&hit.message_type),
        );
        println!("  {}", render_indexed_snippet(&hit.snippet));
        for ctx in &hit.context_before {
            println!(
                "  {} {}",
                ui::role(&ctx.message_type),
                truncate_context(&ctx.content)
            );
        }
        for ctx in &hit.context_after {
            println!(
                "  {} {}",
                ui::role(&ctx.message_type),
                truncate_context(&ctx.content)
            );
        }
        println!();
    }
    Ok(())
}

fn run_from_files(opts: &SearchCommand<'_>) -> Result<()> {
    opts.filter.ensure_no_index_supported()?;

    let store = SessionStore::new()?;
    let files = store.all_session_files(opts.project)?;

    let query_cmp = if opts.case_sensitive {
        opts.query.to_string()
    } else {
        opts.query.to_lowercase()
    };

    let mut found = 0usize;
    let mut json_hits = Vec::new();

    'outer: for (project_raw, path) in &files {
        // Apply the cross-cutting --since/--until/--model filters at the session
        // level (the indexed path filters sessions the same way). Skip the parse
        // when no filter is active so an unfiltered search stays single-pass.
        let stats = if !opts.filter.is_unfiltered()
            || opts.tool.is_some()
            || opts.file.is_some()
            || opts.pr.is_some()
        {
            match parse_session(path) {
                Ok(stats) => Some(stats),
                Err(_) => continue,
            }
        } else {
            None
        };
        if let Some(stats) = &stats
            && !opts.filter.matches("claude", stats, false)
        {
            continue;
        }
        if let Some(tool) = opts.tool
            && stats.as_ref().is_none_or(|s| {
                !s.tool_names
                    .iter()
                    .any(|name| name.to_lowercase().contains(&tool.to_lowercase()))
            })
        {
            continue;
        }
        if let Some(file) = opts.file
            && stats.as_ref().is_none_or(|s| {
                !s.file_paths_modified
                    .iter()
                    .any(|path| path.to_lowercase().contains(&file.to_lowercase()))
            })
        {
            continue;
        }
        if let Some(pr) = opts.pr
            && stats.as_ref().is_none_or(|s| {
                let needle = pr.to_lowercase();
                !s.pr_links.iter().any(|(number, url, repo, _)| {
                    number.to_string().contains(&needle)
                        || url.to_lowercase().contains(&needle)
                        || repo.to_lowercase().contains(&needle)
                })
            })
        {
            continue;
        }

        let project_display = short_name(&decode_project_name(project_raw));
        let mut session_date = None;
        let mut session_id: Option<String> = None;
        let mut stop = false;

        stream_records(path, |record| {
            if session_id.is_none()
                && let Some(sid) = record["sessionId"].as_str()
            {
                session_id = Some(sid.to_string());
            }
            if session_date.is_none()
                && let Some(ts) = record["timestamp"].as_str()
            {
                session_date = DateTime::parse_from_rfc3339(ts)
                    .ok()
                    .map(|d| d.with_timezone(&chrono::Utc));
            }

            let (role, text) = match record["type"].as_str().unwrap_or("") {
                "user" => {
                    let content = record["message"]["content"].as_str().unwrap_or("");
                    ("user", content.to_string())
                }
                "assistant" => {
                    let blocks = record["message"]["content"].as_array();
                    let text = blocks
                        .map(|arr| {
                            arr.iter()
                                .filter(|b| b["type"].as_str() == Some("text"))
                                .map(|b| b["text"].as_str().unwrap_or("").to_string())
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_default();
                    ("assistant", text)
                }
                _ => return true,
            };
            if let Some(role_filter) = opts.role
                && role != role_filter
            {
                return true;
            }

            if text.is_empty() {
                return true;
            }

            let haystack = if opts.case_sensitive {
                text.as_str().to_string()
            } else {
                text.to_lowercase()
            };

            if !haystack.contains(&query_cmp) {
                return true;
            }

            let date_str = session_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "-".to_string());
            let sid: String = session_id
                .as_deref()
                .unwrap_or("-")
                .chars()
                .take(8)
                .collect();

            if !opts.json {
                println!(
                    "{} {} [{}] {}",
                    ui::project_headline(&project_display),
                    ui::session_id(&sid),
                    ui::timestamp(&date_str),
                    ui::role(role),
                );
            }

            for line in text.lines() {
                let line_cmp = if opts.case_sensitive {
                    line.to_string()
                } else {
                    line.to_lowercase()
                };
                if line_cmp.contains(&query_cmp) {
                    let snippet = build_file_scan_snippet(line, opts.query, opts.case_sensitive);
                    if opts.json {
                        json_hits.push(serde_json::json!({
                            "project": decode_project_name(project_raw),
                            "session_id": session_id,
                            "message_timestamp": session_date.map(|d| d.to_rfc3339()),
                            "message_type": role,
                            "snippet": snippet,
                            "rank": serde_json::Value::Null,
                        }));
                    } else {
                        print_highlighted(line, opts.query, opts.case_sensitive);
                        println!();
                    }
                    found += 1;
                    if found >= opts.limit {
                        stop = true;
                        return false;
                    }
                }
            }
            true
        })?;

        if stop {
            break 'outer;
        }
    }

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&json_hits)?);
        return Ok(());
    }

    if found == 0 {
        println!("No matches found for {:?}", opts.query);
    }
    Ok(())
}

fn context_json(ctx: &crate::index::SearchContextMessage) -> serde_json::Value {
    let message_timestamp = ctx
        .timestamp_ms
        .and_then(DateTime::from_timestamp_millis)
        .map(|d| d.to_rfc3339());
    serde_json::json!({
        "message_timestamp": message_timestamp,
        "message_type": ctx.message_type,
        "content": ctx.content,
    })
}

fn truncate_context(content: &str) -> String {
    const MAX: usize = 180;
    let compact = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() <= MAX {
        compact
    } else {
        let mut end = MAX;
        while !compact.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &compact[..end])
    }
}

fn print_highlighted(line: &str, query: &str, case_sensitive: bool) {
    const MAX_LINE: usize = 300;
    let display = if line.len() > MAX_LINE {
        let mut end = MAX_LINE;
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        &line[..end]
    } else {
        line
    };

    let haystack = if case_sensitive {
        display.to_string()
    } else {
        display.to_lowercase()
    };
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut result = String::new();
    let mut last = 0usize;
    let mut search_from = 0usize;

    while let Some(rel) = haystack[search_from..].find(&needle) {
        let pos = search_from + rel;
        let end = pos + needle.len();

        if !display.is_char_boundary(pos) || !display.is_char_boundary(end) {
            search_from = pos + 1;
            continue;
        }

        result.push_str(&display[last..pos]);
        let matched = &display[pos..end];
        result.push_str(&ui::match_highlight(matched));
        last = end;
        search_from = end;
    }
    result.push_str(&display[last..]);
    println!("  {}", result);
}

fn render_indexed_snippet(snippet: &str) -> String {
    let mut out = String::new();
    let mut rest = snippet;
    while let Some(start) = rest.find("[[") {
        let (before, after_start) = rest.split_at(start);
        out.push_str(before);
        let after_start = &after_start[2..];
        if let Some(end) = after_start.find("]]") {
            let (matched, after_end) = after_start.split_at(end);
            out.push_str(&ui::match_highlight(matched));
            rest = &after_end[2..];
        } else {
            out.push_str(after_start);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn build_file_scan_snippet(line: &str, query: &str, case_sensitive: bool) -> String {
    const CONTEXT: usize = 80;
    let haystack = if case_sensitive {
        line.to_string()
    } else {
        line.to_lowercase()
    };
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let Some(pos) = haystack.find(&needle) else {
        return line.to_string();
    };
    let match_end = pos + needle.len();

    let mut window_start = pos.saturating_sub(CONTEXT);
    while window_start > 0 && !line.is_char_boundary(window_start) {
        window_start -= 1;
    }
    let mut window_end = (match_end + CONTEXT).min(line.len());
    while window_end < line.len() && !line.is_char_boundary(window_end) {
        window_end += 1;
    }
    let prefix = if window_start > 0 { "..." } else { "" };
    let suffix = if window_end < line.len() { "..." } else { "" };

    // Non-ASCII folding may shift byte offsets between `line` and the
    // lowercased haystack; skip marker wrapping when offsets don't line up.
    if line.is_char_boundary(pos) && line.is_char_boundary(match_end) {
        format!(
            "{prefix}{}[[{}]]{}{suffix}",
            &line[window_start..pos],
            &line[pos..match_end],
            &line[match_end..window_end],
        )
    } else {
        format!("{prefix}{}{suffix}", &line[window_start..window_end])
    }
}

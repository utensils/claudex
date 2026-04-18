use anyhow::Result;
use chrono::DateTime;
use owo_colors::OwoColorize;

use crate::index::IndexStore;
use crate::parser::stream_records;
use crate::store::{SessionStore, decode_project_name, short_name};

pub fn run_indexed(store: &IndexStore, query: &str, project: Option<&str>, limit: usize) -> Result<()> {
    let rows = store.search_fts(query, project, limit)?;
    if rows.is_empty() {
        println!("No matches found for {:?}", query);
        return Ok(());
    }
    for r in &rows {
        let date = r.timestamp.as_deref().unwrap_or("-");
        let snippet = r.snippet.replace("<<", &"\x1b[1;91m").replace(">>", &"\x1b[0m");
        println!(
            "{} [{}] {}",
            r.project.bright_blue().bold(),
            date.dimmed(),
            r.message_type.bright_yellow(),
        );
        println!("  {snippet}");
        println!();
    }
    Ok(())
}

pub fn run(query: &str, project: Option<&str>, limit: usize, case_sensitive: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let files = store.all_session_files(project)?;

    let query_cmp = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut found = 0usize;

    'outer: for (project_raw, path) in &files {
        let project_display = short_name(&decode_project_name(project_raw));
        let mut session_date = None;
        let mut session_id: Option<String> = None;
        let mut stop = false;

        stream_records(path, |record| {
            if session_id.is_none() {
                if let Some(sid) = record["sessionId"].as_str() {
                    session_id = Some(sid.to_string());
                }
            }
            if session_date.is_none() {
                if let Some(ts) = record["timestamp"].as_str() {
                    session_date = DateTime::parse_from_rfc3339(ts)
                        .ok()
                        .map(|d| d.with_timezone(&chrono::Utc));
                }
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

            if text.is_empty() {
                return true;
            }

            let haystack = if case_sensitive {
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

            println!(
                "{} {} [{}] {}",
                project_display.bright_blue().bold(),
                sid.dimmed(),
                date_str.dimmed(),
                role.bright_yellow(),
            );

            for line in text.lines() {
                let line_cmp = if case_sensitive {
                    line.to_string()
                } else {
                    line.to_lowercase()
                };
                if line_cmp.contains(&query_cmp) {
                    print_highlighted(line, query, case_sensitive);
                }
            }
            println!();

            found += 1;
            if found >= limit {
                stop = true;
                return false;
            }
            true
        })?;

        if stop {
            break 'outer;
        }
    }

    if found == 0 {
        println!("No matches found for {:?}", query);
    }
    Ok(())
}

fn print_highlighted(line: &str, query: &str, case_sensitive: bool) {
    const MAX_LINE: usize = 300;
    let display = if line.len() > MAX_LINE {
        // Back up to a valid char boundary
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

        // Guard against invalid char boundaries (defensive; rare with ASCII queries)
        if !display.is_char_boundary(pos) || !display.is_char_boundary(end) {
            search_from = pos + 1;
            continue;
        }

        result.push_str(&display[last..pos]);
        let matched = &display[pos..end];
        result.push_str(&matched.bright_red().bold().to_string());
        last = end;
        search_from = end;
    }
    result.push_str(&display[last..]);
    println!("  {}", result);
}

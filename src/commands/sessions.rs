use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

use crate::parser::parse_session;
use crate::store::{SessionStore, decode_project_name, display_project_name, short_name};
use crate::types::SessionInfo;

pub fn run(project: Option<&str>, limit: usize, json: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut sessions: Vec<SessionInfo> = Vec::new();

    for (project_raw, path) in store.all_session_files(project)? {
        let stats = match parse_session(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let session_id = stats
            .session_id
            .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_default();
        sessions.push(SessionInfo {
            project: display_project_name(&decode_project_name(&project_raw)),
            session_id,
            date: stats.first_timestamp,
            message_count: stats.message_count,
            duration_ms: stats.total_duration_ms,
            model: stats.model,
        });
    }

    sessions.sort_by_key(|s| std::cmp::Reverse(s.date));
    sessions.truncate(limit);

    if json {
        let output: Vec<_> = sessions
            .iter()
            .map(|s| {
                serde_json::json!({
                    "project": s.project,
                    "session_id": s.session_id,
                    "date": s.date.map(|d| d.to_rfc3339()),
                    "message_count": s.message_count,
                    "duration_ms": s.duration_ms,
                    "model": s.model,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Project", "Date", "Messages", "Duration", "Model"]);

    for s in &sessions {
        let date = s
            .date
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".to_string());
        let proj = short_name(&s.project);
        let model = s
            .model
            .as_deref()
            .map(|m| m.trim_start_matches("claude-"))
            .unwrap_or("-")
            .to_string();
        table.add_row([
            proj,
            date,
            s.message_count.to_string(),
            format_duration(s.duration_ms),
            model,
        ]);
    }
    println!("{table}");
    Ok(())
}

fn format_duration(ms: u64) -> String {
    if ms == 0 {
        return "-".to_string();
    }
    let secs = ms / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_zero() {
        assert_eq!(format_duration(0), "-");
    }

    #[test]
    fn duration_seconds() {
        assert_eq!(format_duration(45_000), "45s");
    }

    #[test]
    fn duration_minutes() {
        assert_eq!(format_duration(90_000), "1m30s");
    }

    #[test]
    fn duration_hours() {
        assert_eq!(format_duration(3_661_000), "1h1m");
    }
}

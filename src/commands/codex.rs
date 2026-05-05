use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::ui;

#[derive(Debug, Default, Serialize)]
pub struct CodexStats {
    pub total_sessions: usize,
    pub archived_sessions: usize,
    pub active_session_files: usize,
    pub sessions_today: usize,
    pub sessions_this_week: usize,
    pub user_messages: usize,
    pub agent_messages: usize,
    pub reasoning_items: usize,
    pub tool_calls: usize,
    pub tool_results: usize,
    pub aborted_turns: usize,
    pub compacted_events: usize,
    pub review_events: usize,
    pub top_projects: Vec<CountRow>,
    pub top_tools: Vec<CountRow>,
    pub cli_versions: Vec<CountRow>,
    pub originators: Vec<CountRow>,
    pub sources: Vec<CountRow>,
    pub most_recent: Option<CodexSessionSummary>,
    pub state: Option<CodexStateStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CountRow {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexSessionSummary {
    pub session_id: String,
    pub title: Option<String>,
    pub project: Option<String>,
    pub date: Option<String>,
    pub cli_version: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexStateStats {
    pub thread_count: usize,
    pub threads_with_user_event: usize,
    pub total_tokens_used: i64,
    pub top_projects: Vec<CountRow>,
    pub top_models: Vec<CountRow>,
}

#[derive(Debug, Default)]
struct SessionStats {
    id: Option<String>,
    title: Option<String>,
    cwd: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    cli_version: Option<String>,
    originator: Option<String>,
    source: Option<String>,
    user_messages: usize,
    agent_messages: usize,
    reasoning_items: usize,
    tool_calls: usize,
    tool_results: usize,
    aborted_turns: usize,
    compacted_events: usize,
    review_events: usize,
    tool_counts: HashMap<String, usize>,
}

struct CodexStore {
    base_dir: PathBuf,
}

impl CodexStore {
    fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        Ok(Self {
            base_dir: home.join(".codex"),
        })
    }

    #[cfg(test)]
    fn at(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn session_files(&self) -> Result<Vec<(PathBuf, bool)>> {
        let mut files = Vec::new();
        collect_jsonl(&self.base_dir.join("sessions"), false, &mut files)?;
        collect_jsonl(&self.base_dir.join("archived_sessions"), true, &mut files)?;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files)
    }

    fn session_titles(&self) -> Result<HashMap<String, String>> {
        let mut titles = HashMap::new();
        let path = self.base_dir.join("session_index.jsonl");
        if !path.exists() {
            return Ok(titles);
        }
        let reader = BufReader::new(File::open(&path)?);
        for line in reader.lines() {
            let line = line?;
            let Ok(record) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(id) = record["id"].as_str() else {
                continue;
            };
            if let Some(title) = record["thread_name"].as_str()
                && !title.is_empty()
            {
                titles.insert(id.to_string(), title.to_string());
            }
        }
        Ok(titles)
    }

    fn state_stats(&self) -> Result<Option<CodexStateStats>> {
        let path = self.base_dir.join("state_5.sqlite");
        if !path.exists() {
            return Ok(None);
        }
        query_state_stats(&path).map(Some)
    }
}

fn collect_jsonl(dir: &Path, archived: bool, out: &mut Vec<(PathBuf, bool)>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, archived, out)?;
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push((path, archived));
        }
    }
    Ok(())
}

pub fn run(json: bool) -> Result<()> {
    let store = CodexStore::new()?;
    let stats = build_stats(&store)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&stats)?);
    } else {
        render_text(&stats);
    }
    Ok(())
}

fn build_stats(store: &CodexStore) -> Result<CodexStats> {
    let titles = store.session_titles()?;
    let mut stats = CodexStats::default();
    let mut seen = BTreeSet::new();
    let now = Utc::now();
    let week_start = now.date_naive() - Duration::days(now.weekday().num_days_from_monday() as i64);
    let mut project_counts: HashMap<String, usize> = HashMap::new();
    let mut tool_counts: HashMap<String, usize> = HashMap::new();
    let mut cli_versions: HashMap<String, usize> = HashMap::new();
    let mut originators: HashMap<String, usize> = HashMap::new();
    let mut sources: HashMap<String, usize> = HashMap::new();

    for (path, archived) in store.session_files()? {
        let mut session = parse_session_file(&path)?;
        let id = session.id.clone().or_else(|| session_id_from_path(&path));
        let Some(id) = id else { continue };
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(title) = titles.get(&id) {
            session.title = Some(title.clone());
        }

        stats.total_sessions += 1;
        if archived {
            stats.archived_sessions += 1;
        } else {
            stats.active_session_files += 1;
        }
        if let Some(ts) = session.timestamp {
            if ts.date_naive() == now.date_naive() {
                stats.sessions_today += 1;
            }
            if ts.date_naive() >= week_start {
                stats.sessions_this_week += 1;
            }
            let replace = stats
                .most_recent
                .as_ref()
                .and_then(|m| m.date.as_deref())
                .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
                .map(|prev| ts > prev.with_timezone(&Utc))
                .unwrap_or(true);
            if replace {
                stats.most_recent = Some(CodexSessionSummary {
                    session_id: id.clone(),
                    title: session.title.clone(),
                    project: session.cwd.clone(),
                    date: Some(ts.to_rfc3339()),
                    cli_version: session.cli_version.clone(),
                    source: session.source.clone(),
                });
            }
        }
        stats.user_messages += session.user_messages;
        stats.agent_messages += session.agent_messages;
        stats.reasoning_items += session.reasoning_items;
        stats.tool_calls += session.tool_calls;
        stats.tool_results += session.tool_results;
        stats.aborted_turns += session.aborted_turns;
        stats.compacted_events += session.compacted_events;
        stats.review_events += session.review_events;

        if let Some(cwd) = session.cwd {
            *project_counts.entry(cwd).or_insert(0) += 1;
        }
        if let Some(version) = session.cli_version {
            *cli_versions.entry(version).or_insert(0) += 1;
        }
        if let Some(originator) = session.originator {
            *originators.entry(originator).or_insert(0) += 1;
        }
        if let Some(source) = session.source {
            *sources.entry(source).or_insert(0) += 1;
        }
        for (tool, count) in session.tool_counts {
            *tool_counts.entry(tool).or_insert(0) += count;
        }
    }

    stats.top_projects = top_counts(project_counts, 10);
    stats.top_tools = top_counts(tool_counts, 15);
    stats.cli_versions = top_counts(cli_versions, 10);
    stats.originators = top_counts(originators, 10);
    stats.sources = top_counts(sources, 10);
    stats.state = store.state_stats().unwrap_or(None);
    Ok(stats)
}

fn parse_session_file(path: &Path) -> Result<SessionStats> {
    let mut stats = SessionStats::default();
    let reader = BufReader::new(File::open(path)?);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if stats.timestamp.is_none()
            && let Some(ts) = record["timestamp"].as_str().and_then(parse_ts)
        {
            stats.timestamp = Some(ts);
        }
        match record["type"].as_str() {
            Some("session_meta") => parse_session_meta(&mut stats, &record["payload"]),
            Some("response_item") | Some("event_msg") => {
                parse_payload(&mut stats, &record["payload"])
            }
            Some("compacted") => stats.compacted_events += 1,
            Some("message") => parse_payload(&mut stats, &record),
            Some("reasoning") => stats.reasoning_items += 1,
            _ => {}
        }
    }
    Ok(stats)
}

fn parse_session_meta(stats: &mut SessionStats, payload: &Value) {
    if let Some(id) = payload["id"].as_str() {
        stats.id = Some(id.to_string());
    }
    if let Some(ts) = payload["timestamp"].as_str().and_then(parse_ts) {
        stats.timestamp = Some(ts);
    }
    if let Some(cwd) = payload["cwd"].as_str()
        && !cwd.is_empty()
    {
        stats.cwd = Some(cwd.to_string());
    }
    if let Some(version) = payload["cli_version"].as_str()
        && !version.is_empty()
    {
        stats.cli_version = Some(version.to_string());
    }
    if let Some(originator) = payload["originator"].as_str()
        && !originator.is_empty()
    {
        stats.originator = Some(originator.to_string());
    }
    if let Some(source) = payload["source"].as_str()
        && !source.is_empty()
    {
        stats.source = Some(source.to_string());
    }
}

fn parse_payload(stats: &mut SessionStats, payload: &Value) {
    match payload["type"].as_str() {
        Some("message") => match payload["role"].as_str() {
            Some("user") => stats.user_messages += 1,
            Some("assistant") => stats.agent_messages += 1,
            _ => {}
        },
        Some("user_message") => stats.user_messages += 1,
        Some("agent_message") => stats.agent_messages += 1,
        Some("reasoning") | Some("agent_reasoning") => stats.reasoning_items += 1,
        Some("function_call") | Some("custom_tool_call") => {
            stats.tool_calls += 1;
            if let Some(name) = payload["name"].as_str()
                && !name.is_empty()
            {
                *stats.tool_counts.entry(name.to_string()).or_insert(0) += 1;
            }
        }
        Some("function_call_output") | Some("custom_tool_call_output") => stats.tool_results += 1,
        Some("exec_command_end") => {
            stats.tool_results += 1;
            *stats.tool_counts.entry("exec".to_string()).or_insert(0) += 1;
        }
        Some("patch_apply_end") => {
            stats.tool_results += 1;
            *stats
                .tool_counts
                .entry("apply_patch".to_string())
                .or_insert(0) += 1;
        }
        Some("web_search_call") => {
            stats.tool_calls += 1;
            *stats
                .tool_counts
                .entry("web_search".to_string())
                .or_insert(0) += 1;
        }
        Some("turn_aborted") => stats.aborted_turns += 1,
        Some("context_compacted") => stats.compacted_events += 1,
        Some("entered_review_mode") | Some("exited_review_mode") => stats.review_events += 1,
        _ => {}
    }
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn session_id_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    if stem.starts_with("rollout-") && stem.len() >= 36 {
        return Some(stem[stem.len() - 36..].to_string());
    }
    stem.rsplit_once('-').map(|(_, id)| id.to_string())
}

fn top_counts(counts: HashMap<String, usize>, limit: usize) -> Vec<CountRow> {
    let mut rows: Vec<_> = counts
        .into_iter()
        .map(|(name, count)| CountRow { name, count })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    rows.truncate(limit);
    rows
}

fn render_text(stats: &CodexStats) {
    section("Codex Sessions");
    println!(
        "  Total:          {}",
        ui::emphasis(&ui::fmt_count(stats.total_sessions as u64))
    );
    println!(
        "  Today:          {}",
        ui::fmt_count(stats.sessions_today as u64)
    );
    println!(
        "  This week:      {}",
        ui::fmt_count(stats.sessions_this_week as u64)
    );
    println!(
        "  Active files:   {}",
        ui::fmt_count(stats.active_session_files as u64)
    );
    println!(
        "  Archived files: {}",
        ui::fmt_count(stats.archived_sessions as u64)
    );

    section("Activity");
    println!(
        "  User messages:  {}",
        ui::count(stats.user_messages as u64)
    );
    println!(
        "  Agent messages: {}",
        ui::count(stats.agent_messages as u64)
    );
    println!(
        "  Reasoning:      {}",
        ui::count(stats.reasoning_items as u64)
    );
    println!("  Tool calls:     {}", ui::count(stats.tool_calls as u64));
    println!("  Tool results:   {}", ui::count(stats.tool_results as u64));
    println!(
        "  Aborted turns:  {}",
        ui::fmt_count(stats.aborted_turns as u64)
    );
    println!(
        "  Compactions:    {}",
        ui::fmt_count(stats.compacted_events as u64)
    );
    println!(
        "  Review events:  {}",
        ui::fmt_count(stats.review_events as u64)
    );

    if let Some(state) = &stats.state {
        section("State DB");
        println!(
            "  Threads:        {}",
            ui::fmt_count(state.thread_count as u64)
        );
        println!(
            "  User threads:   {}",
            ui::fmt_count(state.threads_with_user_event as u64)
        );
        println!(
            "  Tokens used:    {}",
            ui::count(state.total_tokens_used.max(0) as u64)
        );
    }

    print_rows("Top Projects", &stats.top_projects, |name| {
        ui::project(name)
    });
    print_rows("Top Tools", &stats.top_tools, ui::tool_name);
    print_rows("CLI Versions", &stats.cli_versions, |name| {
        ui::model_name(name)
    });

    if let Some(most_recent) = &stats.most_recent {
        section("Most Recent");
        println!("  Session:  {}", ui::session_id(&most_recent.session_id));
        if let Some(title) = &most_recent.title {
            println!("  Title:    {}", title);
        }
        if let Some(project) = &most_recent.project {
            println!("  Project:  {}", ui::project(project));
        }
        if let Some(date) = &most_recent.date {
            println!("  Date:     {}", date);
        }
    }
}

fn print_rows<F>(title: &str, rows: &[CountRow], format_name: F)
where
    F: Fn(&str) -> String,
{
    section(title);
    if rows.is_empty() {
        println!("  (none)");
        return;
    }
    for (i, row) in rows.iter().enumerate() {
        println!(
            "  {}. {}  {}",
            i + 1,
            format_name(&row.name),
            ui::fmt_count(row.count as u64)
        );
    }
}

fn section(title: &str) {
    println!();
    println!("{}", ui::emphasis(title));
}

fn query_state_stats(path: &Path) -> Result<CodexStateStats> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stats = CodexStateStats {
        thread_count: conn.query_row("SELECT COUNT(*) FROM threads", [], |r| r.get::<_, i64>(0))?
            as usize,
        threads_with_user_event: conn.query_row(
            "SELECT COUNT(*) FROM threads WHERE has_user_event != 0",
            [],
            |r| r.get::<_, i64>(0),
        )? as usize,
        total_tokens_used: conn.query_row(
            "SELECT COALESCE(SUM(tokens_used), 0) FROM threads",
            [],
            |r| r.get(0),
        )?,
        top_projects: Vec::new(),
        top_models: Vec::new(),
    };

    stats.top_projects = query_count_rows(
        &conn,
        "SELECT cwd, COUNT(*) FROM threads GROUP BY cwd ORDER BY COUNT(*) DESC, cwd ASC LIMIT 10",
    )?;
    stats.top_models = query_count_rows(
        &conn,
        "SELECT model_provider, COUNT(*) FROM threads GROUP BY model_provider ORDER BY COUNT(*) DESC, model_provider ASC LIMIT 10",
    )?;
    Ok(stats)
}

fn query_count_rows(conn: &rusqlite::Connection, sql: &str) -> Result<Vec<CountRow>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(CountRow {
            name: row.get(0)?,
            count: row.get::<_, i64>(1)? as usize,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_session(dir: &Path, rel: &str, lines: &[&str]) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
    }

    #[test]
    fn extracts_rollout_uuid_from_path() {
        let path = Path::new(
            "/tmp/rollout-2026-01-20T16-40-15-019bddc7-c411-7500-ae7e-d3f2618b4cfc.jsonl",
        );
        assert_eq!(
            session_id_from_path(path).as_deref(),
            Some("019bddc7-c411-7500-ae7e-d3f2618b4cfc")
        );
    }

    #[test]
    fn parses_codex_session_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_session(
            tmp.path(),
            "sessions/2026/05/05/rollout-2026-05-05T00-00-00-abc.jsonl",
            &[
                r#"{"timestamp":"2026-05-05T00:00:00Z","type":"session_meta","payload":{"id":"abc","cwd":"/repo","originator":"codex_cli_rs","cli_version":"0.99.0","source":"cli"}}"#,
                r#"{"timestamp":"2026-05-05T00:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[]}}"#,
                r#"{"timestamp":"2026-05-05T00:00:02Z","type":"response_item","payload":{"type":"agent_message","message":"done"}}"#,
                r#"{"timestamp":"2026-05-05T00:00:03Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{}","call_id":"c"}}"#,
                r#"{"timestamp":"2026-05-05T00:00:04Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c","output":"{}"}}"#,
            ],
        );
        write_session(
            tmp.path(),
            "archived_sessions/rollout-2026-01-01T00-00-00-def.jsonl",
            &[
                r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"id":"def","cwd":"/repo2","cli_version":"0.98.0"}}"#,
            ],
        );

        let stats = build_stats(&CodexStore::at(tmp.path().to_path_buf())).unwrap();
        assert_eq!(stats.total_sessions, 2);
        assert_eq!(stats.archived_sessions, 1);
        assert_eq!(stats.active_session_files, 1);
        assert_eq!(stats.user_messages, 1);
        assert_eq!(stats.agent_messages, 1);
        assert_eq!(stats.tool_calls, 1);
        assert_eq!(stats.tool_results, 1);
        assert_eq!(stats.top_projects[0].name, "/repo");
        assert_eq!(stats.top_tools[0].name, "shell");
    }
}

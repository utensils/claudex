//! Integration tests for `IndexStore` query methods.
//!
//! Each test builds a tiny project tree in a TempDir, syncs the index, then
//! asserts against one or more query methods. Uses `SessionStore::at` and
//! `IndexStore::open_at` so tests don't race on `$HOME`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration, Local};
use claudex::cli::ResolvedFilter;
use claudex::index::IndexStore;
use claudex::providers::{ClaudeProvider, CodexProvider, OpenClawProvider, PiProvider, Provider};
use claudex::store::SessionStore;
use rusqlite::{Connection, params};
use tempfile::TempDir;

/// The unfiltered (all-providers, no date/model) filter used by query tests.
fn all() -> ResolvedFilter {
    ResolvedFilter::default()
}

/// Wrap a `projects` directory in the single-Claude-provider set the index sync
/// methods now expect.
fn claude_providers(projects: PathBuf) -> Vec<Provider> {
    vec![Provider::Claude(ClaudeProvider::at(SessionStore::at(
        projects,
    )))]
}

fn codex_providers(codex: PathBuf) -> Vec<Provider> {
    vec![Provider::Codex(CodexProvider::at(codex))]
}

fn pi_providers(agent: PathBuf) -> Vec<Provider> {
    vec![Provider::Pi(PiProvider::at(agent))]
}

fn openclaw_providers(state: PathBuf) -> Vec<Provider> {
    vec![Provider::OpenClaw(OpenClawProvider::at(state))]
}

/// Write a JSONL session file under `<projects>/<encoded_project>/<session>.jsonl`.
fn write_session(projects: &Path, encoded_project: &str, session: &str, lines: &[&str]) -> PathBuf {
    let dir = projects.join(encoded_project);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{session}.jsonl"));
    write_jsonl(&path, lines)
}

fn write_jsonl(path: &Path, lines: &[&str]) -> PathBuf {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = fs::File::create(path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    f.flush().unwrap();
    path.to_path_buf()
}

/// Build a fixture with three projects and two sessions each, exercising
/// usage/thinking/turn-duration/tool-use/pr-link/file-history records.
fn build_fixture() -> (TempDir, Vec<Provider>, IndexStore) {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");

    // Project A — two sessions with Bash + Edit tools on Opus
    write_session(
        &projects,
        "-Users-test-Projects-alpha",
        "sess-a1",
        &[
            r#"{"type":"user","sessionId":"sess-a1","timestamp":"2026-04-10T10:00:00Z","message":{"content":"hello alpha"}}"#,
            r#"{"type":"assistant","sessionId":"sess-a1","timestamp":"2026-04-10T10:01:00Z","message":{"model":"claude-opus-4-6","stop_reason":"end_turn","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":200,"cache_read_input_tokens":5000},"content":[{"type":"tool_use","name":"Bash","id":"t1","input":{}},{"type":"text","text":"ok"}]}}"#,
            r#"{"type":"assistant","sessionId":"sess-a1","timestamp":"2026-04-10T10:02:00Z","message":{"model":"claude-opus-4-6","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":1000},"content":[{"type":"tool_use","name":"Edit","id":"t2","input":{}},{"type":"thinking","text":"..."}]}}"#,
            r#"{"type":"system","subtype":"turn_duration","durationMs":5000,"timestamp":"2026-04-10T10:01:30Z","sessionId":"sess-a1"}"#,
            r#"{"type":"system","subtype":"turn_duration","durationMs":10000,"timestamp":"2026-04-10T10:02:30Z","sessionId":"sess-a1"}"#,
            r#"{"type":"file-history-snapshot","snapshot":{"messageId":"m1","trackedFileBackups":{"src/a.rs":{"backupFileName":"x","version":1}},"timestamp":"2026-04-10T10:02:00Z"}}"#,
            r#"{"type":"pr-link","prNumber":7,"prUrl":"https://github.com/org/alpha/pull/7","prRepository":"org/alpha","timestamp":"2026-04-10T10:03:00Z","sessionId":"sess-a1"}"#,
        ],
    );
    write_session(
        &projects,
        "-Users-test-Projects-alpha",
        "sess-a2",
        &[
            r#"{"type":"user","sessionId":"sess-a2","timestamp":"2026-04-11T09:00:00Z","message":{"content":"search for foo"}}"#,
            r#"{"type":"assistant","sessionId":"sess-a2","timestamp":"2026-04-11T09:00:05Z","message":{"model":"claude-opus-4-6","stop_reason":"end_turn","usage":{"input_tokens":50,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":500},"content":[{"type":"tool_use","name":"Grep","id":"t3","input":{}},{"type":"text","text":"foo result"}]}}"#,
        ],
    );

    // Project B — Sonnet session with a different tool mix
    write_session(
        &projects,
        "-Users-test-Projects-beta",
        "sess-b1",
        &[
            r#"{"type":"user","sessionId":"sess-b1","timestamp":"2026-04-12T12:00:00Z","message":{"content":"refactor"}}"#,
            r#"{"type":"assistant","sessionId":"sess-b1","timestamp":"2026-04-12T12:00:10Z","message":{"model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":{"input_tokens":200,"output_tokens":80,"cache_creation_input_tokens":0,"cache_read_input_tokens":100},"content":[{"type":"tool_use","name":"Read","id":"t4","input":{}},{"type":"tool_use","name":"Edit","id":"t5","input":{}},{"type":"text","text":"done"}]}}"#,
            r#"{"type":"system","subtype":"turn_duration","durationMs":20000,"timestamp":"2026-04-12T12:00:20Z","sessionId":"sess-b1"}"#,
        ],
    );

    // Project C — empty-ish session to exercise edge cases
    write_session(
        &projects,
        "-Users-test-Projects-gamma",
        "sess-c1",
        &[
            r#"{"type":"user","sessionId":"sess-c1","timestamp":"2026-04-13T00:00:00Z","message":{"content":"ping"}}"#,
        ],
    );

    let providers = claude_providers(projects);
    let mut idx = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    idx.sync_now(&providers).unwrap();
    (tmp, providers, idx)
}

#[test]
fn sync_now_indexes_every_session() {
    let (_tmp, _store, idx) = build_fixture();
    // 2 sessions in alpha + 1 in beta + 1 in gamma = 4
    let rows = idx.query_sessions(None, None, &all(), 100).unwrap();
    assert_eq!(rows.len(), 4);
}

#[test]
fn sync_indexes_subagent_transcripts_and_rolls_up_parent_reports() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    let encoded = "-Users-test-Projects-agents";

    let parent_path = write_session(
        &projects,
        encoded,
        "parent-1",
        &[
            r#"{"type":"user","sessionId":"parent-1","timestamp":"2026-04-10T10:00:00Z","message":{"content":"delegate work"}}"#,
            r#"{"type":"assistant","sessionId":"parent-1","timestamp":"2026-04-10T10:01:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"tool_use","name":"Task","id":"t1","input":{}},{"type":"text","text":"started subagent"}]}}"#,
        ],
    );
    write_jsonl(
        &projects
            .join(encoded)
            .join("parent-1/subagents/workflows/run-1/agent-child.jsonl"),
        &[
            r#"{"type":"user","isSidechain":true,"sessionId":"child-1","timestamp":"2026-04-10T10:02:00Z","message":{"content":"subagent prompt"}}"#,
            r#"{"type":"assistant","isSidechain":true,"sessionId":"child-1","timestamp":"2026-04-10T10:03:00Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":900,"output_tokens":80,"cache_creation_input_tokens":10,"cache_read_input_tokens":50},"content":[{"type":"tool_use","name":"Edit","id":"t2","input":{}},{"type":"text","text":"Authenticated the dev app from a subagent"}]}}"#,
            r#"{"type":"file-history-snapshot","isSidechain":true,"snapshot":{"trackedFileBackups":{"src/subagent.rs":{"backupFileName":"x"}}},"timestamp":"2026-04-10T10:03:10Z"}"#,
        ],
    );
    write_jsonl(
        &projects
            .join(encoded)
            .join("parent-1/subagents/workflows/run-1/journal.jsonl"),
        &[r#"{"type":"started","agentId":"agent-child"}"#],
    );

    let store = SessionStore::at(projects);
    let files = store.all_session_files(None).unwrap();
    assert_eq!(files.len(), 2, "journal.jsonl must not be discovered");

    let mut idx = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    let providers = vec![Provider::Claude(ClaudeProvider::at(store))];
    idx.sync_now(&providers).unwrap();

    let indexed_sessions = idx.query_sessions(None, None, &all(), 10).unwrap();
    assert_eq!(indexed_sessions.len(), 2);

    let hits = idx
        .search_fts("Authenticated the dev app", None, &all(), 10)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id.as_deref(), Some("child-1"));

    let cost_rows = idx.query_cost_per_session(None, &all(), 10).unwrap();
    assert_eq!(cost_rows.len(), 1);
    assert_eq!(cost_rows[0].session_id.as_deref(), Some("parent-1"));
    assert_eq!(cost_rows[0].input_tokens, 1000);
    assert_eq!(cost_rows[0].output_tokens, 100);

    let tools = idx.query_tools_per_session(None, &all(), 10).unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].session_id.as_deref(), Some("parent-1"));
    assert_eq!(tools[0].tools.get("Task"), Some(&1));
    assert_eq!(tools[0].tools.get("Edit"), Some(&1));

    let detail = idx
        .query_session_detail(&parent_path.to_string_lossy())
        .unwrap()
        .expect("parent detail");
    assert_eq!(detail.message_count, 4);
    assert_eq!(detail.input_tokens, 1000);
    assert_eq!(detail.output_tokens, 100);
    assert!(
        detail
            .files_modified
            .contains(&"src/subagent.rs".to_string())
    );
    assert_eq!(detail.subagent_files.len(), 1);
    assert!(detail.subagent_files[0].ends_with("agent-child.jsonl"));
}

#[test]
fn query_sessions_filters_by_project() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx
        .query_sessions(Some("alpha"), None, &all(), 100)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.project_name.contains("alpha")));
}

#[test]
fn query_sessions_respects_limit() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_sessions(None, None, &all(), 2).unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn query_cost_by_project_aggregates_token_usage() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_cost_by_project(None, &all(), 100).unwrap();
    assert_eq!(rows.len(), 3); // alpha, beta, gamma

    let alpha = rows.iter().find(|r| r.project.contains("alpha")).unwrap();
    // alpha: sess-a1 (1000+100) + sess-a2 (50) = 1150 input
    assert_eq!(alpha.input_tokens, 1150);
    assert_eq!(alpha.output_tokens, 570); // 500 + 50 + 20
    assert_eq!(alpha.session_count, 2);
    assert!(alpha.models.iter().any(|m| m == "Opus"));
    assert!(alpha.cost_usd > 0.0);
}

#[test]
fn query_cost_per_session_returns_rows_for_sessions_with_usage() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_cost_per_session(None, &all(), 100).unwrap();
    // gamma has no assistant message (no token usage) so it's filtered out;
    // the three sessions with tokens should all show up.
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r.session_id.is_some()));
    // Sorted by cost descending.
    for w in rows.windows(2) {
        assert!(w[0].cost_usd >= w[1].cost_usd);
    }
}

#[test]
fn query_cost_summary_matches_model_totals_regardless_of_limit() {
    let (_tmp, _store, idx) = build_fixture();

    // The grand total is the single source of truth for `cost`'s TOTAL row.
    // It must equal what `models` sums (raw SUM(token_usage.cost_usd)) and be
    // independent of any per-project display limit.
    let summary = idx.query_cost_summary(None, &all()).unwrap();
    let model_total: f64 = idx
        .query_model_usage(None, &all())
        .unwrap()
        .iter()
        .map(|r| r.cost_usd)
        .sum();
    assert!((summary.cost_usd - model_total).abs() < 1e-9);

    // Population alignment (the zero-usage gamma project is the trap): the
    // by-project caption/TOTAL counts must match the rows actually displayed by
    // `query_cost_by_project` (LEFT JOIN — gamma's $0 row is shown), while
    // `usage_session_count` matches the token-bearing rows of
    // `query_cost_per_session` (gamma excluded).
    let rows = idx.query_cost_by_project(None, &all(), 100).unwrap();
    let per_session = idx.query_cost_per_session(None, &all(), 100).unwrap();
    let row_sessions: i64 = rows.iter().map(|r| r.session_count).sum();
    assert_eq!(summary.project_count as usize, rows.len()); // 3: alpha, beta, gamma
    assert_eq!(summary.session_count, row_sessions); // 4: TOTAL Sessions == sum of rows
    assert_eq!(summary.usage_session_count as usize, per_session.len()); // 3 token-bearing
    assert_eq!(summary.session_count, 4);
    assert_eq!(summary.project_count, 3);
    assert_eq!(summary.usage_session_count, 3);

    // Token columns aggregate every session, matching the unlimited by-project
    // sums (limit=100 returns all rows here).
    let row_input: i64 = rows.iter().map(|r| r.input_tokens).sum();
    let row_cost: f64 = rows.iter().map(|r| r.cost_usd).sum();
    assert_eq!(summary.input_tokens, row_input);
    assert!((summary.cost_usd - row_cost).abs() < 1e-9);

    // Limit-invariance: a limit of 1 truncates the displayed rows but the
    // summary is unchanged.
    let limited = idx.query_cost_by_project(None, &all(), 1).unwrap();
    assert_eq!(limited.len(), 1);
    assert!(
        (idx.query_cost_summary(None, &all()).unwrap().cost_usd - summary.cost_usd).abs() < 1e-9
    );
}

#[test]
fn query_tools_aggregate_counts_tool_invocations() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_tools_aggregate(None, &all(), 100).unwrap();
    let counts: std::collections::HashMap<_, _> = rows
        .iter()
        .map(|r| (r.tool_name.clone(), r.count))
        .collect();
    assert_eq!(counts.get("Bash"), Some(&1));
    assert_eq!(counts.get("Edit"), Some(&2)); // alpha + beta
    assert_eq!(counts.get("Grep"), Some(&1));
    assert_eq!(counts.get("Read"), Some(&1));
}

#[test]
fn query_tools_per_session_breaks_down_by_session() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_tools_per_session(None, &all(), 100).unwrap();
    // Only sessions with tools — gamma has none.
    assert_eq!(rows.len(), 3);
    let sess_a1 = rows
        .iter()
        .find(|r| r.session_id.as_deref() == Some("sess-a1"))
        .unwrap();
    assert_eq!(sess_a1.tools.get("Bash"), Some(&1));
    assert_eq!(sess_a1.tools.get("Edit"), Some(&1));
}

#[test]
fn search_fts_finds_terms_in_user_messages() {
    let (_tmp, _store, idx) = build_fixture();
    let hits = idx.search_fts("foo", None, &all(), 10).unwrap();
    assert!(!hits.is_empty());
    assert!(
        hits.iter().any(|h| h.snippet.contains("foo")),
        "got: {hits:?}",
        hits = hits.iter().map(|h| h.snippet.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn search_fts_filters_by_project() {
    let (_tmp, _store, idx) = build_fixture();
    let hits = idx.search_fts("alpha", Some("alpha"), &all(), 10).unwrap();
    assert!(hits.iter().all(|h| h.project_name.contains("alpha")));
}

#[test]
fn search_fts_respects_limit() {
    let (_tmp, _store, idx) = build_fixture();
    let hits = idx.search_fts("the", None, &all(), 1).unwrap();
    assert!(hits.len() <= 1);
}

#[test]
fn query_turn_stats_returns_percentiles() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_turn_stats(None, &all(), 100).unwrap();
    let alpha = rows.iter().find(|r| r.project.contains("alpha")).unwrap();
    // alpha has turn_durations [5000, 10000]
    assert_eq!(alpha.turn_count, 2);
    assert!(alpha.max_duration_ms >= 10000);
    assert!(alpha.avg_duration_ms > 0.0);
    assert!(alpha.p50_duration_ms > 0.0);
}

#[test]
fn query_pr_links_returns_unique_links() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_pr_links(None, &all(), 100).unwrap();
    // Only alpha has a pr-link.
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pr_number, 7);
    assert_eq!(rows[0].pr_repository, "org/alpha");
}

#[test]
fn pr_link_backfill_repairs_provider_rows_without_full_rebuild() {
    let tmp = TempDir::new().unwrap();
    let codex = tmp.path().join(".codex");
    write_jsonl(
        &codex.join("sessions/2026/05/30/rollout-2026-05-30T00-00-00-codex-pr.jsonl"),
        &[
            r#"{"timestamp":"2026-05-30T00:00:00Z","type":"session_meta","payload":{"id":"codex-pr","cwd":"/repo"}}"#,
            r#"{"timestamp":"2026-05-30T00:01:00Z","type":"event_msg","payload":{"type":"exec_command_end","command":"gh pr create --fill","stdout":"https://github.com/utensils/claudex/pull/38\n"}}"#,
            r#"{"timestamp":"2026-05-30T00:02:00Z","type":"event_msg","payload":{"type":"exec_command_end","command":["sed","-n","1,20p","SKILL.md"],"stdout":"example: gh pr view https://github.com/org/repo/pull/123\n"}}"#,
            r#"{"timestamp":"2026-05-30T00:03:00Z","type":"response_item","payload":{"type":"function_call_output","call_id":"search","output":"src/providers/pr.rs: text contains ::git-create-pr https://github.com/utensils/claudex/pull/1"}}"#,
            r#"{"timestamp":"2026-05-30T00:04:00Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"c1","arguments":"{\"cmd\":\"gh pr view\"}"}}"#,
            r#"{"timestamp":"2026-05-30T00:05:00Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"https://github.com/utensils/aethon/pull/167"}}"#,
            r#"{"timestamp":"2026-05-30T00:06:00Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Opened https://github.com/utensils/ptywright/pull/14"}]}}"#,
        ],
    );
    let providers = codex_providers(codex);
    let db_path = tmp.path().join("index.db");
    let mut idx = IndexStore::open_at(&db_path).unwrap();
    idx.sync_now(&providers).unwrap();
    assert_eq!(idx.query_pr_links(None, &all(), 100).unwrap().len(), 3);
    drop(idx);

    let conn = Connection::open(&db_path).unwrap();
    conn.execute("DELETE FROM pr_links", []).unwrap();
    conn.execute(
        "INSERT INTO pr_links (session_rowid, pr_number, pr_url, pr_repository, timestamp)
         SELECT id, 123, 'https://github.com/org/repo/pull/123', 'org/repo', 'bad'
         FROM sessions WHERE provider = 'codex'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('pr_link_derivation_revision:codex', ?)",
        params!["0"],
    )
    .unwrap();
    drop(conn);

    let mut idx = IndexStore::open_at(&db_path).unwrap();
    idx.ensure_pr_links_fresh(&providers).unwrap();
    let rows = idx.query_pr_links(None, &all(), 100).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter().find(|row| row.pr_number == 38).unwrap().pr_url,
        "https://github.com/utensils/claudex/pull/38"
    );
    assert!(
        rows.iter()
            .any(|row| row.pr_url == "https://github.com/utensils/aethon/pull/167")
    );
    assert!(
        rows.iter()
            .any(|row| row.pr_url == "https://github.com/utensils/ptywright/pull/14")
    );
}

#[test]
fn pi_pr_link_backfill_ignores_bash_execution_output_without_command() {
    let tmp = TempDir::new().unwrap();
    let agent = tmp.path().join(".pi/agent");
    write_jsonl(
        &agent.join("sessions/--repo--/2026-05-30T00-00-00Z_sess-pi-pr.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"sess-pi-pr","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo"}"#,
            r#"{"type":"message","id":"a1","timestamp":"2026-05-30T00:01:00Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"gh pr create --fill"}}],"provider":"anthropic","model":"claude-3-opus","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.01}},"stopReason":"toolUse"}}"#,
            r#"{"type":"message","id":"t1","timestamp":"2026-05-30T00:02:00Z","message":{"role":"toolResult","toolCallId":"c1","toolName":"bash","content":[{"type":"text","text":"https://github.com/utensils/claudex/pull/39"}],"isError":false}}"#,
            r#"{"type":"message","id":"b1","timestamp":"2026-05-30T00:03:00Z","message":{"role":"bashExecution","output":"docs mention gh pr view https://github.com/utensils/claudex/pull/1"}}"#,
            r#"{"type":"message","id":"b2","timestamp":"2026-05-30T00:04:00Z","message":{"role":"bashExecution","command":"gh pr view","output":"https://github.com/utensils/ptywright/pull/14"}}"#,
            r#"{"type":"message","id":"a2","timestamp":"2026-05-30T00:05:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Opened https://github.com/utensils/aethon/pull/168"}],"provider":"anthropic","model":"claude-3-opus","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.01}},"stopReason":"stop"}}"#,
        ],
    );
    let providers = pi_providers(agent);
    let db_path = tmp.path().join("index.db");
    let mut idx = IndexStore::open_at(&db_path).unwrap();
    idx.sync_now(&providers).unwrap();
    drop(idx);

    let conn = Connection::open(&db_path).unwrap();
    conn.execute("DELETE FROM pr_links", []).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('pr_link_derivation_revision:pi', ?)",
        params!["0"],
    )
    .unwrap();
    drop(conn);

    let mut idx = IndexStore::open_at(&db_path).unwrap();
    idx.ensure_pr_links_fresh(&providers).unwrap();
    let rows = idx.query_pr_links(None, &all(), 100).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter().find(|row| row.pr_number == 39).unwrap().pr_url,
        "https://github.com/utensils/claudex/pull/39"
    );
    assert!(
        rows.iter()
            .any(|row| row.pr_url == "https://github.com/utensils/ptywright/pull/14")
    );
    assert!(
        rows.iter()
            .any(|row| row.pr_url == "https://github.com/utensils/aethon/pull/168")
    );
}

#[test]
fn query_file_mods_returns_file_counts() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_file_mods(None, None, &all(), 100).unwrap();
    assert!(rows.iter().any(|r| r.file_path == "src/a.rs"));
    let src_a = rows.iter().find(|r| r.file_path == "src/a.rs").unwrap();
    assert_eq!(src_a.distinct_session_count, 1);
    assert!(src_a.top_project.as_deref().unwrap_or("").contains("alpha"));
}

#[test]
fn query_model_usage_groups_by_model_family() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_model_usage(None, &all()).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| !r.model.is_empty()));
    let models: Vec<_> = rows.iter().map(|r| r.model.as_str()).collect();
    assert!(models.iter().any(|m| m.contains("opus")));
    assert!(models.iter().any(|m| m.contains("sonnet")));
}

#[test]
fn query_summary_reports_totals() {
    let (_tmp, _store, idx) = build_fixture();
    let data = idx.query_summary(&all()).unwrap();
    assert_eq!(data.total_sessions, 4);
    assert!(data.total_cost > 0.0);
    assert_eq!(data.pr_count, 1);
    // file-modified-count is distinct files
    assert!(data.files_modified_count >= 1);
    // top projects should include alpha
    assert!(data.top_projects.iter().any(|(p, _)| p.contains("alpha")));
    // top tools should include Edit (count 2)
    assert!(data.top_tools.iter().any(|(t, _)| t == "Edit"));
}

#[test]
fn query_summary_counts_sessions_active_today_by_last_timestamp() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    let now = Local::now();
    let yesterday = now - Duration::days(1);
    write_session(
        &projects,
        "-Users-test-Projects-active",
        "sess-active-today",
        &[
            &format!(
                r#"{{"type":"user","sessionId":"sess-active-today","timestamp":"{}","message":{{"content":"started yesterday"}}}}"#,
                yesterday.to_rfc3339()
            ),
            &format!(
                r#"{{"type":"assistant","sessionId":"sess-active-today","timestamp":"{}","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":[{{"type":"text","text":"active today"}}]}}}}"#,
                now.to_rfc3339()
            ),
        ],
    );

    let providers = claude_providers(projects);
    let mut idx = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    idx.sync_now(&providers).unwrap();
    let data = idx.query_summary(&all()).unwrap();
    assert_eq!(data.total_sessions, 1);
    assert_eq!(data.sessions_today, 1);
}

#[test]
fn query_summary_counts_sessions_active_this_week_by_last_timestamp() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    let now = Local::now();
    let days_since_monday = now.weekday().num_days_from_monday() as i64;
    let before_week = now - Duration::days(days_since_monday + 1);
    write_session(
        &projects,
        "-Users-test-Projects-active-week",
        "sess-active-week",
        &[
            &format!(
                r#"{{"type":"user","sessionId":"sess-active-week","timestamp":"{}","message":{{"content":"started before week"}}}}"#,
                before_week.to_rfc3339()
            ),
            &format!(
                r#"{{"type":"assistant","sessionId":"sess-active-week","timestamp":"{}","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":[{{"type":"text","text":"active this week"}}]}}}}"#,
                now.to_rfc3339()
            ),
        ],
    );

    let providers = claude_providers(projects);
    let mut idx = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    idx.sync_now(&providers).unwrap();
    let data = idx.query_summary(&all()).unwrap();
    assert_eq!(data.total_sessions, 1);
    assert_eq!(data.sessions_this_week, 1);
    assert!(data.week_cost > 0.0);
}

#[test]
fn ensure_fresh_is_noop_within_staleness_window() {
    let (_tmp, providers, mut idx) = build_fixture();
    // fixture already synced; ensure_fresh should return immediately without
    // changing anything.
    let before = idx.query_sessions(None, None, &all(), 100).unwrap().len();
    idx.ensure_fresh(&providers).unwrap();
    let after = idx.query_sessions(None, None, &all(), 100).unwrap().len();
    assert_eq!(before, after);
}

#[test]
fn force_rebuild_wipes_and_reindexes() {
    let (_tmp, providers, mut idx) = build_fixture();
    let before = idx.query_sessions(None, None, &all(), 100).unwrap().len();
    let indexed = idx.force_rebuild(&providers).unwrap();
    let after = idx.query_sessions(None, None, &all(), 100).unwrap().len();
    assert_eq!(before, after);
    assert!(indexed >= before);
}

#[test]
fn sync_now_is_idempotent() {
    let (_tmp, providers, mut idx) = build_fixture();
    let before = idx.query_sessions(None, None, &all(), 100).unwrap().len();
    idx.sync_now(&providers).unwrap();
    idx.sync_now(&providers).unwrap();
    let after = idx.query_sessions(None, None, &all(), 100).unwrap().len();
    assert_eq!(before, after);
}

#[test]
fn sync_picks_up_new_sessions() {
    let (tmp, providers, mut idx) = build_fixture();
    let before = idx.query_sessions(None, None, &all(), 100).unwrap().len();

    // Add a fresh session to an existing project.
    write_session(
        &tmp.path().join("projects"),
        "-Users-test-Projects-alpha",
        "sess-a3",
        &[
            r#"{"type":"user","sessionId":"sess-a3","timestamp":"2026-04-14T00:00:00Z","message":{"content":"new thing"}}"#,
        ],
    );

    idx.sync_now(&providers).unwrap();
    let after = idx.query_sessions(None, None, &all(), 100).unwrap().len();
    assert_eq!(after, before + 1);
}

#[test]
fn openclaw_source_key_reuses_trajectory_row_when_transcript_appears() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("index.db");
    let state = tmp.path().join(".openclaw");
    let sessions = state.join("agents/main/sessions");
    write_jsonl(
        &sessions.join("sess-open.trajectory.jsonl"),
        &[
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"sess-open","source":"runtime","type":"model.completed","ts":"2026-05-30T00:00:00Z","seq":1,"sessionId":"sess-open","workspaceDir":"/repo/traj","provider":"openai","modelId":"gpt-5.2","data":{"assistantText":"trajectory","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.01}}}}"#,
        ],
    );
    let providers = openclaw_providers(state.clone());
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&providers).unwrap();

    let first_row: i64 = {
        let conn = Connection::open(&db).unwrap();
        conn.query_row(
            "SELECT id FROM sessions WHERE provider = 'openclaw' AND source_key = 'agent:main:session:sess-open'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };

    write_jsonl(
        &sessions.join("sess-open.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"sess-open","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo/classic"}"#,
        ],
    );
    idx.sync_now(&providers).unwrap();

    let conn = Connection::open(&db).unwrap();
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE provider = 'openclaw' AND source_key = 'agent:main:session:sess-open'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let row_id: i64 = conn
        .query_row(
            "SELECT id FROM sessions WHERE provider = 'openclaw' AND source_key = 'agent:main:session:sess-open'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let path: String = conn
        .query_row(
            "SELECT file_path FROM sessions WHERE id = ?",
            params![row_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(row_id, first_row);
    assert!(path.ends_with("sess-open.jsonl"), "{path}");
    let cost: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM token_usage WHERE session_id = ?",
            params![row_id],
            |row| row.get(0),
        )
        .unwrap();
    let fts_hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages_fts WHERE session_id = ? AND content MATCH 'trajectory'",
            params![row_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cost, 0.01);
    assert_eq!(fts_hits, 1);
}

#[test]
fn query_sessions_filters_by_touched_file() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx
        .query_sessions(None, Some("src/a.rs"), &all(), 100)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].session_id.as_deref(), Some("sess-a1"));
}

#[test]
fn query_session_detail_returns_rich_metrics() {
    let (_tmp, _store, idx) = build_fixture();
    let session = idx.query_sessions(Some("alpha"), None, &all(), 10).unwrap();
    let file_path = session
        .iter()
        .find(|row| row.session_id.as_deref() == Some("sess-a1"))
        .map(|row| row.file_path.clone())
        .unwrap();
    let detail = idx
        .query_session_detail(&file_path)
        .unwrap()
        .expect("session detail");
    assert_eq!(detail.project, "/Users/test/Projects/alpha");
    assert_eq!(detail.message_count, 3);
    assert!(detail.cost_usd > 0.0);
    assert_eq!(detail.thinking_block_count, 1);
    assert_eq!(detail.files_modified, vec!["src/a.rs"]);
    assert!(!detail.tools.is_empty());
    assert!(!detail.pr_links.is_empty());
    assert!(!detail.stop_reasons.is_empty());
}

#[test]
fn mixed_model_sessions_are_split_in_token_usage_and_aggregated_per_session() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    write_session(
        &projects,
        "-Users-test-Projects-mixed",
        "sess-mixed",
        &[
            r#"{"type":"user","sessionId":"sess-mixed","timestamp":"2026-04-10T10:00:00Z","message":{"content":"do it"}}"#,
            r#"{"type":"assistant","sessionId":"sess-mixed","timestamp":"2026-04-10T10:01:00Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":20,"cache_creation_input_tokens":10,"cache_read_input_tokens":50},"content":[{"type":"text","text":"opus"}]}}"#,
            r#"{"type":"assistant","sessionId":"sess-mixed","timestamp":"2026-04-10T10:02:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":200,"output_tokens":40,"cache_creation_input_tokens":5,"cache_read_input_tokens":25},"content":[{"type":"text","text":"sonnet"}]}}"#,
        ],
    );

    let providers = claude_providers(projects);
    let mut idx = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    idx.sync_now(&providers).unwrap();

    let rows = idx.query_cost_per_session(None, &all(), 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].input_tokens, 300);
    assert_eq!(rows[0].output_tokens, 60);
    assert_eq!(rows[0].cache_creation_tokens, 15);
    assert_eq!(rows[0].cache_read_tokens, 75);
    assert_eq!(rows[0].models.len(), 2);

    let models = idx.query_model_usage(None, &all()).unwrap();
    assert_eq!(models.len(), 2);
    assert!(models.iter().any(|m| m.model.contains("opus")));
    assert!(models.iter().any(|m| m.model.contains("sonnet")));
}

#[test]
fn zero_token_model_rows_are_skipped() {
    // Mixed-model session where one model records only zero-token assistant
    // messages. The real model should still show up; the zero-token model
    // must not pollute per-model aggregates.
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    write_session(
        &projects,
        "-Users-test-Projects-zero",
        "sess-zero",
        &[
            r#"{"type":"user","sessionId":"sess-zero","timestamp":"2026-04-10T10:00:00Z","message":{"content":"hi"}}"#,
            r#"{"type":"assistant","sessionId":"sess-zero","timestamp":"2026-04-10T10:01:00Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"real"}]}}"#,
            r#"{"type":"assistant","sessionId":"sess-zero","timestamp":"2026-04-10T10:02:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"empty"}]}}"#,
        ],
    );

    let providers = claude_providers(projects);
    let mut idx = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    idx.sync_now(&providers).unwrap();

    let models = idx.query_model_usage(None, &all()).unwrap();
    assert_eq!(models.len(), 1);
    assert!(models[0].model.contains("opus"));
}

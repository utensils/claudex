//! Integration tests for `IndexStore` query methods.
//!
//! Each test builds a tiny project tree in a TempDir, syncs the index, then
//! asserts against one or more query methods. Uses `SessionStore::at` and
//! `IndexStore::open_at` so tests don't race on `$HOME`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use claudex::index::IndexStore;
use claudex::store::SessionStore;
use tempfile::TempDir;

/// Write a JSONL session file under `<projects>/<encoded_project>/<session>.jsonl`.
fn write_session(projects: &Path, encoded_project: &str, session: &str, lines: &[&str]) -> PathBuf {
    let dir = projects.join(encoded_project);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{session}.jsonl"));
    let mut f = fs::File::create(&path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    f.flush().unwrap();
    path
}

/// Build a fixture with three projects and two sessions each, exercising
/// usage/thinking/turn-duration/tool-use/pr-link/file-history records.
fn build_fixture() -> (TempDir, SessionStore, IndexStore) {
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

    let store = SessionStore::at(projects);
    let mut idx = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    idx.sync_now(&store).unwrap();
    (tmp, store, idx)
}

#[test]
fn sync_now_indexes_every_session() {
    let (_tmp, _store, idx) = build_fixture();
    // 2 sessions in alpha + 1 in beta + 1 in gamma = 4
    let rows = idx.query_sessions(None, 100).unwrap();
    assert_eq!(rows.len(), 4);
}

#[test]
fn query_sessions_filters_by_project() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_sessions(Some("alpha"), 100).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.project_name.contains("alpha")));
}

#[test]
fn query_sessions_respects_limit() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_sessions(None, 2).unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn query_cost_by_project_aggregates_token_usage() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_cost_by_project(None, 100).unwrap();
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
    let rows = idx.query_cost_per_session(None, 100).unwrap();
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
fn query_tools_aggregate_counts_tool_invocations() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_tools_aggregate(None, 100).unwrap();
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
    let rows = idx.query_tools_per_session(None, 100).unwrap();
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
    let hits = idx.search_fts("foo", None, 10).unwrap();
    assert!(!hits.is_empty());
    assert!(
        hits.iter().any(|h| h.content.contains("foo")),
        "got: {hits:?}",
        hits = hits.iter().map(|h| h.content.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn search_fts_filters_by_project() {
    let (_tmp, _store, idx) = build_fixture();
    let hits = idx.search_fts("alpha", Some("alpha"), 10).unwrap();
    assert!(hits.iter().all(|h| h.project_name.contains("alpha")));
}

#[test]
fn search_fts_respects_limit() {
    let (_tmp, _store, idx) = build_fixture();
    let hits = idx.search_fts("the", None, 1).unwrap();
    assert!(hits.len() <= 1);
}

#[test]
fn query_turn_stats_returns_percentiles() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_turn_stats(None, 100).unwrap();
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
    let rows = idx.query_pr_links(None, 100).unwrap();
    // Only alpha has a pr-link.
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pr_number, 7);
    assert_eq!(rows[0].pr_repository, "org/alpha");
}

#[test]
fn query_file_mods_returns_file_counts() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_file_mods(None, 100).unwrap();
    assert!(rows.iter().any(|r| r.file_path == "src/a.rs"));
}

#[test]
fn query_model_usage_groups_by_model_family() {
    let (_tmp, _store, idx) = build_fixture();
    let rows = idx.query_model_usage(None).unwrap();
    let models: Vec<_> = rows.iter().map(|r| r.model.as_str()).collect();
    assert!(models.iter().any(|m| m.contains("opus")));
    assert!(models.iter().any(|m| m.contains("sonnet")));
}

#[test]
fn query_summary_reports_totals() {
    let (_tmp, _store, idx) = build_fixture();
    let data = idx.query_summary().unwrap();
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
fn ensure_fresh_is_noop_within_staleness_window() {
    let (_tmp, store, mut idx) = build_fixture();
    // fixture already synced; ensure_fresh should return immediately without
    // changing anything.
    let before = idx.query_sessions(None, 100).unwrap().len();
    idx.ensure_fresh(&store).unwrap();
    let after = idx.query_sessions(None, 100).unwrap().len();
    assert_eq!(before, after);
}

#[test]
fn force_rebuild_wipes_and_reindexes() {
    let (_tmp, store, mut idx) = build_fixture();
    let before = idx.query_sessions(None, 100).unwrap().len();
    let indexed = idx.force_rebuild(&store).unwrap();
    let after = idx.query_sessions(None, 100).unwrap().len();
    assert_eq!(before, after);
    assert!(indexed >= before);
}

#[test]
fn sync_now_is_idempotent() {
    let (_tmp, store, mut idx) = build_fixture();
    let before = idx.query_sessions(None, 100).unwrap().len();
    idx.sync_now(&store).unwrap();
    idx.sync_now(&store).unwrap();
    let after = idx.query_sessions(None, 100).unwrap().len();
    assert_eq!(before, after);
}

#[test]
fn sync_picks_up_new_sessions() {
    let (tmp, store, mut idx) = build_fixture();
    let before = idx.query_sessions(None, 100).unwrap().len();

    // Add a fresh session to an existing project.
    write_session(
        &tmp.path().join("projects"),
        "-Users-test-Projects-alpha",
        "sess-a3",
        &[
            r#"{"type":"user","sessionId":"sess-a3","timestamp":"2026-04-14T00:00:00Z","message":{"content":"new thing"}}"#,
        ],
    );

    idx.sync_now(&store).unwrap();
    let after = idx.query_sessions(None, 100).unwrap().len();
    assert_eq!(after, before + 1);
}

//! End-to-end tests: invoke the compiled `claudex` binary as a subprocess
//! against a fixture `$HOME` so every `commands::*::run` path is exercised.
//!
//! `cargo llvm-cov` instruments subprocesses, so these contribute to coverage
//! on the command modules. Tests deliberately set `HOME`, `NO_COLOR`, and
//! `CLAUDEX_NO_INDEX_DEFAULT` to keep output stable and avoid drawing spinners.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_claudex");

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

/// Build a tempdir set up as a fake `$HOME` with `.claude/projects/...`
/// sessions ready to be indexed by the binary.
fn fixture_home() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join(".claude").join("projects");

    write_session(
        &projects,
        "-Users-test-Projects-alpha",
        "sess-a1",
        &[
            r#"{"type":"user","sessionId":"sess-a1","timestamp":"2026-04-10T10:00:00Z","message":{"content":"find the foo bug"}}"#,
            r#"{"type":"assistant","sessionId":"sess-a1","timestamp":"2026-04-10T10:01:00Z","message":{"model":"claude-opus-4-6","stop_reason":"end_turn","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":200,"cache_read_input_tokens":5000},"content":[{"type":"tool_use","name":"Bash","id":"t1","input":{}},{"type":"text","text":"fixed"}]}}"#,
            r#"{"type":"system","subtype":"turn_duration","durationMs":5000,"timestamp":"2026-04-10T10:01:30Z","sessionId":"sess-a1"}"#,
            r#"{"type":"file-history-snapshot","snapshot":{"messageId":"m1","trackedFileBackups":{"src/a.rs":{"backupFileName":"x","version":1}},"timestamp":"2026-04-10T10:01:00Z"}}"#,
            r#"{"type":"pr-link","prNumber":99,"prUrl":"https://github.com/org/alpha/pull/99","prRepository":"org/alpha","timestamp":"2026-04-10T10:02:00Z","sessionId":"sess-a1"}"#,
        ],
    );

    write_session(
        &projects,
        "-Users-test-Projects-beta",
        "sess-b1",
        &[
            r#"{"type":"user","sessionId":"sess-b1","timestamp":"2026-04-12T12:00:00Z","message":{"content":"refactor the thing"}}"#,
            r#"{"type":"assistant","sessionId":"sess-b1","timestamp":"2026-04-12T12:00:10Z","message":{"model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":{"input_tokens":200,"output_tokens":80,"cache_creation_input_tokens":0,"cache_read_input_tokens":100},"content":[{"type":"tool_use","name":"Edit","id":"t2","input":{}},{"type":"text","text":"done"}]}}"#,
        ],
    );

    tmp
}

fn run(home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn claudex")
}

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn json_of(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "expected JSON stdout, got error {e}\nstdout: {}\nstderr: {}",
            stdout_of(out),
            stderr_of(out),
        )
    })
}

// --- sessions ---

#[test]
fn sessions_json_returns_expected_fields() {
    let home = fixture_home();
    let out = run(home.path(), &["sessions", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2);
    for row in arr {
        assert!(row.get("project").is_some());
        assert!(row.get("session_id").is_some());
        assert!(row.get("message_count").is_some());
    }
}

#[test]
fn sessions_project_filter() {
    let home = fixture_home();
    let out = run(home.path(), &["sessions", "--json", "--project", "alpha"]);
    assert!(out.status.success());
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 1);
    assert!(
        arr[0]
            .get("project")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("alpha")
    );
}

#[test]
fn sessions_text_output_lists_projects() {
    let home = fixture_home();
    let out = run(home.path(), &["sessions", "--limit", "10"]);
    assert!(out.status.success());
    let s = stdout_of(&out);
    assert!(s.contains("alpha"), "got: {s}");
    assert!(s.contains("beta"), "got: {s}");
}

#[test]
fn sessions_no_index_fallback_matches_indexed_count() {
    let home = fixture_home();
    let indexed = run(home.path(), &["sessions", "--json"]);
    let scanned = run(home.path(), &["sessions", "--json", "--no-index"]);
    assert_eq!(
        json_of(&indexed).as_array().unwrap().len(),
        json_of(&scanned).as_array().unwrap().len()
    );
}

// --- cost ---

#[test]
fn cost_by_project_json() {
    let home = fixture_home();
    let out = run(home.path(), &["cost", "--json"]);
    assert!(out.status.success());
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 2);
    assert!(arr.iter().all(|r| r.get("cost_usd").is_some()));
}

#[test]
fn cost_per_session_json() {
    let home = fixture_home();
    let out = run(home.path(), &["cost", "--per-session", "--json"]);
    assert!(out.status.success());
    let arr = json_of(&out).as_array().unwrap().clone();
    assert!(!arr.is_empty());
    assert!(arr.iter().all(|r| r.get("session_id").is_some()));
}

#[test]
fn cost_text_output_has_total_row() {
    let home = fixture_home();
    let out = run(home.path(), &["cost"]);
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("TOTAL"));
}

#[test]
fn cost_no_index_matches_indexed() {
    let home = fixture_home();
    let indexed = run(home.path(), &["cost", "--json"]);
    let scanned = run(home.path(), &["cost", "--json", "--no-index"]);
    assert_eq!(
        json_of(&indexed).as_array().unwrap().len(),
        json_of(&scanned).as_array().unwrap().len()
    );
}

// --- tools ---

#[test]
fn tools_aggregate_json() {
    let home = fixture_home();
    let out = run(home.path(), &["tools", "--json"]);
    assert!(out.status.success());
    let arr = json_of(&out).as_array().unwrap().clone();
    assert!(
        arr.iter()
            .any(|r| r.get("tool").and_then(Value::as_str) == Some("Bash"))
    );
}

#[test]
fn tools_per_session_json() {
    let home = fixture_home();
    let out = run(home.path(), &["tools", "--per-session", "--json"]);
    assert!(out.status.success());
    assert!(!json_of(&out).as_array().unwrap().is_empty());
}

// --- search ---

#[test]
fn search_finds_matches() {
    let home = fixture_home();
    let out = run(home.path(), &["search", "foo"]);
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("foo"));
}

#[test]
fn search_no_matches_is_quiet() {
    let home = fixture_home();
    let out = run(home.path(), &["search", "this-string-does-not-exist-xyz"]);
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("No matches"));
}

#[test]
fn search_case_sensitive_falls_back_to_file_scan() {
    // FTS is case-insensitive; --case-sensitive should still work via the
    // file-scan path.
    let home = fixture_home();
    let out = run(home.path(), &["search", "--case-sensitive", "foo"]);
    assert!(out.status.success());
}

// --- summary ---

#[test]
fn summary_json_has_top_level_fields() {
    let home = fixture_home();
    let out = run(home.path(), &["summary", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    for field in [
        "total_sessions",
        "total_cost_usd",
        "top_projects",
        "top_tools",
    ] {
        assert!(v.get(field).is_some(), "missing {field}");
    }
}

#[test]
fn summary_text_has_sections() {
    let home = fixture_home();
    let out = run(home.path(), &["summary"]);
    assert!(out.status.success());
    let s = stdout_of(&out);
    assert!(s.contains("Sessions"));
    assert!(s.contains("Top Projects"));
}

// --- models / prs / files / turns ---

#[test]
fn models_json_lists_model_families() {
    let home = fixture_home();
    let out = run(home.path(), &["models", "--json"]);
    assert!(out.status.success());
    let arr = json_of(&out).as_array().unwrap().clone();
    let families: Vec<_> = arr
        .iter()
        .filter_map(|r| r.get("model_family").and_then(Value::as_str))
        .collect();
    assert!(families.contains(&"Opus"));
    assert!(families.contains(&"Sonnet"));
}

#[test]
fn prs_json_returns_linked_pr() {
    let home = fixture_home();
    let out = run(home.path(), &["prs", "--json"]);
    assert!(out.status.success());
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0].get("pr_number").unwrap().as_i64(), Some(99));
}

#[test]
fn files_json_lists_modified_files() {
    let home = fixture_home();
    let out = run(home.path(), &["files", "--json"]);
    assert!(out.status.success());
    let arr = json_of(&out).as_array().unwrap().clone();
    assert!(
        arr.iter()
            .any(|r| r.get("file_path").and_then(Value::as_str) == Some("src/a.rs"))
    );
}

#[test]
fn turns_json_returns_percentile_stats() {
    let home = fixture_home();
    let out = run(home.path(), &["turns", "--json"]);
    assert!(out.status.success());
    let arr = json_of(&out).as_array().unwrap().clone();
    assert!(!arr.is_empty());
    assert!(arr[0].get("p95_duration_ms").is_some());
}

// --- index command ---

#[test]
fn index_command_sync() {
    let home = fixture_home();
    let out = run(home.path(), &["index"]);
    assert!(out.status.success());
    // Success line goes to stdout, progress goes to stderr.
    assert!(stdout_of(&out).contains("Updated"));
}

#[test]
fn index_command_force_rebuild() {
    let home = fixture_home();
    let out = run(home.path(), &["index", "--force"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Indexed"));
}

// --- export ---

#[test]
fn export_markdown_by_project() {
    let home = fixture_home();
    let out = run(home.path(), &["export", "alpha"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let s = stdout_of(&out);
    // Markdown transcript should include the user message content.
    assert!(s.contains("foo bug"), "got: {s}");
}

#[test]
fn export_json_by_session_id() {
    let home = fixture_home();
    let out = run(home.path(), &["export", "sess-a1", "--format", "json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("expected JSON, got: {e}\nstdout: {}", stdout_of(&out)));
    // Export emits either an object or array depending on selector; both OK.
    assert!(v.is_object() || v.is_array());
}

#[test]
fn export_to_file() {
    let home = fixture_home();
    let out_path = home.path().join("out.md");
    let out = run(
        home.path(),
        &["export", "alpha", "--output", out_path.to_str().unwrap()],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let contents = fs::read_to_string(&out_path).expect("output file");
    assert!(contents.contains("foo bug"));
}

// --- color flag ---

#[test]
fn color_never_strips_ansi_even_on_tty_force() {
    let home = fixture_home();
    let out = Command::new(BIN)
        .env("HOME", home.path())
        .env_remove("NO_COLOR")
        .args(["--color", "never", "summary"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!stdout_of(&out).contains('\x1b'));
}

#[test]
fn color_always_emits_ansi_even_when_piped() {
    let home = fixture_home();
    let out = Command::new(BIN)
        .env("HOME", home.path())
        .env_remove("NO_COLOR")
        .args(["--color", "always", "summary"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(stdout_of(&out).contains('\x1b'));
}

// --- --no-index fallbacks (exercises the file-scan path in each command) ---

#[test]
fn tools_no_index_matches_indexed() {
    let home = fixture_home();
    let indexed = run(home.path(), &["tools", "--json"]);
    let scanned = run(home.path(), &["tools", "--json", "--no-index"]);
    assert_eq!(
        json_of(&indexed).as_array().unwrap().len(),
        json_of(&scanned).as_array().unwrap().len()
    );
}

#[test]
fn tools_per_session_no_index() {
    let home = fixture_home();
    let out = run(
        home.path(),
        &["tools", "--per-session", "--json", "--no-index"],
    );
    assert!(out.status.success());
    assert!(!json_of(&out).as_array().unwrap().is_empty());
}

#[test]
fn cost_per_session_no_index() {
    let home = fixture_home();
    let out = run(
        home.path(),
        &["cost", "--per-session", "--json", "--no-index"],
    );
    assert!(out.status.success());
    assert!(!json_of(&out).as_array().unwrap().is_empty());
}

#[test]
fn summary_no_index_matches_indexed() {
    let home = fixture_home();
    let indexed = run(home.path(), &["summary", "--json"]);
    let scanned = run(home.path(), &["summary", "--json", "--no-index"]);
    let a = json_of(&indexed);
    let b = json_of(&scanned);
    assert_eq!(a["total_sessions"], b["total_sessions"]);
}

#[test]
fn search_no_index_file_scan() {
    let home = fixture_home();
    let out = run(home.path(), &["search", "--no-index", "foo"]);
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("foo"));
}

#[test]
fn tools_text_output_has_table() {
    let home = fixture_home();
    let out = run(home.path(), &["tools"]);
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("Bash"));
}

#[test]
fn models_text_output_has_total() {
    let home = fixture_home();
    let out = run(home.path(), &["models"]);
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("TOTAL"));
}

// --- empty-index edge cases ---

#[test]
fn sessions_on_empty_home_returns_empty_array() {
    let empty = TempDir::new().unwrap();
    // No .claude/projects dir at all.
    let out = run(empty.path(), &["sessions", "--json"]);
    assert!(out.status.success());
    assert_eq!(json_of(&out).as_array().unwrap().len(), 0);
}

#[test]
fn prs_on_home_without_pr_links_is_empty() {
    let tmp = TempDir::new().unwrap();
    // Session with no pr-link.
    write_session(
        &tmp.path().join(".claude").join("projects"),
        "-p",
        "s",
        &[
            r#"{"type":"user","sessionId":"s","timestamp":"2026-04-10T10:00:00Z","message":{"content":"x"}}"#,
        ],
    );
    let out = run(tmp.path(), &["prs", "--json"]);
    assert!(out.status.success());
    assert_eq!(json_of(&out).as_array().unwrap().len(), 0);
}

// --- completions (hit every supported shell) ---

#[test]
fn completions_bash() {
    let out = Command::new(BIN)
        .args(["completions", "bash"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!out.stdout.is_empty());
}

#[test]
fn completions_fish() {
    let out = Command::new(BIN)
        .args(["completions", "fish"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!out.stdout.is_empty());
}

#[test]
fn claudex_dir_env_override_creates_index_under_custom_path() {
    // Confirms that `CLAUDEX_DIR=...` redirects index.db away from
    // `~/.claudex/`. Documented in guide/installation.md and reference/environment.md.
    let home = fixture_home();
    let state = tempfile::tempdir().expect("state tempdir");
    let out = Command::new(BIN)
        .env("HOME", home.path())
        .env("NO_COLOR", "1")
        .env("CLAUDEX_DIR", state.path())
        .args(["summary", "--json"])
        .output()
        .expect("spawn claudex");
    assert!(out.status.success(), "claudex failed: {}", stderr_of(&out));
    // Index lives under the overridden dir, not under $HOME/.claudex.
    assert!(
        state.path().join("index.db").is_file(),
        "index.db should exist under CLAUDEX_DIR; got: {:?}",
        std::fs::read_dir(state.path())
            .map(|it| it.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
            .unwrap_or_default()
    );
    assert!(
        !home.path().join(".claudex").exists(),
        "$HOME/.claudex should NOT be created when CLAUDEX_DIR is set"
    );
}

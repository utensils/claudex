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

use rusqlite::Connection;
use serde_json::Value;
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_claudex");

fn write_session(projects: &Path, encoded_project: &str, session: &str, lines: &[&str]) -> PathBuf {
    let path = projects
        .join(encoded_project)
        .join(format!("{session}.jsonl"));
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
            r#"{"type":"assistant","sessionId":"sess-a1","timestamp":"2026-04-10T10:01:15Z","message":{"model":"claude-sonnet-4-6","stop_reason":"tool_use","usage":{"input_tokens":300,"output_tokens":120,"cache_creation_input_tokens":0,"cache_read_input_tokens":200},"content":[{"type":"thinking","text":"checking follow-up"},{"type":"tool_use","name":"Edit","id":"t1b","input":{}},{"type":"text","text":"follow up on foo"}]}}"#,
            r#"{"type":"system","subtype":"turn_duration","durationMs":5000,"timestamp":"2026-04-10T10:01:30Z","sessionId":"sess-a1"}"#,
            r#"{"type":"file-history-snapshot","snapshot":{"messageId":"m1","trackedFileBackups":{"src/a.rs":{"backupFileName":"x","version":1}},"timestamp":"2026-04-10T10:01:00Z"}}"#,
            r#"{"type":"pr-link","prNumber":99,"prUrl":"https://github.com/org/alpha/pull/99","prRepository":"org/alpha","timestamp":"2026-04-10T10:02:00Z","sessionId":"sess-a1"}"#,
            r#"{"type":"attachment","filename":"bug.png","mimeType":"image/png","timestamp":"2026-04-10T10:02:10Z","sessionId":"sess-a1"}"#,
            r#"{"type":"permission-mode","mode":"bypassPermissions","timestamp":"2026-04-10T10:02:20Z","sessionId":"sess-a1"}"#,
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

fn fixture_home_with_claude_local_models() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join(".claude").join("projects");

    write_session(
        &projects,
        "-Users-test-Projects-local",
        "sess-local-qwen",
        &[
            r#"{"type":"user","sessionId":"sess-local-qwen","timestamp":"2026-04-10T10:00:00Z","message":{"content":"use local qwen"}}"#,
            r#"{"type":"assistant","sessionId":"sess-local-qwen","timestamp":"2026-04-10T10:01:00Z","message":{"model":"qwen3.6-35b-a3b-ud-mlx","usage":{"input_tokens":1000000,"output_tokens":1000000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"local qwen"}]}}"#,
        ],
    );

    write_session(
        &projects,
        "-Users-test-Projects-local",
        "sess-local-ollama",
        &[
            r#"{"type":"user","sessionId":"sess-local-ollama","timestamp":"2026-04-10T11:00:00Z","message":{"content":"use local gemma"}}"#,
            r#"{"type":"assistant","sessionId":"sess-local-ollama","timestamp":"2026-04-10T11:01:00Z","message":{"model":"ollama/gemma4:31b","usage":{"input_tokens":1000000,"output_tokens":1000000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"local gemma"}]}}"#,
        ],
    );

    tmp
}

fn run(home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .env_remove("OPENCLAW_STATE_DIR")
        .env_remove("OPENCLAW_TRAJECTORY_DIR")
        .env_remove("CLAUDEX_COPILOT_DIR")
        .env_remove("CLAUDEX_VSCODE_USER_DIR")
        .env_remove("XDG_CONFIG_HOME")
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

// --- codex provider (unified index) ---

fn fixture_home_with_codex() -> TempDir {
    let tmp = fixture_home();
    let codex = tmp.path().join(".codex");
    let active = codex.join("sessions").join("2026").join("05").join("05");
    fs::create_dir_all(&active).unwrap();
    let mut f = fs::File::create(active.join("rollout-2026-05-05T00-00-00-codex-a.jsonl")).unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-05-05T00:00:00Z","type":"session_meta","payload":{{"id":"codex-a","cwd":"/Users/test/codexproj","originator":"codex_cli_rs","cli_version":"0.99.0","source":"cli"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-05-05T00:00:30Z","type":"turn_context","payload":{{"cwd":"/Users/test/codexproj","model":"gpt-5-codex"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-05-05T00:01:00Z","type":"response_item","payload":{{"type":"user_message","message":"hello from codex"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-05-05T00:01:30Z","type":"response_item","payload":{{"type":"agent_message","message":"codex says hello back"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-05-05T00:03:00Z","type":"response_item","payload":{{"type":"function_call","name":"shell","arguments":"{{}}","call_id":"c"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-05-05T00:03:30Z","type":"event_msg","payload":{{"type":"exec_command_end","command":"gh pr create --fill","stdout":"https://github.com/utensils/claudex/pull/38\n"}}}}"#
    )
    .unwrap();
    // Cumulative token counts: only the LAST must be used (1,000,000 input of
    // which 200,000 cached, 500,000 output) → gpt-5 pricing.
    writeln!(
        f,
        r#"{{"timestamp":"2026-05-05T00:02:00Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5,"total_tokens":15}}}}}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-05-05T00:04:00Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":1000000,"cached_input_tokens":200000,"output_tokens":500000,"total_tokens":1500000}}}}}}}}"#
    )
    .unwrap();
    f.flush().unwrap();

    let archived = codex.join("archived_sessions");
    fs::create_dir_all(&archived).unwrap();
    let mut f =
        fs::File::create(archived.join("rollout-2026-01-01T00-00-00-codex-b.jsonl")).unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{{"id":"codex-b","cwd":"/Users/test/archive","cli_version":"0.98.0"}}}}"#
    )
    .unwrap();
    f.flush().unwrap();

    tmp
}

#[test]
fn codex_sessions_appear_in_unified_index() {
    let home = fixture_home_with_codex();
    let out = run(home.path(), &["sessions", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let projects: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["project"].as_str())
        .collect();
    assert!(
        projects.iter().any(|p| p.contains("codexproj")),
        "codex session must surface in unified sessions, got: {projects:?}"
    );
    // Claude fixture sessions remain present too — the index spans providers.
    assert!(
        projects
            .iter()
            .any(|p| p.contains("alpha") || p.contains("beta")),
        "claude sessions must still be present, got: {projects:?}"
    );
}

#[test]
fn codex_cost_uses_last_cumulative_tokens_and_gpt_pricing() {
    let home = fixture_home_with_codex();
    let out = run(home.path(), &["cost", "--per-session", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let codex = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["project"]
                .as_str()
                .is_some_and(|p| p.contains("codexproj"))
        })
        .expect("codex session present in cost");
    // Last cumulative total: input 1,000,000 (200,000 cached → cache_read),
    // billed input = 800,000; output 500,000.
    assert_eq!(codex["input_tokens"].as_i64(), Some(800_000));
    assert_eq!(codex["cache_read_tokens"].as_i64(), Some(200_000));
    assert_eq!(codex["output_tokens"].as_i64(), Some(500_000));
    // gpt-5: 0.8*1.25 + 0.2*0.125 + 0.5*10.0 = 1.0 + 0.025 + 5.0 = $6.025
    let cost = codex["cost_usd"].as_f64().unwrap();
    assert!((cost - 6.025).abs() < 0.001, "expected ~$6.025, got {cost}");
}

#[test]
fn codex_session_drilldown_resolves_indexed_id() {
    let home = fixture_home_with_codex();
    let out = run(home.path(), &["session", "codex-a", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    assert_eq!(v["session_id"].as_str(), Some("codex-a"));
    assert!(
        v["project"]
            .as_str()
            .is_some_and(|p| p.contains("codexproj")),
        "expected codex project, got {v}"
    );
}

#[test]
fn indexed_session_drilldown_renders_text_metadata() {
    let home = fixture_home_with_codex();
    let out = run(home.path(), &["session", "codex-a"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let s = stdout_of(&out);
    assert!(s.contains("Overview"), "overview section missing: {s}");
    assert!(s.contains("Source:"), "source state missing: {s}");
    assert!(s.contains("live"), "live source missing: {s}");
    assert!(s.contains("Metadata:"), "metadata line missing: {s}");
    assert!(s.contains("0.99.0"), "extras metadata missing: {s}");
}

// --- openclaw provider (unified index) ---

fn fixture_home_with_openclaw() -> TempDir {
    let tmp = fixture_home();
    let sessions = tmp
        .path()
        .join(".openclaw")
        .join("agents")
        .join("main")
        .join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("sessions.json"),
        r#"{"agent:main:dm":{"sessionId":"sess-open","status":"running","workspaceDir":"/Users/test/openapp","modelProvider":"openai"}}"#,
    )
    .unwrap();
    let mut f = fs::File::create(sessions.join("sess-open.jsonl")).unwrap();
    writeln!(
        f,
        r#"{{"type":"session","version":3,"id":"sess-open","timestamp":"2026-05-30T00:00:00Z","cwd":"/Users/test/openapp"}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"message","id":"u1","timestamp":"2026-05-30T00:01:00Z","message":{{"role":"user","content":[{{"type":"text","text":"openclaw indexing"}}]}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"message","id":"a1","timestamp":"2026-05-30T00:02:00Z","message":{{"role":"assistant","content":[{{"type":"toolCall","id":"c1","name":"bash","arguments":{{"command":"gh pr create --fill"}}}},{{"type":"text","text":"opened"}}],"provider":"openai","model":"gpt-5.2","usage":{{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"cost":{{"total":0.33}}}},"stopReason":"toolUse"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"message","id":"t1","timestamp":"2026-05-30T00:03:00Z","message":{{"role":"toolResult","toolCallId":"c1","toolName":"bash","content":[{{"type":"text","text":"https://github.com/utensils/claudex/pull/41"}}]}}}}"#
    )
    .unwrap();
    f.flush().unwrap();
    write_jsonl(
        &sessions.join("sess-traj.trajectory.jsonl"),
        &[
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"sess-traj","source":"runtime","type":"session.started","ts":"2026-05-30T01:00:00Z","seq":1,"sessionId":"sess-traj","workspaceDir":"/Users/test/trajapp","provider":"openai","modelId":"gpt-5.2"}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"sess-traj","source":"runtime","type":"prompt.submitted","ts":"2026-05-30T01:01:00Z","seq":2,"sessionId":"sess-traj","workspaceDir":"/Users/test/trajapp","provider":"openai","modelId":"gpt-5.2","data":{"prompt":"find trajectory"}}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"sess-traj","source":"runtime","type":"model.completed","ts":"2026-05-30T01:02:00Z","seq":3,"sessionId":"sess-traj","workspaceDir":"/Users/test/trajapp","provider":"openai","modelId":"gpt-5.2","data":{"assistantText":"trajectory done","usage":{"input":5,"output":4,"cacheRead":1,"cacheWrite":0,"cost":{"total":0.07}}}}"#,
        ],
    );
    tmp
}

#[test]
fn openclaw_sessions_appear_in_unified_index_with_embedded_cost() {
    let home = fixture_home_with_openclaw();
    let out = run(
        home.path(),
        &["cost", "--per-session", "--provider", "openclaw", "--json"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let arr = rows.as_array().unwrap();
    assert_eq!(
        arr.len(),
        2,
        "classic and trajectory sessions indexed: {rows}"
    );
    assert!(arr.iter().all(|r| r["provider"] == "openclaw"));
    let classic = arr
        .iter()
        .find(|r| r["project"].as_str().is_some_and(|p| p.contains("openapp")))
        .expect("classic openclaw session");
    assert_eq!(classic["input_tokens"].as_i64(), Some(100));
    assert_eq!(classic["cache_read_tokens"].as_i64(), Some(10));
    assert_eq!(classic["cost_usd"].as_f64(), Some(0.33));
}

#[test]
fn openclaw_search_session_tools_and_prs_work() {
    let home = fixture_home_with_openclaw();
    let out = run(
        home.path(),
        &["search", "trajectory", "--provider", "openclaw", "--json"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(!json_of(&out).as_array().unwrap().is_empty());

    let out = run(home.path(), &["session", "sess-open", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let session = json_of(&out);
    assert_eq!(session["provider"].as_str(), Some("openclaw"));
    assert_eq!(session["cost_usd"].as_f64(), Some(0.33));

    let out = run(home.path(), &["tools", "--provider", "openclaw", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(
        json_of(&out)
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["tool"] == "bash")
    );

    let out = run(home.path(), &["prs", "--provider", "openclaw", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(
        json_of(&out)
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["pr_url"] == "https://github.com/utensils/claudex/pull/41")
    );
}

#[test]
fn summary_accepts_openclaw_provider_filter() {
    let home = fixture_home_with_openclaw();
    let out = run(
        home.path(),
        &["summary", "--provider", "openclaw", "--json"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let summary = json_of(&out);
    assert_eq!(summary["total_sessions"].as_i64(), Some(2));
    assert_eq!(summary["total_input_tokens"].as_i64(), Some(105));
    assert_eq!(summary["total_output_tokens"].as_i64(), Some(54));
    assert_eq!(summary["total_cache_read_tokens"].as_i64(), Some(11));

    let projects: Vec<&str> = summary["top_projects"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["project"].as_str())
        .collect();
    assert!(
        projects.iter().any(|p| p.contains("openapp")),
        "OpenClaw classic project should be present: {summary}"
    );
    assert!(
        projects
            .iter()
            .all(|p| !p.contains("alpha") && !p.contains("beta")),
        "Claude projects should be filtered out: {summary}"
    );
}

#[test]
fn no_index_rejects_non_claude_provider_filter() {
    let home = fixture_home_with_openclaw();
    let out = run(
        home.path(),
        &["sessions", "--provider", "openclaw", "--no-index", "--json"],
    );
    assert!(!out.status.success(), "stdout: {}", stdout_of(&out));
    let stderr = stderr_of(&out);
    assert!(stderr.contains("--no-index only scans Claude transcripts"));
    assert!(stderr.contains("remove --no-index"));
}

// --- skills subcommand ---

#[test]
fn skills_generate_writes_all_targets() {
    let home = fixture_home();
    let out_dir = TempDir::new().unwrap();
    let out = run(
        home.path(),
        &[
            "skills",
            "generate",
            "--dir",
            out_dir.path().to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    for rel in [
        ".claude/skills/claudex/SKILL.md",
        ".agents/skills/claudex/SKILL.md",
        ".pi/skills/claudex/SKILL.md",
        "skills/claudex/SKILL.md",
        "AGENTS.md",
    ] {
        assert!(
            out_dir.path().join(rel).exists(),
            "{rel} should have been written"
        );
    }
}

#[test]
fn skills_install_global_openclaw_uses_state_dir() {
    let home = fixture_home();
    let out = Command::new(BIN)
        .env("HOME", home.path())
        .env("NO_COLOR", "1")
        .env("OPENCLAW_STATE_DIR", "~/openclaw-state")
        .args(["skills", "install", "--global", "--target", "openclaw"])
        .output()
        .expect("spawn claudex");
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let skill = home.path().join("openclaw-state/skills/claudex/SKILL.md");
    assert!(skill.exists());
    let md = fs::read_to_string(skill).unwrap();
    assert!(md.contains("OpenClaw"));
    assert!(!md.contains("allowed-tools"));
    assert!(!Path::new("~/openclaw-state/skills/claudex/SKILL.md").exists());
}

#[test]
fn skills_generate_refuses_overwrite_without_force() {
    let home = fixture_home();
    let out_dir = TempDir::new().unwrap();
    let args = [
        "skills",
        "generate",
        "--target",
        "claude-code",
        "--dir",
        out_dir.path().to_str().unwrap(),
    ];
    assert!(run(home.path(), &args).status.success());
    // Second run without --force must fail.
    let second = run(home.path(), &args);
    assert!(!second.status.success(), "should refuse to clobber");
    assert!(stderr_of(&second).contains("already exists"));
    // With --force it succeeds.
    let mut forced = args.to_vec();
    forced.push("--force");
    assert!(run(home.path(), &forced).status.success());
}

#[test]
fn skills_agents_md_splice_is_idempotent() {
    let home = fixture_home();
    let out_dir = TempDir::new().unwrap();
    let args = [
        "skills",
        "generate",
        "--target",
        "agents-md",
        "--dir",
        out_dir.path().to_str().unwrap(),
    ];
    run(home.path(), &args);
    run(home.path(), &args);
    let agents = fs::read_to_string(out_dir.path().join("AGENTS.md")).unwrap();
    assert_eq!(
        agents.matches("<!-- claudex:start -->").count(),
        1,
        "splice must not duplicate the block on re-run"
    );
}

#[test]
fn skills_generate_json_summary_shape() {
    let home = fixture_home();
    let out_dir = TempDir::new().unwrap();
    let out = run(
        home.path(),
        &[
            "skills",
            "generate",
            "--target",
            "claude-code",
            "--dir",
            out_dir.path().to_str().unwrap(),
            "--json",
        ],
    );
    let v = json_of(&out);
    assert_eq!(v["mode"], "generate");
    assert_eq!(v["written"].as_array().unwrap().len(), 1);
    assert!(v["hint"].is_string(), "generate nudges toward install");
}

#[test]
fn skills_command_list_includes_skills_itself() {
    let home = fixture_home();
    let out_dir = TempDir::new().unwrap();
    run(
        home.path(),
        &[
            "skills",
            "generate",
            "--target",
            "claude-code",
            "--dir",
            out_dir.path().to_str().unwrap(),
        ],
    );
    let md = fs::read_to_string(out_dir.path().join(".claude/skills/claudex/SKILL.md")).unwrap();
    // The command list is clap-derived, so every subcommand appears.
    assert!(md.contains("`claudex sessions`"));
    assert!(md.contains("`claudex skills`"));
    assert!(md.contains("Claude Code, Codex, Pi, or OpenClaw"));
    assert!(md.contains("`claudex cost`"));
}

#[test]
fn committed_skill_matches_generator_output() {
    // Drift guard: the checked-in .claude/skills/claudex/SKILL.md must be exactly
    // what `claudex skills generate --target claude-code` produces. Regenerate it
    // with `claudex skills generate --target claude-code --dir . --force`.
    let home = fixture_home();
    let out_dir = TempDir::new().unwrap();
    run(
        home.path(),
        &[
            "skills",
            "generate",
            "--target",
            "claude-code",
            "--dir",
            out_dir.path().to_str().unwrap(),
        ],
    );
    let generated =
        fs::read_to_string(out_dir.path().join(".claude/skills/claudex/SKILL.md")).unwrap();
    let committed_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".claude/skills/claudex/SKILL.md");
    let committed = fs::read_to_string(&committed_path).unwrap();
    assert_eq!(
        generated, committed,
        "committed SKILL.md is stale — regenerate with \
         `claudex skills generate --target claude-code --dir . --force`"
    );
}

// --- shared filtering args ---

#[test]
fn provider_filter_scopes_results() {
    let home = fixture_home_with_codex();

    // --provider codex returns only codex sessions.
    let out = run(home.path(), &["sessions", "--provider", "codex", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let arr = rows.as_array().unwrap();
    assert!(!arr.is_empty(), "expected codex rows");
    assert!(
        arr.iter().all(|r| r["provider"] == "codex"),
        "every row must be codex, got: {rows}"
    );

    // --provider claude excludes codex.
    let out = run(home.path(), &["sessions", "--provider", "claude", "--json"]);
    let rows = json_of(&out);
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .all(|r| r["provider"] == "claude"),
        "every row must be claude, got: {rows}"
    );
    assert!(
        !rows.as_array().unwrap().iter().any(|r| r["project"]
            .as_str()
            .is_some_and(|p| p.contains("codexproj"))),
        "claude scope must not include codex projects"
    );
}

#[test]
fn since_until_filter_by_date() {
    let home = fixture_home();
    // Everything in the fixture is dated 2026-04; a 2027 floor yields nothing.
    let out = run(
        home.path(),
        &["sessions", "--since", "2027-01-01", "--json"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(json_of(&out).as_array().unwrap().is_empty());

    // A 2026 floor keeps them.
    let out = run(
        home.path(),
        &["sessions", "--since", "2026-01-01", "--json"],
    );
    assert!(!json_of(&out).as_array().unwrap().is_empty());

    // An --until before the data excludes everything.
    let out = run(
        home.path(),
        &["sessions", "--until", "2026-01-01", "--json"],
    );
    assert!(json_of(&out).as_array().unwrap().is_empty());
}

#[test]
fn model_filter_matches_mixed_model_sessions() {
    // sess-a1 uses claude-opus-4-6 AND claude-sonnet-4-6, so its sessions.model
    // label is "mixed". A --model filter must still match it via its per-model
    // token_usage rows — not just the label.
    let home = fixture_home();
    for needle in ["opus", "sonnet"] {
        let out = run(
            home.path(),
            &["cost", "--per-session", "--model", needle, "--json"],
        );
        assert!(out.status.success(), "stderr: {}", stderr_of(&out));
        let rows = json_of(&out);
        assert!(
            rows.as_array()
                .unwrap()
                .iter()
                .any(|r| r["session_id"].as_str() == Some("sess-a1")),
            "--model {needle} must match the mixed-model session, got: {rows}"
        );
    }
    // A model nobody used matches nothing.
    let out = run(
        home.path(),
        &["cost", "--per-session", "--model", "gpt-5", "--json"],
    );
    assert!(json_of(&out).as_array().unwrap().is_empty());
}

#[test]
fn on_disk_only_excludes_archived_sessions() {
    // The Codex fixture's codex-b lives under archived_sessions/ — on disk but
    // archived. --on-disk-only must drop it.
    let home = fixture_home_with_codex();
    let projects: Vec<String> = json_of(&run(home.path(), &["sessions", "--json"]))
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["project"].as_str().map(str::to_string))
        .collect();
    assert!(
        projects.iter().any(|p| p.contains("archive")),
        "archived codex session is present by default"
    );

    let on_disk: Vec<String> =
        json_of(&run(home.path(), &["sessions", "--on-disk-only", "--json"]))
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["project"].as_str().map(str::to_string))
            .collect();
    assert!(
        !on_disk.iter().any(|p| p.contains("archive")),
        "--on-disk-only must exclude the archived session, got: {on_disk:?}"
    );
    // The live codex session is still there.
    assert!(on_disk.iter().any(|p| p.contains("codexproj")));
}

#[test]
fn no_index_search_applies_date_filter() {
    // "foo" appears in sess-a1 (dated 2026-04). The file-scan search path must
    // honor --since, not just emit every textual match.
    let home = fixture_home();
    let hit = run(home.path(), &["search", "foo", "--no-index", "--json"]);
    assert!(
        !json_of(&hit).as_array().unwrap().is_empty(),
        "baseline: foo is found without a date filter"
    );

    let filtered = run(
        home.path(),
        &[
            "search",
            "foo",
            "--no-index",
            "--since",
            "2027-01-01",
            "--json",
        ],
    );
    assert!(
        json_of(&filtered).as_array().unwrap().is_empty(),
        "a future --since must exclude the 2026 match in the file-scan path"
    );
}

#[test]
fn provider_column_shows_only_when_results_span_providers() {
    // Mixed providers → Provider column present.
    let home = fixture_home_with_codex();
    let out = run(home.path(), &["sessions"]);
    let s = stdout_of(&out);
    assert!(s.contains("Provider"), "mixed providers show a column: {s}");
    assert!(s.contains("codex"), "codex rows labeled");

    // Single provider (claude only) → no Provider column noise.
    let home = fixture_home();
    let out = run(home.path(), &["sessions"]);
    let s = stdout_of(&out);
    assert!(
        !s.contains("Provider"),
        "claude-only output omits the provider column: {s}"
    );
}

#[test]
fn json_always_carries_provider_key() {
    let home = fixture_home();
    for args in [
        vec!["sessions", "--json"],
        vec!["cost", "--per-session", "--json"],
    ] {
        let out = run(home.path(), &args);
        let rows = json_of(&out);
        assert!(
            rows.as_array()
                .unwrap()
                .iter()
                .all(|r| r.get("provider").is_some()),
            "{args:?} rows must carry a provider key, got: {rows}"
        );
    }
}

// --- pi provider (unified index) ---

fn fixture_home_with_pi() -> TempDir {
    let tmp = fixture_home();
    let dir = tmp
        .path()
        .join(".pi")
        .join("agent")
        .join("sessions")
        .join("--Users-test-Projects-piapp--");
    fs::create_dir_all(&dir).unwrap();
    let mut f = fs::File::create(dir.join("2026-05-13T22-05-15-161Z_sess-pi.jsonl")).unwrap();
    writeln!(
        f,
        r#"{{"type":"session","version":3,"id":"sess-pi","timestamp":"2026-05-13T22:05:15Z","cwd":"/Users/test/Projects/piapp"}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"message","id":"u1","timestamp":"2026-05-13T22:05:35Z","message":{{"role":"user","content":[{{"type":"text","text":"do the pithing"}}]}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"message","id":"a1","timestamp":"2026-05-13T22:05:52Z","message":{{"role":"assistant","content":[{{"type":"toolCall","id":"c1","name":"bash","arguments":{{"command":"gh pr create --fill"}}}},{{"type":"text","text":"on it"}}],"provider":"anthropic","model":"claude-3-opus","usage":{{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"cost":{{"total":0.75}}}},"stopReason":"toolUse"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"message","id":"t1","timestamp":"2026-05-13T22:06:00Z","message":{{"role":"toolResult","toolCallId":"c1","toolName":"bash","content":[{{"type":"text","text":"https://github.com/utensils/claudex/pull/40"}}],"isError":false}}}}"#
    )
    .unwrap();
    f.flush().unwrap();
    tmp
}

#[test]
fn pi_sessions_appear_in_unified_index_with_embedded_cost() {
    let home = fixture_home_with_pi();
    let out = run(home.path(), &["cost", "--per-session", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let pi = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["project"].as_str().is_some_and(|p| p.contains("piapp")))
        .expect("pi session present in cost");
    assert_eq!(pi["input_tokens"].as_i64(), Some(100));
    assert_eq!(pi["cache_read_tokens"].as_i64(), Some(10));
    // Pi's own per-message cost is trusted verbatim.
    let cost = pi["cost_usd"].as_f64().unwrap();
    assert!((cost - 0.75).abs() < 0.0001, "expected $0.75, got {cost}");
}

#[test]
fn pi_session_drilldown_resolves_indexed_id() {
    let home = fixture_home_with_pi();
    let out = run(home.path(), &["session", "sess-pi", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    assert_eq!(v["session_id"].as_str(), Some("sess-pi"));
    assert_eq!(v["cost_usd"].as_f64(), Some(0.75));
}

#[test]
fn pi_search_finds_indexed_content() {
    let home = fixture_home_with_pi();
    let out = run(home.path(), &["search", "pithing", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    assert!(
        !v.as_array().unwrap().is_empty(),
        "pi transcript content should be full-text searchable, got: {v}"
    );
}

#[test]
fn multi_provider_export_codex_json_includes_records_and_metadata() {
    let home = fixture_home_with_codex();
    let out = run(home.path(), &["export", "codex-a", "--format", "json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    assert_eq!(v["provider"], "codex");
    assert_eq!(v["session_id"], "codex-a");
    assert!(v["records"].as_array().is_some_and(|rows| rows.len() >= 4));
    assert_eq!(v["extras"]["cli_version"], "0.99.0");
}

#[test]
fn multi_provider_export_codex_markdown_includes_normalized_messages() {
    let home = fixture_home_with_codex();
    let out = run(home.path(), &["export", "codex-a"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let s = stdout_of(&out);
    assert!(
        s.contains("# Session: codex-a"),
        "session heading missing: {s}"
    );
    assert!(
        s.contains("**Provider:** codex"),
        "provider metadata missing: {s}"
    );
    assert!(
        s.contains("**Model:** gpt-5-codex"),
        "model metadata missing: {s}"
    );
    assert!(s.contains("**Metadata:**"), "extras metadata missing: {s}");
    assert!(
        s.contains("## User"),
        "normalized user heading missing: {s}"
    );
    assert!(s.contains("hello from codex"), "message text missing: {s}");
}

#[test]
fn search_facets_and_context_are_available_in_json() {
    let home = fixture_home_with_codex();
    let out = run(
        home.path(),
        &[
            "search",
            "hello",
            "--provider",
            "codex",
            "--role",
            "user",
            "--tool",
            "shell",
            "--context",
            "1",
            "--json",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 1, "expected one faceted hit: {arr:?}");
    assert_eq!(arr[0]["provider"], "codex");
    assert_eq!(arr[0]["message_type"], "user");
    assert!(arr[0].get("context_before").is_some());
    assert!(arr[0].get("context_after").is_some());
}

#[test]
fn indexed_search_facets_and_context_render_text() {
    let home = fixture_home_with_codex();
    let out = run(
        home.path(),
        &[
            "search",
            "hello",
            "--provider",
            "codex",
            "--role",
            "user",
            "--tool",
            "shell",
            "--context",
            "1",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let s = stdout_of(&out);
    assert!(s.contains("codexproj"), "project headline missing: {s}");
    assert!(s.contains("user"), "role missing: {s}");
    assert!(s.contains("hello from codex"), "hit snippet missing: {s}");
    assert!(
        s.contains("assistant") || s.contains("shell"),
        "context output missing: {s}"
    );
}

#[test]
fn no_index_search_facets_filter_file_scan() {
    let home = fixture_home();
    let matched = run(
        home.path(),
        &[
            "search",
            "foo",
            "--no-index",
            "--role",
            "assistant",
            "--tool",
            "Edit",
            "--file",
            "src/a.rs",
            "--pr",
            "99",
            "--limit",
            "1",
        ],
    );
    assert!(matched.status.success(), "stderr: {}", stderr_of(&matched));
    let s = stdout_of(&matched);
    assert!(s.contains("alpha"), "expected filtered hit: {s}");
    assert!(
        s.contains("follow up on foo"),
        "expected assistant text: {s}"
    );

    let missed = run(
        home.path(),
        &["search", "foo", "--no-index", "--tool", "MissingTool"],
    );
    assert!(missed.status.success(), "stderr: {}", stderr_of(&missed));
    assert!(stdout_of(&missed).contains("No matches"));
}

#[test]
fn provider_timeline_budget_and_activity_reports_emit_json() {
    let home = fixture_home_with_codex();

    let providers = run(home.path(), &["providers", "--json"]);
    assert!(
        providers.status.success(),
        "stderr: {}",
        stderr_of(&providers)
    );
    let providers_json = json_of(&providers).as_array().unwrap().clone();
    assert!(providers_json.iter().any(|r| r["provider"] == "codex"));
    assert!(providers_json.iter().all(|r| r["parse_failures"].is_null()));

    let providers_since = run(
        home.path(),
        &[
            "providers",
            "--provider",
            "codex",
            "--since",
            "2026-05-01",
            "--json",
        ],
    );
    assert!(
        providers_since.status.success(),
        "stderr: {}",
        stderr_of(&providers_since)
    );
    let providers_since_json = json_of(&providers_since).as_array().unwrap().clone();
    assert_eq!(providers_since_json.len(), 1);
    assert_eq!(providers_since_json[0]["provider"], "codex");
    assert_eq!(providers_since_json[0]["indexed_sessions"], 1);

    let timeline = run(home.path(), &["timeline", "--json", "--limit", "2"]);
    assert!(
        timeline.status.success(),
        "stderr: {}",
        stderr_of(&timeline)
    );
    let timeline_json = json_of(&timeline).as_array().unwrap().clone();
    assert!(timeline_json.iter().any(|r| r.get("bucket").is_some()));

    let budget = run(home.path(), &["budget", "--monthly", "250", "--json"]);
    assert!(budget.status.success(), "stderr: {}", stderr_of(&budget));
    let budget_json = json_of(&budget);
    assert_eq!(budget_json["monthly_budget_usd"], 250.0);
    assert!(budget_json.get("projected_month_end_usd").is_some());

    let scoped_budget = run(
        home.path(),
        &[
            "budget",
            "--monthly",
            "250",
            "--since",
            "2026-05-01",
            "--until",
            "2026-05-31",
            "--json",
        ],
    );
    assert!(
        scoped_budget.status.success(),
        "stderr: {}",
        stderr_of(&scoped_budget)
    );
    let scoped_budget_json = json_of(&scoped_budget);
    assert_eq!(scoped_budget_json["period_start"], "2026-05-01");
    assert_eq!(scoped_budget_json["period_end"], "2026-05-31");
    assert_eq!(scoped_budget_json["days_elapsed"], 31);
    assert_eq!(scoped_budget_json["days_in_month"], 31);
    assert_eq!(
        scoped_budget_json["spent_usd"],
        scoped_budget_json["projected_month_end_usd"]
    );

    let activity = run(home.path(), &["activity", "--json", "--limit", "2"]);
    assert!(
        activity.status.success(),
        "stderr: {}",
        stderr_of(&activity)
    );
    let activity_json = json_of(&activity);
    assert!(activity_json.get("summary").is_some());
    assert!(activity_json.get("recent_sessions").is_some());
}

#[test]
fn provider_timeline_budget_and_activity_reports_render_text() {
    let home = fixture_home_with_codex();

    let providers = run(home.path(), &["providers", "--deep"]);
    assert!(
        providers.status.success(),
        "stderr: {}",
        stderr_of(&providers)
    );
    let provider_text = stdout_of(&providers);
    assert!(
        provider_text.contains("Provider"),
        "missing header: {provider_text}"
    );
    assert!(
        provider_text.contains("Parse Failures"),
        "missing deep column: {provider_text}"
    );
    assert!(
        provider_text.contains("codex"),
        "missing codex row: {provider_text}"
    );

    let timeline = run(home.path(), &["timeline", "--weekly", "--limit", "4"]);
    assert!(
        timeline.status.success(),
        "stderr: {}",
        stderr_of(&timeline)
    );
    let timeline_text = stdout_of(&timeline);
    assert!(
        timeline_text.contains("Week"),
        "missing weekly column: {timeline_text}"
    );
    assert!(
        timeline_text.contains("Sessions"),
        "missing sessions column: {timeline_text}"
    );
    assert!(
        timeline_text.contains("Cost"),
        "missing cost column: {timeline_text}"
    );

    let budget = run(
        home.path(),
        &[
            "budget",
            "--monthly",
            "250",
            "--since",
            "2026-05-01",
            "--until",
            "2026-05-31",
        ],
    );
    assert!(budget.status.success(), "stderr: {}", stderr_of(&budget));
    let budget_text = stdout_of(&budget);
    assert!(
        budget_text.contains("Budget"),
        "missing budget table: {budget_text}"
    );
    assert!(
        budget_text.contains("Projected"),
        "missing projected column: {budget_text}"
    );

    let invalid_budget = run(home.path(), &["budget", "--monthly", "0"]);
    assert!(
        !invalid_budget.status.success(),
        "zero monthly budget should fail"
    );
    assert!(
        stderr_of(&invalid_budget).contains("--monthly must be greater than 0"),
        "missing validation message: {}",
        stderr_of(&invalid_budget)
    );

    let activity = run(home.path(), &["activity", "--limit", "3"]);
    assert!(
        activity.status.success(),
        "stderr: {}",
        stderr_of(&activity)
    );
    let activity_text = stdout_of(&activity);
    for label in [
        "Sessions",
        "Cost",
        "Tokens",
        "Recent sessions",
        "Recent PRs",
        "Hot files",
        "Slow projects",
    ] {
        assert!(
            activity_text.contains(label),
            "missing {label}: {activity_text}"
        );
    }
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
        assert!(row.get("file_path").is_some());
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

#[test]
fn sessions_file_filter_returns_matching_session() {
    let home = fixture_home();
    let out = run(home.path(), &["sessions", "--json", "--file", "src/a.rs"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["session_id"].as_str(), Some("sess-a1"));
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
    assert!(arr.iter().all(|r| r.get("models").is_some()));
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

/// The `$X,XXX.XX` figure from a table's `TOTAL` line (last `$`-prefixed token).
fn total_cost_figure(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| l.contains("TOTAL"))
        .expect("a TOTAL line");
    let dollar = line.rfind('$').expect("a $ figure in the TOTAL line");
    line[dollar..].trim().to_string()
}

#[test]
fn cost_total_matches_models_and_is_limit_invariant() {
    let home = fixture_home();

    let models = run(home.path(), &["models"]);
    let cost_full = run(home.path(), &["cost"]);
    let cost_limited = run(home.path(), &["cost", "--limit", "1"]);
    assert!(models.status.success() && cost_full.status.success() && cost_limited.status.success());

    let models_total = total_cost_figure(&stdout_of(&models));
    let cost_total = total_cost_figure(&stdout_of(&cost_full));
    let cost_limited_total = total_cost_figure(&stdout_of(&cost_limited));

    // The headline fix: cost's TOTAL equals models' total, and a smaller
    // `--limit` does not shrink it (it's the grand total, not a page subtotal).
    assert_eq!(
        cost_total, models_total,
        "cost TOTAL must equal models total"
    );
    assert_eq!(
        cost_limited_total, cost_total,
        "TOTAL must be limit-invariant"
    );

    // Truncation caption appears only when rows are limited below the full set.
    let limited_out = stdout_of(&cost_limited);
    assert!(
        limited_out.contains("Showing top 1 of 2 projects"),
        "expected truncation caption, got:\n{limited_out}"
    );
    assert!(!stdout_of(&cost_full).contains("Showing top"));
}

#[test]
fn cost_per_session_total_matches_by_project() {
    let home = fixture_home();
    let by_project = run(home.path(), &["cost"]);
    let per_session = run(home.path(), &["cost", "--per-session"]);
    assert!(by_project.status.success() && per_session.status.success());

    let ps_out = stdout_of(&per_session);
    assert!(
        ps_out.contains("TOTAL"),
        "per-session should carry a TOTAL row"
    );
    assert_eq!(
        total_cost_figure(&ps_out),
        total_cost_figure(&stdout_of(&by_project)),
    );
}

#[test]
fn cost_caption_counts_zero_usage_projects_in_the_population() {
    // Regression: a zero-usage project still shows as a $0 row in the
    // by-project view (LEFT JOIN), so the truncation caption must count it.
    let home = fixture_home();
    let projects = home.path().join(".claude").join("projects");
    write_session(
        &projects,
        "-Users-test-Projects-gamma",
        "sess-g1",
        // A user message only — no assistant/token usage.
        &[
            r#"{"type":"user","sessionId":"sess-g1","timestamp":"2026-04-13T00:00:00Z","message":{"content":"ping"}}"#,
        ],
    );

    // Three projects (alpha, beta paid + gamma at $0); `--limit 2` hides one.
    let out = run(home.path(), &["cost", "--limit", "2"]);
    assert!(out.status.success());
    let text = stdout_of(&out);
    assert!(
        text.contains("Showing top 2 of 3 projects"),
        "zero-usage project must be counted in the caption population:\n{text}"
    );
}

#[test]
fn cost_json_has_no_totals_object() {
    // The fix is human-table only; JSON stays a flat per-row array.
    let home = fixture_home();
    let out = run(home.path(), &["cost", "--limit", "1", "--json"]);
    let arr = json_of(&out);
    assert!(arr.is_array(), "cost --json must remain a bare array");
    assert!(
        arr.as_array()
            .unwrap()
            .iter()
            .all(|r| r.get("project").is_some()),
        "every element is a per-project row (no totals object)"
    );
}

// --- clean, scoped CLI error rendering ---

#[test]
fn parse_error_shows_scoped_usage_and_help_hint() {
    let home = fixture_home();
    let out = run(home.path(), &["models", "--since"]);
    assert_eq!(out.status.code(), Some(2), "usage errors exit 2");
    let err = stderr_of(&out);
    assert!(err.contains("a value is required"), "keeps clap's message");
    assert!(
        err.contains("Usage: claudex models"),
        "shows scoped usage:\n{err}"
    );
    assert!(
        err.contains("--since/--until: YYYY-MM-DD, RFC3339"),
        "shows accepted date formats:\n{err}"
    );
    assert!(
        err.contains("claudex models --since 7d"),
        "shows command examples:\n{err}"
    );
    assert!(
        err.contains("try 'claudex models --help'"),
        "scoped help hint:\n{err}"
    );
}

#[test]
fn invalid_since_value_shows_formats_and_examples() {
    let home = fixture_home();
    let out = run(home.path(), &["models", "--since", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_of(&out);
    assert!(err.contains("invalid date/time 'nope'"));
    assert!(err.contains("Usage: claudex models"));
    assert!(err.contains("--since/--until: YYYY-MM-DD, RFC3339"));
    assert!(err.contains("claudex models --since 7d"));
}

#[test]
fn nested_parse_error_scopes_usage_to_subsubcommand() {
    let home = fixture_home();
    let out = run(home.path(), &["skills", "generate", "--nope"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_of(&out);
    assert!(
        err.contains("Usage: claudex skills generate"),
        "scopes to nested command:\n{err}"
    );
    assert!(
        err.contains("claudex skills generate --target codex --dir ."),
        "shows nested command examples:\n{err}"
    );
    assert!(err.contains("try 'claudex skills generate --help'"));
}

#[test]
fn invalid_skills_target_shows_target_examples() {
    let home = fixture_home();
    let out = run(home.path(), &["skills", "generate", "--target", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_of(&out);
    assert!(err.contains("invalid value 'nope'"));
    assert!(err.contains("Accepted targets:"));
    assert!(err.contains("claude-code, codex, pi, openclaw, agents-md, plugin, all"));
}

#[test]
fn skills_parent_parse_error_shows_examples() {
    let home = fixture_home();
    let out = run(home.path(), &["skills", "--target", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_of(&out);
    assert!(err.contains("Usage: claudex skills"));
    assert!(err.contains("claudex skills generate"));
    assert!(err.contains("claudex skills install --global --target codex"));
}

#[test]
fn invalid_export_format_is_usage_error_with_examples() {
    let home = fixture_home();
    let out = run(home.path(), &["export", "--format", "xml", "abc12345"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_of(&out);
    assert!(err.contains("invalid value 'xml'"));
    assert!(err.contains("possible values: markdown, json"));
    assert!(err.contains("Usage: claudex export"));
    assert!(err.contains("claudex export claudex --format json --output session.json"));
}

#[test]
fn invalid_search_role_is_usage_error() {
    let home = fixture_home();
    let out = run(home.path(), &["search", "foo", "--role", "system"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_of(&out);
    assert!(err.contains("invalid value 'system'"));
    assert!(err.contains("possible values: user, assistant"));
    assert!(err.contains("Usage: claudex search"));
}

#[test]
fn invalid_completions_shell_is_usage_error_with_examples() {
    let home = fixture_home();
    let out = run(home.path(), &["completions", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_of(&out);
    assert!(err.contains("invalid value 'nope'"));
    assert!(err.contains("bash, zsh, fish, elvish, powershell"));
    assert!(err.contains("Usage: claudex completions"));
    assert!(err.contains("source <(claudex completions zsh)"));
}

#[test]
fn unknown_subcommand_falls_back_to_top_level_usage() {
    let home = fixture_home();
    let out = run(home.path(), &["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr_of(&out);
    assert!(err.contains("Usage: claudex"), "top-level usage:\n{err}");
    assert!(err.contains("try 'claudex --help'"));
}

#[test]
fn help_flag_still_exits_zero_on_stdout() {
    let home = fixture_home();
    let out = run(home.path(), &["--help"]);
    assert!(out.status.success(), "--help exits 0");
    assert!(stdout_of(&out).contains("Usage: claudex"));
    assert!(
        stderr_of(&out).is_empty(),
        "help goes to stdout, not stderr"
    );
}

#[test]
fn representative_help_outputs_include_examples() {
    let home = fixture_home();
    for (args, expected) in [
        (&["models", "--help"][..], "claudex models --since 7d"),
        (
            &["search", "--help"][..],
            "claudex search \"panic\" --since 7d",
        ),
        (
            &["export", "--help"][..],
            "claudex export claudex --format json --output session.json",
        ),
        (
            &["watch", "--help"][..],
            "claudex watch --follow /tmp/my-claude.log",
        ),
        (
            &["skills", "generate", "--help"][..],
            "claudex skills generate --target codex --dir .",
        ),
        (
            &["skills", "--help"][..],
            "claudex skills install --global --target codex",
        ),
    ] {
        let out = run(home.path(), args);
        assert!(out.status.success(), "stderr: {}", stderr_of(&out));
        let help = stdout_of(&out);
        assert!(
            help.contains("Examples:"),
            "help should include an examples section:\n{help}"
        );
        assert!(
            help.contains(expected),
            "help should include {expected:?}:\n{help}"
        );
    }
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

#[test]
fn search_json_returns_structured_hits() {
    let home = fixture_home();
    let out = run(home.path(), &["search", "foo", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert!(!arr.is_empty());
    assert!(arr[0].get("message_timestamp").is_some());
    assert!(arr[0].get("snippet").is_some());
}

#[test]
fn search_pr_facet_refreshes_stale_codex_pr_derivation() {
    let home = fixture_home_with_codex();
    let indexed = run(home.path(), &["index"]);
    assert!(indexed.status.success(), "stderr: {}", stderr_of(&indexed));

    let db = home.path().join(".claudex/index.db");
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "DELETE FROM pr_links WHERE pr_url = 'https://github.com/utensils/claudex/pull/38'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('pr_link_derivation_revision:codex', '0')",
        [],
    )
    .unwrap();
    drop(conn);

    let out = run(
        home.path(),
        &[
            "search",
            "hello",
            "--provider",
            "codex",
            "--pr",
            "38",
            "--json",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert!(
        arr.iter()
            .any(|h| h["session_id"].as_str() == Some("codex-a")),
        "expected stale PR links to be backfilled before search: {arr:?}"
    );
}

#[test]
fn timeline_refreshes_stale_codex_pr_derivation() {
    let home = fixture_home_with_codex();
    let indexed = run(home.path(), &["index"]);
    assert!(indexed.status.success(), "stderr: {}", stderr_of(&indexed));

    let db = home.path().join(".claudex/index.db");
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "DELETE FROM pr_links WHERE pr_url = 'https://github.com/utensils/claudex/pull/38'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('pr_link_derivation_revision:codex', '0')",
        [],
    )
    .unwrap();
    drop(conn);

    let out = run(home.path(), &["timeline", "--provider", "codex", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert!(
        arr.iter().any(|row| row["pr_count"].as_i64() == Some(1)),
        "expected stale PR links to be backfilled before timeline: {arr:?}"
    );
}

#[test]
fn subagent_transcripts_are_indexed_and_scanned() {
    let home = TempDir::new().unwrap();
    let projects = home.path().join(".claude/projects");
    let encoded = "-Users-test-Projects-agents";
    write_session(
        &projects,
        encoded,
        "parent-1",
        &[
            r#"{"type":"user","sessionId":"parent-1","timestamp":"2026-04-10T10:00:00Z","message":{"content":"delegate"}}"#,
            r#"{"type":"assistant","sessionId":"parent-1","timestamp":"2026-04-10T10:01:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":10},"content":[{"type":"text","text":"delegated"}]}}"#,
        ],
    );
    write_jsonl(
        &projects
            .join(encoded)
            .join("parent-1/subagents/workflows/run-1/agent-child.jsonl"),
        &[
            r#"{"type":"user","isSidechain":true,"sessionId":"child-1","timestamp":"2026-04-10T10:02:00Z","message":{"content":"subagent"}}"#,
            r#"{"type":"assistant","isSidechain":true,"sessionId":"child-1","timestamp":"2026-04-10T10:03:00Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":900,"output_tokens":90},"content":[{"type":"tool_use","name":"Edit","id":"t2","input":{}},{"type":"text","text":"Authenticated the dev app"}]}}"#,
        ],
    );
    write_jsonl(
        &projects
            .join(encoded)
            .join("parent-1/subagents/workflows/run-1/journal.jsonl"),
        &[r#"{"type":"started","agentId":"child-1"}"#],
    );

    let indexed = run(
        home.path(),
        &["search", "Authenticated the dev app", "--json"],
    );
    assert!(indexed.status.success(), "stderr: {}", stderr_of(&indexed));
    let indexed_hits = json_of(&indexed).as_array().unwrap().clone();
    assert_eq!(indexed_hits.len(), 1);
    assert_eq!(indexed_hits[0]["session_id"].as_str(), Some("child-1"));

    let scanned = run(
        home.path(),
        &[
            "search",
            "Authenticated the dev app",
            "--json",
            "--no-index",
        ],
    );
    assert!(scanned.status.success(), "stderr: {}", stderr_of(&scanned));
    assert_eq!(json_of(&scanned).as_array().unwrap().len(), 1);

    let cost = run(home.path(), &["cost", "--per-session", "--json"]);
    assert!(cost.status.success(), "stderr: {}", stderr_of(&cost));
    let rows = json_of(&cost).as_array().unwrap().clone();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["session_id"].as_str(), Some("parent-1"));
    assert_eq!(rows[0]["input_tokens"].as_u64(), Some(1000));

    // The `session` drill-down must roll the subagent up on BOTH paths. The
    // indexed path reads `parent_session_id`; the `--no-index` path discovers
    // and merges the child transcript directly. Their output must match.
    let detail_indexed = run(home.path(), &["session", "parent-1", "--json"]);
    assert!(
        detail_indexed.status.success(),
        "stderr: {}",
        stderr_of(&detail_indexed)
    );
    let detail_scanned = run(
        home.path(),
        &["session", "parent-1", "--json", "--no-index"],
    );
    assert!(
        detail_scanned.status.success(),
        "stderr: {}",
        stderr_of(&detail_scanned)
    );
    let di = json_of(&detail_indexed);
    let ds = json_of(&detail_scanned);
    for v in [&di, &ds] {
        assert_eq!(v["subagent_files"].as_array().unwrap().len(), 1);
        assert!(
            v["subagent_files"][0]
                .as_str()
                .unwrap()
                .ends_with("agent-child.jsonl")
        );
        // Parent (100) + child (900) input tokens rolled up.
        assert_eq!(v["input_tokens"].as_u64(), Some(1000));
        assert_eq!(v["output_tokens"].as_u64(), Some(100));
        // Both models appear in the per-model breakdown.
        assert_eq!(v["models"].as_array().unwrap().len(), 2);
    }
    assert_eq!(di["cost_usd"], ds["cost_usd"]);
    assert_eq!(di["total_tokens"], ds["total_tokens"]);
}

#[test]
fn search_json_snippet_preserves_highlight_markers() {
    // Both indexed and file-scan JSON paths wrap matches in `[[…]]` so
    // downstream consumers can reproduce highlighting deterministically.
    let home = fixture_home();

    let indexed = run(home.path(), &["search", "foo", "--json"]);
    assert!(indexed.status.success(), "stderr: {}", stderr_of(&indexed));
    let arr = json_of(&indexed).as_array().unwrap().clone();
    assert!(
        arr.iter()
            .any(|h| h["snippet"].as_str().unwrap_or_default().contains("[[")),
        "indexed snippets missing markers: {arr:?}"
    );

    let scanned = run(
        home.path(),
        &["search", "foo", "--json", "--case-sensitive"],
    );
    assert!(scanned.status.success(), "stderr: {}", stderr_of(&scanned));
    let arr = json_of(&scanned).as_array().unwrap().clone();
    assert!(
        arr.iter().any(|h| h["snippet"]
            .as_str()
            .unwrap_or_default()
            .contains("[[foo]]")),
        "file-scan snippets missing markers: {arr:?}"
    );
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
        "total_input_tokens",
        "top_projects",
        "top_tools",
        "top_stop_reasons",
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

#[test]
fn summary_json_plan_api_is_byte_compatible_with_default() {
    // Explicitly passing `--plan api` must not change the JSON shape — that's
    // the backward-compat contract for users who never set the flag.
    let home = fixture_home();
    let default = run(home.path(), &["summary", "--json"]);
    let explicit = run(home.path(), &["summary", "--json", "--plan", "api"]);
    assert!(default.status.success() && explicit.status.success());
    assert_eq!(stdout_of(&default), stdout_of(&explicit));
}

#[test]
fn summary_json_plan_flat_monthly_emits_plan_keys_and_keeps_legacy() {
    let home = fixture_home();
    let out = run(
        home.path(),
        &["summary", "--json", "--plan", "flat-monthly:250"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    // Discriminator + new keys.
    assert_eq!(v.get("plan").and_then(Value::as_str), Some("flat-monthly"));
    assert_eq!(
        v.get("actual_monthly_cost_usd").and_then(Value::as_f64),
        Some(250.0)
    );
    for field in [
        "api_equivalent_total_usd",
        "api_equivalent_week_usd",
        "leverage_this_week_multiple",
    ] {
        assert!(v.get(field).is_some(), "missing {field}");
    }
    // Backward-compat keys are still present.
    assert!(v.get("total_cost_usd").is_some());
    assert!(v.get("cost_this_week_usd").is_some());
    // The aliases agree with the legacy keys.
    assert_eq!(v["total_cost_usd"], v["api_equivalent_total_usd"]);
    assert_eq!(v["cost_this_week_usd"], v["api_equivalent_week_usd"]);
}

#[test]
fn summary_text_plan_flat_monthly_renders_leverage_row() {
    let home = fixture_home();
    let out = run(
        home.path(),
        &["--color", "never", "summary", "--plan", "flat-monthly:250"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let s = stdout_of(&out);
    assert!(s.contains("flat-monthly"), "missing plan label: {s}");
    assert!(
        s.contains("Leverage this week"),
        "missing leverage row: {s}"
    );
    assert!(s.contains("API equivalent"), "missing API-equivalent: {s}");
}

#[test]
fn summary_plan_invalid_value_is_rejected() {
    let home = fixture_home();
    let out = run(home.path(), &["summary", "--plan", "flat-monthly:abc"]);
    assert!(!out.status.success(), "expected non-zero exit");
    let err = stderr_of(&out);
    assert!(err.contains("--plan") || err.contains("invalid"));
    assert!(err.contains("Usage: claudex summary"));
    assert!(err.contains("claudex summary --plan flat-monthly:250"));
}

#[test]
fn summary_plan_is_summary_scoped_not_global() {
    // --plan is a `summary`-local flag now (not global). It must not be
    // accepted at the top level alongside another subcommand.
    let home = fixture_home();
    let out = run(home.path(), &["--plan", "flat-monthly:250", "cost"]);
    assert!(
        !out.status.success(),
        "expected --plan to be rejected as a global flag, got success: {}",
        stdout_of(&out)
    );
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
    assert!(arr.iter().all(|r| r.get("cache_read_tokens").is_some()));
}

#[test]
fn claude_local_models_are_grouped_and_priced_as_local_not_sonnet() {
    let home = fixture_home_with_claude_local_models();

    let models = run(home.path(), &["models", "--json"]);
    assert!(models.status.success(), "stderr: {}", stderr_of(&models));
    let rows = json_of(&models).as_array().unwrap().clone();
    let qwen = rows
        .iter()
        .find(|r| r["model"].as_str() == Some("qwen3.6-35b-a3b-ud-mlx"))
        .expect("qwen model row");
    assert_eq!(qwen["model_family"].as_str(), Some("Qwen"));
    assert_eq!(qwen["cost_usd"].as_f64(), Some(0.0));
    let gemma = rows
        .iter()
        .find(|r| r["model"].as_str() == Some("ollama/gemma4:31b"))
        .expect("ollama gemma model row");
    assert_eq!(gemma["model_family"].as_str(), Some("Gemma"));
    assert_eq!(gemma["cost_usd"].as_f64(), Some(0.0));
    assert!(
        rows.iter()
            .all(|r| r["model_family"].as_str() != Some("Sonnet")),
        "local/open rows must not be grouped as Sonnet: {rows:?}"
    );

    for args in [
        &["cost", "--json"][..],
        &["cost", "--no-index", "--json"][..],
    ] {
        let cost = run(home.path(), args);
        assert!(cost.status.success(), "stderr: {}", stderr_of(&cost));
        let projects = json_of(&cost).as_array().unwrap().clone();
        assert_eq!(projects.len(), 1, "args: {args:?}");
        assert_eq!(
            projects[0]["cost_usd"].as_f64(),
            Some(0.0),
            "args: {args:?}"
        );
        let families: Vec<_> = projects[0]["models"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(families.contains(&"Qwen"), "args: {args:?}");
        assert!(families.contains(&"Gemma"), "args: {args:?}");
        assert!(!families.contains(&"Sonnet"), "args: {args:?}");
    }

    for args in [
        &["summary", "--json"][..],
        &["summary", "--no-index", "--json"][..],
    ] {
        let summary = run(home.path(), args);
        assert!(summary.status.success(), "stderr: {}", stderr_of(&summary));
        let summary_json = json_of(&summary);
        let families: Vec<_> = summary_json["model_distribution"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row["model"].as_str())
            .collect();
        assert!(families.contains(&"Qwen"), "args: {args:?}");
        assert!(families.contains(&"Gemma"), "args: {args:?}");
        assert!(!families.contains(&"Sonnet"), "args: {args:?}");
    }
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
fn prs_provider_filter_returns_codex_extracted_links() {
    let home = fixture_home_with_codex();
    let out = run(home.path(), &["prs", "--provider", "codex", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 1, "expected one Codex PR, got {arr:?}");
    assert_eq!(arr[0]["provider"].as_str(), Some("codex"));
    assert_eq!(arr[0]["pr_number"].as_i64(), Some(38));
    assert_eq!(
        arr[0]["pr_url"].as_str(),
        Some("https://github.com/utensils/claudex/pull/38")
    );
}

#[test]
fn prs_provider_filter_returns_pi_extracted_links() {
    let home = fixture_home_with_pi();
    let out = run(home.path(), &["prs", "--provider", "pi", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 1, "expected one Pi PR, got {arr:?}");
    assert_eq!(arr[0]["provider"].as_str(), Some("pi"));
    assert_eq!(arr[0]["pr_number"].as_i64(), Some(40));
    assert_eq!(
        arr[0]["pr_url"].as_str(),
        Some("https://github.com/utensils/claudex/pull/40")
    );
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
    assert!(
        arr.iter()
            .all(|r| r.get("distinct_session_count").is_some())
    );
    assert!(arr.iter().all(|r| r.get("top_project").is_some()));
}

#[test]
fn files_path_filter_limits_results() {
    let home = fixture_home();
    let out = run(home.path(), &["files", "--json", "--path", "src/a.rs"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["file_path"].as_str(), Some("src/a.rs"));
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

#[test]
fn index_status_prune_and_vacuum_report_retention() {
    let home = fixture_home();
    let retained = write_session(
        &home.path().join(".claude/projects"),
        "-Users-test-Projects-alpha",
        "sess-prune",
        &[
            r#"{"type":"user","sessionId":"sess-prune","timestamp":"2026-04-11T10:00:00Z","message":{"content":"old retained session"}}"#,
        ],
    );

    let indexed = run(home.path(), &["index"]);
    assert!(indexed.status.success(), "stderr: {}", stderr_of(&indexed));
    fs::remove_file(&retained).unwrap();
    let retained_marked = run(home.path(), &["index", "--status"]);
    assert!(
        retained_marked.status.success(),
        "stderr: {}",
        stderr_of(&retained_marked)
    );
    let status = stdout_of(&retained_marked);
    assert!(
        status.contains("Retained"),
        "retention status missing: {status}"
    );

    let db_path = home.path().join(".claudex/index.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE sessions SET archived_at = 1 WHERE session_id = 'sess-prune'",
        [],
    )
    .unwrap();

    let pruned = run(
        home.path(),
        &[
            "index",
            "--prune-retained-days",
            "0",
            "--vacuum",
            "--status",
        ],
    );
    assert!(pruned.status.success(), "stderr: {}", stderr_of(&pruned));
    let s = stdout_of(&pruned);
    assert!(
        s.contains("Pruned 1 retained sessions"),
        "prune line missing: {s}"
    );
    assert!(
        s.contains("Vacuumed index database"),
        "vacuum line missing: {s}"
    );
    assert!(s.contains("Total"), "status table missing: {s}");
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
    assert!(
        s.contains("**Tool: Bash**"),
        "indexed Claude markdown should preserve tool-use blocks: {s}"
    );
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
fn export_json_by_project_is_valid_array() {
    let home = fixture_home();
    let out = run(home.path(), &["export", "projects", "--format", "json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    assert!(v.is_array(), "expected JSON array, got: {v:?}");
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

#[test]
fn export_missing_selector_does_not_truncate_output_file() {
    let home = fixture_home();
    let out_path = home.path().join("keep.md");
    fs::write(&out_path, "keep me").unwrap();

    let out = run(
        home.path(),
        &[
            "export",
            "this-session-does-not-exist",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success(), "expected missing selector to fail");
    assert_eq!(fs::read_to_string(&out_path).unwrap(), "keep me");
}

#[test]
fn export_falls_back_to_files_when_index_is_unreadable() {
    let home = fixture_home();
    let claudex_dir = home.path().join(".claudex");
    fs::create_dir_all(&claudex_dir).unwrap();
    fs::write(claudex_dir.join("index.db"), "not a sqlite database").unwrap();

    let out = run(home.path(), &["export", "alpha"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(stdout_of(&out).contains("foo bug"));
}

#[test]
fn export_project_skips_retained_index_rows() {
    let home = fixture_home();
    let retained = write_session(
        &home.path().join(".claude/projects"),
        "-Users-test-Projects-alpha",
        "sess-retained",
        &[
            r#"{"type":"user","sessionId":"sess-retained","timestamp":"2026-04-11T10:00:00Z","message":{"content":"deleted but retained"}}"#,
        ],
    );
    let indexed = run(home.path(), &["index"]);
    assert!(indexed.status.success(), "stderr: {}", stderr_of(&indexed));
    fs::remove_file(&retained).unwrap();
    let retained_marked = run(home.path(), &["index"]);
    assert!(
        retained_marked.status.success(),
        "stderr: {}",
        stderr_of(&retained_marked)
    );

    let out = run(home.path(), &["export", "alpha"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let s = stdout_of(&out);
    assert!(
        s.contains("foo bug"),
        "live matching session should export: {s}"
    );
    assert!(
        !s.contains("deleted but retained"),
        "retained off-disk rows should not be read from raw files: {s}"
    );
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
fn tools_per_session_no_index_sorts_missing_dates_last() {
    let home = fixture_home();
    let projects = home.path().join(".claude").join("projects");
    write_session(
        &projects,
        "-Users-test-Projects-delta",
        "sess-d1",
        &[
            r#"{"type":"assistant","sessionId":"sess-d1","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":1,"output_tokens":1},"content":[{"type":"tool_use","name":"Read","id":"t9","input":{}},{"type":"text","text":"ok"}]}}"#,
        ],
    );

    let indexed = run(home.path(), &["tools", "--per-session", "--json"]);
    let scanned = run(
        home.path(),
        &["tools", "--per-session", "--json", "--no-index"],
    );
    assert!(indexed.status.success(), "stderr: {}", stderr_of(&indexed));
    assert!(scanned.status.success(), "stderr: {}", stderr_of(&scanned));

    let indexed_rows = json_of(&indexed).as_array().unwrap().clone();
    let scanned_rows = json_of(&scanned).as_array().unwrap().clone();
    assert_eq!(
        indexed_rows
            .iter()
            .map(|r| r["session_id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        scanned_rows
            .iter()
            .map(|r| r["session_id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        scanned_rows.last().and_then(|r| r["session_id"].as_str()),
        Some("sess-d1")
    );
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
    assert_eq!(a["total_input_tokens"], b["total_input_tokens"]);
    assert_eq!(a["pr_count"], b["pr_count"]);
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
fn prs_dedupes_by_pr_url() {
    // Two sessions reference the same PR URL; `prs` should emit one row, not two.
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join(".claude").join("projects");
    write_session(
        &projects,
        "-Users-test-Projects-alpha",
        "sess-a",
        &[
            r#"{"type":"user","sessionId":"sess-a","timestamp":"2026-04-10T10:00:00Z","message":{"content":"open pr"}}"#,
            r#"{"type":"pr-link","prNumber":42,"prUrl":"https://github.com/org/alpha/pull/42","prRepository":"org/alpha","timestamp":"2026-04-10T10:01:00Z","sessionId":"sess-a"}"#,
        ],
    );
    write_session(
        &projects,
        "-Users-test-Projects-alpha",
        "sess-b",
        &[
            r#"{"type":"user","sessionId":"sess-b","timestamp":"2026-04-10T11:00:00Z","message":{"content":"checked pr"}}"#,
            r#"{"type":"pr-link","prNumber":42,"prUrl":"https://github.com/org/alpha/pull/42","prRepository":"org/alpha","timestamp":"2026-04-10T11:05:00Z","sessionId":"sess-b"}"#,
        ],
    );
    let out = run(tmp.path(), &["prs", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let arr = json_of(&out).as_array().unwrap().clone();
    assert_eq!(arr.len(), 1, "PRs should dedupe by pr_url; got: {arr:?}");
    assert_eq!(arr[0]["pr_number"].as_i64(), Some(42));
}

#[test]
fn files_text_column_header_is_modifications() {
    // Files should report both edit events and distinct sessions now.
    let home = fixture_home();
    let out = run(home.path(), &["files", "--limit", "5"]);
    assert!(out.status.success());
    let s = stdout_of(&out);
    assert!(
        s.contains("Modifications"),
        "expected header 'Modifications'; got:\n{s}"
    );
    assert!(
        s.contains("Sessions"),
        "expected header 'Sessions'; got:\n{s}"
    );
}

#[test]
fn session_json_returns_drilldown_fields() {
    let home = fixture_home();
    let out = run(home.path(), &["session", "sess-a1", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let v = json_of(&out);
    for field in [
        "project",
        "file_path",
        "cost_usd",
        "models",
        "tools",
        "files_modified",
        "pr_links",
        "stop_reasons",
        "attachments",
        "permission_changes",
    ] {
        assert!(v.get(field).is_some(), "missing {field}");
    }
}

#[test]
fn session_no_index_matches_indexed_core_fields() {
    let home = fixture_home();
    let indexed = run(home.path(), &["session", "sess-a1", "--json"]);
    let scanned = run(home.path(), &["session", "sess-a1", "--json", "--no-index"]);
    let a = json_of(&indexed);
    let b = json_of(&scanned);
    assert_eq!(a["session_id"], b["session_id"]);
    assert_eq!(a["message_count"], b["message_count"]);
    assert_eq!(a["files_modified"], b["files_modified"]);
    // Mixed-model sessions must collapse to `"mixed"` in both paths.
    assert_eq!(a["model"].as_str(), Some("mixed"));
    assert_eq!(b["model"].as_str(), Some("mixed"));
}

#[test]
fn session_uuid_like_selector_does_not_fallback_to_project_matching() {
    let home = fixture_home();
    let projects = home.path().join(".claude").join("projects");
    write_session(
        &projects,
        "-Users-test-Projects-e1a2f4-app",
        "sess-c1",
        &[
            r#"{"type":"user","sessionId":"sess-c1","timestamp":"2026-04-18T10:00:00Z","message":{"content":"hi"}}"#,
        ],
    );

    let out = run(home.path(), &["session", "e1a2f4", "--json"]);
    assert!(!out.status.success());
    assert!(
        stderr_of(&out).contains("no sessions found matching"),
        "stderr: {}",
        stderr_of(&out)
    );
}

#[test]
fn claudex_dir_resyncs_when_sessions_root_changes() {
    // Codex review P1: a CLAUDEX_DIR shared across different $HOME values must
    // not serve stale rows from a previous HOME for the staleness window.
    // home_a has 2 sessions; home_b has 1 — a stale-cache bug would make the
    // second run return 2 instead of 1.
    let state = tempfile::tempdir().expect("state tempdir");
    let home_a = fixture_home();

    let home_b = TempDir::new().unwrap();
    let projects_b = home_b.path().join(".claude").join("projects");
    write_session(
        &projects_b,
        "-Users-test-Projects-gamma",
        "sess-g1",
        &[
            r#"{"type":"user","sessionId":"sess-g1","timestamp":"2026-04-15T09:00:00Z","message":{"content":"hi from home b"}}"#,
        ],
    );

    let run_with = |home: &Path| -> Vec<Value> {
        let out = Command::new(BIN)
            .env("HOME", home)
            .env("NO_COLOR", "1")
            .env("CLAUDEX_DIR", state.path())
            .args(["sessions", "--json"])
            .output()
            .expect("spawn claudex");
        assert!(out.status.success(), "stderr: {}", stderr_of(&out));
        json_of(&out).as_array().unwrap().clone()
    };

    let rows_a = run_with(home_a.path());
    assert_eq!(rows_a.len(), 2, "home_a should have two sessions");

    let rows_b = run_with(home_b.path());
    // The index is additive: home_a's sessions are retained (soft-deleted, not
    // purged) when its files vanish from this root. The root change must still
    // trigger a re-sync that indexes home_b — a stale-cache bug would return
    // only home_a's two sessions, with gamma absent.
    assert!(
        rows_b
            .iter()
            .any(|r| r["project"].as_str().unwrap_or("").contains("gamma")),
        "sharing CLAUDEX_DIR across HOMEs must trigger a re-sync that indexes home_b; got {rows_b:?}"
    );
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

#[test]
fn update_help_mentions_supported_install_sources() {
    // The long help is the public contract for how users discover what the
    // update command does with non-managed installs. Keep the recipes findable.
    let out = Command::new(BIN)
        .args(["update", "--help"])
        .output()
        .expect("spawn claudex");
    assert!(
        out.status.success(),
        "update --help failed: {}",
        stderr_of(&out)
    );
    let help = stdout_of(&out);
    assert!(help.contains("--check"));
    assert!(help.contains("--force"));
    assert!(help.contains("--version"));
    assert!(help.contains("Nix"));
    assert!(help.contains("cargo"));
    assert!(help.contains("Homebrew"));
    assert!(help.contains("pacman"));
}

#[test]
fn update_fails_gracefully_without_curl() {
    // `update` shells out to curl for network I/O. When curl is missing the
    // command should exit non-zero with a clear "curl is required" message —
    // not panic, not emit a confusing rusage error. We point PATH at an
    // empty temp dir to guarantee curl isn't resolvable without blanking
    // PATH (which on some platforms falls back to system directories).
    let empty = TempDir::new().unwrap();
    let out = Command::new(BIN)
        .env("HOME", TempDir::new().unwrap().path())
        .env("NO_COLOR", "1")
        .env("PATH", empty.path())
        .args(["update", "--check"])
        .output()
        .expect("spawn claudex");
    assert!(!out.status.success());
    let stderr = stderr_of(&out);
    assert!(
        stderr.contains("not found in PATH"),
        "expected curl-missing message, got: {stderr}"
    );
}

// --- copilot providers (unified index) ---

fn fixture_home_with_copilot() -> TempDir {
    let tmp = fixture_home();
    let dir = tmp
        .path()
        .join(".copilot/session-state/aaaaaaaa-2222-0000-0000-000000000001");
    fs::create_dir_all(&dir).unwrap();
    let mut f = fs::File::create(dir.join("events.jsonl")).unwrap();
    writeln!(
        f,
        r#"{{"type":"session.start","data":{{"sessionId":"copilot-a","copilotVersion":"1.0.61","selectedModel":"claude-sonnet-4.6","context":{{"cwd":"/Users/test/copilotproj","repository":"utensils/demo","branch":"main"}}}},"timestamp":"2026-06-01T10:00:00Z"}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"user.message","data":{{"content":"hello from copilot"}},"timestamp":"2026-06-01T10:00:01Z"}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"assistant.message","data":{{"model":"claude-sonnet-4.6","content":"copilot says hello back","toolRequests":[{{"name":"bash","arguments":{{}}}}]}},"timestamp":"2026-06-01T10:00:05Z"}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"session.shutdown","data":{{"totalPremiumRequests":1,"modelMetrics":{{"claude-sonnet-4.6":{{"requests":{{"count":1}},"usage":{{"inputTokens":1000000,"outputTokens":100000,"cacheReadTokens":150000,"cacheWriteTokens":50000}}}}}}}},"timestamp":"2026-06-01T10:01:00Z"}}"#
    )
    .unwrap();
    f.flush().unwrap();
    fs::write(
        dir.join("workspace.yaml"),
        "id: copilot-a\ncwd: /Users/test/copilotproj\nname: demo session\n",
    )
    .unwrap();
    tmp
}

/// VS Code chat fixture inside the fake HOME, addressed explicitly through
/// `CLAUDEX_VSCODE_USER_DIR` so the test is independent of the platform's
/// config-dir layout.
fn fixture_home_with_copilot_vscode() -> (TempDir, std::path::PathBuf) {
    let tmp = fixture_home();
    let user_dir = tmp.path().join("vscode-user");
    let hash = user_dir.join("workspaceStorage/hash1");
    fs::create_dir_all(hash.join("chatSessions")).unwrap();
    fs::write(
        hash.join("workspace.json"),
        r#"{"folder":"file:///Users/test/vscodeproj"}"#,
    )
    .unwrap();
    fs::write(
        hash.join("chatSessions/vsc-a.json"),
        r#"{"version":3,"sessionId":"vsc-a","creationDate":1769763926111,"lastMessageDate":1769763999999,"customTitle":"Demo Chat","requests":[{"message":{"text":"hello from vscode chat"},"modelId":"copilot/claude-sonnet-4.5","timestamp":1769763934260,"response":[{"value":"vscode chat says hello back"}]}]}"#,
    )
    .unwrap();
    (tmp, user_dir)
}

fn run_vscode(home: &Path, user_dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .env("CLAUDEX_VSCODE_USER_DIR", user_dir)
        .env_remove("CLAUDEX_COPILOT_DIR")
        .args(args)
        .output()
        .expect("spawn claudex")
}

#[test]
fn copilot_sessions_appear_in_unified_index() {
    let home = fixture_home_with_copilot();
    let out = run(home.path(), &["sessions", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let projects: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["project"].as_str())
        .collect();
    assert!(
        projects.iter().any(|p| p.contains("copilotproj")),
        "copilot session must surface in unified sessions, got: {projects:?}"
    );

    let out = run(
        home.path(),
        &["sessions", "--provider", "copilot", "--json"],
    );
    let rows = json_of(&out);
    let arr = rows.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["provider"].as_str(), Some("copilot"));
}

#[test]
fn copilot_cost_derives_non_cache_input_and_uses_sonnet_pricing() {
    let home = fixture_home_with_copilot();
    let out = run(
        home.path(),
        &["cost", "--per-session", "--provider", "copilot", "--json"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let row = &rows.as_array().unwrap()[0];
    // inputTokens (1,000,000) includes cache; non-cache input is derived:
    // 1,000,000 - 150,000 - 50,000 = 800,000.
    assert_eq!(row["input_tokens"].as_i64(), Some(800_000));
    assert_eq!(row["cache_read_tokens"].as_i64(), Some(150_000));
    assert_eq!(row["cache_creation_tokens"].as_i64(), Some(50_000));
    assert_eq!(row["output_tokens"].as_i64(), Some(100_000));
    // Sonnet: 0.8*3 + 0.15*0.3 + 0.05*3.75 + 0.1*15 = 2.4+0.045+0.1875+1.5 = $4.1325
    let cost = row["cost_usd"].as_f64().unwrap();
    assert!(
        (cost - 4.1325).abs() < 0.001,
        "expected ~$4.1325, got {cost}"
    );
}

#[test]
fn copilot_search_and_session_drilldown_work() {
    let home = fixture_home_with_copilot();
    let out = run(
        home.path(),
        &[
            "search",
            "hello from copilot",
            "--provider",
            "copilot",
            "--json",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    assert!(
        !rows.as_array().unwrap().is_empty(),
        "search must find the copilot user message"
    );

    let out = run(home.path(), &["session", "copilot-a", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let session = json_of(&out);
    assert_eq!(session["provider"].as_str(), Some("copilot"));
}

#[test]
fn copilot_providers_row_and_no_index_rejection() {
    let home = fixture_home_with_copilot();
    let out = run(home.path(), &["providers", "--json"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let copilot = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["provider"].as_str() == Some("copilot"))
        .expect("copilot listed in providers");
    assert_eq!(copilot["discovered_files"].as_i64(), Some(1));
    assert_eq!(copilot["indexed_sessions"].as_i64(), Some(1));

    let out = run(
        home.path(),
        &["sessions", "--provider", "copilot", "--no-index", "--json"],
    );
    assert!(!out.status.success());
    assert!(stderr_of(&out).contains("--no-index only scans Claude transcripts"));
}

#[test]
fn copilot_vscode_sessions_search_and_export() {
    let (home, user_dir) = fixture_home_with_copilot_vscode();

    let out = run_vscode(
        home.path(),
        &user_dir,
        &["sessions", "--provider", "copilot-vscode", "--json"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let rows = json_of(&out);
    let arr = rows.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["provider"].as_str(), Some("copilot-vscode"));
    assert!(
        arr[0]["project"]
            .as_str()
            .is_some_and(|p| p.contains("vscodeproj"))
    );

    let out = run_vscode(
        home.path(),
        &user_dir,
        &[
            "search",
            "vscode chat says",
            "--provider",
            "copilot-vscode",
            "--json",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(!json_of(&out).as_array().unwrap().is_empty());

    // Export must replay the session document, not stream it line-by-line.
    let out = run_vscode(home.path(), &user_dir, &["export", "vsc-a"]);
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let md = stdout_of(&out);
    assert!(md.contains("hello from vscode chat"), "markdown: {md}");
    assert!(md.contains("vscode chat says hello back"), "markdown: {md}");

    let out = run_vscode(
        home.path(),
        &user_dir,
        &["export", "vsc-a", "--format", "json"],
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let exported = json_of(&out);
    assert_eq!(
        exported["normalized_messages"][0]["text"].as_str(),
        Some("hello from vscode chat")
    );
}

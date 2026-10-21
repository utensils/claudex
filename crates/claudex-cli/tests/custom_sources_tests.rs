//! Independent subprocess proof of additive discovery through public CLI entry points.
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use rusqlite::Connection;
use serde_json::{Value, json};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_claudex");

fn command(home: &Path) -> Command {
    let mut command = Command::new(BIN);
    for key in [
        "CLAUDEX_CONFIG",
        "CLAUDEX_DIR",
        "CLAUDEX_COPILOT_DIR",
        "CLAUDEX_VSCODE_USER_DIR",
        "CLAUDEX_NO_INDEX_DEFAULT",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "CODEX_HOME",
        "CLAUDE_CONFIG_DIR",
        "PI_CODING_AGENT_DIR",
        "PI_CONFIG_DIR",
        "OMP_PROFILE",
        "PI_PROFILE",
        "OPENCLAW_STATE_DIR",
        "OPENCLAW_TRAJECTORY_DIR",
    ] {
        command.env_remove(key);
    }
    command
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .current_dir(home);
    command
}

fn run(home: &Path, args: &[&str]) -> Output {
    command(home).args(args).output().unwrap()
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn write(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn jsonl(path: &Path, values: &[Value]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        values.iter().map(|v| format!("{v}\n")).collect::<String>(),
    )
    .unwrap();
}

fn claude(path: &Path, id: &str) {
    jsonl(
        path,
        &[
            json!({"type":"user","sessionId":id,"cwd":"/projects/demo","timestamp":"2026-10-20T10:00:00Z","message":{"content":"distinct discovery marker"}}),
            json!({"type":"assistant","sessionId":id,"timestamp":"2026-10-20T10:01:00Z","message":{"model":"claude-sonnet-5-5","usage":{"input_tokens":1000,"output_tokens":100},"content":[{"type":"text","text":"finished"}]}}),
        ],
    );
}

fn cline_and_roo(root: &Path) {
    write(
        &root.join("cline/sdk-task/sdk-task.messages.json"),
        &json!({
            "sessionId":"cline-extra", "updated_at":"2026-10-20T10:02:00Z", "messages":[
                {"role":"user","ts":1792490400000i64,"content":"additional harness searchable marker"},
                {"role":"assistant","ts":1792490460000i64,"modelInfo":{"provider":"anthropic","id":"claude-sonnet-5-5"},"content":[{"type":"text","text":"Cline completed custom root proof"}],"metrics":{"inputTokens":1200,"outputTokens":120,"cacheReadTokens":40,"cacheWriteTokens":30,"cost":0.25}}
            ]
        }),
    );
    write(
        &root.join("cline/sdk-task/sdk-task.json"),
        &json!({"cwd":"/projects/example","started_at":"2026-10-20T10:00:00Z"}),
    );
    write(
        &root.join("roo/tasks/roo-extra/api_conversation_history.json"),
        &json!([
            {"role":"user","ts":1792490400000i64,"content":"additional harness searchable marker"},
            {"role":"assistant","ts":1792490460000i64,"content":[{"type":"text","text":"Roo completed custom root proof"}]}
        ]),
    );
    write(
        &root.join("roo/tasks/roo-extra/ui_messages.json"),
        &json!([
            {"type":"say","say":"api_req_started","ts":1792490400000i64,"text":json!({"tokensIn":2300,"tokensOut":230,"cacheReads":50,"cacheWrites":60,"cost":0.45}).to_string()}
        ]),
    );
    write(
        &root.join("roo/tasks/roo-extra/task_metadata.json"),
        &json!({"model_usage":[{"ts":1792490400000i64,"model_provider_id":"openai","model_id":"gpt-6.1-sol"}]}),
    );
    write(
        &root.join("roo/state/taskHistory.json"),
        &json!([{"id":"roo-extra","cwdOnTaskInitialization":"/projects/example"}]),
    );
}

fn indexed(home: &Path) -> Connection {
    Connection::open(home.join(".claudex/index.db")).unwrap()
}

fn ids(db: &Connection) -> Vec<String> {
    db.prepare("SELECT session_id FROM sessions ORDER BY session_id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn cli_explicit_config_is_additive_and_cli_directories_merge_and_deduplicate() {
    let home = TempDir::new().unwrap();
    claude(
        &home
            .path()
            .join(".claude/projects/-projects-demo/default.jsonl"),
        "default",
    );
    claude(
        &home.path().join("sources/configured/one.jsonl"),
        "configured",
    );
    claude(&home.path().join("sources/cli/two.jsonl"), "cli");
    write(
        &home.path().join("settings/config.json"),
        &json!({"providers":{"claude":{"session_dirs":["../sources/configured"]}}}),
    );
    success(run(
        home.path(),
        &[
            "--config",
            "settings/config.json",
            "--session-dir",
            "claude=sources/cli",
            "--session-dir",
            "claude=sources/configured",
            "index",
        ],
    ));
    assert_eq!(ids(&indexed(home.path())), ["cli", "configured", "default"]);
    success(run(
        home.path(),
        &[
            "--config",
            "settings/config.json",
            "--session-dir",
            "claude=sources/cli",
            "search",
            "distinct discovery marker",
            "--json",
        ],
    ));
    let db = indexed(home.path());
    assert_eq!(ids(&db).len(), 3);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sessions WHERE present_on_disk=1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        3
    );
}

#[test]
fn cli_default_config_and_env_config_and_explicit_precedence_work() {
    let home = TempDir::new().unwrap();
    claude(&home.path().join("a/a.jsonl"), "a");
    claude(&home.path().join("b/b.jsonl"), "b");
    write(
        &home.path().join(".claudex/config.json"),
        &json!({"providers":{"claude":{"session_dirs":["../a"]}}}),
    );
    write(
        &home.path().join("selected.json"),
        &json!({"providers":{"claude":{"session_dirs":["b"]}}}),
    );
    success(run(home.path(), &["index"]));
    assert_eq!(ids(&indexed(home.path())), ["a"]);
    let out = command(home.path())
        .env("CLAUDEX_CONFIG", home.path().join("selected.json"))
        .args(["index"])
        .output()
        .unwrap();
    success(out);
    let db = indexed(home.path());
    assert_eq!(ids(&db), ["a", "b"]);
    assert_eq!(
        db.query_row(
            "SELECT present_on_disk FROM sessions WHERE session_id='b'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    // An invalid environment selection must be overridden by explicit --config.
    let out = command(home.path())
        .env("CLAUDEX_CONFIG", home.path().join("missing.json"))
        .args(["--config", "selected.json", "index"])
        .output()
        .unwrap();
    success(out);
}

#[test]
fn cli_rejects_bad_source_arguments_and_configuration_without_creating_index() {
    let home = TempDir::new().unwrap();
    for value in ["omp", "omp=", "not-a-provider=sessions"] {
        let output = run(home.path(), &["--session-dir", value, "index"]);
        assert!(!output.status.success(), "accepted {value}");
    }
    assert!(
        !run(home.path(), &["--config", "missing.json", "index"])
            .status
            .success()
    );
    write(
        &home.path().join("bad.json"),
        &json!({"providers":{"omp":{"session_dir":["sessions"]}}}),
    );
    assert!(
        !run(home.path(), &["--config", "bad.json", "index"])
            .status
            .success()
    );
    assert!(!home.path().join(".claudex/index.db").exists());
}

#[test]
fn cli_config_indexes_additional_directories_for_every_provider() {
    let home = TempDir::new().unwrap();
    let root = home.path().join("sources");
    claude(&root.join("claude/custom.jsonl"), "claude-extra");
    jsonl(
        &root.join("codex/custom.jsonl"),
        &[
            json!({"type":"session_meta","timestamp":"2026-10-20T10:00:00Z","payload":{"id":"codex-extra","cwd":"/projects/demo"}}),
            json!({"type":"response_item","timestamp":"2026-10-20T10:01:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"distinct discovery marker"}]}}),
        ],
    );
    for provider in ["pi", "omp", "openclaw"] {
        jsonl(
            &root.join(provider).join("custom.jsonl"),
            &[
                json!({"type":"session","version":3,"id":format!("{provider}-extra"),"cwd":"/projects/demo","timestamp":"2026-10-20T10:00:00Z"}),
                json!({"type":"message","timestamp":"2026-10-20T10:01:00Z","message":{"role":"user","content":"distinct discovery marker"}}),
                json!({"type":"message","timestamp":"2026-10-20T10:02:00Z","message":{"role":"assistant","provider":"anthropic","model":"claude-sonnet-5-5","content":[{"type":"text","text":"finished"}],"usage":{"input":100,"output":10,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.123}}}}),
            ],
        );
    }
    jsonl(
        &root.join("copilot/custom/events.jsonl"),
        &[
            json!({"type":"session.start","timestamp":"2026-10-20T10:00:00Z","data":{"sessionId":"copilot-extra","selectedModel":"claude-sonnet-5-5","context":{"cwd":"/projects/demo"}}}),
            json!({"type":"user.message","timestamp":"2026-10-20T10:01:00Z","data":{"content":"distinct discovery marker"}}),
        ],
    );
    write(
        &root.join("copilot-vscode/custom.json"),
        &json!({"version":3,"sessionId":"vscode-extra","creationDate":1792490400000i64,"requests":[{"message":{"text":"distinct discovery marker"},"modelId":"copilot/claude-sonnet-5-5","timestamp":1792490400000i64,"response":[{"value":"finished"}]}]}),
    );
    write(
        &root.join("gemini/session-custom.json"),
        &json!({"sessionId":"gemini-extra","startTime":"2026-10-20T10:00:00Z","directories":["/projects/demo"],"messages":[{"id":"u","type":"user","content":"distinct discovery marker","timestamp":"2026-10-20T10:00:00Z"}]}),
    );
    write(
        &root.join("opencode/storage/session/project/extra.json"),
        &json!({"id":"opencode-extra","directory":"/projects/demo","time":{"created":1792490400000i64,"updated":1792490460000i64}}),
    );
    write(
        &root.join("opencode/storage/message/opencode-extra/msg.json"),
        &json!({"id":"msg","role":"assistant","modelID":"claude-sonnet-5-5","providerID":"anthropic","cost":0.42,"tokens":{"input":100,"output":20,"cache":{"read":30,"write":5}},"time":{"created":1792490400000i64}}),
    );
    write(
        &root.join("opencode/storage/part/msg/text.json"),
        &json!({"id":"text","type":"text","text":"distinct discovery marker"}),
    );
    cline_and_roo(&root);
    let providers = [
        "claude",
        "codex",
        "copilot",
        "copilot-vscode",
        "openclaw",
        "pi",
        "omp",
        "gemini",
        "opencode",
        "cline",
        "roo",
    ];
    let config: serde_json::Map<String, Value> = providers
        .iter()
        .map(|p| {
            (
                p.to_string(),
                json!({"session_dirs":[format!("sources/{p}")]}),
            )
        })
        .collect();
    write(&home.path().join("all.json"), &json!({"providers":config}));
    success(run(home.path(), &["--config", "all.json", "index"]));
    let db = indexed(home.path());
    for provider in providers {
        let (count, messages): (i64, i64) = db
            .query_row(
                "SELECT count(*), coalesce(sum(message_count),0) FROM sessions WHERE provider=?1",
                [provider],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "{provider} additional root discovery");
        assert!(messages > 0, "{provider} additional root messages missing");
    }
    let (tokens,cost): (i64,f64) = db.query_row("SELECT input_tokens,cost_usd FROM token_usage JOIN sessions ON sessions.id=token_usage.session_id WHERE sessions.provider='opencode'", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(tokens, 100);
    assert!((cost - 0.42).abs() < 1e-9);
    // Reindexing the identical roots must retain one row per source.
    success(run(home.path(), &["--config", "all.json", "index"]));
    assert_eq!(ids(&indexed(home.path())).len(), 11);
}

#[test]
fn cli_custom_opencode_database_reads_live_wal_and_refreshes_messages() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("custom.db");
    let source = Connection::open(&path).unwrap();
    source.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA wal_autocheckpoint=0;
         CREATE TABLE session_v2(id TEXT PRIMARY KEY,directory TEXT,parent_id TEXT,time_created INTEGER,time_updated INTEGER,time_archived INTEGER);
         CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,time_created INTEGER,data TEXT);
         INSERT INTO session_v2 VALUES('db-extra','/projects/demo',NULL,1792490400000,1792490460000,NULL);"
    ).unwrap();
    let message = |input| {
        json!({"role":"assistant","model":{"id":"gpt-6.1-sol","provider":"openai"},"tokens":{"input":input,"output":10},"content":[{"type":"text","text":"live database proof"}]}).to_string()
    };
    source
        .execute(
            "INSERT INTO session_message VALUES('m1','db-extra',1792490460000,?1)",
            [message(100)],
        )
        .unwrap();
    let option = format!("opencode={}", path.display());
    let before = fs::read(&path).unwrap();
    success(run(home.path(), &["--session-dir", &option, "index"]));
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "reader modified source DB"
    );
    let usage = || {
        indexed(home.path())
            .query_row("SELECT input_tokens FROM token_usage", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
    };
    assert_eq!(usage(), 100);
    source
        .execute(
            "UPDATE session_message SET data=?1 WHERE id='m1'",
            [message(1200)],
        )
        .unwrap();
    success(run(home.path(), &["--session-dir", &option, "index"]));
    assert_eq!(usage(), 1200, "WAL-only commit must refresh indexed usage");
    assert_eq!(ids(&indexed(home.path())), ["db-extra"]);
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "reader checkpointed source DB"
    );
}

#[test]
fn cli_extra_only_root_disappearance_retains_and_marks_session_off_disk() {
    let home = TempDir::new().unwrap();
    let extra = home.path().join("external");
    claude(&extra.join("retained.jsonl"), "retained");
    write(
        &home.path().join("config.json"),
        &json!({"providers":{"claude":{"session_dirs":["external"]}}}),
    );
    success(run(home.path(), &["--config", "config.json", "index"]));
    fs::remove_dir_all(&extra).unwrap();
    success(run(home.path(), &["--config", "config.json", "index"]));
    let db = indexed(home.path());
    assert_eq!(ids(&db), ["retained"]);
    assert_eq!(
        db.query_row(
            "SELECT present_on_disk FROM sessions WHERE session_id='retained'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT input_tokens FROM token_usage", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1000
    );
    claude(&extra.join("retained.jsonl"), "retained");
    success(run(home.path(), &["--config", "config.json", "index"]));
    assert_eq!(ids(&indexed(home.path())), ["retained"]);
    assert_eq!(
        indexed(home.path())
            .query_row(
                "SELECT present_on_disk FROM sessions WHERE session_id='retained'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn cli_cline_roo_custom_sources_support_reports_export_and_retention() {
    let home = TempDir::new().unwrap();
    cline_and_roo(&home.path().join("sources"));
    let config = home.path().join("config.json");
    write(
        &config,
        &json!({"providers":{"cline":{"session_dirs":["sources/cline"]},"roo":{"session_dirs":["sources/roo"]}}}),
    );
    success(run(home.path(), &["--config", "config.json", "index"]));
    for (provider, id, model, input, expected_cost) in [
        ("cline", "cline-extra", "claude-sonnet-5-5", 1200, 0.25),
        ("roo", "roo-extra", "gpt-6.1-sol", 2300, 0.45),
    ] {
        let output = success(run(
            home.path(),
            &[
                "--config",
                "config.json",
                "sessions",
                "--provider",
                provider,
                "--json",
            ],
        ));
        let sessions: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(sessions.as_array().unwrap().len(), 1);
        assert_eq!(sessions[0]["provider"], provider);
        assert_eq!(sessions[0]["session_id"], id);
        let output = success(run(
            home.path(),
            &[
                "--config",
                "config.json",
                "search",
                "additional harness searchable marker",
                "--provider",
                provider,
                "--json",
            ],
        ));
        let results: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(!results.as_array().unwrap().is_empty());
        assert!(
            results
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["provider"] == provider)
        );
        let output = success(run(
            home.path(),
            &[
                "--config",
                "config.json",
                "models",
                "--provider",
                provider,
                "--json",
            ],
        ));
        assert!(String::from_utf8_lossy(&output.stdout).contains(model));
        let output = success(run(
            home.path(),
            &[
                "--config",
                "config.json",
                "cost",
                "--provider",
                provider,
                "--json",
            ],
        ));
        let cost: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            cost.to_string().contains(&expected_cost.to_string()),
            "cost report: {cost}"
        );
        let output = success(run(
            home.path(),
            &["--config", "config.json", "session", id, "--json"],
        ));
        let details: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(details["provider"], provider);
        assert_eq!(details["project"], "/projects/example");
        assert_eq!(details["input_tokens"], input);
        assert!((details["cost_usd"].as_f64().unwrap() - expected_cost).abs() < 1e-9);
        let output = success(run(
            home.path(),
            &["--config", "config.json", "export", id, "--format", "json"],
        ));
        let export: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(export["provider"], provider);
        let markdown = success(run(
            home.path(),
            &[
                "--config",
                "config.json",
                "export",
                id,
                "--format",
                "markdown",
            ],
        ));
        assert!(
            String::from_utf8_lossy(&markdown.stdout)
                .contains("additional harness searchable marker")
        );
        assert!(
            export
                .to_string()
                .contains("additional harness searchable marker")
        );
    }
    fs::remove_dir_all(home.path().join("sources")).unwrap();
    success(run(home.path(), &["--config", "config.json", "index"]));
    for id in ["cline-extra", "roo-extra"] {
        let output = success(run(
            home.path(),
            &["--config", "config.json", "session", id, "--json"],
        ));
        let details: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(details["present_on_disk"], false);
        assert!(details["input_tokens"].as_i64().unwrap() > 0);
        let output = success(run(
            home.path(),
            &[
                "--config",
                "config.json",
                "search",
                "additional harness searchable marker",
                "--provider",
                details["provider"].as_str().unwrap(),
                "--json",
            ],
        ));
        let retained_search: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(!retained_search.as_array().unwrap().is_empty());
    }
}

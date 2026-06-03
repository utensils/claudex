use std::fs;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use claudex::api::{Claudex, ClaudexConfig, Filter, Provider};
use tempfile::TempDir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn write_session(home: &TempDir) {
    let dir = home
        .path()
        .join(".claude/projects/-Users-test-Projects-api");
    fs::create_dir_all(&dir).unwrap();
    let mut f = fs::File::create(dir.join("api-session.jsonl")).unwrap();
    writeln!(
        f,
        r#"{{"type":"user","sessionId":"api-session","timestamp":"2026-04-10T10:00:00Z","message":{{"content":"find the api bug"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"assistant","sessionId":"api-session","timestamp":"2026-04-10T10:01:00Z","message":{{"model":"claude-sonnet-4-6","usage":{{"input_tokens":100,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":[{{"type":"tool_use","name":"Read","id":"t1","input":{{}}}},{{"type":"text","text":"api result"}}]}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"system","subtype":"turn_duration","durationMs":12000,"timestamp":"2026-04-10T10:01:30Z","sessionId":"api-session"}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"file-history-snapshot","snapshot":{{"trackedFileBackups":{{"src/api.rs":{{"backupFileName":"x"}}}}}},"timestamp":"2026-04-10T10:02:00Z","sessionId":"api-session"}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"pr-link","prNumber":66,"prUrl":"https://github.com/utensils/claudex/pull/66","prRepository":"utensils/claudex","timestamp":"2026-04-10T10:03:00Z","sessionId":"api-session"}}"#
    )
    .unwrap();
}

fn write_codex_session(home: &TempDir) {
    let dir = home.path().join(".codex/sessions/2026/04/10");
    fs::create_dir_all(&dir).unwrap();
    let mut f = fs::File::create(dir.join("rollout-2026-04-10T11-00-00-codex-api.jsonl")).unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-04-10T11:00:00Z","type":"session_meta","payload":{{"id":"codex-api","cwd":"/Users/test/Projects/codex-api","originator":"codex_cli_rs","cli_version":"0.99.0","source":"cli"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-04-10T11:00:30Z","type":"turn_context","payload":{{"model":"gpt-5-codex"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-04-10T11:01:00Z","type":"response_item","payload":{{"type":"user_message","message":"codex prompt"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-04-10T11:01:30Z","type":"response_item","payload":{{"type":"agent_message","message":"codex result"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"timestamp":"2026-04-10T11:02:00Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":1000,"cached_input_tokens":200,"output_tokens":500}}}}}}}}"#
    )
    .unwrap();
}

#[test]
fn api_default_config_indexes_and_queries_temp_home() {
    let _guard = env_lock().lock().unwrap();
    let home = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    write_session(&home);

    let old_home = std::env::var_os("HOME");
    let old_state = std::env::var_os("CLAUDEX_DIR");
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("CLAUDEX_DIR", state.path());
    }

    let result = (|| {
        let mut cx = Claudex::new()?;
        cx.ensure_fresh()?;

        let summary = cx.summary(Filter::default())?;
        assert_eq!(summary.total_sessions, 1);

        let sessions = cx.sessions(None, None, Filter::default(), 10)?;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].provider, "claude");

        let hits = cx.search("api", Filter::default(), 10)?;
        assert!(!hits.is_empty());

        let filtered = cx.sessions(
            None,
            None,
            Filter {
                providers: vec![Provider::Claude],
                model: Some("sonnet".to_string()),
                since: Some("2026-04-01".to_string()),
                until: Some("2026-04-30".to_string()),
                on_disk_only: true,
            },
            10,
        )?;
        assert_eq!(filtered.len(), 1);

        Ok::<(), Box<dyn std::error::Error>>(())
    })();

    unsafe {
        match old_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match old_state {
            Some(value) => std::env::set_var("CLAUDEX_DIR", value),
            None => std::env::remove_var("CLAUDEX_DIR"),
        }
    }

    result.unwrap();
}

#[test]
fn api_configured_providers_scope_default_filters() {
    let _guard = env_lock().lock().unwrap();
    let home = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    write_session(&home);
    write_codex_session(&home);

    let old_home = std::env::var_os("HOME");
    let old_state = std::env::var_os("CLAUDEX_DIR");
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("CLAUDEX_DIR", state.path());
    }

    let result = (|| {
        let mut all = Claudex::new()?;
        all.sync_now()?;
        assert_eq!(all.summary(Filter::default())?.total_sessions, 2);

        let mut codex_only = Claudex::with_config(ClaudexConfig {
            state_dir: Some(state.path().to_path_buf()),
            providers: vec![Provider::Codex],
        })?;

        let summary = codex_only.summary(Filter::default())?;
        assert_eq!(summary.total_sessions, 1);

        let sessions = codex_only.sessions(None, None, Filter::default(), 10)?;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].provider, "codex");

        let hits = codex_only.search("api", Filter::default(), 10)?;
        assert!(
            hits.is_empty(),
            "default Codex scope must not search existing Claude rows"
        );

        let explicit_codex = codex_only.sessions(
            None,
            None,
            Filter {
                providers: vec![Provider::Codex],
                ..Filter::default()
            },
            10,
        )?;
        assert_eq!(explicit_codex.len(), 1);

        let explicit_claude = codex_only.sessions(
            None,
            None,
            Filter {
                providers: vec![Provider::Claude],
                ..Filter::default()
            },
            10,
        )?;
        assert!(
            explicit_claude.is_empty(),
            "call filters should narrow within the configured provider scope"
        );

        let matches = codex_only.session_matches("codex-api", None)?;
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].provider, "codex");

        let claude_matches = codex_only.session_matches("api-session", None)?;
        assert!(
            claude_matches.is_empty(),
            "session matching should not leak existing Claude rows"
        );

        let claude_detail = all
            .session_matches("api-session", None)?
            .into_iter()
            .find(|row| row.provider == "claude")
            .expect("claude row");
        assert!(
            codex_only
                .session_detail(&claude_detail.file_path)?
                .is_none(),
            "session detail should also honor configured provider scope"
        );

        Ok::<(), Box<dyn std::error::Error>>(())
    })();

    unsafe {
        match old_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match old_state {
            Some(value) => std::env::set_var("CLAUDEX_DIR", value),
            None => std::env::remove_var("CLAUDEX_DIR"),
        }
    }

    result.unwrap();
}

#[test]
fn api_report_methods_return_typed_results() {
    let _guard = env_lock().lock().unwrap();
    let home = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    write_session(&home);

    let old_home = std::env::var_os("HOME");
    let old_state = std::env::var_os("CLAUDEX_DIR");
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("CLAUDEX_DIR", state.path());
    }

    let result = (|| {
        let mut cx = Claudex::with_config(ClaudexConfig {
            state_dir: Some(state.path().to_path_buf()),
            providers: vec![Provider::Claude],
        })?;

        assert_eq!(cx.force_rebuild()?, 1);
        assert_eq!(
            cx.index()
                .query_sessions(None, None, &Default::default(), 10)?
                .len(),
            1
        );
        cx.index_mut().ensure_fresh(&[])?;

        let filter = Filter::default();
        assert_eq!(cx.summary(filter.clone())?.total_sessions, 1);
        assert_eq!(cx.sessions(Some("api"), None, filter.clone(), 10)?.len(), 1);
        assert_eq!(cx.search("api", filter.clone(), 10)?.len(), 2);

        let costs_by_project = cx.costs_by_project(None, filter.clone(), 10)?;
        assert_eq!(costs_by_project.len(), 1);
        let costs_per_session = cx.costs_per_session(None, filter.clone(), 10)?;
        assert_eq!(costs_per_session.len(), 1);
        let cost_summary = cx.cost_summary(None, filter.clone())?;
        assert_eq!(cost_summary.session_count, 1);

        let tools = cx.tools(None, filter.clone(), 10)?;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_name, "Read");
        let tools_per_session = cx.tools_per_session(None, filter.clone(), 10)?;
        assert_eq!(tools_per_session.len(), 1);
        assert_eq!(tools_per_session[0].tools.get("Read"), Some(&1));

        let models = cx.models(None, filter.clone())?;
        assert_eq!(models.len(), 1);
        assert!(models[0].model.contains("sonnet"));

        assert_eq!(cx.timeline(filter.clone(), false, 10)?.len(), 1);
        assert_eq!(cx.timeline(filter.clone(), true, 10)?.len(), 1);

        let activity = cx.activity(filter.clone(), 10)?;
        assert_eq!(activity.recent_sessions.len(), 1);
        assert_eq!(activity.recent_prs.len(), 1);
        assert_eq!(activity.hot_files.len(), 1);
        assert_eq!(activity.slow_projects.len(), 1);

        let status = cx.provider_status(filter.clone(), false)?;
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].provider, "claude");

        let retention = cx.retention_stats(filter.clone())?;
        assert_eq!(retention.total_sessions, 1);

        let matches = cx.session_matches("api-session", None)?;
        assert_eq!(matches.len(), 1);
        let detail = cx
            .session_detail(&matches[0].file_path)?
            .expect("session detail");
        assert_eq!(detail.provider, "claude");

        let prs = cx.prs(None, filter.clone(), 10)?;
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].pr_number, 66);

        let files = cx.files(None, Some("api.rs"), filter.clone(), 10)?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_path, "src/api.rs");

        let turns = cx.turns(None, filter, 10)?;
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].turn_count, 1);

        Ok::<(), Box<dyn std::error::Error>>(())
    })();

    unsafe {
        match old_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match old_state {
            Some(value) => std::env::set_var("CLAUDEX_DIR", value),
            None => std::env::remove_var("CLAUDEX_DIR"),
        }
    }

    result.unwrap();
}

#[test]
fn library_crate_omits_cli_only_dependencies() {
    let manifest =
        fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for forbidden in [
        "clap",
        "clap_complete",
        "comfy-table",
        "indicatif",
        "owo-colors",
        "terminal_size",
        "tar",
        "flate2",
        "sha2",
    ] {
        assert!(
            !manifest.contains(&format!("{forbidden}.workspace")),
            "library crate should not depend on {forbidden}"
        );
    }
}

use std::fs;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use claudex::api::{Claudex, Filter, Provider};
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

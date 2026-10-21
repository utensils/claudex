use claudex::config::SourceConfig;
use claudex::filter::ResolvedFilter;
use claudex::index::IndexStore;
use claudex::providers::{ClaudeProvider, Provider};
use claudex::store::SessionStore;
use std::fs;

fn transcript(dir: &std::path::Path, id: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(format!("{id}.jsonl")), format!(r#"{{"type":"user","sessionId":"{id}","cwd":"/projects/demo","timestamp":"2026-10-21T10:00:00Z","message":{{"content":"a searchable phrase"}}}}
{{"type":"assistant","sessionId":"{id}","timestamp":"2026-10-21T10:01:00Z","message":{{"model":"claude-sonnet-5-5","usage":{{"input_tokens":100,"output_tokens":10}},"content":[{{"type":"text","text":"done"}}]}}}}
"#)).unwrap();
}

#[test]
fn config_resolves_relative_paths_and_rejects_typos() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    fs::write(
        &path,
        r#"{"providers":{"omp":{"session_dirs":["./extra"]}}}"#,
    )
    .unwrap();
    let config = SourceConfig::load(&path).unwrap();
    assert_eq!(
        config.providers["omp"].session_dirs[0],
        std::fs::canonicalize(tmp.path()).unwrap().join("./extra")
    );
    fs::write(&path, r#"{"providers":{"ompp":{"session_dirs":[]}}}"#).unwrap();
    assert!(SourceConfig::load(&path).is_err());
    fs::write(&path, r#"{"providers":{"omp":{"session_dir":[]}}}"#).unwrap();
    assert!(SourceConfig::load(&path).is_err());
    assert!(SourceConfig::default().add_directory("omp=").is_err());
}

#[test]
fn extra_roots_are_additive_deduplicated_and_immediately_invalidate_staleness() {
    let tmp = tempfile::tempdir().unwrap();
    let primary = tmp.path().join("projects");
    let extra = tmp.path().join("extra");
    transcript(&primary.join("-projects-demo"), "primary");
    transcript(&extra, "extra");
    let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    let base = || Provider::Claude(ClaudeProvider::at(SessionStore::at(primary.clone())));
    index.sync_now(&[base()]).unwrap();
    let combined =
        || base().with_session_dirs(vec![extra.clone(), tmp.path().to_path_buf(), extra.clone()]);
    index.ensure_fresh(&[combined()]).unwrap();
    let rows = index
        .query_sessions(None, None, &ResolvedFilter::default(), 100)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.present_on_disk));
    index.sync_now(&[combined()]).unwrap();
    fs::remove_dir_all(&extra).unwrap();
    index.sync_now(&[combined()]).unwrap();
    let rows = index
        .query_sessions(None, None, &ResolvedFilter::default(), 100)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|row| row.present_on_disk).count(), 1);
}

#[test]
fn openclaw_extra_sources_merge_trajectory_and_keep_archive_names() {
    use claudex::providers::OpenClawProvider;
    let tmp = tempfile::tempdir().unwrap();
    let extra = tmp.path().join("extra");
    fs::create_dir_all(&extra).unwrap();
    let source = r#"{"type":"session","id":"alpha","cwd":"/projects/demo","timestamp":"2026-10-21T10:00:00Z"}
{"type":"message","message":{"role":"assistant","model":"claude-sonnet-5-5","usage":{"input":100,"output":10,"cost":{"total":0.25}},"content":[{"type":"text","text":"completed"}]}}
"#;
    fs::write(extra.join("alpha.jsonl"), source).unwrap();
    fs::write(extra.join("alpha.trajectory.jsonl"), "{}\n").unwrap();
    fs::write(
        extra.join("beta.jsonl.deleted.2026-10-21T10-00-00Z"),
        source.replace("alpha", "beta"),
    )
    .unwrap();
    let provider = Provider::OpenClaw(OpenClawProvider::at(tmp.path().join("absent")))
        .with_session_dirs(vec![extra]);
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files.iter().filter(|file| file.archived).count(), 1);
    let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    index.sync_now(&[provider]).unwrap();
    assert_eq!(
        index
            .query_sessions(None, None, &ResolvedFilter::default(), 20)
            .unwrap()
            .len(),
        2
    );
}

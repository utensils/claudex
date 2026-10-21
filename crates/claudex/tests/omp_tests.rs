use std::fs;

use claudex::providers::{SessionProvider, omp::OmpProvider};
use tempfile::TempDir;

fn write(path: &std::path::Path, records: &[serde_json::Value]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        records.iter().map(|r| format!("{r}\n")).collect::<String>(),
    )
    .unwrap();
}

#[test]
fn omp_preserves_conversation_and_accounts_for_background_calls() {
    use serde_json::json;
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("sessions/project/session.jsonl");
    write(
        &path,
        &[
            json!({"type":"session","id":"main","timestamp":"2026-01-01T00:00:00Z","cwd":"/work/project"}),
            json!({"type":"message","timestamp":"2026-01-01T00:00:01Z","message":{"role":"user","content":"Find the parser"}}),
            json!({"type":"message","timestamp":"2026-01-01T00:00:02Z","message":{"role":"assistant","provider":"anthropic","model":"claude-sonnet-4-6","content":[{"type":"thinking","text":"check source"},{"type":"toolCall","id":"t1","name":"read","arguments":{"path":"src/parser.rs"}},{"type":"text","text":"Reading the parser"}],"usage":{"input":100,"output":20,"cacheRead":30,"cacheWrite":40,"cost":{"total":0.02}},"stopReason":"toolUse"}}),
            json!({"type":"model_usage","timestamp":"2026-01-01T00:00:03Z","provider":"openrouter","model":"typesafe/model","purpose":"find","role":"typesafe","usage":{"input":7,"output":8,"cacheRead":9,"cacheWrite":10,"cost":{"total":0.001}},"stopReason":"stop"}),
            json!({"type":"model_usage","timestamp":"2026-01-01T00:00:04Z","provider":"anthropic","model":"claude-sonnet-4-6","purpose":"title","usage":{"input":1,"output":2,"cost":{"total":0.002}},"stopReason":"stop"}),
        ],
    );
    let provider = OmpProvider::at(tmp.path().to_path_buf());
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    let record = provider.parse(&files[0]).unwrap();
    assert_eq!(provider.id(), "omp");
    assert_eq!(record.session_id.as_deref(), Some("main"));
    assert_eq!(record.project_display, "/work/project");
    assert_eq!(record.message_count, 2);
    assert_eq!(record.messages.len(), 2);
    assert_eq!(record.messages[0].content, "Find the parser");
    assert_eq!(record.tool_names, ["read"]);
    assert_eq!(record.thinking_block_count, 1);
    assert!((record.embedded_cost.unwrap() - 0.023).abs() < 1e-12);
    let main = &record.model_usage["anthropic/claude-sonnet-4-6"];
    assert_eq!(main.assistant_message_count, 2);
    assert_eq!(main.usage.input_tokens, 101);
    assert_eq!(main.usage.output_tokens, 22);
    assert_eq!(main.usage.cache_read_tokens, 30);
    assert_eq!(main.usage.cache_creation_tokens, 40);
    let auxiliary = &record.model_usage["openrouter/typesafe/model"];
    assert_eq!(auxiliary.assistant_message_count, 1);
    assert_eq!(auxiliary.usage.input_tokens, 7);
    assert_eq!(auxiliary.usage.output_tokens, 8);
    assert_eq!(auxiliary.usage.cache_read_tokens, 9);
    assert_eq!(auxiliary.usage.cache_creation_tokens, 10);
    assert_eq!(record.stop_reason_counts["stop"], 2);
}

#[test]
fn omp_recurses_into_subagents_and_resolves_parent_headers() {
    use serde_json::json;
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("custom-storage");
    let parent = root.join("project/date_parent-id.jsonl");
    write(
        &parent,
        &[json!({"type":"session","id":"actual-parent","cwd":"/work/project"})],
    );
    write(
        &root.join("project/date_parent-id/agents/worker.jsonl"),
        &[
            json!({"type":"session","id":"child","cwd":"/work/project","parentSession":parent}),
            json!({"type":"model_usage","provider":"local","model":"small","usage":{"input":10,"cost":{"total":0.0}}}),
        ],
    );
    fs::write(root.join("ignored.json"), "{}").unwrap();
    let provider = OmpProvider::sessions_at(root);
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 2);
    let records: Vec<_> = files.iter().map(|f| provider.parse(f).unwrap()).collect();
    let child = records
        .iter()
        .find(|r| r.session_id.as_deref() == Some("child"))
        .unwrap();
    assert_eq!(child.parent_session_id.as_deref(), Some("actual-parent"));
    assert_eq!(child.embedded_cost, Some(0.0));
    assert_eq!(child.message_count, 0);
}

#[test]
fn omp_retains_parent_id_when_source_is_missing() {
    use serde_json::json;
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("sessions");
    write(
        &root.join("child.jsonl"),
        &[json!({"type":"session","id":"child","parentSession":"/missing/date_parent-uuid.jsonl"})],
    );
    let provider = OmpProvider::sessions_at(root);
    let record = provider.parse(&provider.enumerate().unwrap()[0]).unwrap();
    assert_eq!(record.parent_session_id.as_deref(), Some("parent-uuid"));
}

#[test]
fn omp_missing_root_is_empty() {
    let tmp = TempDir::new().unwrap();
    let provider = OmpProvider::sessions_at(tmp.path().join("missing"));
    assert!(!provider.enabled());
    assert!(provider.enumerate().unwrap().is_empty());
}

#[test]
fn omp_custom_roots_survive_repeat_sync_and_removal() {
    use claudex::filter::ResolvedFilter;
    use claudex::index::IndexStore;
    use claudex::providers::Provider;
    use serde_json::json;
    let tmp = TempDir::new().unwrap();
    let standard = tmp.path().join("standard");
    let extra = tmp.path().join("extra");
    for (root, id) in [(&standard, "primary"), (&extra, "additional")] {
        write(
            &root.join("session.jsonl"),
            &[
                json!({"type":"session","id":id,"cwd":"/work/project","timestamp":"2026-01-01T00:00:00Z"}),
                json!({"type":"model_usage","timestamp":"2026-01-01T00:00:01Z","provider":"local","model":"small","usage":{"input":5,"output":2,"cost":{"total":0.25}}}),
            ],
        );
    }
    let make_provider = || {
        Provider::Omp(OmpProvider::sessions_at(standard.clone()))
            .with_session_dirs(vec![extra.clone(), standard.clone()])
    };
    let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    index.sync_now(&[make_provider()]).unwrap();
    index.sync_now(&[make_provider()]).unwrap();
    let rows = index
        .query_sessions(None, None, &ResolvedFilter::default(), 100)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|s| s.present_on_disk));
    let costs = index
        .query_cost_per_session(None, &ResolvedFilter::default(), 100)
        .unwrap();
    assert_eq!(costs.len(), 2);
    assert!(costs.iter().all(|s| (s.cost_usd - 0.25).abs() < 1e-12));
    fs::remove_dir_all(&extra).unwrap();
    index.sync_now(&[make_provider()]).unwrap();
    let rows = index
        .query_sessions(None, None, &ResolvedFilter::default(), 100)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|s| s.present_on_disk).count(), 1);
}

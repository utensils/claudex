use claudex::{
    filter::ResolvedFilter,
    index::IndexStore,
    providers::{ClineProvider, Provider, RooProvider, SessionProvider},
};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;
fn write(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn legacy(root: &Path) {
    let task = root.join("tasks/task-example");
    write(
        &task.join("api_conversation_history.json"),
        &json!([
            {"role":"user","content":"Fix the generic module"},
            {"role":"assistant","content":[{"type":"thinking","thinking":"review"},{"type":"text","text":"Implemented the generic change"},{"type":"tool_use","id":"call1","name":"write_to_file","input":{"path":"src/lib.rs"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call1","content":"saved"}]}
        ]),
    );
    write(
        &task.join("ui_messages.json"),
        &json!([
            {"type":"say","say":"task","text":"Fix the generic module","ts":1000},
            {"type":"say","say":"api_req_started","text":"{\"tokensIn\":100,\"tokensOut\":10,\"cost\":0.1}","ts":1100},
            {"type":"say","say":"api_req_finished","text":"{\"tokensIn\":120,\"tokensOut\":20,\"cacheReads\":40,\"cacheWrites\":10,\"cost\":0.2}","ts":1200},
            {"type":"say","say":"deleted_api_reqs","text":"{\"tokensIn\":30,\"tokensOut\":5,\"cost\":0.05}","ts":1300},
            {"type":"say","say":"subagent_usage","text":"{\"tokensIn\":50,\"tokensOut\":10,\"cost\":0.1}","ts":1400}
        ]),
    );
    write(
        &task.join("task_metadata.json"),
        &json!({"model_usage":[{"ts":1000,"model_id":"claude-sonnet-4-6","model_provider_id":"anthropic"}],"files_in_context":[{"path":"src/extra.rs","record_source":"cline_edited","cline_edit_date":1200}]}),
    );
    write(
        &root.join("state/taskHistory.json"),
        &json!([{"id":"task-example","cwdOnTaskInitialization":"/work/example","workspace":"/work/roo-example","parentTaskId":"parent-example","totalCost":99,"tokensIn":999999}]),
    );
}

#[test]
fn cline_and_roo_legacy_tasks_merge_usage_once_and_parse_messages() {
    for roo in [false, true] {
        let tmp = TempDir::new().unwrap();
        legacy(tmp.path());
        let provider = if roo {
            Provider::Roo(RooProvider::at(tmp.path().to_path_buf()))
        } else {
            Provider::Cline(ClineProvider::at(tmp.path().to_path_buf()))
        };
        let files = provider.enumerate().unwrap();
        assert_eq!(files.len(), 1, "UI and API are one session");
        let record = provider.parse(&files[0]).unwrap();
        assert_eq!(record.session_id.as_deref(), Some("task-example"));
        assert_eq!(record.parent_session_id.as_deref(), Some("parent-example"));
        assert_eq!(record.project_display, "/work/example");
        assert_eq!(record.message_count, 3);
        assert_eq!(
            record.messages.len(),
            2,
            "tool-result output is not user-authored text"
        );
        assert_eq!(record.thinking_block_count, 1);
        assert_eq!(record.tool_names, vec!["write_to_file"]);
        assert!(
            record
                .file_paths_modified
                .contains(&"src/lib.rs".to_string())
        );
        assert!(
            record
                .file_paths_modified
                .contains(&"src/extra.rs".to_string())
        );
        let model = &record.model_usage["anthropic/claude-sonnet-4-6"];
        assert_eq!(model.usage.input_tokens, 200);
        assert_eq!(model.usage.output_tokens, 35);
        assert_eq!(model.usage.cache_read_tokens, 40);
        assert_eq!(model.usage.cache_creation_tokens, 10);
        assert!(
            (record.embedded_cost.unwrap() - 0.35).abs() < 1e-9,
            "finished usage overrides started usage and history aggregate is not added twice"
        );
        let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
        assert_eq!(index.sync_now(&[provider]).unwrap(), 1);
        assert_eq!(
            index
                .query_sessions(None, None, &ResolvedFilter::default(), 100)
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn legacy_ui_and_history_updates_invalidate_fingerprint_independently() {
    let tmp = TempDir::new().unwrap();
    legacy(tmp.path());
    let provider = ClineProvider::at(tmp.path().to_path_buf());
    let files = provider.enumerate().unwrap();
    let before = provider.source_fingerprint(&files[0]).unwrap();
    write(
        &tmp.path().join("tasks/task-example/ui_messages.json"),
        &json!([{"type":"say","say":"api_req_started","text":"{\"tokensIn\":1234,\"cost\":0}","ts":1000}]),
    );
    let after = provider.source_fingerprint(&files[0]).unwrap();
    assert_ne!(before, after);
    assert_eq!(
        provider.parse(&files[0]).unwrap().model_usage["anthropic/claude-sonnet-4-6"]
            .usage
            .input_tokens,
        1234
    );
    write(
        &tmp.path().join("state/taskHistory.json"),
        &json!([{"id":"task-example","workspace":"/work/changed"}]),
    );
    assert_ne!(after, provider.source_fingerprint(&files[0]).unwrap());
    assert_eq!(
        provider.parse(&files[0]).unwrap().project_display,
        "/work/changed"
    );
}

#[test]
fn sdk_root_and_subagent_artifacts_use_per_message_metrics_and_models() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions/root-example");
    write(
        &dir.join("root-example.json"),
        &json!({"session_id":"root-example","cwd":"/work/sdk","provider":"anthropic","model":"claude-sonnet-4-6","started_at":"2026-10-01T10:00:00Z"}),
    );
    write(
        &dir.join("root-example.messages.json"),
        &json!({"version":1,"sessionId":"root-example","agent":"lead","updated_at":"2026-10-01T10:01:00Z","origin":{"sessionId":"root-example"},"messages":[
            {"role":"user","content":"Check the generic fixture","ts":1790848800000i64},
            {"role":"assistant","content":[{"type":"text","text":"Checked the generic fixture"},{"type":"tool_use","id":"tool1","name":"read_file","input":{"path":"src/lib.rs"}}],"modelInfo":{"id":"claude-sonnet-4-6","provider":"anthropic"},"metrics":{"inputTokens":100,"outputTokens":20,"cacheReadTokens":40,"cacheWriteTokens":10,"cost":0.03},"ts":1790848801000i64},
            {"role":"assistant","content":"Second model answer","modelInfo":{"id":"gpt-5.4","provider":"openai"},"metrics":{"inputTokens":50,"outputTokens":10,"cost":0.01}}
        ]}),
    );
    write(
        &dir.join("researcher.messages.json"),
        &json!({"version":1,"sessionId":"root-example__researcher","agent":"subagent","origin":{"sessionId":"root-example__researcher","parentThreadId":"root-example"},"messages":[{"role":"assistant","content":"Subagent answer","metrics":{"inputTokens":10,"outputTokens":5,"cost":0}}]}),
    );
    let provider = ClineProvider::at(tmp.path().to_path_buf());
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 2);
    let records = files
        .iter()
        .map(|f| provider.parse(f).unwrap())
        .collect::<Vec<_>>();
    let root = records
        .iter()
        .find(|r| r.session_id.as_deref() == Some("root-example"))
        .unwrap();
    assert_eq!(root.project_display, "/work/sdk");
    assert_eq!(root.message_count, 3);
    assert_eq!(root.tool_names, vec!["read_file"]);
    assert_eq!(
        root.model_usage["anthropic/claude-sonnet-4-6"]
            .usage
            .input_tokens,
        100
    );
    assert_eq!(root.model_usage["openai/gpt-5.4"].usage.output_tokens, 10);
    assert_eq!(root.embedded_cost, Some(0.04));
    let child = records
        .iter()
        .find(|r| r.session_id.as_deref() == Some("root-example__researcher"))
        .unwrap();
    assert_eq!(child.parent_session_id.as_deref(), Some("root-example"));
    assert_eq!(child.project_display, "/work/sdk");
    assert_eq!(child.embedded_cost, Some(0.0));
    let file = files
        .iter()
        .find(|f| f.path.file_name().unwrap() == "researcher.messages.json")
        .unwrap();
    let before = provider.source_fingerprint(file).unwrap();
    write(
        &dir.join("root-example.json"),
        &json!({"cwd":"/work/moved","model":"gpt-5.4","provider":"openai"}),
    );
    assert_ne!(before, provider.source_fingerprint(file).unwrap());
    assert_eq!(provider.parse(file).unwrap().project_display, "/work/moved");
}

#[test]
fn ui_only_and_xml_tool_calls_remain_searchable() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp.path().join("tasks/ui-only/ui_messages.json"),
        &json!([
            {"type":"say","say":"task","text":"Inspect module","ts":1000},
            {"type":"say","say":"text","text":"<read_file><path>src/lib.rs</path></read_file>","ts":2000},
            {"type":"say","say":"api_req_started","text":"{\"tokensIn\":10,\"tokensOut\":5,\"cost\":0}","ts":1500}
        ]),
    );
    let provider = RooProvider::at(tmp.path().join("tasks/ui-only"));
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    let record = provider.parse(&files[0]).unwrap();
    assert_eq!(record.message_count, 2);
    assert_eq!(record.tool_names, vec!["read_file"]);
    assert_eq!(record.embedded_cost, Some(0.0));
}

#[test]
fn ui_only_task_transition_to_api_transcript_reuses_one_retained_index_row() {
    let tmp = TempDir::new().unwrap();
    let task = tmp.path().join("tasks/task-transition");
    write(
        &task.join("ui_messages.json"),
        &json!([{"type":"say","say":"task","text":"Initial prompt","ts":1000}]),
    );
    let providers = [Provider::Cline(ClineProvider::at(tmp.path().to_path_buf()))];
    let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    assert_eq!(index.sync_now(&providers).unwrap(), 1);
    let old = index
        .query_sessions(None, None, &ResolvedFilter::default(), 100)
        .unwrap();
    write(
        &task.join("api_conversation_history.json"),
        &json!([{"role":"user","content":"Initial prompt"},{"role":"assistant","content":"Complete answer"}]),
    );
    assert_eq!(index.sync_now(&providers).unwrap(), 1);
    let current = index
        .query_sessions(None, None, &ResolvedFilter::default(), 100)
        .unwrap();
    assert_eq!(
        current.len(),
        1,
        "same task must not leave a retained UI-only duplicate"
    );
    assert_eq!(
        current[0].rowid, old[0].rowid,
        "source transition preserves row identity"
    );
    assert_eq!(current[0].message_count, 2);
}

#[test]
fn roo_per_task_history_supplies_workspace_parent_and_fallback_usage() {
    let tmp = TempDir::new().unwrap();
    let task = tmp.path().join("tasks/roo-current");
    write(
        &task.join("api_conversation_history.json"),
        &json!([{"role":"assistant","content":"Roo answer"}]),
    );
    let history = task.join("history_item.json");
    write(
        &history,
        &json!({"id":"roo-current","workspace":"/work/roo","parentTaskId":"parent-roo","tokensIn":123,"tokensOut":45,"cacheReads":20,"totalCost":0.1}),
    );
    let provider = RooProvider::at(tmp.path().to_path_buf());
    let files = provider.enumerate().unwrap();
    let record = provider.parse(&files[0]).unwrap();
    assert_eq!(record.project_display, "/work/roo");
    assert_eq!(record.parent_session_id.as_deref(), Some("parent-roo"));
    assert_eq!(record.model_usage["unknown"].usage.input_tokens, 123);
    assert_eq!(record.embedded_cost, Some(0.1));
    let before = provider.source_fingerprint(&files[0]).unwrap();
    write(&history, &json!({"workspace":"/work/changed"}));
    assert_ne!(before, provider.source_fingerprint(&files[0]).unwrap());
    assert_eq!(
        provider.parse(&files[0]).unwrap().project_display,
        "/work/changed"
    );
}

#[test]
fn unfinished_api_request_does_not_steal_a_later_requests_finish() {
    let tmp = TempDir::new().unwrap();
    let task = tmp.path().join("tasks/requests");
    write(
        &task.join("ui_messages.json"),
        &json!([
            {"type":"say","say":"api_req_started","text":"{\"tokensIn\":100,\"cost\":0.1}","ts":1000},
            {"type":"say","say":"api_req_started","text":"{\"tokensIn\":200,\"cost\":0.2}","ts":2000},
            {"type":"say","say":"api_req_finished","text":"{\"tokensIn\":300,\"cost\":0.3}","ts":3000}
        ]),
    );
    let provider = ClineProvider::at(tmp.path().to_path_buf());
    let file = provider.enumerate().unwrap().remove(0);
    let record = provider.parse(&file).unwrap();
    assert_eq!(record.model_usage["unknown"].usage.input_tokens, 400);
    assert!((record.embedded_cost.unwrap() - 0.4).abs() < 1e-9);
}

#[test]
#[cfg(unix)]
fn aliased_task_root_preserves_identity_when_ui_becomes_api_transcript() {
    let tmp = TempDir::new().unwrap();
    let real = tmp.path().join("real");
    let task = real.join("tasks/transition");
    write(
        &task.join("ui_messages.json"),
        &json!([{"type":"say","say":"task","text":"Prompt","ts":1000}]),
    );
    let alias = tmp.path().join("alias");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let first = Provider::Cline(ClineProvider::at(alias));
    let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    assert_eq!(index.sync_now(&[first]).unwrap(), 1);
    write(
        &task.join("api_conversation_history.json"),
        &json!([{"role":"user","content":"Prompt"},{"role":"assistant","content":"Answer"}]),
    );
    let second = Provider::Cline(ClineProvider::at(real));
    assert_eq!(index.sync_now(&[second]).unwrap(), 1);
    assert_eq!(
        index
            .query_sessions(None, None, &ResolvedFilter::default(), 100)
            .unwrap()
            .len(),
        1
    );
}

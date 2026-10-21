//! Generic fixtures for the Gemini CLI and OpenCode on-disk schemas.
use std::{fs, path::Path};

use claudex::{
    filter::ResolvedFilter,
    index::IndexStore,
    providers::{GeminiProvider, OpenCodeProvider, Provider, SessionProvider},
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use tempfile::TempDir;

fn write(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

#[test]
fn gemini_json_recovers_project_tools_cache_and_reasoning() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("tmp/project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join(".project_root"), "/work/example\n").unwrap();
    write(
        &project.join("chats/session-example.json"),
        &json!({
            "sessionId":"gemini-example","startTime":"2026-10-01T10:00:00Z","lastUpdated":"2026-10-01T10:01:00Z",
            "messages":[{"type":"user","content":[{"text":"Inspect the module"}],"timestamp":"2026-10-01T10:00:00Z"},
            {"type":"gemini","model":"gemini-2.5-pro","content":"Checked the module","tokens":{"input":1000,"cached":400,"output":30,"thoughts":20},"toolCalls":[{"id":"tool1","name":"read_file"}],"thoughts":[{"subject":"inspect"}]}]
        }),
    );
    let provider = GeminiProvider::at(tmp.path().to_path_buf());
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    let record = provider.parse(&files[0]).unwrap();
    assert_eq!(record.project_display, "/work/example");
    assert_eq!(record.message_count, 2);
    assert_eq!(record.messages[0].content, "Inspect the module");
    let stats = &record.model_usage["gemini-2.5-pro"];
    assert_eq!(stats.usage.input_tokens, 600);
    assert_eq!(stats.usage.cache_read_tokens, 400);
    assert_eq!(stats.usage.output_tokens, 50);
    assert_eq!(record.tool_names, vec!["read_file"]);
    assert_eq!(record.duration_ms, 60000);
}

#[test]
fn gemini_jsonl_materializes_updates_patches_rewind_and_partial_tail() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("session-new.jsonl");
    let values = [
        json!({"sessionId":"new","startTime":"2026-10-01T10:00:00Z","directories":["/work/example"]}),
        json!({"id":"u","type":"user","content":"hello"}),
        json!({"id":"a","type":"gemini","model":"gemini-2.5-flash","content":"partial","tokens":{"input":100,"output":10}}),
        json!({"id":"removed","type":"gemini","model":"gemini-2.5-flash","content":"discarded","tokens":{"input":9999}}),
        json!({"$rewindTo":"removed"}),
        json!({"$patch":{"id":"a","content":[{"text":"complete"}],"tokens":{"input":200,"output":20}}}),
        json!({"$set":{"lastUpdated":"2026-10-01T10:01:00Z"}}),
    ];
    fs::write(
        path,
        values
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n{\"id\":",
    )
    .unwrap();
    let provider = GeminiProvider::at(tmp.path().to_path_buf());
    let record = provider.parse(&provider.enumerate().unwrap()[0]).unwrap();
    assert_eq!(record.session_id.as_deref(), Some("new"));
    assert_eq!(record.project_display, "/work/example");
    assert_eq!(record.message_count, 2);
    assert_eq!(record.messages[1].content, "complete");
    assert_eq!(
        record.model_usage["gemini-2.5-flash"].usage.input_tokens,
        200
    );
}

fn fixture_database(root: &Path, modern: bool) -> Connection {
    let conn = Connection::open(root.join("opencode.db")).unwrap();
    let session = if modern { "session_v2" } else { "session" };
    let message = if modern { "session_message" } else { "message" };
    conn.execute_batch(&format!("PRAGMA journal_mode=WAL;
        CREATE TABLE {session}(id TEXT PRIMARY KEY,directory TEXT,parent_id TEXT,time_created INTEGER,time_updated INTEGER,time_archived INTEGER);
        CREATE TABLE {message}(id TEXT PRIMARY KEY,session_id TEXT,time_created INTEGER,data TEXT);
        CREATE TABLE part(id TEXT PRIMARY KEY,message_id TEXT,time_created INTEGER,data TEXT);
        INSERT INTO {session} VALUES ('s1','/work/example',NULL,1000000,1060000,NULL);
        INSERT INTO {session} VALUES ('s2','/work/other','s1',1000000,1060000,NULL);")).unwrap();
    conn
}

fn insert_assistant(conn: &Connection, modern: bool) {
    let message = if modern { "session_message" } else { "message" };
    let data = if modern {
        json!({"type":"assistant","model":{"providerID":"anthropic","id":"claude-sonnet-4-6"},"tokens":{"input":100,"output":20,"reasoning":5,"cache":{"read":40,"write":10}},"cost":0.012,"content":[{"type":"text","text":"Implemented the change"},{"type":"tool","name":"edit","state":{"input":{"filePath":"/work/example/src/lib.rs"}}},{"type":"reasoning","text":"plan"}],"time":{"created":1000000,"completed":1002000}})
    } else {
        json!({"role":"assistant","modelID":"claude-sonnet-4-6","providerID":"anthropic","tokens":{"input":100,"output":20,"reasoning":5,"cache":{"read":40,"write":10}},"cost":0.012,"time":{"created":1000000,"completed":1002000}})
    };
    conn.execute(
        &format!("INSERT INTO {message} VALUES (?1,'s1',1000000,?2)"),
        params!["m1", data.to_string()],
    )
    .unwrap();
    if !modern {
        for (id, part) in [
            ("p1", json!({"type":"text","text":"Implemented the change"})),
            (
                "p2",
                json!({"type":"tool","tool":"edit","state":{"input":{"filePath":"/work/example/src/lib.rs"}}}),
            ),
            ("p3", json!({"type":"reasoning","text":"plan"})),
        ] {
            conn.execute(
                "INSERT INTO part VALUES (?1,'m1',1000000,?2)",
                params![id, part.to_string()],
            )
            .unwrap();
        }
    }
}

#[test]
fn opencode_old_and_current_sqlite_schemas_have_independent_index_rows() {
    for modern in [false, true] {
        let tmp = TempDir::new().unwrap();
        let conn = fixture_database(tmp.path(), modern);
        insert_assistant(&conn, modern);
        let provider = OpenCodeProvider::at(tmp.path().to_path_buf());
        let files = provider.enumerate().unwrap();
        assert_eq!(files.len(), 2);
        assert_ne!(files[0].path, files[1].path);
        let record = provider.parse(&files[0]).unwrap();
        assert_eq!(record.messages[0].content, "Implemented the change");
        assert_eq!(record.project_display, "/work/example");
        assert_eq!(record.tool_names, vec!["edit"]);
        assert_eq!(record.file_paths_modified, vec!["/work/example/src/lib.rs"]);
        assert_eq!(record.embedded_cost, Some(0.012));
        assert_eq!(record.thinking_block_count, 1);
        let stats = &record.model_usage["anthropic/claude-sonnet-4-6"];
        assert_eq!(stats.usage.input_tokens, 100);
        assert_eq!(stats.usage.output_tokens, 25);
        assert_eq!(stats.usage.cache_read_tokens, 40);
        assert_eq!(stats.usage.cache_creation_tokens, 10);
        let before = provider.source_fingerprint(&files[0]).unwrap();
        conn.execute("INSERT INTO part VALUES ('new','m1',1000001,'{\"type\":\"text\",\"text\":\"additional text\"}')",[]).unwrap();
        assert_ne!(
            provider.source_fingerprint(&files[0]).unwrap(),
            before,
            "WAL-only writes must invalidate the index"
        );
        let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
        let providers = [Provider::OpenCode(provider)];
        assert_eq!(index.sync_now(&providers).unwrap(), 2);
        let sessions = index
            .query_sessions(None, None, &ResolvedFilter::default(), 100)
            .unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(
            index.sync_now(&providers).unwrap(),
            0,
            "unchanged DB should not reparse"
        );
        conn.execute("DELETE FROM part WHERE id='new'", []).unwrap();
        assert_eq!(
            index.sync_now(&providers).unwrap(),
            2,
            "WAL changes must refresh virtual session sources"
        );
    }
}

#[test]
fn opencode_legacy_json_discovers_parts_and_detects_independent_updates() {
    let tmp = TempDir::new().unwrap();
    let storage = tmp.path().join("storage");
    write(
        &storage.join("session/project/custom-name.json"),
        &json!({"id":"s1","directory":"/work/example","time":{"created":1000000,"updated":1060000}}),
    );
    write(
        &storage.join("message/s1/m1.json"),
        &json!({"id":"m1","role":"assistant","modelID":"local-model","providerID":"local","cost":0,"tokens":{"input":20,"output":10}}),
    );
    let part = storage.join("part/m1/p1.json");
    write(&part, &json!({"type":"text","text":"initial answer"}));
    let provider = OpenCodeProvider::at(tmp.path().to_path_buf());
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    let before = provider.source_fingerprint(&files[0]).unwrap();
    let record = provider.parse(&files[0]).unwrap();
    assert_eq!(record.messages[0].content, "initial answer");
    assert_eq!(record.embedded_cost, Some(0.0));
    write(
        &part,
        &json!({"type":"text","text":"updated and longer answer"}),
    );
    assert_ne!(before, provider.source_fingerprint(&files[0]).unwrap());
    assert_eq!(
        provider.parse(&files[0]).unwrap().messages[0].content,
        "updated and longer answer"
    );
}

#[test]
#[ignore = "opt-in read-only proof against installed Gemini and OpenCode sources"]
fn installed_additional_harness_smoke() {
    for provider in [
        Provider::Gemini(GeminiProvider::new().unwrap()),
        Provider::OpenCode(OpenCodeProvider::new().unwrap()),
    ] {
        if !provider.enabled() {
            continue;
        }
        let files = provider.enumerate().unwrap();
        let mut messages = 0;
        let mut models = std::collections::BTreeSet::new();
        for file in &files {
            let before = provider.source_fingerprint(file).unwrap();
            let record = provider.parse(file).unwrap();
            messages += record.message_count;
            models.extend(record.model_usage.keys().cloned());
            assert_eq!(
                before,
                provider.source_fingerprint(file).unwrap(),
                "parsing must never mutate source storage"
            );
        }
        let tmp = TempDir::new().unwrap();
        let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
        let id = provider.id();
        let count = index.sync_now(&[provider]).unwrap();
        assert_eq!(count, files.len());
        assert_eq!(
            index
                .query_sessions(None, None, &ResolvedFilter::default(), usize::MAX)
                .unwrap()
                .len(),
            files.len()
        );
        eprintln!(
            "{id}: parsed and independently indexed {} sessions, {messages} messages, {} model keys",
            files.len(),
            models.len()
        );
    }
}

#[test]
#[cfg(unix)]
fn opencode_symlink_database_aliases_share_virtual_identity() {
    let tmp = TempDir::new().unwrap();
    let real = tmp.path().join("real");
    fs::create_dir_all(&real).unwrap();
    let _conn = fixture_database(&real, false);
    let alias = tmp.path().join("alias");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let canonical = OpenCodeProvider::at(real).enumerate().unwrap();
    let aliased = OpenCodeProvider::at(alias).enumerate().unwrap();
    assert_eq!(
        canonical.iter().map(|f| &f.path).collect::<Vec<_>>(),
        aliased.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}

#[test]
fn opencode_additional_legacy_root_is_self_describing_for_export() {
    let tmp = TempDir::new().unwrap();
    let storage = tmp.path().join("storage");
    write(
        &storage.join("session/project/s1.json"),
        &json!({"id":"s1","directory":"/work/example"}),
    );
    write(
        &storage.join("message/s1/m1.json"),
        &json!({"id":"m1","role":"assistant","modelID":"model","cost":0}),
    );
    write(
        &storage.join("part/m1/p1.json"),
        &json!({"type":"text","text":"custom source answer"}),
    );
    let files = OpenCodeProvider::at(tmp.path().to_path_buf())
        .enumerate()
        .unwrap();
    let default = OpenCodeProvider::new().unwrap();
    assert_eq!(
        default.parse(&files[0]).unwrap().messages[0].content,
        "custom source answer"
    );
    assert!(default.source_fingerprint(&files[0]).unwrap().0 > 0);
}

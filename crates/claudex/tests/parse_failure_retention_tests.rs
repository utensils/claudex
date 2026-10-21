use claudex::filter::ResolvedFilter;
use claudex::index::IndexStore;
use claudex::providers::{GeminiProvider, Provider};
use serde_json::json;

#[test]
fn temporarily_invalid_transcript_preserves_indexed_data_and_recovers() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sessions");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("session-example.json");
    let valid = json!({
        "sessionId":"example",
        "messages":[
            {"type":"user","content":"Searchable retained message"},
            {"type":"gemini","model":"gemini-3.8-flash","tokens":{"input":1000,"output":10},"content":"Assistant response"}
        ]
    });
    std::fs::write(&path, valid.to_string()).unwrap();
    let provider = || Provider::Gemini(GeminiProvider::at(root.clone()));
    let mut index = IndexStore::open_at(&tmp.path().join("index.db")).unwrap();
    index.sync_now(&[provider()]).unwrap();
    let snapshot = || {
        let conn = rusqlite::Connection::open(tmp.path().join("index.db")).unwrap();
        let tokens: (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), SUM(input_tokens) FROM token_usage",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let messages: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages_fts", [], |row| row.get(0))
            .unwrap();
        (tokens, messages)
    };
    let original = snapshot();
    assert_eq!(original, ((1, 1000), 2));
    std::fs::write(&path, "{temporarily incomplete JSON").unwrap();
    index.sync_now(&[provider()]).unwrap();
    assert_eq!(snapshot(), original);
    let sessions = index
        .query_sessions(None, None, &ResolvedFilter::default(), 100)
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].present_on_disk);
    let mut updated = valid;
    updated["messages"][1]["tokens"]["input"] = json!(20000);
    std::fs::write(&path, updated.to_string()).unwrap();
    index.sync_now(&[provider()]).unwrap();
    assert_eq!(snapshot(), ((1, 20000), 2));
}

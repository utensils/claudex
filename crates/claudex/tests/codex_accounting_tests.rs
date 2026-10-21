use claudex::filter::ResolvedFilter;
use claudex::index::IndexStore;
use claudex::providers::{Provider, codex::CodexProvider};
use rusqlite::Connection;
use std::fs;

const ROOT: &str = "019e6c47-f2f4-7dc0-a22f-c053a99c0777";
const CHILD: &str = "019e6c6c-47bd-7d60-b5d4-34de398f1437";

fn transcript(id: &str, parent: Option<&str>, input: u64) -> String {
    let mut meta = serde_json::json!({"id":id,"cwd":"/repo"});
    if let Some(parent) = parent {
        meta["forked_from_id"] = parent.into();
    }
    [
        serde_json::json!({"type":"session_meta","payload":meta}),
        serde_json::json!({"type":"turn_context","payload":{"model":"gpt-5.6-sol"}}),
        serde_json::json!({"type":"event_msg","payload":{"type":"token_count","info":{
            "total_token_usage":{"input_tokens":9_000_000,"output_tokens":400_000},
            "last_token_usage":{"input_tokens":input,"cached_input_tokens":100,"output_tokens":50}
        }}}),
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect()
}

#[test]
fn archive_and_compression_moves_keep_row_identity_and_forks_additive() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::create_dir_all(root.join("archived_sessions")).unwrap();
    let path = root.join(format!("sessions/rollout-2026-05-27T00-00-00-{ROOT}.jsonl"));
    fs::write(&path, transcript(ROOT, None, 1000)).unwrap();
    let child_path = root.join(format!(
        "sessions/rollout-2026-05-27T00-01-00-{CHILD}.jsonl"
    ));
    // Some historical children contain a parent's metadata ID. The rollout
    // filename is the authoritative child identity, even with inherited totals.
    fs::write(&child_path, transcript(ROOT, Some(ROOT), 2000)).unwrap();
    let providers = [Provider::Codex(CodexProvider::at(root.to_path_buf()))];
    let db = root.join("index.db");
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&providers).unwrap();
    let conn = Connection::open(&db).unwrap();
    let rowid: i64 = conn
        .query_row(
            "SELECT id FROM sessions WHERE session_id = ?",
            [ROOT],
            |r| r.get(0),
        )
        .unwrap();
    let archived = root
        .join("archived_sessions")
        .join(path.file_name().unwrap());
    fs::rename(&path, &archived).unwrap();
    idx.sync_now(&providers).unwrap();
    let compressed = archived.with_extension("jsonl.zst");
    fs::write(
        &compressed,
        zstd::stream::encode_all(fs::read(&archived).unwrap().as_slice(), 1).unwrap(),
    )
    .unwrap();
    fs::remove_file(&archived).unwrap();
    idx.sync_now(&providers).unwrap();
    idx.ensure_pr_links_fresh(&providers).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sessions WHERE superseded_by IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row(
            "SELECT id FROM sessions WHERE session_id = ?",
            [ROOT],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        rowid
    );
    let cost = idx
        .query_cost_summary(None, &ResolvedFilter::default())
        .unwrap();
    assert_eq!(cost.input_tokens, 2800);
    assert_eq!(cost.cache_read_tokens, 200);
    assert_eq!(cost.output_tokens, 100);
    // A fresh index reads compressed archives without any retained plaintext.
    let mut fresh = IndexStore::open_at(&root.join("fresh.db")).unwrap();
    fresh.sync_now(&providers).unwrap();
    assert_eq!(
        fresh
            .query_cost_summary(None, &ResolvedFilter::default())
            .unwrap()
            .input_tokens,
        2800
    );
}

#[test]
fn retained_legacy_copies_reconcile_by_filename_without_collapsing_children() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("sessions")).unwrap();
    for (id, input) in [(ROOT, 1000), (CHILD, 2000)] {
        fs::write(
            root.join(format!("sessions/rollout-2026-05-27T00-00-00-{id}.jsonl")),
            transcript(id, None, input),
        )
        .unwrap();
    }
    let db = root.join("index.db");
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&[Provider::Codex(CodexProvider::at(root.to_path_buf()))])
        .unwrap();
    drop(idx);
    let conn = Connection::open(&db).unwrap();
    // Create a legacy archive copy and a child misidentified by copied metadata.
    conn.execute("INSERT INTO sessions (file_path,project_name,session_id,provider,indexed_at,present_on_disk,file_size,file_mtime) SELECT file_path || '.zst',project_name,session_id,provider,indexed_at,0,0,0 FROM sessions WHERE session_id = ?", [ROOT]).unwrap();
    conn.execute(
        "UPDATE sessions SET session_id = ?, source_key = NULL, present_on_disk = 0",
        [ROOT],
    )
    .unwrap();
    conn.execute("DELETE FROM meta WHERE key = 'codex_identity_revision'", [])
        .unwrap();
    conn.execute("INSERT INTO messages_fts (session_id,message_type,content,timestamp) SELECT id,'assistant','unique retained copy marker',0 FROM sessions WHERE file_path LIKE '%.zst'", []).unwrap();
    fs::remove_dir_all(root.join("sessions")).unwrap();
    let idx = IndexStore::open_at(&db).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM messages_fts WHERE content = 'unique retained copy marker'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );

    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sessions WHERE superseded_by IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        idx.query_cost_summary(None, &ResolvedFilter::default())
            .unwrap()
            .input_tokens,
        2800
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sessions WHERE session_id = ?",
            [CHILD],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn claude_mixed_speed_prices_only_tagged_tokens_and_survives_repricing() {
    use claudex::providers::claude::ClaudeProvider;
    use claudex::store::SessionStore;
    let temp = tempfile::tempdir().unwrap();
    let projects = temp.path().join("projects");
    fs::create_dir_all(projects.join("-repo")).unwrap();
    let path = projects.join("-repo/session.jsonl");
    let mut text = String::new();
    for speed in ["standard", "fast"] {
        text.push_str(&format!("{}\n", serde_json::json!({"type":"assistant","sessionId":"s","message":{
            "model":"claude-opus-5-5","usage":{"input_tokens":1_000_000,"output_tokens":1_000_000,"speed":speed},"content":[]
        }})));
    }
    fs::write(&path, text).unwrap();
    let db = temp.path().join("index.db");
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&[Provider::Claude(ClaudeProvider::at(SessionStore::at(
        projects,
    )))])
    .unwrap();
    let cost = idx
        .query_cost_summary(None, &ResolvedFilter::default())
        .unwrap()
        .cost_usd;
    assert!((cost - 72.0).abs() < 1e-9); // $24 standard + $48 fast
    let direct = claudex::parser::parse_session(&path).unwrap();
    assert!((direct.cost_usd() - cost).abs() < 1e-9);
    drop(idx);
    let conn = Connection::open(&db).unwrap();
    conn.execute("UPDATE token_usage SET cost_usd = 999", [])
        .unwrap();
    conn.execute(
        "UPDATE meta SET value = '0' WHERE key = 'pricing_revision'",
        [],
    )
    .unwrap();
    fs::remove_file(path).unwrap();
    let idx = IndexStore::open_at(&db).unwrap();
    assert!(
        (idx.query_cost_summary(None, &ResolvedFilter::default())
            .unwrap()
            .cost_usd
            - cost)
            .abs()
            < 1e-9
    );
}

#[test]
fn corrupt_or_stale_compressed_copy_cannot_hide_complete_plaintext() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::create_dir_all(root.join("archived_sessions")).unwrap();
    let filename = format!("rollout-2026-05-27T00-00-00-{ROOT}.jsonl");
    fs::write(
        root.join("sessions").join(&filename),
        transcript(ROOT, None, 2000),
    )
    .unwrap();
    let archive = root
        .join("archived_sessions")
        .join(&filename)
        .with_extension("jsonl.zst");
    fs::write(&archive, b"not a zstd stream").unwrap();
    let providers = [Provider::Codex(CodexProvider::at(root.to_path_buf()))];
    let mut idx = IndexStore::open_at(&root.join("index.db")).unwrap();
    idx.sync_now(&providers).unwrap();
    assert_eq!(
        idx.query_cost_summary(None, &ResolvedFilter::default())
            .unwrap()
            .input_tokens,
        1900
    );
    fs::write(
        &archive,
        zstd::stream::encode_all(transcript(ROOT, None, 1000).as_bytes(), 1).unwrap(),
    )
    .unwrap();
    idx.sync_now(&providers).unwrap();
    idx.sync_now(&providers).unwrap();
    assert_eq!(
        idx.query_cost_summary(None, &ResolvedFilter::default())
            .unwrap()
            .input_tokens,
        1900
    );
    assert_eq!(
        idx.query_sessions(None, None, &ResolvedFilter::default(), 10)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(idx.query_session_matches(ROOT, None).unwrap().len(), 1);
    let conn = Connection::open(root.join("index.db")).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn simultaneous_archive_copies_cannot_overwrite_one_another() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::create_dir_all(root.join("archived_sessions")).unwrap();
    let filename = format!("rollout-2026-05-27T00-00-00-{ROOT}.jsonl");
    let live = root.join("sessions").join(&filename);
    fs::write(&live, transcript(ROOT, None, 2000)).unwrap();
    let providers = [Provider::Codex(CodexProvider::at(root.to_path_buf()))];
    let mut idx = IndexStore::open_at(&root.join("index.db")).unwrap();
    idx.sync_now(&providers).unwrap();
    let archive = root.join("archived_sessions").join(&filename);
    fs::rename(&live, &archive).unwrap();
    fs::write(
        archive.with_extension("jsonl.zst"),
        zstd::stream::encode_all(transcript(ROOT, None, 1000).as_bytes(), 1).unwrap(),
    )
    .unwrap();
    idx.sync_now(&providers).unwrap();
    assert_eq!(
        idx.query_cost_summary(None, &ResolvedFilter::default())
            .unwrap()
            .input_tokens,
        1900
    );
}

#[test]
fn stale_archive_import_keeps_larger_retained_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::create_dir_all(root.join("archived_sessions")).unwrap();
    let filename = format!("rollout-2026-05-27T00-00-00-{ROOT}.jsonl");
    let live = root.join("sessions").join(&filename);
    fs::write(&live, transcript(ROOT, None, 2000)).unwrap();
    let providers = [Provider::Codex(CodexProvider::at(root.to_path_buf()))];
    let mut idx = IndexStore::open_at(&root.join("index.db")).unwrap();
    idx.sync_now(&providers).unwrap();
    fs::remove_file(&live).unwrap();
    fs::write(
        root.join("archived_sessions").join(filename),
        transcript(ROOT, None, 1000),
    )
    .unwrap();
    idx.sync_now(&providers).unwrap();
    let on_disk = ResolvedFilter {
        on_disk_only: true,
        ..Default::default()
    };
    assert_eq!(
        idx.query_sessions(None, None, &on_disk, 10).unwrap().len(),
        0
    );
    assert_eq!(
        idx.query_cost_summary(None, &ResolvedFilter::default())
            .unwrap()
            .input_tokens,
        1900
    );
}

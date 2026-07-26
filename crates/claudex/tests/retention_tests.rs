//! Tests for the additive/retentive index: non-destructive schema migration,
//! soft-delete retention of vanished files, in-place re-index with a stable
//! rowid, and un-archival of restored files.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use claudex::index::IndexStore;
use claudex::providers::{ClaudeProvider, Provider};
use claudex::store::SessionStore;
use rusqlite::Connection;
use tempfile::TempDir;

/// Wrap a `projects` directory in the single-Claude-provider set the sync
/// methods expect.
fn claude_providers(projects: PathBuf) -> Vec<Provider> {
    vec![Provider::Claude(ClaudeProvider::at(SessionStore::at(
        projects,
    )))]
}

fn write_session(
    projects: &Path,
    encoded_project: &str,
    session: &str,
    lines: &[String],
) -> PathBuf {
    let dir = projects.join(encoded_project);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{session}.jsonl"));
    let mut f = fs::File::create(&path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    f.flush().unwrap();
    path
}

fn user(session: &str, ts: &str, text: &str) -> String {
    format!(
        r#"{{"type":"user","sessionId":"{session}","timestamp":"{ts}","message":{{"content":"{text}"}}}}"#
    )
}

fn assistant(session: &str, ts: &str) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{session}","timestamp":"{ts}","message":{{"model":"claude-opus-4-6","stop_reason":"end_turn","usage":{{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":200,"cache_read_input_tokens":5000}},"content":[{{"type":"text","text":"ok"}}]}}}}"#
    )
}

/// Open a throwaway read connection to the on-disk index for assertions.
fn read_db(db: &Path) -> Connection {
    Connection::open(db).unwrap()
}

fn retention(db: &Path, session_id: &str) -> Option<(i64, i64, Option<i64>)> {
    let conn = read_db(db);
    conn.query_row(
        "SELECT id, present_on_disk, archived_at FROM sessions WHERE session_id = ?",
        [session_id],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        },
    )
    .ok()
}

fn count(db: &Path, sql: &str, param: &str) -> i64 {
    let conn = read_db(db);
    conn.query_row(sql, [param], |r| r.get::<_, i64>(0))
        .unwrap()
}

#[test]
fn migration_from_v4_preserves_data() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("index.db");

    // Hand-build a v4 database: the pre-retention `sessions` shape (no provider
    // / present_on_disk / archived_at / last_seen / extras columns), one
    // session row with a token_usage child and an FTS row, stamped v4.
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE sessions (
                id INTEGER PRIMARY KEY, project_name TEXT NOT NULL,
                file_path TEXT NOT NULL UNIQUE, file_size INTEGER NOT NULL,
                file_mtime INTEGER NOT NULL, session_id TEXT, parent_session_id TEXT,
                first_timestamp INTEGER, last_timestamp INTEGER,
                duration_ms INTEGER NOT NULL DEFAULT 0, message_count INTEGER NOT NULL DEFAULT 0,
                model TEXT, indexed_at INTEGER NOT NULL
            );
            CREATE TABLE token_usage (
                session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                model TEXT, assistant_message_count INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0, cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cost_usd REAL NOT NULL DEFAULT 0.0, inference_geo TEXT, speed REAL,
                service_tier TEXT, iterations INTEGER NOT NULL DEFAULT 0
            );
            CREATE VIRTUAL TABLE messages_fts USING fts5(
                session_id UNINDEXED, message_type, content, timestamp UNINDEXED,
                tokenize = 'porter unicode61'
            );
            INSERT INTO sessions (project_name, file_path, file_size, file_mtime, session_id, first_timestamp, last_timestamp, model, indexed_at)
                VALUES ('/Users/test/Projects/legacy', '/tmp/legacy/sess-old.jsonl', 10, 20, 'sess-old', 100, 200, 'claude-opus-4-6', 50);
            INSERT INTO token_usage (session_id, model, input_tokens, output_tokens, cost_usd)
                VALUES (1, 'claude-opus-4-6', 1234, 567, 1.5);
            INSERT INTO messages_fts (session_id, message_type, content, timestamp)
                VALUES (1, 'user', 'legacy message content', 100);
            INSERT INTO meta (key, value) VALUES ('schema_version', '4');
            "#,
        )
        .unwrap();
    }

    // Opening through IndexStore must MIGRATE (not drop): add the new columns,
    // bump the version, and keep every pre-existing row.
    let _idx = IndexStore::open_at(&db).unwrap();

    let conn = read_db(&db);
    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, "7", "schema version should be migrated to 7");

    // The retained row survives, with the new columns defaulted.
    let row = retention(&db, "sess-old").expect("legacy session must survive migration");
    assert_eq!(row.1, 1, "migrated rows default to present_on_disk = 1");
    let provider: String = conn
        .query_row(
            "SELECT provider FROM sessions WHERE session_id='sess-old'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        provider, "claude",
        "migrated rows default provider to claude"
    );

    // Child + FTS data is intact.
    assert_eq!(
        count(
            &db,
            "SELECT input_tokens FROM token_usage WHERE session_id = ?",
            "1"
        ),
        1234
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM messages_fts WHERE content MATCH ?",
            "legacy"
        ),
        1
    );

    // The v6 migration adds `cost_source` and the automatic reprice corrects the
    // stale stored cost (seeded at $1.50) using current Opus 4.6 rates:
    // (1234*5 + 567*25) / 1e6 = $0.020345.
    let (cost, source) = token_cost(&db, "sess-old");
    assert_eq!(source, "computed", "claude rows are repriceable");
    assert!(
        (cost - 0.020345).abs() < 1e-9,
        "stale cost should be repriced, got {cost}"
    );
}

#[test]
fn deleted_file_is_retained_and_archived() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    let db = tmp.path().join("index.db");

    let keep = write_session(
        &projects,
        "-Users-test-Projects-keep",
        "sess-keep",
        &[
            user("sess-keep", "2026-04-10T10:00:00Z", "keep me"),
            assistant("sess-keep", "2026-04-10T10:01:00Z"),
        ],
    );
    let gone = write_session(
        &projects,
        "-Users-test-Projects-gone",
        "sess-gone",
        &[
            user(
                "sess-gone",
                "2026-04-11T10:00:00Z",
                "searchable gone content",
            ),
            assistant("sess-gone", "2026-04-11T10:01:00Z"),
        ],
    );

    let providers = claude_providers(projects.clone());
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&providers).unwrap();

    let before = retention(&db, "sess-gone").expect("gone session indexed");
    assert_eq!(before.1, 1, "freshly indexed file is present");
    assert!(before.2.is_none(), "not archived while on disk");

    // Delete one transcript from disk and re-sync.
    fs::remove_file(&gone).unwrap();
    idx.sync_now(&providers).unwrap();

    // The vanished session is RETAINED, flagged not-present and archived.
    let after = retention(&db, "sess-gone").expect("gone session must be retained, not purged");
    assert_eq!(after.0, before.0, "retained row keeps its rowid");
    assert_eq!(after.1, 0, "vanished file flagged present_on_disk = 0");
    assert!(after.2.is_some(), "vanished file gets an archived_at stamp");

    // Its derived rows and FTS content are still queryable.
    assert!(
        count(
            &db,
            "SELECT COUNT(*) FROM token_usage WHERE session_id = ?",
            &after.0.to_string()
        ) >= 1,
        "token_usage retained for archived session"
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM messages_fts WHERE content MATCH ?",
            "searchable"
        ),
        1,
        "FTS content retained for archived session"
    );

    // The surviving file is untouched.
    let kept = retention(&db, "sess-keep").unwrap();
    assert_eq!(kept.1, 1);
    assert!(keep.exists());
}

#[test]
fn changed_file_reuses_rowid_without_orphan_fts() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    let db = tmp.path().join("index.db");

    let path = write_session(
        &projects,
        "-Users-test-Projects-edit",
        "sess-edit",
        &[
            user(
                "sess-edit",
                "2026-04-10T10:00:00Z",
                "original alpha content",
            ),
            assistant("sess-edit", "2026-04-10T10:01:00Z"),
        ],
    );

    let providers = claude_providers(projects.clone());
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&providers).unwrap();
    let original_rowid = retention(&db, "sess-edit").unwrap().0;

    // Rewrite the file with new content and a bumped mtime.
    let mut f = fs::File::create(&path).unwrap();
    writeln!(
        f,
        "{}",
        user("sess-edit", "2026-04-10T10:00:00Z", "revised beta content")
    )
    .unwrap();
    writeln!(f, "{}", assistant("sess-edit", "2026-04-10T10:05:00Z")).unwrap();
    f.flush().unwrap();
    drop(f);
    // Ensure mtime differs even on coarse-resolution filesystems.
    filetime_bump(&path);
    idx.sync_now(&providers).unwrap();

    // Same rowid (in-place re-index), fresh content, no stale FTS rows.
    let after = retention(&db, "sess-edit").unwrap();
    assert_eq!(
        after.0, original_rowid,
        "changed file re-indexes in place under a stable rowid"
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM messages_fts WHERE content MATCH ?",
            "original"
        ),
        0,
        "old FTS content must be gone (no orphan rows)"
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM messages_fts WHERE content MATCH ?",
            "revised"
        ),
        1,
        "new FTS content present"
    );
    // No duplicate token_usage rows accumulated across re-indexes.
    let tu = count(
        &db,
        "SELECT COUNT(*) FROM token_usage WHERE session_id = ?",
        &original_rowid.to_string(),
    );
    assert_eq!(tu, 1, "exactly one token_usage row after re-index");
}

#[test]
fn restored_identical_file_is_unarchived() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    let db = tmp.path().join("index.db");

    let lines = [
        user("sess-rt", "2026-04-10T10:00:00Z", "round trip"),
        assistant("sess-rt", "2026-04-10T10:01:00Z"),
    ];
    let path = write_session(&projects, "-Users-test-Projects-rt", "sess-rt", &lines);

    let providers = claude_providers(projects.clone());
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&providers).unwrap();

    // Capture exact bytes + mtime so the restored file is byte-and-stat identical.
    let bytes = fs::read(&path).unwrap();
    let meta = fs::metadata(&path).unwrap();
    let mtime = filetime_of(&meta);

    fs::remove_file(&path).unwrap();
    idx.sync_now(&providers).unwrap();
    assert_eq!(
        retention(&db, "sess-rt").unwrap().1,
        0,
        "archived after delete"
    );

    // Restore identical content + mtime; sync must un-archive it.
    fs::write(&path, &bytes).unwrap();
    set_mtime(&path, mtime);
    idx.sync_now(&providers).unwrap();
    let restored = retention(&db, "sess-rt").unwrap();
    assert_eq!(restored.1, 1, "restored file flagged present_on_disk = 1");
    assert!(restored.2.is_none(), "restored file clears archived_at");
}

// --- small mtime helpers (avoid pulling in the `filetime` crate) ---

fn filetime_of(meta: &fs::Metadata) -> std::time::SystemTime {
    meta.modified().unwrap()
}

fn set_mtime(path: &Path, when: std::time::SystemTime) {
    let f = fs::File::options().write(true).open(path).unwrap();
    f.set_modified(when).unwrap();
}

fn filetime_bump(path: &Path) {
    let meta = fs::metadata(path).unwrap();
    let bumped = meta.modified().unwrap() + std::time::Duration::from_secs(5);
    set_mtime(path, bumped);
}

// --- v5 → v6 automatic reprice (Layer B/C) ---

/// The v5 (pre-cost_source) schema, hand-built so we can stamp stale costs and
/// assert the migration + automatic reprice corrects them.
const V5_SCHEMA: &str = r#"
    CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE sessions (
        id INTEGER PRIMARY KEY, project_name TEXT NOT NULL,
        file_path TEXT NOT NULL UNIQUE, file_size INTEGER NOT NULL,
        file_mtime INTEGER NOT NULL, session_id TEXT, parent_session_id TEXT,
        first_timestamp INTEGER, last_timestamp INTEGER,
        duration_ms INTEGER NOT NULL DEFAULT 0, message_count INTEGER NOT NULL DEFAULT 0,
        model TEXT, indexed_at INTEGER NOT NULL,
        provider TEXT NOT NULL DEFAULT 'claude', present_on_disk INTEGER NOT NULL DEFAULT 1,
        archived_at INTEGER, last_seen INTEGER, extras TEXT
    );
    CREATE TABLE token_usage (
        session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        model TEXT, assistant_message_count INTEGER NOT NULL DEFAULT 0,
        input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
        cache_creation_tokens INTEGER NOT NULL DEFAULT 0, cache_read_tokens INTEGER NOT NULL DEFAULT 0,
        cost_usd REAL NOT NULL DEFAULT 0.0, inference_geo TEXT, speed REAL,
        service_tier TEXT, iterations INTEGER NOT NULL DEFAULT 0
    );
    CREATE VIRTUAL TABLE messages_fts USING fts5(
        session_id UNINDEXED, message_type, content, timestamp UNINDEXED,
        tokenize = 'porter unicode61'
    );
    INSERT INTO meta (key, value) VALUES ('schema_version', '5');
"#;

/// Seed a v5 DB with one session + one token_usage row each.
/// Row tuple: `(session_id, provider, model, input_tokens, output_tokens, cost_usd)`.
fn seed_v5_db(db: &Path, rows: &[(&str, &str, &str, i64, i64, f64)]) {
    let conn = Connection::open(db).unwrap();
    conn.execute_batch(V5_SCHEMA).unwrap();
    for (i, (sid, provider, model, input, output, cost)) in rows.iter().enumerate() {
        let id = (i + 1) as i64;
        conn.execute(
            "INSERT INTO sessions
             (id, project_name, file_path, file_size, file_mtime, session_id, indexed_at, provider)
             VALUES (?, ?, ?, 1, 1, ?, 1, ?)",
            rusqlite::params![
                id,
                format!("/p/{sid}"),
                format!("/tmp/{sid}.jsonl"),
                sid,
                provider
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO token_usage (session_id, model, input_tokens, output_tokens, cost_usd)
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![id, model, input, output, cost],
        )
        .unwrap();
    }
}

/// `(cost_usd, cost_source)` of a session's single token_usage row.
fn token_cost(db: &Path, session_id: &str) -> (f64, String) {
    let conn = read_db(db);
    conn.query_row(
        "SELECT t.cost_usd, t.cost_source FROM token_usage t
         JOIN sessions s ON s.id = t.session_id WHERE s.session_id = ?",
        [session_id],
        |r| Ok((r.get::<_, f64>(0)?, r.get::<_, String>(1)?)),
    )
    .unwrap()
}

fn meta_val(db: &Path, key: &str) -> Option<String> {
    let conn = read_db(db);
    conn.query_row("SELECT value FROM meta WHERE key = ?", [key], |r| {
        r.get::<_, String>(0)
    })
    .ok()
}

#[test]
fn reprice_corrects_stale_computed_costs() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("index.db");

    // A Claude session priced by an old binary at the pre-4.5 Opus rate.
    // 1M input @ current Opus = $5.00; the stale stored value is $15.00.
    seed_v5_db(
        &db,
        &[(
            "sess-claude",
            "claude",
            "claude-opus-4-6",
            1_000_000,
            0,
            15.0,
        )],
    );

    let _idx = IndexStore::open_at(&db).unwrap();

    let (cost, source) = token_cost(&db, "sess-claude");
    assert_eq!(source, "computed");
    assert!(
        (cost - 5.0).abs() < 1e-9,
        "expected repriced $5.00, got {cost}"
    );
    assert_eq!(meta_val(&db, "schema_version").as_deref(), Some("7"));
    assert_eq!(
        meta_val(&db, "pricing_revision"),
        Some(claudex::index::PRICING_REVISION.to_string())
    );
}

#[test]
fn reprice_applies_sonnet_5_introductory_rates() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("index.db");

    // A row ingested before Sonnet 5 had its dedicated introductory card.
    seed_v5_db(
        &db,
        &[(
            "sess-sonnet-5",
            "claude",
            "claude-sonnet-5",
            1_000_000,
            0,
            3.0,
        )],
    );

    let _idx = IndexStore::open_at(&db).unwrap();

    let (cost, source) = token_cost(&db, "sess-sonnet-5");
    assert_eq!(source, "computed");
    assert!(
        (cost - 2.0).abs() < 1e-9,
        "expected repriced $2.00, got {cost}"
    );
    assert_eq!(
        meta_val(&db, "pricing_revision"),
        Some(claudex::index::PRICING_REVISION.to_string())
    );
}

#[test]
fn reprice_corrects_open_source_models_mispriced_as_sonnet() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("index.db");

    seed_v5_db(
        &db,
        &[
            (
                "sess-qwen",
                "claude",
                "qwen3.6-35b-a3b-ud-mlx",
                1_000_000,
                1_000_000,
                18.0,
            ),
            (
                "sess-ollama",
                "claude",
                "ollama/gemma4:31b",
                1_000_000,
                1_000_000,
                18.0,
            ),
        ],
    );

    let _idx = IndexStore::open_at(&db).unwrap();

    let (qwen, qwen_src) = token_cost(&db, "sess-qwen");
    let (gemma, gemma_src) = token_cost(&db, "sess-ollama");
    assert_eq!(qwen_src, "computed");
    assert_eq!(gemma_src, "computed");
    assert_eq!(qwen, 0.0, "open-weight qwen is not Sonnet-priced");
    assert_eq!(gemma, 0.0, "Ollama Gemma is not Sonnet-priced");
    assert_eq!(
        meta_val(&db, "pricing_revision"),
        Some(claudex::index::PRICING_REVISION.to_string())
    );
}

#[test]
fn reprice_never_clobbers_provider_costs() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("index.db");

    // Pi rows carry the provider's own cost: a free local model ($0) and a
    // billed remote one ($1.23). Neither must be recomputed from the tier table.
    seed_v5_db(
        &db,
        &[
            ("sess-local", "pi", "ollama/llama3", 1_000_000, 0, 0.0),
            ("sess-remote", "pi", "claude-opus-4-6", 1_000_000, 0, 1.23),
        ],
    );

    let _idx = IndexStore::open_at(&db).unwrap();

    let (local, local_src) = token_cost(&db, "sess-local");
    let (remote, remote_src) = token_cost(&db, "sess-remote");
    assert_eq!(
        local_src, "provider",
        "pi rows flagged provider by migration"
    );
    assert_eq!(remote_src, "provider");
    assert_eq!(local, 0.0, "free local model stays $0");
    assert!(
        (remote - 1.23).abs() < 1e-9,
        "provider-billed cost preserved, got {remote}"
    );
}

#[test]
fn reprice_runs_once_per_revision() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("index.db");

    seed_v5_db(
        &db,
        &[(
            "sess-claude",
            "claude",
            "claude-opus-4-6",
            1_000_000,
            0,
            15.0,
        )],
    );

    // First open reprices to $5.00 and stamps the current pricing_revision.
    let _idx = IndexStore::open_at(&db).unwrap();
    assert!((token_cost(&db, "sess-claude").0 - 5.0).abs() < 1e-9);

    // Tamper the stored cost; a second open must NOT reprice again (revision is
    // already current), so the sentinel survives.
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute("UPDATE token_usage SET cost_usd = 999.0", [])
            .unwrap();
    }
    let _idx2 = IndexStore::open_at(&db).unwrap();
    assert_eq!(
        token_cost(&db, "sess-claude").0,
        999.0,
        "reprice must be a one-off per PRICING_REVISION bump"
    );
}

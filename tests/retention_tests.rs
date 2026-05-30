//! Tests for the additive/retentive index: non-destructive schema migration,
//! soft-delete retention of vanished files, in-place re-index with a stable
//! rowid, and un-archival of restored files.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use claudex::index::IndexStore;
use claudex::store::SessionStore;
use rusqlite::Connection;
use tempfile::TempDir;

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
    assert_eq!(version, "5", "schema version should be migrated to 5");

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

    let store = SessionStore::at(projects.clone());
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&store).unwrap();

    let before = retention(&db, "sess-gone").expect("gone session indexed");
    assert_eq!(before.1, 1, "freshly indexed file is present");
    assert!(before.2.is_none(), "not archived while on disk");

    // Delete one transcript from disk and re-sync.
    fs::remove_file(&gone).unwrap();
    idx.sync_now(&store).unwrap();

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

    let store = SessionStore::at(projects.clone());
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&store).unwrap();
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
    idx.sync_now(&store).unwrap();

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

    let store = SessionStore::at(projects.clone());
    let mut idx = IndexStore::open_at(&db).unwrap();
    idx.sync_now(&store).unwrap();

    // Capture exact bytes + mtime so the restored file is byte-and-stat identical.
    let bytes = fs::read(&path).unwrap();
    let meta = fs::metadata(&path).unwrap();
    let mtime = filetime_of(&meta);

    fs::remove_file(&path).unwrap();
    idx.sync_now(&store).unwrap();
    assert_eq!(
        retention(&db, "sess-rt").unwrap().1,
        0,
        "archived after delete"
    );

    // Restore identical content + mtime; sync must un-archive it.
    fs::write(&path, &bytes).unwrap();
    set_mtime(&path, mtime);
    idx.sync_now(&store).unwrap();
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

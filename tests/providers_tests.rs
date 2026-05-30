//! Unit tests for the provider abstraction. These pin the Claude provider's
//! `enumerate`/`parse` behavior so the move behind the `SessionProvider` trait
//! stays faithful to the original indexing logic.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use claudex::providers::{ClaudeProvider, DiscoveredFile, SessionProvider};
use claudex::store::SessionStore;
use tempfile::TempDir;

fn write_session(projects: &Path, encoded_project: &str, session: &str, lines: &[&str]) -> PathBuf {
    let dir = projects.join(encoded_project);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{session}.jsonl"));
    write_jsonl(&path, lines)
}

fn write_jsonl(path: &Path, lines: &[&str]) -> PathBuf {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = fs::File::create(path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    f.flush().unwrap();
    path.to_path_buf()
}

fn provider_at(projects: &Path) -> ClaudeProvider {
    ClaudeProvider::at(SessionStore::at(projects.to_path_buf()))
}

fn find<'a>(files: &'a [DiscoveredFile], session: &str) -> &'a DiscoveredFile {
    files
        .iter()
        .find(|f| f.path.file_stem().unwrap().to_string_lossy() == session)
        .unwrap_or_else(|| panic!("{session} not discovered"))
}

#[test]
fn id_and_root_reflect_the_session_store() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    fs::create_dir_all(&projects).unwrap();
    let provider = provider_at(&projects);
    assert_eq!(provider.id(), "claude");
    assert_eq!(provider.root_dir(), projects.as_path());
    assert!(provider.enabled(), "existing root is enabled");
}

#[test]
fn enumerate_decodes_projects_and_marks_files_present() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    write_session(
        &projects,
        "-Users-test-Projects-alpha",
        "sess-a1",
        &[
            r#"{"type":"user","sessionId":"sess-a1","timestamp":"2026-04-10T10:00:00Z","message":{"content":"hi"}}"#,
        ],
    );

    let files = provider_at(&projects).enumerate().unwrap();
    assert_eq!(files.len(), 1);
    let f = &files[0];
    assert_eq!(f.project_display, "/Users/test/Projects/alpha");
    assert!(!f.archived, "Claude transcripts are never archive-sourced");
    assert!(
        f.parent_session_id.is_none(),
        "top-level session has no parent"
    );
}

#[test]
fn enumerate_attaches_parent_session_id_to_subagent_transcripts() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    let encoded = "-Users-test-Projects-agents";

    write_session(
        &projects,
        encoded,
        "parent-1",
        &[
            r#"{"type":"user","sessionId":"parent-1","timestamp":"2026-04-10T10:00:00Z","message":{"content":"go"}}"#,
        ],
    );
    write_jsonl(
        &projects
            .join(encoded)
            .join("parent-1/subagents/workflows/run-1/agent-child.jsonl"),
        &[
            r#"{"type":"user","isSidechain":true,"sessionId":"child-1","timestamp":"2026-04-10T10:02:00Z","message":{"content":"sub"}}"#,
        ],
    );

    let files = provider_at(&projects).enumerate().unwrap();
    assert_eq!(files.len(), 2);
    let child = find(&files, "agent-child");
    assert_eq!(
        child.parent_session_id.as_deref(),
        Some("parent-1"),
        "subagent transcript rolls up to its parent session"
    );
    let parent = find(&files, "parent-1");
    assert!(parent.parent_session_id.is_none());
}

#[test]
fn parse_extracts_tokens_tools_thinking_and_fts_content() {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    write_session(
        &projects,
        "-Users-test-Projects-alpha",
        "sess-a1",
        &[
            r#"{"type":"user","sessionId":"sess-a1","timestamp":"2026-04-10T10:00:00Z","message":{"content":"hello world"}}"#,
            r#"{"type":"assistant","sessionId":"sess-a1","timestamp":"2026-04-10T10:01:00Z","message":{"model":"claude-opus-4-6","stop_reason":"end_turn","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":200,"cache_read_input_tokens":5000},"content":[{"type":"tool_use","name":"Bash","id":"t1","input":{}},{"type":"thinking","text":"hmm"},{"type":"text","text":"done"}]}}"#,
        ],
    );

    let provider = provider_at(&projects);
    let files = provider.enumerate().unwrap();
    let record = provider.parse(&files[0]).unwrap();

    assert_eq!(record.session_id.as_deref(), Some("sess-a1"));
    assert_eq!(record.model.as_deref(), Some("claude-opus-4-6"));
    assert_eq!(record.usage.input_tokens, 1000);
    assert_eq!(record.usage.output_tokens, 500);
    assert_eq!(record.usage.cache_creation_tokens, 200);
    assert_eq!(record.usage.cache_read_tokens, 5000);
    assert_eq!(record.tool_names, vec!["Bash".to_string()]);
    assert_eq!(record.thinking_block_count, 1);
    assert_eq!(*record.stop_reason_counts.get("end_turn").unwrap(), 1);
    // One user message + one assistant text message captured for FTS.
    assert_eq!(record.messages.len(), 2);
    assert!(
        record
            .messages
            .iter()
            .any(|m| m.content.contains("hello world")),
        "user content indexed"
    );
    assert!(
        record.messages.iter().any(|m| m.content.contains("done")),
        "assistant text indexed"
    );
    // Provider-derived fields default empty (Claude derives cost from a table).
    assert!(record.embedded_cost.is_none());
    assert!(record.extras.is_none());
}

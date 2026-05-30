//! Unit tests for the provider abstraction. These pin the Claude provider's
//! `enumerate`/`parse` behavior so the move behind the `SessionProvider` trait
//! stays faithful to the original indexing logic.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use claudex::providers::{
    ClaudeProvider, CodexProvider, DiscoveredFile, PiProvider, SessionProvider,
};
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

// --- Codex provider ---

fn write_lines(path: &Path, lines: &[&str]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = fs::File::create(path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    f.flush().unwrap();
}

#[test]
fn codex_enumerate_flags_archived_and_parse_reads_cwd_tokens_tools() {
    let tmp = TempDir::new().unwrap();
    let codex = tmp.path().join(".codex");
    write_lines(
        &codex.join("sessions/2026/05/05/rollout-2026-05-05T00-00-00-codex-a.jsonl"),
        &[
            r#"{"timestamp":"2026-05-05T00:00:00Z","type":"session_meta","payload":{"id":"codex-a","cwd":"/repo","cli_version":"0.99.0"}}"#,
            r#"{"timestamp":"2026-05-05T00:00:30Z","type":"turn_context","payload":{"model":"gpt-5-codex"}}"#,
            r#"{"timestamp":"2026-05-05T00:01:00Z","type":"response_item","payload":{"type":"function_call","name":"shell","call_id":"c"}}"#,
            r#"{"timestamp":"2026-05-05T00:01:30Z","type":"event_msg","payload":{"type":"agent_reasoning","text":"thinking"}}"#,
            r#"{"timestamp":"2026-05-05T00:02:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5}}}}"#,
            r#"{"timestamp":"2026-05-05T00:03:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":200,"output_tokens":500}}}}"#,
        ],
    );
    write_lines(
        &codex.join("archived_sessions/rollout-2026-01-01T00-00-00-codex-b.jsonl"),
        &[
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"id":"codex-b","cwd":"/old"}}"#,
        ],
    );

    let provider = CodexProvider::at(codex);
    assert_eq!(provider.id(), "codex");
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 2);

    let active = files.iter().find(|f| !f.archived).unwrap();
    let archived = files.iter().find(|f| f.archived).unwrap();
    assert!(archived.archived, "archived_sessions files are flagged");

    let rec = provider.parse(active).unwrap();
    assert_eq!(rec.session_id.as_deref(), Some("codex-a"));
    assert_eq!(rec.project_display, "/repo");
    assert_eq!(rec.model.as_deref(), Some("gpt-5-codex"));
    // Cumulative: last total wins; cached input becomes a cache read.
    assert_eq!(rec.usage.input_tokens, 800);
    assert_eq!(rec.usage.cache_read_tokens, 200);
    assert_eq!(rec.usage.output_tokens, 500);
    assert_eq!(rec.tool_names, vec!["shell".to_string()]);
    assert_eq!(rec.thinking_block_count, 1);
    assert!(rec.extras.as_deref().unwrap().contains("0.99.0"));

    let arch = provider.parse(archived).unwrap();
    assert_eq!(arch.project_display, "/old");
}

// --- Pi provider ---

#[test]
fn pi_enumerate_decodes_cwd_and_parse_uses_embedded_cost() {
    let tmp = TempDir::new().unwrap();
    let agent = tmp.path().join(".pi/agent");
    write_lines(
        &agent.join("sessions/--Users-me-Projects-foo--/2026-05-13T22-05-15-161Z_sess-pi.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"sess-pi","timestamp":"2026-05-13T22:05:15.161Z","cwd":"/Users/me/Projects/foo"}"#,
            r#"{"type":"model_change","provider":"anthropic","modelId":"claude-3-opus","timestamp":"2026-05-13T22:05:16Z"}"#,
            r#"{"type":"message","id":"u1","timestamp":"2026-05-13T22:05:35Z","message":{"role":"user","content":[{"type":"text","text":"explore the repo"}]}}"#,
            r#"{"type":"message","id":"a1","timestamp":"2026-05-13T22:05:52Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c1","name":"read"},{"type":"thinking","text":"hmm"},{"type":"text","text":"on it"}],"provider":"anthropic","model":"claude-3-opus","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"cost":{"total":0.42}},"stopReason":"toolUse"}}"#,
        ],
    );

    let provider = PiProvider::at(agent);
    assert_eq!(provider.id(), "pi");
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    // cwd decoded from the directory name as a fallback…
    assert_eq!(files[0].project_display, "/Users/me/Projects/foo");

    let rec = provider.parse(&files[0]).unwrap();
    assert_eq!(rec.session_id.as_deref(), Some("sess-pi"));
    // …and confirmed from the session record.
    assert_eq!(rec.project_display, "/Users/me/Projects/foo");
    assert_eq!(rec.tool_names, vec!["read".to_string()]);
    assert_eq!(rec.thinking_block_count, 1);
    // Pi supplies its own cost, which the index trusts verbatim.
    assert_eq!(rec.embedded_cost, Some(0.42));
    let model_stats = rec.model_usage.get("anthropic/claude-3-opus").unwrap();
    assert_eq!(model_stats.usage.input_tokens, 100);
    assert_eq!(model_stats.usage.cache_read_tokens, 10);
    assert_eq!(model_stats.embedded_cost, Some(0.42));
    assert_eq!(*rec.stop_reason_counts.get("toolUse").unwrap(), 1);
}

#[test]
fn pi_local_ollama_session_reports_zero_cost() {
    let tmp = TempDir::new().unwrap();
    let agent = tmp.path().join(".pi/agent");
    write_lines(
        &agent.join("sessions/--repo--/2026-05-13T22-05-15-161Z_sess-ollama.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"sess-ollama","timestamp":"2026-05-13T22:05:15Z","cwd":"/repo"}"#,
            r#"{"type":"message","id":"a1","timestamp":"2026-05-13T22:05:52Z","message":{"role":"assistant","content":[{"type":"text","text":"local"}],"provider":"ollama","model":"qwen3","usage":{"input":6000,"output":120,"cacheRead":0,"cacheWrite":0,"cost":{"total":0}},"stopReason":"stop"}}"#,
        ],
    );
    let provider = PiProvider::at(agent);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();
    // Local model has real tokens but zero cost — trusted verbatim, not priced.
    assert_eq!(rec.embedded_cost, Some(0.0));
    let stats = rec.model_usage.get("ollama/qwen3").unwrap();
    assert_eq!(stats.usage.input_tokens, 6000);
    assert_eq!(stats.embedded_cost, Some(0.0));
}

//! Unit tests for the provider abstraction. These pin the Claude provider's
//! `enumerate`/`parse` behavior so the move behind the `SessionProvider` trait
//! stays faithful to the original indexing logic.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use claudex::providers::claude::parse_claude_session;
use claudex::providers::{
    ClaudeProvider, CodexProvider, CopilotProvider, CopilotVscodeProvider, DiscoveredFile,
    OpenClawProvider, PiProvider, SessionProvider,
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

#[test]
fn claude_parse_prefers_embedded_cwd_over_lossy_storage_key() {
    let tmp = TempDir::new().unwrap();
    let path = write_session(
        &tmp.path().join("projects"),
        "-Users-test-Projects-nyc-real-estate",
        "session-1",
        &[
            r#"{"type":"user","sessionId":"session-1","cwd":"/Users/test/Projects/nyc-real-estate","timestamp":"2026-08-11T00:00:00Z","message":{"content":"hi"}}"#,
        ],
    );

    let rec = parse_claude_session(&path).unwrap();
    assert_eq!(rec.project_display, "/Users/test/Projects/nyc-real-estate");
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
            r#"{"timestamp":"2026-05-05T00:02:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":900010,"cached_input_tokens":900000,"output_tokens":900005},"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5}}}}"#,
            r#"{"timestamp":"2026-05-05T00:03:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":901010,"cached_input_tokens":900200,"output_tokens":900505},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":200,"cache_write_input_tokens":100,"output_tokens":500}}}}"#,
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
    // Per-turn deltas are summed; inherited cumulative totals are ignored.
    assert_eq!(rec.usage.input_tokens, 710);
    assert_eq!(rec.usage.cache_read_tokens, 200);
    assert_eq!(rec.usage.cache_creation_tokens, 100);
    assert_eq!(rec.usage.output_tokens, 505);
    assert_eq!(rec.tool_names, vec!["shell".to_string()]);
    assert_eq!(rec.thinking_block_count, 1);
    assert!(rec.extras.as_deref().unwrap().contains("0.99.0"));

    let arch = provider.parse(archived).unwrap();
    assert_eq!(arch.project_display, "/old");
}

#[test]
fn codex_parse_records_fork_parent_and_structured_source() {
    let tmp = TempDir::new().unwrap();
    let codex = tmp.path().join(".codex");
    write_lines(
        &codex.join("sessions/2026/05/30/rollout-2026-05-30T00-00-00-child.jsonl"),
        &[
            r#"{"timestamp":"2026-05-30T00:00:00Z","type":"session_meta","payload":{"id":"child","forked_from_id":"parent","cwd":"/repo","originator":"codex_exec","source":{"subagent":"review"},"model_provider":"openai"}}"#,
            r#"{"timestamp":"2026-05-30T00:00:30Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
        ],
    );

    let provider = CodexProvider::at(codex);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();

    assert_eq!(rec.session_id.as_deref(), Some("child"));
    assert_eq!(rec.parent_session_id.as_deref(), Some("parent"));
    let extras: serde_json::Value = serde_json::from_str(rec.extras.as_deref().unwrap()).unwrap();
    assert_eq!(extras["forked_from_id"].as_str(), Some("parent"));
    assert_eq!(extras["source"]["subagent"].as_str(), Some("review"));
}

#[test]
fn codex_extracts_pr_links_from_gh_pr_output_and_agent_marker() {
    let tmp = TempDir::new().unwrap();
    let codex = tmp.path().join(".codex");
    write_lines(
        &codex.join("sessions/2026/05/30/rollout-2026-05-30T00-00-00-codex-pr.jsonl"),
        &[
            r#"{"timestamp":"2026-05-30T00:00:00Z","type":"session_meta","payload":{"id":"codex-pr","cwd":"/repo"}}"#,
            r#"{"timestamp":"2026-05-30T00:01:00Z","type":"event_msg","payload":{"type":"exec_command_end","command":"gh pr create --fill","stdout":"https://github.com/utensils/claudex/pull/38\n"}}"#,
            r#"{"timestamp":"2026-05-30T00:02:00Z","type":"response_item","payload":{"type":"agent_message","message":"::git-create-pr{url=\"https://github.com/utensils/aethon/pull/167\"}"}}"#,
            r#"{"timestamp":"2026-05-30T00:03:00Z","type":"event_msg","payload":{"type":"exec_command_end","command":"gh pr view","stdout":"https://github.com/utensils/claudex/pull/38"}}"#,
            r#"{"timestamp":"2026-05-30T00:04:00Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"c1","arguments":"{\"cmd\":\"gh pr view\"}"}}"#,
            r#"{"timestamp":"2026-05-30T00:05:00Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"https://github.com/utensils/claudex/pull/41"}}"#,
            r#"{"timestamp":"2026-05-30T00:06:00Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Opened https://github.com/utensils/ptywright/pull/14"}]}}"#,
        ],
    );

    let provider = CodexProvider::at(codex);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();

    assert_eq!(rec.pr_links.len(), 4);
    assert!(rec.pr_links.iter().any(|(n, url, repo, _)| *n == 38
        && url == "https://github.com/utensils/claudex/pull/38"
        && repo == "utensils/claudex"));
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 167 && repo == "utensils/aethon")
    );
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 41 && repo == "utensils/claudex")
    );
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 14 && repo == "utensils/ptywright")
    );
}

#[test]
fn codex_ignores_pr_links_from_unrelated_tool_output() {
    let tmp = TempDir::new().unwrap();
    let codex = tmp.path().join(".codex");
    write_lines(
        &codex.join("sessions/2026/05/30/rollout-2026-05-30T00-00-00-codex-read.jsonl"),
        &[
            r#"{"timestamp":"2026-05-30T00:00:00Z","type":"session_meta","payload":{"id":"codex-read","cwd":"/repo"}}"#,
            r#"{"timestamp":"2026-05-30T00:01:00Z","type":"response_item","payload":{"type":"function_call","name":"read","call_id":"r1","arguments":"{\"path\":\"CHANGELOG.md\"}"}}"#,
            r#"{"timestamp":"2026-05-30T00:02:00Z","type":"response_item","payload":{"type":"function_call_output","call_id":"r1","output":"historical note: ::git-create-pr https://github.com/utensils/claudex/pull/1"}}"#,
        ],
    );

    let provider = CodexProvider::at(codex);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();
    assert!(rec.pr_links.is_empty());
}

#[test]
fn codex_ignores_pr_links_from_non_pr_exec_output() {
    let tmp = TempDir::new().unwrap();
    let codex = tmp.path().join(".codex");
    write_lines(
        &codex.join("sessions/2026/05/30/rollout-2026-05-30T00-00-00-codex-doc.jsonl"),
        &[
            r#"{"timestamp":"2026-05-30T00:00:00Z","type":"session_meta","payload":{"id":"codex-doc","cwd":"/repo"}}"#,
            r#"{"timestamp":"2026-05-30T00:01:00Z","type":"event_msg","payload":{"type":"exec_command_end","command":["sed","-n","1,20p","SKILL.md"],"stdout":"example command: gh pr view https://github.com/utensils/claudex/pull/1\n"}}"#,
        ],
    );

    let provider = CodexProvider::at(codex);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();
    assert!(rec.pr_links.is_empty());
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
fn pi_extracts_pr_links_from_bash_tool_result_and_assistant_text() {
    let tmp = TempDir::new().unwrap();
    let agent = tmp.path().join(".pi/agent");
    write_lines(
        &agent.join("sessions/--repo--/2026-05-30T00-00-00Z_sess-pi-pr.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"sess-pi-pr","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo"}"#,
            r#"{"type":"message","id":"a1","timestamp":"2026-05-30T00:01:00Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"gh pr create --fill"}},{"type":"text","text":"opening a PR"}],"provider":"anthropic","model":"claude-3-opus","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.01}},"stopReason":"toolUse"}}"#,
            r#"{"type":"message","id":"t1","timestamp":"2026-05-30T00:02:00Z","message":{"role":"toolResult","toolCallId":"c1","toolName":"bash","content":[{"type":"text","text":"https://github.com/utensils/claudex/pull/39"}],"isError":false}}"#,
            r#"{"type":"message","id":"a2","timestamp":"2026-05-30T00:03:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Opened https://github.com/utensils/aethon/pull/168"}],"provider":"anthropic","model":"claude-3-opus","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.01}},"stopReason":"stop"}}"#,
            r#"{"type":"message","id":"b1","timestamp":"2026-05-30T00:04:00Z","message":{"role":"bashExecution","command":"gh pr view","output":"https://github.com/utensils/ptywright/pull/14"}}"#,
        ],
    );

    let provider = PiProvider::at(agent);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();
    assert_eq!(rec.pr_links.len(), 3);
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 39 && repo == "utensils/claudex")
    );
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 168 && repo == "utensils/aethon")
    );
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 14 && repo == "utensils/ptywright")
    );
}

#[test]
fn pi_ignores_pr_links_from_read_tool_results() {
    let tmp = TempDir::new().unwrap();
    let agent = tmp.path().join(".pi/agent");
    write_lines(
        &agent.join("sessions/--repo--/2026-05-30T00-00-00Z_sess-pi-read.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"sess-pi-read","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo"}"#,
            r#"{"type":"message","id":"a1","timestamp":"2026-05-30T00:01:00Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"r1","name":"read","arguments":{"path":"CHANGELOG.md"}}],"provider":"anthropic","model":"claude-3-opus","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.01}},"stopReason":"toolUse"}}"#,
            r#"{"type":"message","id":"t1","timestamp":"2026-05-30T00:02:00Z","message":{"role":"toolResult","toolCallId":"r1","toolName":"read","content":[{"type":"text","text":"historical note: https://github.com/utensils/claudex/pull/1"}],"isError":false}}"#,
        ],
    );

    let provider = PiProvider::at(agent);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();
    assert!(rec.pr_links.is_empty());
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

// --- OpenClaw provider ---

#[test]
fn openclaw_enumerates_agents_and_skips_sidecars() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join(".openclaw");
    let main = state.join("agents/main/sessions");
    let ops = state.join("agents/ops/sessions");
    write_jsonl(
        &main.join("sess-main.jsonl"),
        &[
            r#"{"type":"session","id":"sess-main","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo/main"}"#,
        ],
    );
    write_jsonl(
        &main.join("sess-main.trajectory.jsonl"),
        &[
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"sess-main","source":"runtime","type":"session.started","ts":"2026-05-30T00:00:00Z","seq":1,"sessionId":"sess-main"}"#,
        ],
    );
    write_jsonl(
        &main.join("sess-main.checkpoint.11111111-1111-4111-8111-111111111111.jsonl"),
        &[],
    );
    fs::write(main.join("sessions.json"), "{}\n").unwrap();
    fs::write(main.join("sess-main.trajectory-path.json"), "{}\n").unwrap();
    write_jsonl(
        &ops.join("sess-deleted.jsonl.deleted.2026-05-30T00-00-00Z"),
        &[
            r#"{"type":"session","id":"sess-deleted","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo/ops"}"#,
        ],
    );
    write_jsonl(
        &ops.join("trajectory-only.trajectory.jsonl"),
        &[
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"trajectory-only","source":"runtime","type":"session.started","ts":"2026-05-30T00:00:00Z","seq":1,"sessionId":"trajectory-only","workspaceDir":"/repo/traj"}"#,
        ],
    );

    let provider = OpenClawProvider::at(state);
    assert_eq!(provider.id(), "openclaw");
    let files = provider.enumerate().unwrap();
    let names: Vec<_> = files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(names.contains(&"sess-main.jsonl".to_string()));
    assert!(names.contains(&"sess-deleted.jsonl.deleted.2026-05-30T00-00-00Z".to_string()));
    assert!(names.contains(&"trajectory-only.trajectory.jsonl".to_string()));
    assert!(
        files
            .iter()
            .find(|f| f
                .path
                .ends_with("sess-deleted.jsonl.deleted.2026-05-30T00-00-00Z"))
            .unwrap()
            .archived
    );
    assert!(files.iter().all(|f| {
        f.source_key
            .as_deref()
            .is_some_and(|k| k.starts_with("agent:"))
    }));
}

#[test]
fn openclaw_classic_parse_reads_usage_tools_prs_and_store_extras() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join(".openclaw");
    let sessions = state.join("agents/main/sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("sessions.json"),
        r#"{"agent:main:telegram:direct:owner":{"sessionId":"sess-open","status":"running","modelProvider":"anthropic","messageProvider":"telegram","workspaceDir":"/repo/open","usageFamilySessionIds":["sess-open"]}}"#,
    )
    .unwrap();
    write_jsonl(
        &sessions.join("sess-open.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"sess-open","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo/open"}"#,
            r#"{"type":"model_change","provider":"anthropic","modelId":"claude-3-opus","timestamp":"2026-05-30T00:00:01Z"}"#,
            r#"{"type":"message","id":"u1","timestamp":"2026-05-30T00:01:00Z","message":{"role":"user","content":[{"type":"text","text":"open the pr"}]}}"#,
            r#"{"type":"message","id":"a1","timestamp":"2026-05-30T00:02:00Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"gh pr create --fill"}},{"type":"thinking","text":"hmm"},{"type":"text","text":"on it"}],"provider":"anthropic","model":"claude-3-opus","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"cost":{"total":0.42}},"stopReason":"toolUse"}}"#,
            r#"{"type":"message","id":"t1","timestamp":"2026-05-30T00:03:00Z","message":{"role":"toolResult","toolCallId":"c1","toolName":"bash","content":[{"type":"text","text":"https://github.com/utensils/claudex/pull/41"}]}}"#,
        ],
    );
    write_jsonl(
        &sessions.join("sess-open.trajectory.jsonl"),
        &[
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"sess-open","source":"runtime","type":"session.ended","ts":"2026-05-30T00:04:00Z","seq":1,"sessionId":"sess-open","data":{"status":"done"}}"#,
        ],
    );

    let provider = OpenClawProvider::at(state);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();
    assert_eq!(rec.session_id.as_deref(), Some("sess-open"));
    assert_eq!(rec.project_display, "/repo/open");
    assert_eq!(rec.embedded_cost, Some(0.42));
    assert_eq!(rec.tool_names, vec!["bash".to_string()]);
    assert_eq!(rec.thinking_block_count, 1);
    assert_eq!(*rec.stop_reason_counts.get("toolUse").unwrap(), 1);
    assert_eq!(*rec.stop_reason_counts.get("done").unwrap(), 1);
    assert!(
        rec.messages
            .iter()
            .any(|m| m.content.contains("open the pr"))
    );
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 41 && repo == "utensils/claudex")
    );
    let stats = rec.model_usage.get("anthropic/claude-3-opus").unwrap();
    assert_eq!(stats.usage.input_tokens, 100);
    assert_eq!(stats.embedded_cost, Some(0.42));
    let extras = rec.extras.unwrap();
    assert!(extras.contains("agent_id"));
    assert!(extras.contains("session_keys"));
    assert!(extras.contains("trajectory_path"));
}

#[test]
fn openclaw_classic_usage_without_model_uses_unknown_bucket() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join(".openclaw");
    let sessions = state.join("agents/main/sessions");
    write_jsonl(
        &sessions.join("sess-open.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"sess-open","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo/open"}"#,
            r#"{"type":"message","id":"a1","timestamp":"2026-05-30T00:01:00Z","message":{"role":"assistant","content":[{"type":"text","text":"no model"}],"usage":{"input":4,"output":2,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.05}}}}"#,
        ],
    );

    let provider = OpenClawProvider::at(state);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();
    assert_eq!(rec.model.as_deref(), Some("unknown"));
    let stats = rec.model_usage.get("unknown").unwrap();
    assert_eq!(stats.usage.input_tokens, 4);
    assert_eq!(stats.assistant_message_count, 1);
    assert_eq!(stats.embedded_cost, Some(0.05));
}

#[test]
fn openclaw_archived_classic_merges_sidecar_cost_and_metadata() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join(".openclaw");
    let sessions = state.join("agents/main/sessions");
    write_jsonl(
        &sessions.join("archived.jsonl.deleted.2026-05-30T00:00:00Z"),
        &[
            r#"{"type":"session","version":3,"id":"archived","timestamp":"2026-05-30T00:00:00Z","cwd":"/repo/open"}"#,
            r#"{"type":"message","id":"a1","timestamp":"2026-05-30T00:01:00Z","message":{"role":"assistant","content":[{"type":"text","text":"classic usage"}],"provider":"openai","model":"gpt-5.2","usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0}}}"#,
        ],
    );
    write_jsonl(
        &sessions.join("archived.trajectory.jsonl"),
        &[
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"archived","source":"runtime","type":"model.completed","ts":"2026-05-30T00:02:00Z","seq":1,"sessionId":"archived","workspaceDir":"/repo/open","provider":"openai","modelId":"gpt-5.2","data":{"assistantText":"trajectory","usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.33}}}}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"archived","source":"runtime","type":"session.ended","ts":"2026-05-30T00:03:00Z","seq":2,"sessionId":"archived","data":{"status":"done"}}"#,
        ],
    );

    let provider = OpenClawProvider::at(state);
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].archived);
    let rec = provider.parse(&files[0]).unwrap();
    let stats = rec.model_usage.get("openai/gpt-5.2").unwrap();
    assert_eq!(stats.embedded_cost, Some(0.33));
    assert_eq!(*rec.stop_reason_counts.get("done").unwrap(), 1);
    assert!(rec.extras.unwrap().contains("trajectory_path"));
}

#[test]
fn openclaw_trajectory_only_parse_reads_runtime_events() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join(".openclaw");
    let sessions = state.join("agents/main/sessions");
    write_jsonl(
        &sessions.join("traj-only.trajectory.jsonl"),
        &[
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"traj-only","source":"runtime","type":"session.started","ts":"2026-05-30T00:00:00Z","seq":1,"sessionId":"traj-only","workspaceDir":"/repo/traj","provider":"openai","modelId":"gpt-5.2"}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"traj-only","source":"runtime","type":"prompt.submitted","ts":"2026-05-30T00:01:00Z","seq":2,"sessionId":"traj-only","workspaceDir":"/repo/traj","provider":"openai","modelId":"gpt-5.2","data":{"prompt":"ship it"}}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"traj-only","source":"runtime","type":"tool.call","ts":"2026-05-30T00:02:00Z","seq":3,"entryId":"c1","sessionId":"traj-only","provider":"openai","modelId":"gpt-5.2","data":{"name":"bash","arguments":{"command":"gh pr view"}}}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"traj-only","source":"runtime","type":"tool.result","ts":"2026-05-30T00:03:00Z","seq":4,"parentEntryId":"c1","sessionId":"traj-only","provider":"openai","modelId":"gpt-5.2","data":{"name":"bash","output":"https://github.com/utensils/aethon/pull/168"}}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"traj-only","source":"runtime","type":"model.completed","ts":"2026-05-30T00:04:00Z","seq":5,"sessionId":"traj-only","workspaceDir":"/repo/traj","provider":"openai","modelId":"gpt-5.2","data":{"assistantText":"Opened https://github.com/utensils/claudex/pull/42","usage":{"input":10,"output":5,"cacheRead":2,"cacheWrite":1,"cost":{"total":0.12}}}}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"traj-only","source":"runtime","type":"model.completed","ts":"2026-05-30T00:04:30Z","seq":6,"sessionId":"traj-only","workspaceDir":"/repo/traj","provider":"openai","modelId":"gpt-5.2","data":{"assistantText":"second completion","usage":{"input":3,"output":2,"cacheRead":0,"cacheWrite":0,"cost":{"total":0.03}}}}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"traj-only","source":"runtime","type":"model.fallback_step","ts":"2026-05-30T00:05:00Z","seq":7,"sessionId":"traj-only"}"#,
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"traj-only","source":"runtime","type":"session.ended","ts":"2026-05-30T00:06:00Z","seq":8,"sessionId":"traj-only","data":{"status":"done"}}"#,
        ],
    );

    let provider = OpenClawProvider::at(state);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();
    assert_eq!(rec.session_id.as_deref(), Some("traj-only"));
    assert_eq!(rec.project_display, "/repo/traj");
    assert_eq!(rec.tool_names, vec!["bash".to_string()]);
    assert_eq!(rec.embedded_cost, Some(0.15));
    assert!(rec.messages.iter().any(|m| m.content.contains("ship it")));
    assert!(rec.messages.iter().any(|m| m.content.contains("Opened")));
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 168 && repo == "utensils/aethon")
    );
    assert!(
        rec.pr_links
            .iter()
            .any(|(n, _, repo, _)| *n == 42 && repo == "utensils/claudex")
    );
    assert_eq!(
        *rec.stop_reason_counts.get("model_fallback_step").unwrap(),
        1
    );
    assert_eq!(*rec.stop_reason_counts.get("done").unwrap(), 1);
    let stats = rec.model_usage.get("openai/gpt-5.2").unwrap();
    assert_eq!(stats.usage.cache_read_tokens, 2);
    assert_eq!(stats.assistant_message_count, 2);
}

#[test]
fn openclaw_enumerates_external_trajectory_dir_from_session_store() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join(".openclaw");
    let trajectory = tmp.path().join("traces");
    let sessions = state.join("agents/main/sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::create_dir_all(&trajectory).unwrap();
    fs::write(
        sessions.join("sessions.json"),
        r#"{"agent:main:dm":{"sessionId":"external.session","workspaceDir":"/repo/ext"}}"#,
    )
    .unwrap();
    write_jsonl(
        &trajectory.join("external_session.jsonl"),
        &[
            r#"{"traceSchema":"openclaw-trajectory","schemaVersion":1,"traceId":"external.session","source":"runtime","type":"session.started","ts":"2026-05-30T00:00:00Z","seq":1,"sessionId":"external.session","workspaceDir":"/repo/ext"}"#,
        ],
    );
    let provider = OpenClawProvider::at_with_trajectory_dir(state, trajectory);
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("external_session.jsonl"));
    assert_eq!(files[0].project_display, "/repo/ext");
}

// --- GitHub Copilot CLI ---

fn copilot_session_start(session_id: &str, cwd: &str, model: &str) -> String {
    format!(
        r#"{{"type":"session.start","data":{{"sessionId":"{session_id}","copilotVersion":"1.0.61","selectedModel":"{model}","context":{{"cwd":"{cwd}","gitRoot":"{cwd}","branch":"main","repository":"utensils/claudex"}}}},"timestamp":"2026-06-11T17:31:01.212Z"}}"#
    )
}

#[test]
fn copilot_enumerate_finds_event_logs_and_skips_extras() {
    let tmp = TempDir::new().unwrap();
    let copilot = tmp.path().join(".copilot");
    write_lines(
        &copilot.join("session-state/aaaaaaaa-0000-0000-0000-000000000001/events.jsonl"),
        &[&copilot_session_start("s1", "/repo", "claude-sonnet-4.6")],
    );
    // Sidecar artifacts in the same session dir must not be enumerated.
    let dir = copilot.join("session-state/aaaaaaaa-0000-0000-0000-000000000001");
    fs::write(dir.join("workspace.yaml"), "id: s1\ncwd: /repo\n").unwrap();
    fs::write(dir.join("session.db"), b"sqlite").unwrap();
    fs::create_dir_all(dir.join("checkpoints")).unwrap();
    // A session dir without an events.jsonl (created but never run) is skipped.
    fs::create_dir_all(copilot.join("session-state/bbbbbbbb-0000-0000-0000-000000000002")).unwrap();
    // Stray files directly under session-state are not session dirs.
    fs::write(copilot.join("session-state/notes.txt"), "x").unwrap();

    let provider = CopilotProvider::at(copilot);
    assert_eq!(provider.id(), "copilot");
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("events.jsonl"));
    assert!(!files[0].archived);
}

#[test]
fn copilot_parse_reads_messages_tools_turns_and_derived_tokens() {
    let tmp = TempDir::new().unwrap();
    let copilot = tmp.path().join(".copilot");
    write_lines(
        &copilot.join("session-state/cccccccc-0000-0000-0000-000000000003/events.jsonl"),
        &[
            &copilot_session_start(
                "copilot-sess-1",
                "/Users/me/Projects/demo",
                "claude-sonnet-4.6",
            ),
            r#"{"type":"user.message","data":{"content":"fix the bug","transformedContent":"<reminder>noise</reminder> fix the bug"},"timestamp":"2026-06-11T17:31:02.733Z"}"#,
            r#"{"type":"assistant.turn_start","data":{"turnId":"0"},"timestamp":"2026-06-11T17:31:03.000Z"}"#,
            r#"{"type":"assistant.message","data":{"model":"claude-sonnet-4.6","content":"","reasoningText":"thinking about it","toolRequests":[{"name":"bash","arguments":{"command":"ls"}},{"name":"azure-devops-mcp-wit_get_work_item","arguments":{"id":1}}]},"timestamp":"2026-06-11T17:31:05.000Z"}"#,
            r#"{"type":"assistant.message","data":{"model":"claude-sonnet-4.6","content":"Opened https://github.com/utensils/claudex/pull/99","toolRequests":[]},"timestamp":"2026-06-11T17:31:09.000Z"}"#,
            r#"{"type":"assistant.turn_end","data":{"turnId":"0"},"timestamp":"2026-06-11T17:31:11.500Z"}"#,
            r#"{"type":"abort","data":{},"timestamp":"2026-06-11T17:31:12.000Z"}"#,
            r#"{"type":"session.shutdown","data":{"totalPremiumRequests":2,"totalApiDurationMs":188035,"codeChanges":{"linesAdded":10,"linesRemoved":3,"filesModified":["src/main.rs"]},"modelMetrics":{"claude-sonnet-4.6":{"requests":{"count":17,"cost":1},"usage":{"inputTokens":753841,"outputTokens":10563,"cacheReadTokens":659637,"cacheWriteTokens":85868,"reasoningTokens":1950}}}},"timestamp":"2026-06-11T17:42:58.090Z"}"#,
        ],
    );

    let provider = CopilotProvider::at(copilot);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();

    assert_eq!(rec.session_id.as_deref(), Some("copilot-sess-1"));
    assert_eq!(rec.project_display, "/Users/me/Projects/demo");
    assert_eq!(rec.model.as_deref(), Some("claude-sonnet-4.6"));
    assert_eq!(rec.message_count, 3);

    // FTS keeps the raw user text, not the transformed prompt.
    let user_texts: Vec<&str> = rec
        .messages
        .iter()
        .filter(|m| m.msg_type == "user")
        .map(|m| m.content.as_str())
        .collect();
    assert_eq!(user_texts, vec!["fix the bug"]);

    assert_eq!(
        rec.tool_names,
        vec!["bash", "azure-devops-mcp-wit_get_work_item"]
    );
    assert_eq!(rec.thinking_block_count, 1);
    assert_eq!(rec.stop_reason_counts.get("abort"), Some(&1));

    // One turn: 17:31:03.000 -> 17:31:11.500.
    assert_eq!(rec.turn_durations.len(), 1);
    assert_eq!(rec.turn_durations[0].0, 8500);

    // usage.inputTokens includes cache; without per-model tokenDetails the
    // non-cache input is derived: 753841 - 659637 - 85868 = 8336.
    let stats = rec.model_usage.get("claude-sonnet-4.6").unwrap();
    assert_eq!(stats.usage.input_tokens, 8336);
    assert_eq!(stats.usage.output_tokens, 10563);
    assert_eq!(stats.usage.cache_read_tokens, 659637);
    assert_eq!(stats.usage.cache_creation_tokens, 85868);
    assert_eq!(stats.assistant_message_count, 17);
    assert!(stats.embedded_cost.is_none(), "copilot cost is computed");

    assert_eq!(rec.file_paths_modified, vec!["src/main.rs"]);
    assert_eq!(rec.pr_links.len(), 1);
    assert_eq!(rec.pr_links[0].0, 99);

    let extras: serde_json::Value = serde_json::from_str(rec.extras.as_deref().unwrap()).unwrap();
    assert_eq!(extras["premium_requests"].as_u64(), Some(2));
    assert_eq!(extras["lines_added"].as_u64(), Some(10));
    assert_eq!(extras["repository"].as_str(), Some("utensils/claudex"));
    assert_eq!(extras["copilot_version"].as_str(), Some("1.0.61"));
}

#[test]
fn copilot_prefers_per_model_token_details_and_keeps_last_shutdown() {
    let tmp = TempDir::new().unwrap();
    let copilot = tmp.path().join(".copilot");
    write_lines(
        &copilot.join("session-state/dddddddd-0000-0000-0000-000000000004/events.jsonl"),
        &[
            &copilot_session_start("copilot-sess-2", "/repo", "gpt-5.5"),
            // An earlier shutdown (pre-resume) must be superseded by the last one.
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.5":{"requests":{"count":1},"usage":{"inputTokens":10,"outputTokens":1,"cacheReadTokens":0,"cacheWriteTokens":0}}}},"timestamp":"2026-06-11T18:00:00Z"}"#,
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.5":{"requests":{"count":4},"usage":{"inputTokens":1000,"outputTokens":50,"cacheReadTokens":600,"cacheWriteTokens":100},"tokenDetails":{"input":{"tokenCount":280},"output":{"tokenCount":50}}},"claude-sonnet-4.6":{"requests":{"count":2},"usage":{"inputTokens":500,"outputTokens":20,"cacheReadTokens":400,"cacheWriteTokens":0}}}},"timestamp":"2026-06-11T18:10:00Z"}"#,
        ],
    );

    let provider = CopilotProvider::at(copilot);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();

    assert_eq!(rec.model_usage.len(), 2);
    // Per-model tokenDetails wins over the derived subtraction (1000-600-100=300).
    let gpt = rec.model_usage.get("gpt-5.5").unwrap();
    assert_eq!(gpt.usage.input_tokens, 280);
    assert_eq!(gpt.assistant_message_count, 4);
    // No tokenDetails for the second model: derived 500-400-0=100.
    let sonnet = rec.model_usage.get("claude-sonnet-4.6").unwrap();
    assert_eq!(sonnet.usage.input_tokens, 100);
    assert_eq!(sonnet.usage.cache_read_tokens, 400);
    // The record-level rollup sums the last shutdown only.
    assert_eq!(rec.usage.input_tokens, 380);
    assert_eq!(rec.usage.output_tokens, 70);
}

#[test]
fn copilot_session_without_shutdown_degrades_to_zero_usage() {
    let tmp = TempDir::new().unwrap();
    let copilot = tmp.path().join(".copilot");
    write_lines(
        &copilot.join("session-state/eeeeeeee-0000-0000-0000-000000000005/events.jsonl"),
        &[
            r#"{"type":"user.message","data":{"content":"hello"},"timestamp":"2026-06-11T19:00:00Z"}"#,
            r#"{"type":"assistant.message","data":{"model":"claude-sonnet-4.6","content":"hi"},"timestamp":"2026-06-11T19:00:10Z"}"#,
        ],
    );
    // Killed sessions keep their sidecar; cwd and title come from it.
    fs::write(
        copilot.join("session-state/eeeeeeee-0000-0000-0000-000000000005/workspace.yaml"),
        "id: eeeeeeee-0000-0000-0000-000000000005\ncwd: /Users/me/work\nname: \"quoted \\\"title\\\"\"\nclient_name: github/cli\n",
    )
    .unwrap();

    let provider = CopilotProvider::at(copilot);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();

    // No session.start: the directory uuid stands in for the session id.
    assert_eq!(
        rec.session_id.as_deref(),
        Some("eeeeeeee-0000-0000-0000-000000000005")
    );
    assert_eq!(rec.project_display, "/Users/me/work");
    assert_eq!(rec.usage.total_tokens(), 0);
    assert!(rec.model_usage.is_empty());
    assert_eq!(rec.message_count, 2);
    assert_eq!(rec.duration_ms, 10_000);

    let extras: serde_json::Value = serde_json::from_str(rec.extras.as_deref().unwrap()).unwrap();
    assert_eq!(extras["name"].as_str(), Some("quoted \"title\""));
    assert_eq!(extras["client_name"].as_str(), Some("github/cli"));
}

// --- VS Code Copilot Chat ---

#[test]
fn copilot_vscode_enumerate_maps_workspace_folders_and_empty_window() {
    let tmp = TempDir::new().unwrap();
    let user_dir = tmp.path().join("Code/User");
    let ws = user_dir.join("workspaceStorage");

    fs::create_dir_all(ws.join("hash1/chatSessions")).unwrap();
    fs::write(
        ws.join("hash1/workspace.json"),
        r#"{"folder":"file:///Users/me/My%20Project"}"#,
    )
    .unwrap();
    fs::write(ws.join("hash1/chatSessions/a.json"), "{}").unwrap();

    // .code-workspace setups store a `workspace` URI instead of `folder`.
    fs::create_dir_all(ws.join("hash2/chatSessions")).unwrap();
    fs::write(
        ws.join("hash2/workspace.json"),
        r#"{"workspace":"file:///Users/me/multi.code-workspace"}"#,
    )
    .unwrap();
    fs::write(ws.join("hash2/chatSessions/b.jsonl"), "").unwrap();

    // Missing workspace.json is tolerated (placeholder project label).
    fs::create_dir_all(ws.join("hash3/chatSessions")).unwrap();
    fs::write(ws.join("hash3/chatSessions/c.json"), "{}").unwrap();

    // Hashes without chatSessions are not sessions.
    fs::create_dir_all(ws.join("hash4")).unwrap();
    fs::write(ws.join("hash4/workspace.json"), r#"{"folder":"file:///x"}"#).unwrap();

    fs::create_dir_all(user_dir.join("globalStorage/emptyWindowChatSessions")).unwrap();
    fs::write(
        user_dir.join("globalStorage/emptyWindowChatSessions/e.json"),
        "{}",
    )
    .unwrap();

    let provider = CopilotVscodeProvider::at(user_dir);
    assert_eq!(provider.id(), "copilot-vscode");
    assert!(provider.enabled());
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 4);

    let by_name = |name: &str| {
        files
            .iter()
            .find(|f| f.path.file_name().unwrap().to_string_lossy() == name)
            .unwrap()
    };
    assert_eq!(by_name("a.json").project_display, "/Users/me/My Project");
    assert_eq!(
        by_name("b.jsonl").project_display,
        "/Users/me/multi.code-workspace"
    );
    assert_eq!(by_name("c.json").project_display, "(unknown workspace)");
    assert_eq!(by_name("e.json").project_display, "(empty window)");
}

fn vscode_request(model: &str, ts: i64, user_text: &str, answer: &str) -> serde_json::Value {
    serde_json::json!({
        "requestId": format!("req-{ts}"),
        "message": {"text": user_text},
        "modelId": model,
        "timestamp": ts,
        "response": [
            {"value": answer, "supportThemeIcons": false},
            {"kind": "thinking", "value": "hmm"},
            {"kind": "toolInvocationSerialized", "toolId": "copilot_readFile"},
            {"kind": "textEditGroup", "uri": {"path": "/Users/me/proj/src/lib.rs", "scheme": "file"}}
        ],
        "result": {
            "metadata": {"toolCallRounds": [{"toolCalls": [{"name": "read_file"}, {"name": "mcp_nixos_nix"}]}]},
            "timings": {"totalElapsed": 4200}
        },
        "modelState": {"completedAt": ts + 5000}
    })
}

#[test]
fn copilot_vscode_parse_monolithic_session() {
    let tmp = TempDir::new().unwrap();
    let user_dir = tmp.path().join("Code/User");
    let dir = user_dir.join("workspaceStorage/hash1/chatSessions");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        user_dir.join("workspaceStorage/hash1/workspace.json"),
        r#"{"folder":"file:///Users/me/proj"}"#,
    )
    .unwrap();

    let session = serde_json::json!({
        "version": 3,
        "sessionId": "vsc-sess-1",
        "creationDate": 1769763926111i64,
        "lastMessageDate": 1769764442409i64,
        "customTitle": "MCP Tools Overview",
        "initialLocation": "panel",
        "requests": [vscode_request("copilot/claude-sonnet-4.5", 1769763934260i64, "list your MCP tools", "The server exposes **2 tools**.")]
    });
    fs::write(dir.join("vsc-sess-1.json"), session.to_string()).unwrap();

    let provider = CopilotVscodeProvider::at(user_dir);
    let files = provider.enumerate().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].project_display, "/Users/me/proj");
    let rec = provider.parse(&files[0]).unwrap();

    assert_eq!(rec.session_id.as_deref(), Some("vsc-sess-1"));
    // The `copilot/` prefix is stripped so pricing tiers match.
    assert_eq!(rec.model.as_deref(), Some("claude-sonnet-4.5"));
    assert_eq!(rec.message_count, 2);
    assert_eq!(
        rec.usage.total_tokens(),
        0,
        "VS Code stores no token counts"
    );
    assert!(rec.embedded_cost.is_none());

    let stats = rec.model_usage.get("claude-sonnet-4.5").unwrap();
    assert_eq!(stats.usage.total_tokens(), 0);
    assert_eq!(stats.assistant_message_count, 1);

    let texts: Vec<(&str, &str)> = rec
        .messages
        .iter()
        .map(|m| (m.msg_type.as_str(), m.content.as_str()))
        .collect();
    assert_eq!(
        texts,
        vec![
            ("user", "list your MCP tools"),
            ("assistant", "The server exposes **2 tools**.")
        ]
    );

    // toolCallRounds is authoritative; the serialized part is its rendering.
    assert_eq!(rec.tool_names, vec!["read_file", "mcp_nixos_nix"]);
    assert_eq!(rec.thinking_block_count, 1);
    assert_eq!(rec.file_paths_modified, vec!["/Users/me/proj/src/lib.rs"]);
    assert_eq!(
        rec.turn_durations,
        vec![(4200, "2026-01-30T09:05:34.260Z".to_string())]
    );

    assert_eq!(
        rec.first_timestamp.unwrap().timestamp_millis(),
        1769763926111
    );
    assert_eq!(
        rec.last_timestamp.unwrap().timestamp_millis(),
        1769764442409
    );

    let extras: serde_json::Value = serde_json::from_str(rec.extras.as_deref().unwrap()).unwrap();
    assert_eq!(extras["custom_title"].as_str(), Some("MCP Tools Overview"));
    assert_eq!(extras["format"].as_str(), Some("json"));
}

#[test]
fn copilot_vscode_parse_replays_delta_log() {
    let tmp = TempDir::new().unwrap();
    let user_dir = tmp.path().join("Code/User");
    let dir = user_dir.join("workspaceStorage/hash1/chatSessions");
    fs::create_dir_all(&dir).unwrap();

    // Snapshot has no requests and no lastMessageDate (common in delta logs);
    // ops append two requests, then patch the second one's text in place.
    let snapshot = serde_json::json!({
        "kind": 0,
        "v": {"version": 3, "sessionId": "vsc-delta-1", "creationDate": 1769700000000i64, "requests": []}
    });
    let append = serde_json::json!({
        "kind": 2, "k": ["requests"], "i": null,
        "v": [vscode_request("copilot/gpt-5.5", 1769700010000i64, "first", "one")]
    });
    let insert = serde_json::json!({
        "kind": 2, "k": ["requests"], "i": 1,
        "v": [vscode_request("copilot/gpt-5.5", 1769700020000i64, "second", "two")]
    });
    let set = serde_json::json!({
        "kind": 1, "k": ["requests", 1, "message", "text"], "v": "second (edited)"
    });
    let unknown = serde_json::json!({"kind": 9, "k": ["whatever"], "v": 1});
    write_lines(
        &dir.join("vsc-delta-1.jsonl"),
        &[
            &snapshot.to_string(),
            &append.to_string(),
            &insert.to_string(),
            &set.to_string(),
            &unknown.to_string(),
        ],
    );

    let provider = CopilotVscodeProvider::at(user_dir);
    let files = provider.enumerate().unwrap();
    let rec = provider.parse(&files[0]).unwrap();

    assert_eq!(rec.session_id.as_deref(), Some("vsc-delta-1"));
    assert_eq!(rec.message_count, 4);
    let user_texts: Vec<&str> = rec
        .messages
        .iter()
        .filter(|m| m.msg_type == "user")
        .map(|m| m.content.as_str())
        .collect();
    assert_eq!(user_texts, vec!["first", "second (edited)"]);

    // No lastMessageDate: the newest request activity stands in
    // (second request's modelState.completedAt).
    assert_eq!(
        rec.first_timestamp.unwrap().timestamp_millis(),
        1769700000000
    );
    assert_eq!(
        rec.last_timestamp.unwrap().timestamp_millis(),
        1769700025000
    );
    assert_eq!(rec.usage.total_tokens(), 0);

    let extras: serde_json::Value = serde_json::from_str(rec.extras.as_deref().unwrap()).unwrap();
    assert_eq!(extras["format"].as_str(), Some("jsonl"));
}

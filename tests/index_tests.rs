use std::fs;
use std::io::Write;
use std::path::PathBuf;

use tempfile::TempDir;

// We test the index through a standalone integration approach:
// create temp dirs mimicking ~/.claude/projects/<encoded-project>/*.jsonl,
// then use the index module's public API.

/// Helper: write JSONL lines to a file inside a temp project dir.
fn setup_project(
    base: &std::path::Path,
    project_encoded: &str,
    session_name: &str,
    lines: &[&str],
) -> PathBuf {
    let proj_dir = base.join(project_encoded);
    fs::create_dir_all(&proj_dir).expect("create project dir");
    let file_path = proj_dir.join(format!("{session_name}.jsonl"));
    let mut f = fs::File::create(&file_path).expect("create jsonl");
    for line in lines {
        writeln!(f, "{line}").expect("write line");
    }
    f.flush().expect("flush");
    file_path
}

/// Realistic session with user + assistant + system + pr-link + file-history-snapshot
fn realistic_session_lines() -> Vec<&'static str> {
    vec![
        r#"{"type":"user","sessionId":"sess-001","timestamp":"2026-04-18T10:00:00Z","message":{"content":"fix the bug"}}"#,
        r#"{"type":"assistant","sessionId":"sess-001","timestamp":"2026-04-18T10:01:00Z","message":{"model":"claude-opus-4-6","stop_reason":"tool_use","usage":{"input_tokens":500,"output_tokens":200,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"inference_geo":"us-east-1","speed":35.2,"service_tier":"default","iterations":1},"content":[{"type":"thinking","text":"let me think"},{"type":"tool_use","name":"Bash","id":"t1","input":{"command":"ls"}},{"type":"text","text":"I found the issue"}]}}"#,
        r#"{"type":"assistant","sessionId":"sess-001","timestamp":"2026-04-18T10:02:00Z","message":{"model":"claude-opus-4-6","stop_reason":"end_turn","usage":{"input_tokens":300,"output_tokens":150,"cache_creation_input_tokens":50,"cache_read_input_tokens":800},"content":[{"type":"tool_use","name":"Edit","id":"t2","input":{}},{"type":"text","text":"Fixed it"}]}}"#,
        r#"{"type":"system","subtype":"turn_duration","durationMs":45000,"timestamp":"2026-04-18T10:01:30Z","sessionId":"sess-001"}"#,
        r#"{"type":"system","subtype":"turn_duration","durationMs":30000,"timestamp":"2026-04-18T10:02:30Z","sessionId":"sess-001"}"#,
        r#"{"type":"pr-link","prNumber":42,"prUrl":"https://github.com/org/repo/pull/42","prRepository":"org/repo","timestamp":"2026-04-18T10:03:00Z","sessionId":"sess-001"}"#,
        r#"{"type":"file-history-snapshot","snapshot":{"messageId":"m1","trackedFileBackups":{"src/main.rs":{"backupFileName":"x","version":1},"src/lib.rs":{"backupFileName":"y","version":1}},"timestamp":"2026-04-18T10:02:00Z"}}"#,
        r#"{"type":"permission-mode","mode":"bypassPermissions","timestamp":"2026-04-18T10:00:00Z","sessionId":"sess-001"}"#,
    ]
}

#[test]
fn test_parser_realistic_session() {
    let dir = TempDir::new().unwrap();
    let lines = realistic_session_lines();
    let file = setup_project(dir.path(), "-Users-test-Projects-myapp", "sess-001", &lines);

    let stats = claudex::parser::parse_session(&file).unwrap();

    assert_eq!(stats.session_id.as_deref(), Some("sess-001"));
    assert_eq!(stats.message_count, 3); // 1 user + 2 assistant
    assert_eq!(stats.model.as_deref(), Some("claude-opus-4-6"));
    assert_eq!(stats.usage.input_tokens, 800);
    assert_eq!(stats.usage.output_tokens, 350);
    assert_eq!(stats.usage.cache_creation_tokens, 150);
    assert_eq!(stats.usage.cache_read_tokens, 1800);
    assert_eq!(stats.tool_names, vec!["Bash", "Edit"]);
    assert_eq!(stats.thinking_block_count, 1);
    assert_eq!(stats.turn_durations.len(), 2);
    assert_eq!(stats.turn_durations[0].0, 45000);
    assert_eq!(stats.pr_links.len(), 1);
    assert_eq!(stats.pr_links[0].0, 42);
    assert_eq!(stats.pr_links[0].2, "org/repo");
    assert_eq!(stats.file_paths_modified.len(), 2);
    assert!(
        stats
            .file_paths_modified
            .contains(&"src/main.rs".to_string())
    );
    assert_eq!(stats.permission_modes.len(), 1);
    assert_eq!(stats.inference_geo.as_deref(), Some("us-east-1"));
    assert_eq!(stats.speed, Some(35.2));
    assert_eq!(stats.iterations, 1);
    assert_eq!(*stats.stop_reason_counts.get("tool_use").unwrap(), 1);
    assert_eq!(*stats.stop_reason_counts.get("end_turn").unwrap(), 1);
}

#[test]
fn test_store_canonical_project_path() {
    // Normal path
    assert_eq!(
        claudex::store::canonical_project_path("/Users/test/Projects/myapp"),
        "/Users/test/Projects/myapp"
    );

    // Worktree path should strip to parent
    assert_eq!(
        claudex::store::canonical_project_path(
            "/Users/test/Projects/myapp/.claude/worktrees/branch-hash"
        ),
        "/Users/test/Projects/myapp"
    );

    // Nested worktree
    assert_eq!(
        claudex::store::canonical_project_path(
            "/Users/test/Projects/deep/nested/.claude/worktrees/abc"
        ),
        "/Users/test/Projects/deep/nested"
    );
}

#[test]
fn test_store_decode_and_canonical() {
    let encoded = "-Users-test-Projects-myapp--claude-worktrees-branch-hash";
    let decoded = claudex::store::decode_project_name(encoded);
    assert_eq!(
        decoded,
        "/Users/test/Projects/myapp/.claude/worktrees/branch/hash"
    );
    let canonical = claudex::store::canonical_project_path(&decoded);
    assert_eq!(canonical, "/Users/test/Projects/myapp");
}

#[test]
fn test_cost_for_different_models() {
    use claudex::types::TokenUsage;

    let usage = TokenUsage {
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
    };

    let opus_cost = usage.cost_for_model(Some("claude-opus-4-6"));
    let sonnet_cost = usage.cost_for_model(Some("claude-sonnet-4-6"));
    let haiku_cost = usage.cost_for_model(Some("claude-haiku-4-5-20251001"));

    // Opus: $15 input + $75 output = $90
    assert!((opus_cost - 90.0).abs() < 0.01);
    // Sonnet: $3 input + $15 output = $18
    assert!((sonnet_cost - 18.0).abs() < 0.01);
    // Haiku: $0.80 input + $4 output = $4.80
    assert!((haiku_cost - 4.80).abs() < 0.01);

    // Verify ordering
    assert!(opus_cost > sonnet_cost);
    assert!(sonnet_cost > haiku_cost);
}

#[test]
fn test_cache_read_cost_dominates() {
    use claudex::types::TokenUsage;

    // Realistic scenario: lots of cache reads
    let usage = TokenUsage {
        input_tokens: 1_000,
        output_tokens: 100_000,
        cache_creation_tokens: 10_000,
        cache_read_tokens: 1_000_000_000, // 1B cache reads
    };

    let cost = usage.cost_for_model(Some("claude-opus-4-6"));
    let cache_read_portion = 1_000_000_000.0 * 1.50 / 1_000_000.0;

    // Cache read should be the dominant cost
    assert!(cache_read_portion / cost > 0.95);
}

#[test]
fn test_format_duration_edge_cases() {
    use claudex::commands::sessions::format_duration;

    assert_eq!(format_duration(0), "-");
    assert_eq!(format_duration(999), "0s"); // sub-second rounds to 0
    assert_eq!(format_duration(1000), "1s");
    assert_eq!(format_duration(59_999), "59s");
    assert_eq!(format_duration(60_000), "1m0s");
    assert_eq!(format_duration(3_599_999), "59m59s");
    assert_eq!(format_duration(3_600_000), "1h0m");
    assert_eq!(format_duration(86_400_000), "24h0m");
}

#[test]
fn test_model_pricing_name_detection() {
    use claudex::types::ModelPricing;

    assert_eq!(ModelPricing::name(Some("claude-opus-4-6")), "Opus");
    assert_eq!(ModelPricing::name(Some("claude-opus-4-7")), "Opus");
    assert_eq!(ModelPricing::name(Some("claude-sonnet-4-6")), "Sonnet");
    assert_eq!(
        ModelPricing::name(Some("claude-haiku-4-5-20251001")),
        "Haiku"
    );
    assert_eq!(ModelPricing::name(Some("<synthetic>")), "Sonnet"); // fallback
    assert_eq!(ModelPricing::name(None), "Sonnet"); // fallback
    assert_eq!(ModelPricing::name(Some("")), "Sonnet"); // empty fallback
}

#[test]
fn test_short_name_truncation() {
    use claudex::store::short_name;

    // Short paths pass through unchanged
    let short = "/Users/test/foo";
    assert_eq!(short_name(short), short);

    // Long paths get truncated with ellipsis at a path boundary
    let long = "/Users/jamesbrink/Projects/utensils/claudex/.claude/worktrees/something-very-long";
    let result = short_name(long);
    assert!(result.starts_with('…'));
    assert!(result.len() <= 55);
    // Should cut at a '/' boundary
    assert!(result[3..].starts_with('/') || result.len() == long.len());
}

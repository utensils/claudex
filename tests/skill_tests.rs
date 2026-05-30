//! Unit tests for the skill templates: per-flavor frontmatter, the idempotency
//! markers, and the plugin manifest. End-to-end generate/install behavior (file
//! writing, AGENTS.md splicing, drift vs the committed skill) is covered in
//! cli_tests.rs through the compiled binary.

use claudex::skill::templates::{self, Flavor};

const CL: &str = "- `claudex sessions` — List sessions";

#[test]
fn claude_code_frontmatter_has_allowed_tools_and_hint() {
    let md = templates::skill_md(Flavor::ClaudeCode, CL);
    assert!(md.starts_with("---\nname: claudex\n"));
    assert!(md.contains("allowed-tools: Bash(claudex:*)"));
    assert!(md.contains("argument-hint:"));
    assert!(md.contains("license: MIT"));
    assert!(md.contains(CL), "body carries the command list");
}

#[test]
fn codex_frontmatter_is_minimal() {
    let md = templates::skill_md(Flavor::Codex, CL);
    assert!(md.contains("name: claudex"));
    assert!(md.contains("description:"));
    // Codex gets the minimal Agent Skills subset — no Claude-specific keys.
    assert!(!md.contains("allowed-tools"));
    assert!(!md.contains("argument-hint"));
}

#[test]
fn pi_frontmatter_has_allowed_tools_without_hint() {
    let md = templates::skill_md(Flavor::Pi, CL);
    assert!(md.contains("allowed-tools: Bash(claudex:*)"));
    assert!(!md.contains("argument-hint"));
}

#[test]
fn agents_block_is_wrapped_in_idempotency_markers() {
    let block = templates::agents_block(CL);
    assert!(block.starts_with(templates::AGENTS_START));
    assert!(block.trim_end().ends_with(templates::AGENTS_END));
    assert!(block.contains(CL));
}

#[test]
fn plugin_json_is_valid_and_versioned() {
    let manifest: serde_json::Value =
        serde_json::from_str(&templates::plugin_json()).expect("valid JSON manifest");
    assert_eq!(manifest["name"], "claudex");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert!(
        manifest["description"]
            .as_str()
            .unwrap()
            .contains("claudex")
    );
}

//! Content templates for the claudex agent skill.
//!
//! One shared markdown body is wrapped in target-specific frontmatter so each
//! harness gets a header it understands:
//! - Claude Code / Codex / Pi: `SKILL.md` (YAML frontmatter + markdown)
//! - AGENTS.md: plain markdown wrapped in idempotency markers
//! - Claude Code plugin: `plugin.json` (JSON manifest, no frontmatter)
//!
//! `command_list` is derived from the live clap command tree so the skill never
//! drifts from the real CLI.

/// Markers bounding the claudex block inside a shared `AGENTS.md`, so the block
/// is replaced in place rather than duplicated on re-runs.
pub const AGENTS_START: &str = "<!-- claudex:start -->";
pub const AGENTS_END: &str = "<!-- claudex:end -->";

/// Frontmatter flavor for a generated `SKILL.md`.
#[derive(Debug, Clone, Copy)]
pub enum Flavor {
    /// Claude Code: supports `allowed-tools` + `argument-hint`.
    ClaudeCode,
    /// OpenAI Codex CLI: minimal Agent Skills subset.
    Codex,
    /// Pi coding agent: Agent Skills subset plus `allowed-tools`.
    Pi,
}

/// One-line skill description, reused by every frontmatter generator.
pub fn description() -> &'static str {
    "Query, search, and analyze Claude Code, OpenAI Codex, and Pi sessions using the claudex CLI. \
Use when asked about session history, token costs, tool usage, search past conversations, export \
sessions, or inspect agent activity across providers and projects."
}

/// A concise, always-accurate list of top-level commands, derived from the clap
/// command tree so it never drifts from the real CLI.
pub fn command_list(root: &clap::Command) -> String {
    let mut lines = Vec::new();
    for sub in root.get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        let name = sub.get_name();
        let about = sub
            .get_about()
            .map(|about| about.to_string())
            .unwrap_or_default();
        lines.push(format!("- `claudex {name}` — {about}"));
    }
    lines.join("\n")
}

/// The shared markdown body (everything after the frontmatter).
fn body(command_list: &str) -> String {
    format!(
        r#"# claudex — multi-provider agent session analytics

claudex indexes the local session transcripts of three coding agents into a
SQLite database at `~/.claudex/index.db` and reports across all of them:

- **Claude Code** — `~/.claude/projects/**.jsonl`
- **OpenAI Codex** — `~/.codex/sessions/**` and `~/.codex/archived_sessions/`
- **Pi** — `~/.pi/agent/sessions/**`

Every reporting command spans all three providers by default. The index is
**additive**: sessions archived or deleted from disk are retained, so historical
usage never disappears.

## Commands

{command_list}

Run `claudex <command> --help` for full flags.

## Filtering (every reporting command)

| Flag | Effect |
| --- | --- |
| `--provider <claude\|codex\|pi>` | Restrict to provider(s); repeatable or comma-separated. Default: all. |
| `--project <substr>` | Filter by project path substring. |
| `--model <substr>` | Filter by model (e.g. `opus`, `gpt-5`). |
| `--since <when>` / `--until <when>` | Date range. Accepts `YYYY-MM-DD`, RFC3339, or a relative span (`7d`, `12h`, `2w`). |
| `--on-disk-only` | Exclude retained sessions whose file was archived/deleted. |
| `--json` | Machine-readable output. Always includes a `provider` key per row. |
| `--no-index` | Scan Claude transcripts directly (Claude-only escape hatch). |

## When to use

- "How much have I spent on Codex this month?" → `claudex cost --provider codex --since 30d`
- "List my recent Pi sessions" → `claudex sessions --provider pi`
- "Find where I discussed schema migrations" → `claudex search "schema migration"`
- "What did session e1a2f4 do?" → `claudex session e1a2f4`
- "Overall dashboard" → `claudex summary`
- "Model cost breakdown across providers" → `claudex models`

## Output for agents

Add `--json` to any reporting command for stable, scriptable output. Each row
carries a `"provider"` key so results are unambiguous across providers. Cost is
in USD; Pi sessions report the provider's own per-message cost (local models are
$0), Claude/Codex are priced from a built-in per-model table.

## Notes

- The index refreshes automatically (staleness window 5 min); `claudex index`
  forces a sync, `claudex index --force` wipes and rebuilds (the only path that
  discards retained data).
- Worktree sessions roll up to their parent project; Claude subagent transcripts
  roll up to their parent session.
"#
    )
}

/// Render a `SKILL.md` with target-specific frontmatter.
pub fn skill_md(flavor: Flavor, command_list: &str) -> String {
    let description = description();
    let frontmatter = match flavor {
        Flavor::ClaudeCode => format!(
            "---\nname: claudex\ndescription: {description}\nargument-hint: [subcommand or query]\nallowed-tools: Bash(claudex:*), Read, Glob, Grep\nlicense: MIT\n---\n"
        ),
        Flavor::Codex => format!("---\nname: claudex\ndescription: {description}\n---\n"),
        Flavor::Pi => format!(
            "---\nname: claudex\ndescription: {description}\nallowed-tools: Bash(claudex:*)\nlicense: MIT\n---\n"
        ),
    };
    format!("{frontmatter}\n{}", body(command_list))
}

/// Render the `AGENTS.md` block, wrapped in idempotency markers.
pub fn agents_block(command_list: &str) -> String {
    format!(
        r#"{AGENTS_START}
## claudex — agent session analytics

Use the `claudex` CLI to query and analyze local Claude Code, OpenAI Codex, and
Pi session transcripts: token cost, tool usage, full-text search, per-session
drill-down, and exports — across all three providers from one index.

- **Spans providers by default;** narrow with `--provider claude|codex|pi`.
- **Filter** with `--project`, `--model`, `--since`/`--until` (`7d`/`2w`/dates).
- **JSON for agents:** add `--json`; every row carries a `provider` key.
- The index is additive — archived/deleted sessions are retained.

Commands:

{command_list}

Run `claudex --help` or `claudex <command> --help` for full flags.
{AGENTS_END}"#
    )
}

/// Render a Claude Code plugin manifest (`.claude-plugin/plugin.json`).
pub fn plugin_json() -> String {
    let manifest = serde_json::json!({
        "name": "claudex",
        "description": description(),
        "version": env!("CARGO_PKG_VERSION"),
        "author": { "name": "James Brink" },
        "homepage": "https://utensils.io/claudex/",
        "repository": "https://github.com/utensils/claudex",
        "license": "MIT",
        "keywords": ["claude-code", "codex", "pi", "cli", "agents", "sessions"],
    });
    serde_json::to_string_pretty(&manifest).unwrap_or_default()
}

//! Shared command-line filtering: the cross-cutting `--provider/--model/
//! --since/--until/--on-disk-only` flags every reporting command accepts,
//! resolved into a [`ResolvedFilter`] that the index queries (and the
//! `--no-index` fallback) apply uniformly.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
pub use claudex::filter::ResolvedFilter;
use claudex::filter::{ProviderKind, parse_when};

/// Provider selector accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProviderArg {
    Claude,
    Codex,
    Copilot,
    #[value(name = "copilot-vscode")]
    CopilotVscode,
    #[value(name = "openclaw")]
    OpenClaw,
    Pi,
}

impl From<ProviderArg> for ProviderKind {
    fn from(provider: ProviderArg) -> Self {
        match provider {
            ProviderArg::Claude => ProviderKind::Claude,
            ProviderArg::Codex => ProviderKind::Codex,
            ProviderArg::Copilot => ProviderKind::Copilot,
            ProviderArg::CopilotVscode => ProviderKind::CopilotVscode,
            ProviderArg::OpenClaw => ProviderKind::OpenClaw,
            ProviderArg::Pi => ProviderKind::Pi,
        }
    }
}

/// Cross-cutting filter flags shared by every reporting command. Flattened into
/// each command alongside its own options (`--project`, `--limit`, …).
#[derive(Args, Clone, Debug, Default)]
pub struct FilterArgs {
    /// Restrict to one or more providers (repeatable or comma-separated).
    /// Default: all indexed providers.
    #[arg(long, value_enum, value_delimiter = ',')]
    pub provider: Vec<ProviderArg>,
    /// Only sessions whose model matches this substring (e.g. `opus`, `gpt-5`).
    #[arg(long)]
    pub model: Option<String>,
    /// Only sessions at/after this time — a date (`2026-01-01`), an RFC3339
    /// timestamp, or a relative span (`7d`, `12h`, `2w`).
    #[arg(long, value_parser = validate_when_arg)]
    pub since: Option<String>,
    /// Only sessions at/before this time (same formats as `--since`).
    #[arg(long, value_parser = validate_when_arg)]
    pub until: Option<String>,
    /// Exclude sessions whose source file has been archived or deleted from
    /// disk (retained in the index by default).
    #[arg(long)]
    pub on_disk_only: bool,
}

impl FilterArgs {
    pub fn resolve(&self) -> Result<ResolvedFilter> {
        let mut providers: Vec<String> = self
            .provider
            .iter()
            .map(|p| ProviderKind::from(*p).id().to_string())
            .collect();
        providers.sort();
        providers.dedup();
        Ok(ResolvedFilter {
            providers,
            model: self.model.clone(),
            since_ms: self
                .since
                .as_deref()
                .map(|s| parse_when(s, false))
                .transpose()?,
            until_ms: self
                .until
                .as_deref()
                .map(|s| parse_when(s, true))
                .transpose()?,
            on_disk_only: self.on_disk_only,
        })
    }
}

// --- `claudex skills` ---

/// Generate or install the agent skill that describes claudex.
#[derive(Subcommand, Debug)]
pub enum SkillCommand {
    /// Write skill files to a directory for review (default ./claudex-skills)
    #[command(after_long_help = crate::cli_help::SKILLS_GENERATE_EXAMPLES)]
    Generate(SkillArgs),
    /// Write skill files into live harness configuration locations
    #[command(after_long_help = crate::cli_help::SKILLS_INSTALL_EXAMPLES)]
    Install(SkillArgs),
}

/// Options shared by `skills generate` and `skills install`.
#[derive(Args, Debug, Clone)]
pub struct SkillArgs {
    /// Harness target(s) to write for (repeatable or comma-separated).
    #[arg(long, value_enum, value_delimiter = ',', default_value = "all")]
    pub target: Vec<SkillTarget>,
    /// Output root (generate) or base directory override (install).
    #[arg(long)]
    pub dir: Option<PathBuf>,
    /// Install to user-level config (~/) instead of the current project.
    #[arg(long)]
    pub global: bool,
    /// Overwrite existing files.
    #[arg(long)]
    pub force: bool,
    /// Output the summary as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Harness flavor a skill is generated for.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillTarget {
    /// Claude Code — `.claude/skills/claudex/SKILL.md`
    ClaudeCode,
    /// OpenAI Codex — `.agents/skills/claudex/SKILL.md`
    Codex,
    /// Pi — `.pi/skills/claudex/SKILL.md`
    Pi,
    /// OpenClaw — `skills/claudex/SKILL.md` or `$OPENCLAW_STATE_DIR/skills/claudex/SKILL.md`
    #[value(name = "openclaw")]
    OpenClaw,
    /// Idempotent block spliced into `AGENTS.md`
    AgentsMd,
    /// Claude Code plugin — `.claude-plugin/plugin.json` + skill
    Plugin,
    /// Expand to claude-code + codex + pi + openclaw + agents-md
    All,
}

pub fn validate_when_arg(value: &str) -> std::result::Result<String, String> {
    parse_when(value, false)
        .map(|_| value.to_string())
        .map_err(|e| e.to_string())
}

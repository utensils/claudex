//! `claudex skills` — generate or install an agent skill describing claudex.
//!
//! `generate` writes files into a directory for review; `install` writes them
//! into live harness configuration locations (project by default, or the
//! user-level config with `--global`). One shared skill body is wrapped in
//! target-appropriate frontmatter (see [`templates`]).

pub mod templates;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::cli::{SkillArgs, SkillCommand, SkillTarget};
use templates::Flavor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Generate,
    Install,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::Generate => "generate",
            Mode::Install => "install",
        }
    }
}

/// A single file to write.
struct Artifact {
    path: PathBuf,
    content: Content,
}

enum Content {
    /// A text file written verbatim (honors `--force` for overwrites).
    Text(String),
    /// A markdown block spliced idempotently into a shared `AGENTS.md`.
    AgentsBlock(String),
}

/// Entry point dispatched from `main`. `root` is the live clap command tree,
/// used to derive the always-accurate command list.
pub fn execute(command: SkillCommand, root: &clap::Command) -> Result<()> {
    let (mode, args) = match command {
        SkillCommand::Generate(args) => (Mode::Generate, args),
        SkillCommand::Install(args) => (Mode::Install, args),
    };

    let targets = expand_targets(&args.target);
    let command_list = templates::command_list(root);

    let mut artifacts = Vec::new();
    for target in targets {
        artifacts.extend(plan_target(target, mode, &args, &command_list)?);
    }

    let mut written = Vec::new();
    for artifact in &artifacts {
        write_artifact(artifact, args.force)?;
        written.push(artifact.path.display().to_string());
    }

    render_summary(args.json, mode, &written);
    Ok(())
}

/// Expand `all` and de-duplicate while preserving order.
fn expand_targets(requested: &[SkillTarget]) -> Vec<SkillTarget> {
    let mut targets = Vec::new();
    let push = |target: SkillTarget, targets: &mut Vec<SkillTarget>| {
        if !targets.contains(&target) {
            targets.push(target);
        }
    };

    for &target in requested {
        match target {
            SkillTarget::All => {
                push(SkillTarget::ClaudeCode, &mut targets);
                push(SkillTarget::Codex, &mut targets);
                push(SkillTarget::Pi, &mut targets);
                push(SkillTarget::OpenClaw, &mut targets);
                push(SkillTarget::AgentsMd, &mut targets);
            }
            other => push(other, &mut targets),
        }
    }
    targets
}

/// Build the artifacts for one target.
fn plan_target(
    target: SkillTarget,
    mode: Mode,
    args: &SkillArgs,
    command_list: &str,
) -> Result<Vec<Artifact>> {
    match target {
        SkillTarget::ClaudeCode => {
            let base = base_dir(mode, args, ".claude", ".claude")?;
            Ok(vec![Artifact {
                path: base.join("skills").join("claudex").join("SKILL.md"),
                content: Content::Text(templates::skill_md(Flavor::ClaudeCode, command_list)),
            }])
        }
        SkillTarget::Codex => {
            let base = base_dir(mode, args, ".agents", ".agents")?;
            Ok(vec![Artifact {
                path: base.join("skills").join("claudex").join("SKILL.md"),
                content: Content::Text(templates::skill_md(Flavor::Codex, command_list)),
            }])
        }
        SkillTarget::Pi => {
            let base = base_dir(mode, args, ".pi", ".pi")?;
            Ok(vec![Artifact {
                path: base.join("skills").join("claudex").join("SKILL.md"),
                content: Content::Text(templates::skill_md(Flavor::Pi, command_list)),
            }])
        }
        SkillTarget::OpenClaw => {
            let base = openclaw_base_dir(mode, args)?;
            Ok(vec![Artifact {
                path: base.join("skills").join("claudex").join("SKILL.md"),
                content: Content::Text(templates::skill_md(Flavor::OpenClaw, command_list)),
            }])
        }
        SkillTarget::AgentsMd => {
            let base = base_dir(mode, args, "", ".codex")?;
            Ok(vec![Artifact {
                path: base.join("AGENTS.md"),
                content: Content::AgentsBlock(templates::agents_block(command_list)),
            }])
        }
        SkillTarget::Plugin => {
            // The plugin is a project artifact; `--global` does not relocate it.
            let base = bundle_base(mode, args).join("claudex");
            Ok(vec![
                Artifact {
                    path: base.join(".claude-plugin").join("plugin.json"),
                    content: Content::Text(templates::plugin_json()),
                },
                Artifact {
                    path: base.join("skills").join("claudex").join("SKILL.md"),
                    content: Content::Text(templates::skill_md(Flavor::ClaudeCode, command_list)),
                },
            ])
        }
        SkillTarget::All => unreachable!("expand_targets removes All"),
    }
}

/// Directory that contains a target's `skills/` (or holds `AGENTS.md`).
fn base_dir(
    mode: Mode,
    args: &SkillArgs,
    project_prefix: &str,
    global_prefix: &str,
) -> Result<PathBuf> {
    match mode {
        Mode::Generate => {
            let root = args
                .dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("claudex-skills"));
            Ok(join_prefix(root, project_prefix))
        }
        Mode::Install if args.global => {
            let root = match &args.dir {
                Some(dir) => dir.clone(),
                None => home_dir()?,
            };
            Ok(join_prefix(root, global_prefix))
        }
        Mode::Install => {
            let root = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));
            Ok(join_prefix(root, project_prefix))
        }
    }
}

/// Root for the project-shaped plugin bundle that `--global` does not move.
fn bundle_base(mode: Mode, args: &SkillArgs) -> PathBuf {
    let default = match mode {
        Mode::Generate => PathBuf::from("claudex-skills"),
        Mode::Install => PathBuf::from("."),
    };
    args.dir.clone().unwrap_or(default)
}

fn openclaw_base_dir(mode: Mode, args: &SkillArgs) -> Result<PathBuf> {
    match mode {
        Mode::Generate => Ok(args
            .dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("claudex-skills"))),
        Mode::Install if args.global => {
            if let Some(dir) = &args.dir {
                return Ok(dir.clone());
            }
            if let Ok(state_dir) = std::env::var("OPENCLAW_STATE_DIR")
                && !state_dir.trim().is_empty()
            {
                return Ok(PathBuf::from(state_dir));
            }
            Ok(home_dir()?.join(".openclaw"))
        }
        Mode::Install => Ok(args.dir.clone().unwrap_or_else(|| PathBuf::from("."))),
    }
}

fn join_prefix(root: PathBuf, prefix: &str) -> PathBuf {
    if prefix.is_empty() {
        root
    } else {
        root.join(prefix)
    }
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("could not determine your home directory for --global install")
}

fn write_artifact(artifact: &Artifact, force: bool) -> Result<()> {
    if let Some(parent) = artifact.path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    match &artifact.content {
        Content::Text(text) => write_new_file(&artifact.path, text.as_bytes(), force),
        Content::AgentsBlock(block) => splice_agents(&artifact.path, block),
    }
}

/// Write a file, refusing to clobber an existing one unless `force` is set.
fn write_new_file(path: &Path, bytes: &[u8], force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            path.display()
        );
    }
    fs::write(path, bytes).with_context(|| format!("could not write {}", path.display()))
}

/// Splice the claudex block into `AGENTS.md`, replacing an existing marked block
/// in place so re-running never duplicates it.
fn splice_agents(path: &Path, block: &str) -> Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();

    let updated = match (
        existing.find(templates::AGENTS_START),
        existing.find(templates::AGENTS_END),
    ) {
        (Some(start), Some(end)) if end > start => {
            let end = end + templates::AGENTS_END.len();
            format!("{}{}{}", &existing[..start], block, &existing[end..])
        }
        _ if existing.trim().is_empty() => format!("{block}\n"),
        _ => format!("{}\n\n{block}\n", existing.trim_end()),
    };

    fs::write(path, updated).with_context(|| format!("could not write {}", path.display()))
}

fn render_summary(json: bool, mode: Mode, written: &[String]) {
    // `generate` writes a review copy; nudge the user toward activating it.
    let hint = (mode == Mode::Generate).then(install_hint);

    if json {
        let mut payload = json!({ "mode": mode.label(), "written": written });
        if let Some(hint) = &hint {
            payload["hint"] = Value::String(hint.clone());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
        return;
    }

    println!("Wrote {} file(s) ({}):", written.len(), mode.label());
    for path in written {
        println!("  {path}");
    }
    if let Some(hint) = &hint {
        println!();
        println!("{hint}");
    }
}

/// Guidance shown after `generate`: how to turn the review copy into a live
/// skill, with a manual-copy escape hatch for non-standard layouts.
fn install_hint() -> String {
    "These files are for review. Activate the skill with `claudex skills install` \
     (add --global for user-level, or --target to choose a harness), or copy them \
     into your harness configuration directories manually."
        .to_string()
}

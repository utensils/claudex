# Agent Skill

claudex ships an [agent skill](https://docs.anthropic.com/en/docs/claude-code/skills) that lets
Claude Code, OpenAI Codex, and Pi — and agents like
[openclaw](https://github.com/utensils/openclaw) — run claudex commands on your behalf without
extra setup. Generate and install it with [`claudex skills`](/commands/skills).

## What is a skill?

A Claude Code skill is a `SKILL.md` file that teaches Claude how to use a tool. When the skill is
loaded, Claude knows every subcommand, flag, and JSON output shape for claudex. It can answer
questions like "which project cost the most this week?" or "find sessions where I worked on the auth
middleware" by running the right `claudex` command and interpreting the result.

## Installing the skill

The easiest way is the built-in [`skills`](/commands/skills) command, which
generates the skill from the live CLI (so it never drifts) and writes it into
the right place for your harness — Claude Code, OpenAI Codex, or Pi.

```bash
claudex skills install --global              # personal: ~/.claude/skills/claudex
claudex skills install                       # project-local: ./.claude/skills/claudex
claudex skills install --target codex,pi     # also install for Codex and Pi
```

Run `claudex skills generate` first if you want to review the files before they
go live — it writes a copy to `./claudex-skills/`. See the
[`skills` command](/commands/skills) for every target and flag.

### Manual install

If you'd rather not run claudex, fetch the committed skill directly:

```bash
mkdir -p ~/.claude/skills/claudex
curl -sL https://raw.githubusercontent.com/utensils/claudex/main/.claude/skills/claudex/SKILL.md \
  -o ~/.claude/skills/claudex/SKILL.md
```

Use `.claude/skills/claudex/` (no `~`) instead for a project-local skill.

## Using the skill

Once installed, you can invoke it directly:

```
/claudex summary
/claudex search "schema migration"
/claudex cost --project myrepo
```

Or just ask Claude naturally — it will invoke the skill automatically when your question is about
session history, costs, or tool usage:

> "How much have I spent on the utensils project this week?"
> "Find all sessions where I worked on the auth middleware."
> "What are my most-used tools across all projects?"

## For agents (openclaw and others)

The skill is designed so autonomous agents can extract structured data reliably. Every subcommand
that produces output supports `--json`, and the skill documents the exact JSON shape for each
command. Agents should:

1. Run `claudex <subcommand> --json` to get machine-readable output.
2. Pipe to `jq` (or parse in-process) for the specific field they need.
3. Never rely on the human-readable table output — column widths and formatting are
   terminal-dependent.

Example agent pattern — find the most expensive project:

```bash
claudex cost --json | jq 'max_by(.cost_usd) | {project, cost_usd}'
```

## Keeping the skill up to date

Pull the latest version any time claudex ships new subcommands or flag changes:

```bash
curl -sL https://raw.githubusercontent.com/utensils/claudex/main/.claude/skills/claudex/SKILL.md \
  -o ~/.claude/skills/claudex/SKILL.md
```

The skill file itself contains a self-update reminder at the bottom with this same command.

## Skill source

The canonical skill lives at
[`.claude/skills/claudex/SKILL.md`](https://github.com/utensils/claudex/blob/main/.claude/skills/claudex/SKILL.md)
in the repository.

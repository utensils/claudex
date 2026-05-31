# skills

Generate or install the claudex [agent skill](/guide/skill) for Claude Code,
OpenAI Codex, Pi, or OpenClaw — a `SKILL.md` that teaches an agent how to drive claudex on
your behalf.

```bash
claudex skills generate            # write a review copy to ./claudex-skills
claudex skills install             # write into live config (project)
claudex skills install --global    # write into your user-level config (~/)
```

The skill body is generated from the live CLI, so it never drifts from the real
command set and flags.

## Subcommands

| Command                   | Effect                                                                      |
| ------------------------- | --------------------------------------------------------------------------- |
| `claudex skills generate` | Write skill files into a directory for review (default `./claudex-skills`). |
| `claudex skills install`  | Write skill files into live harness configuration locations.                |

## Options

Both subcommands accept the same flags:

| Flag           | Description                                                                    |
| -------------- | ------------------------------------------------------------------------------ |
| `--target <t>` | Which harness(es) to write for (repeatable or comma-separated). Default `all`. |
| `--dir <path>` | Output root (`generate`) or base directory override (`install`).               |
| `--global`     | Install into your user-level config (`~/`) instead of the current project.     |
| `--force`      | Overwrite existing files.                                                      |
| `--json`       | Emit the summary as JSON (`{ "mode", "written", "hint" }`).                    |

## Targets

| Target        | Writes                                                                                                     |
| ------------- | ---------------------------------------------------------------------------------------------------------- |
| `claude-code` | `.claude/skills/claudex/SKILL.md` (with `allowed-tools` + `argument-hint`)                                 |
| `codex`       | `.agents/skills/claudex/SKILL.md` (minimal Agent Skills subset)                                            |
| `pi`          | `.pi/skills/claudex/SKILL.md`                                                                              |
| `openclaw`    | `skills/claudex/SKILL.md`, or `${OPENCLAW_STATE_DIR:-~/.openclaw}/skills/claudex/SKILL.md` with `--global` |
| `agents-md`   | An idempotent block spliced into `AGENTS.md` (project root, or `~/.codex/AGENTS.md` with `--global`)       |
| `plugin`      | `.claude-plugin/plugin.json` + skill, as a distributable Claude Code plugin                                |
| `all`         | Expands to `claude-code` + `codex` + `pi` + `openclaw` + `agents-md`                                       |

## Examples

```bash
# Install the skill for Claude Code and Codex in this project
claudex skills install --target claude-code,codex

# Install globally for Pi
claudex skills install --target pi --global

# Install globally for OpenClaw
claudex skills install --target openclaw --global

# Generate a plugin bundle for distribution
claudex skills generate --target plugin --dir ./dist
```

`generate` writes a review copy and prints how to activate it; `install` writes
straight into the live locations. Re-running `install` refuses to overwrite an
existing file unless you pass `--force` — except the `AGENTS.md` block, which is
spliced in place idempotently (re-running never duplicates it).

::: tip Keep the committed skill in sync
The skill checked into the claudex repo at `.claude/skills/claudex/SKILL.md` is
itself generated. Maintainers regenerate it with
`claudex skills generate --target claude-code --dir . --force`; a test fails if
it drifts.
:::

See the [Claude Code Skill guide](/guide/skill) for how skills are loaded and
used.

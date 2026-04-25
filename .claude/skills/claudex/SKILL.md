---
name: claudex
description: Query, search, and analyze Claude Code sessions using the claudex CLI. Use when asked about session history, token costs, tool usage, search past conversations, export sessions, or inspect Claude Code activity across projects.
argument-hint: [subcommand or query]
allowed-tools: Bash, Read, Glob, Grep
---

# claudex — Claude Code Session Analysis CLI

claudex reads the JSONL transcripts Claude Code writes under `~/.claude/projects/`, indexes them into a local SQLite database at `~/.claudex/index.db`, and exposes reports as subcommands.

## Quick Reference

```bash
claudex summary                          # Dashboard: sessions, cost, tools, models
claudex cost                             # Token cost by project
claudex sessions                         # List sessions grouped by project
claudex search "schema migration"        # Full-text search across all sessions
claudex session e1a2f4                   # Drill into a single session
claudex export e1a2f4 --output out.md    # Export session as Markdown
claudex tools                            # Tool usage frequency
claudex models                           # Model breakdown (calls, tokens, cost)
claudex turns                            # Per-turn timing (avg, p50, p95, max)
claudex prs                              # Sessions linked to pull requests
claudex files                            # Most-modified files across sessions
claudex index                            # Trigger incremental index sync
claudex index --force                    # Wipe and rebuild index from scratch
claudex update                           # Self-update to latest release
```

## Global Options

Every subcommand accepts:

| Flag | Description |
|------|-------------|
| `--json` | Machine-readable JSON output (most subcommands) |
| `--no-index` | Skip SQLite index, scan JSONL files directly |
| `--color auto\|always\|never` | Color output control (honors `NO_COLOR`) |

**For agents**: always pass `--json` and pipe to `jq` for reliable parsing. The `--no-index` path produces matching results when the index hasn't been synced yet.

## How to Use This Skill

Parse `$ARGUMENTS` to determine the action:

- If arguments look like a **question** about sessions, cost, or activity, pick the relevant subcommand(s)
- If arguments start with a **subcommand** (`summary`, `cost`, `sessions`, `search`, `session`, `export`, `tools`, `models`, `turns`, `prs`, `files`, `index`, `update`, `completions`), run that subcommand
- If arguments include a **query string** (e.g. "find sessions about Rust"), use `claudex search`
- If no arguments, run `claudex summary`

## Dashboard

```bash
claudex summary              # Human-readable dashboard
claudex summary --json       # Machine-readable metrics
```

JSON shape:
```json
{
  "total_sessions": 142,
  "sessions_today": 3,
  "sessions_this_week": 18,
  "total_cost_usd": 47.23,
  "cost_this_week_usd": 8.14,
  "total_input_tokens": 1234567,
  "total_output_tokens": 456789,
  "total_cache_creation_tokens": 98765,
  "total_cache_read_tokens": 234567,
  "total_tokens": 2024688,
  "thinking_block_count": 341,
  "avg_turn_duration_ms": 4823,
  "pr_count": 27,
  "files_modified_count": 1893,
  "top_projects": [{"project": "myrepo", "sessions": 34}],
  "top_tools": [{"tool": "Bash", "calls": 4201}],
  "top_stop_reasons": [{"stop_reason": "end_turn", "count": 98}],
  "model_distribution": [{"model": "claude-opus-4-7", "sessions": 55, "cost_usd": 32.10}],
  "most_recent": {"project": "myrepo", "session_id": "abc123...", "date": "2026-04-24T..."}
}
```

## Sessions

List sessions grouped by project:

```bash
claudex sessions                          # Most recent 20
claudex sessions --limit 50              # More results
claudex sessions --project myrepo        # Filter by project name (substring)
claudex sessions --file src/main.rs      # Only sessions that touched this file
claudex sessions --json                  # Machine-readable
```

## Session Drill-Down

Detailed report for a single session:

```bash
claudex session e1a2f4                   # By session ID prefix
claudex session myrepo                   # By project name (picks most recent)
claudex session e1a2f4 --project foo     # Disambiguate when prefix is ambiguous
claudex session e1a2f4 --json
```

The `--json` output includes per-model token breakdown, tool call counts, file modifications, stop reasons, PR links, and thinking block details.

## Cost

Token usage and approximate cost (Opus/Sonnet/Haiku pricing tiers applied per message):

```bash
claudex cost                             # Total cost by project
claudex cost --limit 10                  # Top 10 projects by cost
claudex cost --project utensils          # Filter to one project
claudex cost --per-session               # Break down by session, not project
claudex cost --json
```

JSON shape per row:
```json
[{"project": "myrepo", "sessions": 14, "cost_usd": 12.34, "input_tokens": 500000, "output_tokens": 200000}]
```

## Full-Text Search

Search across all session messages (uses SQLite FTS5):

```bash
claudex search "schema migration"
claudex search serde --project claudex --limit 5
claudex search "async runtime" --json
claudex search "TODO" --case-sensitive    # FTS5 is case-insensitive by default;
                                          # --case-sensitive falls back to file scan
```

Each hit shows: project, session ID prefix, date, role (user/assistant), and the matching line with the query highlighted.

**For agents**: use `--json` to get structured hits:
```json
[{"project": "...", "session_id": "...", "timestamp_ms": 1714000000000, "role": "assistant", "text": "..."}]
```

## Tools

Tool usage frequency across all sessions:

```bash
claudex tools                            # Aggregate across all projects
claudex tools --project myrepo           # One project
claudex tools --per-session              # Break down per session
claudex tools --limit 15 --json
```

## Models

Model usage breakdown:

```bash
claudex models                           # All projects
claudex models --project myrepo          # One project
claudex models --json
```

JSON shape:
```json
[{"model": "claude-opus-4-7", "calls": 1234, "input_tokens": 5000000, "output_tokens": 1500000, "cost_usd": 45.67}]
```

## Turns

Per-turn timing analysis (how long each Claude response takes):

```bash
claudex turns                            # Avg, p50, p95, max across projects
claudex turns --project myrepo
claudex turns --limit 10 --json
```

JSON shape:
```json
[{"project": "myrepo", "turn_count": 203, "avg_duration_ms": 5120, "p50_duration_ms": 3800, "p95_duration_ms": 18500, "max_duration_ms": 42100}]
```

## PRs

Sessions linked to pull requests (detected from `gh pr create` and similar tool calls):

```bash
claudex prs                              # All PR-linked sessions
claudex prs --project myrepo
claudex prs --limit 10 --json
```

## Files

Most-modified files across sessions:

```bash
claudex files                            # Top files globally
claudex files --project myrepo           # One project
claudex files --path src/               # Filter by path substring
claudex files --limit 25 --json
```

JSON shape:
```json
[{"file": "src/main.rs", "modification_count": 47, "session_count": 12}]
```

## Export

Export a session transcript:

```bash
claudex export e1a2f4                    # Markdown to stdout
claudex export e1a2f4 --output sess.md  # Write to file
claudex export e1a2f4 --format json     # JSON transcript
claudex export myrepo --project myrepo  # Disambiguate by project
```

The Markdown export includes all messages (user + assistant) with tool calls formatted as code blocks.

## Index Management

The index syncs automatically (5-minute staleness window) on every read command. Use these only when you need explicit control:

```bash
claudex index                            # Incremental sync right now
claudex index --force                    # Drop and fully rebuild the index
```

Run `claudex index --force` after schema migrations (the binary also auto-rebuilds when it detects a schema version bump).

## Self-Update

```bash
claudex update                           # Update to latest release
claudex update --check                   # Check without writing to disk
claudex update --version v0.2.0          # Install a specific tag
claudex update --force                   # Reinstall even if already current
```

For Nix/Cargo/Homebrew installs, `claudex update` prints the right upgrade command and exits non-zero (safe in automation).

## Shell Completions

```bash
claudex completions zsh   >> ~/.zshrc    # zsh
claudex completions bash  >> ~/.bashrc   # bash
claudex completions fish  > ~/.config/fish/completions/claudex.fish
claudex completions elvish
claudex completions powershell
```

## Agent Patterns

**Get structured summary for a report:**
```bash
claudex summary --json | jq '{sessions: .total_sessions, cost: .total_cost_usd, top_project: .top_projects[0].project}'
```

**Find the most expensive project this week:**
```bash
claudex cost --json | jq 'max_by(.cost_usd) | {project, cost_usd}'
```

**Search for past work on a topic and get session IDs:**
```bash
claudex search "database migration" --json | jq '.[].session_id' | sort -u
```

**Inspect a specific session by prefix:**
```bash
claudex session e1a2f4 --json | jq '{project, cost_usd: .cost.total_usd, tools: .tool_calls | to_entries | sort_by(-.value) | .[0:5]}'
```

**Find sessions that touched a specific file:**
```bash
claudex sessions --file migrations/ --json | jq '.[].session_id'
```

**p95 turn duration for one project:**
```bash
claudex turns --project claudex --json | jq '.[0].p95_duration_ms'
```

**Export the most recent session as Markdown:**
```bash
claudex sessions --limit 1 --json | jq -r '.[0].session_id' | xargs -I{} claudex export {}
```

## Key Invariants for Agents

- **Index staleness window is 300s** — the first read after 5 minutes triggers an incremental sync automatically. You rarely need `claudex index` explicitly.
- **`--no-index` always matches** — if you bypass the index, results are identical; useful for freshness-critical queries.
- **Session IDs are UUID prefixes** — 6+ hex chars disambiguate; if ambiguous, pass `--project` to filter candidates.
- **Project names are path-decoded** — `/.hidden` in the filesystem shows as `--hidden` in output; `-seg` shows as `/seg`. Use the displayed name for `--project` filters.
- **Costs are approximate** — Opus/Sonnet/Haiku tiers applied to raw token counts. Not billing-exact.
- **`--json` output shape is stable** — safe to script against; breaking changes would be a semver bump.

## Practical Tips

- Use `claudex summary` for a quick health check of your Claude Code activity
- Use `claudex cost --per-session --project foo` to find expensive sessions in a specific project
- Use `claudex search` to locate past conversations about a topic before starting a new session
- Use `claudex files --project foo` to see what a project's "hot" files are across all sessions
- Use `claudex export` + `--format json` when you need to feed session content to another tool
- Use `claudex turns` to diagnose projects where Claude is consistently slow
- The index lives at `~/.claudex/index.db` — safe to delete; it rebuilds on next run

## Updating This Skill

This skill is maintained in the claudex repository on GitHub. To pull the latest version:

```bash
# Source repository
https://github.com/utensils/claudex

# Skill file location within the repo
.claude/skills/claudex/SKILL.md

# Fetch the latest skill directly
curl -sL https://raw.githubusercontent.com/utensils/claudex/main/.claude/skills/claudex/SKILL.md \
  -o ~/.claude/skills/claudex/SKILL.md
```

When copying this skill to other workspaces or agents, always pull from `main` to get the latest subcommand flags and JSON shapes.

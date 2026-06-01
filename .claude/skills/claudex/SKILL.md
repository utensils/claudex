---
name: claudex
description: Query, search, and analyze Claude Code, OpenAI Codex, Pi, and OpenClaw sessions using the claudex CLI. Use when asked about session history, token costs, tool usage, search past conversations, export sessions, or inspect agent activity across providers and projects.
argument-hint: [subcommand or query]
allowed-tools: Bash(claudex:*), Read, Glob, Grep
license: MIT
---

# claudex — multi-provider agent session analytics

claudex indexes the local session transcripts of four coding agents into a
SQLite database at `~/.claudex/index.db` and reports across all of them:

- **Claude Code** — `~/.claude/projects/**.jsonl`
- **OpenAI Codex** — `~/.codex/sessions/**` and `~/.codex/archived_sessions/`
- **Pi** — `~/.pi/agent/sessions/**`
- **OpenClaw** — `${OPENCLAW_STATE_DIR:-~/.openclaw}/agents/*/sessions/`

Every reporting command spans all four providers by default. The index is
**additive**: sessions archived or deleted from disk are retained, so historical
usage never disappears.

## Commands

- `claudex sessions` — List sessions grouped by project
- `claudex cost` — Token usage and approximate cost report
- `claudex search` — Full-text search across session messages
- `claudex tools` — Tool usage frequency report
- `claudex watch` — Tail Claude Code's debug log in real time with formatted output
- `claudex summary` — Dashboard overview of sessions, cost, and tool usage
- `claudex session` — Detailed report for a single session
- `claudex export` — Export session transcripts to markdown or JSON
- `claudex index` — Manage the session index (normally updated automatically)
- `claudex turns` — Per-turn timing analysis (avg, p50, p95, max duration)
- `claudex prs` — PR linkage report — sessions linked to pull requests
- `claudex files` — Most frequently modified files across sessions
- `claudex models` — Model usage breakdown — call counts, token usage, cost per model
- `claudex update` — Self-update to the latest claudex release (or a specific tag)
- `claudex completions` — Generate shell completions
- `claudex skills` — Generate or install the claudex agent skill for Claude Code, Codex, Pi, or OpenClaw

Run `claudex <command> --help` for full flags.

## Filtering

| Flag | Effect |
| --- | --- |
| `--provider <claude\|codex\|pi\|openclaw>` | Restrict indexed reports to provider(s); repeatable or comma-separated. Default: all. |
| `--model <substr>` | Filter indexed reports by model (e.g. `opus`, `gpt-5`). |
| `--since <when>` / `--until <when>` | Date range. Accepts `YYYY-MM-DD`, RFC3339, or a relative span (`7d`, `12h`, `2w`). |
| `--on-disk-only` | Exclude retained sessions whose file was archived/deleted. |
| `--project <substr>` | Filter by project path substring on commands that expose project scoping. |
| `--json` | Machine-readable output. Row-oriented reports include a `provider` key per row. |
| `--no-index` | Scan Claude transcripts directly; this rejects non-Claude providers. |

Provider/date/model filters work on indexed reporting commands including
`summary`, `sessions`, `cost`, `tools`, `models`, `search`, `turns`, `prs`, and
`files`. Session drill-down resolves OpenClaw/Codex/Pi sessions through indexed
records. Use `--no-index` only for Claude transcript recovery/debugging.

## When to use

- "How much have I spent on Codex this month?" → `claudex cost --provider codex --since 30d`
- "List my recent Pi sessions" → `claudex sessions --provider pi`
- "Search OpenClaw trajectory-backed sessions" → `claudex search "tool timeout" --provider openclaw --json`
- "Find where I discussed schema migrations" → `claudex search "schema migration"`
- "What did session e1a2f4 do?" → `claudex session e1a2f4`
- "Overall dashboard" → `claudex summary`
- "Model cost breakdown across providers" → `claudex models`

## Output for agents

Add `--json` to any reporting command for stable, scriptable output. Each row
carries a `"provider"` key so results are unambiguous across providers. Cost is
in USD; Pi/OpenClaw sessions report the provider's own per-message cost when
available (local models are $0), Claude/Codex are priced from a built-in
per-model table.

## Notes

- The index refreshes automatically (staleness window 5 min); `claudex index`
  forces a sync, `claudex index --force` wipes and rebuilds (the only path that
  discards retained data).
- Worktree sessions roll up to their parent project; Claude subagent transcripts
  roll up to their parent session.

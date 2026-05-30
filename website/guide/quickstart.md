# Quickstart

This is the five-minute tour. It assumes you've
[installed claudex](/guide/installation) and run at least one of Claude Code,
OpenAI Codex, or Pi locally so there are transcripts to index.

## 1. See the dashboard

```bash
claudex summary
```

You'll get one screenful: total sessions, today/this-week counts, cost totals,
top projects, top tools, model distribution, and the most recent session.

First run indexes every provider's transcripts (`~/.claude/projects`,
`~/.codex`, `~/.pi/agent`) — the spinner shows progress on stderr. Subsequent
runs reuse the index (5-minute staleness window), so they're near-instant.

## 2. Narrow by provider

Every report spans all three providers by default. Narrow with `--provider`:

```bash
claudex cost --provider codex          # just OpenAI Codex
claudex sessions --provider pi         # just Pi
claudex cost --provider claude,codex   # Claude + Codex combined
```

See [Providers & filtering](/guide/providers) for the full filter set.

## 3. Cost, by project

```bash
claudex cost --limit 10
```

Aggregates token usage across every session in each project and every provider,
applies the correct pricing tier per model (Opus / Sonnet / Haiku, OpenAI
gpt-5 / gpt-4; Pi reports its own cost), and sorts by cost descending.

Add `--per-session` to break it out by individual session,
`--project utensils` to filter to projects whose path contains `utensils`, or
`--since 30d` to scope to the last 30 days.

## 4. Full-text search

```bash
claudex search "schema migration"
claudex search serde --project claudex --limit 5
```

Uses SQLite's FTS5 virtual table. Case-insensitive by default; pass
`--case-sensitive` to drop back to a file scan (FTS5 doesn't support
case-sensitive queries).

Each hit shows the project, session ID prefix, date, role, and the matching
line with the query highlighted.

## 5. Tool and model usage

```bash
claudex tools --limit 15     # How often you reach for Bash, Edit, Read, ...
claudex models               # Calls, token usage, cost per model
```

## 6. Export a session

```bash
# Find a recent session and grab its prefix
claudex sessions --limit 5

# Export as Markdown (default)
claudex export e1a2f4 --output my-session.md

# Or as JSON
claudex export e1a2f4 --format json --output my-session.json
```

The selector matches by session-ID prefix _or_ by project name (substring).
Combine `--project` for disambiguation when a short prefix is ambiguous.

## Bonus: pipe to jq

Every report supports `--json`:

```bash
# Top three cost projects, numeric only
claudex cost --json --limit 3 \
  | jq '.[] | {project, cost_usd}'

# p95 turn duration for one project
claudex turns --project claudex --json \
  | jq '.[0].p95_duration_ms'
```

## Bonus: live tail

When you want structured output from a running Claude Code session:

```bash
# In one shell
claudex watch

# In another shell
claude --debug-file ~/.claudex/debug/latest.log
```

claudex watches the debug log, formats tool calls, and inserts a banner every
time `claude` starts a new session (which truncates the log).

See [Watch mode](/guide/watch) for details.

## Where next?

- [The index](/guide/indexing) — when it syncs, when to force a rebuild.
- [Commands overview](/commands/) — every subcommand.
- [Recipes](/guide/recipes) — one-liners for common questions.

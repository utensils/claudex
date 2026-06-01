# What is claudex?

claudex is a Rust CLI that reads the local session transcripts of four coding
agents — **Claude Code** (`~/.claude/projects/`), **OpenAI Codex** (`~/.codex/`),
**Pi** (`~/.pi/agent/`), and **OpenClaw** (`${OPENCLAW_STATE_DIR:-~/.openclaw}`) — ingests them into a single SQLite index at
`~/.claudex/index.db`, and exposes reports as subcommands.

Each agent persists every conversation — every user message, every assistant
reply, every tool call, every token-usage block, every file edit, every PR link.
Those files are flat logs in several different formats, hard to query by hand.
claudex normalizes them into one store you can actually _ask questions of_,
across every provider.

## The shape of the tool

```
 ~/.claude/projects/**.jsonl   (Claude Code)  ┐
 ~/.codex/sessions/**          (OpenAI Codex) ├─ provider transcripts
 ~/.pi/agent/sessions/**       (Pi)           │
 ~/.openclaw/agents/*/sessions (OpenClaw)     ┘
         │
         ▼
 claudex providers  →  SQLite index (~/.claudex/index.db)
         │
         ▼
 claudex <subcommand>  →  table + palette on TTY, --json for pipelines
```

Every read command:

- Spans all four providers by default; narrow with the shared
  [filter flags](/guide/providers) (`--provider`, `--since`/`--until`, …).
- Supports `--json` to emit a stable, machine-readable shape (with a `provider`
  key on every row).
- Honors `--color auto|always|never` (and `NO_COLOR`) for color output.
- Uses the index by default (per-provider incremental sync, 5-minute staleness
  window).

The index is **additive**: sessions you archive or delete from disk are
retained. `models`, `turns`, `prs`, and `files` are derived from the index only
and don't accept `--no-index`; the `--no-index` fallback (where offered) scans
Claude transcripts directly. See the [flag support matrix](/commands/) for the
per-command breakdown.

## Who is it for?

You use Claude Code, OpenAI Codex, Pi, or OpenClaw and want to:

- Understand where your token spend is going (per-provider, per-project,
  per-session, per-model).
- Search across past conversations from every agent without grepping logs.
- See which files get modified the most across all your projects.
- Track how sessions turn into PRs.
- Measure turn latency (avg, p50, p95, max).
- Export a past session as Markdown or JSON.

If you've never run any of the supported agents locally, there's nothing for
claudex to read — the provider directories will be empty.

## What it isn't

- **Not a launcher for Claude Code.** You still start sessions with `claude`;
  claudex just reads what those sessions wrote.
- **Not a sync service.** Everything lives locally. No network calls.
- **Not authoritative pricing.** Costs are _approximate_ — they apply published
  Opus / Sonnet / Haiku tiers to the token-usage blocks in each record. See
  [Pricing model](/reference/pricing).

## Where to go next

- [Installation](/guide/installation) — Cargo, Nix flake, from source.
- [Quickstart](/guide/quickstart) — the five commands you'll run in the first
  minute.
- [How it works](/guide/architecture) — parser, store, index, and how fallbacks
  stay in sync.
- [Commands overview](/commands/) — every subcommand, with flags and examples.

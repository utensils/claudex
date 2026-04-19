# What is claudex?

claudex is a Rust CLI that reads the JSONL transcripts Claude Code writes under
`~/.claude/projects/`, ingests them into a local SQLite index at
`~/.claudex/index.db`, and exposes reports as subcommands.

Claude Code already persists every conversation you have with it — every user
message, every assistant reply, every tool call, every token-usage block, every
file edit, every PR link. Those files are flat JSONL logs, and they're hard to
query by hand. claudex turns them into something you can actually _ask
questions of_.

## The shape of the tool

```
 ~/.claude/projects/<encoded-path>/<session>.jsonl
         │   (Claude Code writes these)
         ▼
 claudex parser  →  SQLite index (~/.claudex/index.db)
         │
         ▼
 claudex <subcommand>  →  table + palette on TTY, --json for pipelines
```

Every read command:

- Uses the index by default (incremental sync, 5-minute staleness window).
- Supports `--no-index` to bypass the index and scan JSONL files directly — the
  fallback path always matches the indexed path.
- Supports `--json` to emit a stable, machine-readable shape.
- Honors `--color auto|always|never` (and `NO_COLOR`) for color output.

## Who is it for?

You already use Claude Code and want to:

- Understand where your token spend is going (per-project, per-session,
  per-model).
- Search across past conversations without grepping JSONL by hand.
- See which files get modified the most across all your projects.
- Track how sessions turn into PRs.
- Measure turn latency (avg, p50, p95, max).
- Export a past session as Markdown or JSON.

If you've never run `claude` locally, there's nothing for claudex to read —
`~/.claude/projects/` will be empty.

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

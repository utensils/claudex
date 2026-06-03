---
layout: home

hero:
  name: claudex
  text: Query your AI coding sessions
  tagline:
    A Rust CLI that indexes the local transcripts of Claude Code, OpenAI Codex,
    Pi, and OpenClaw into one SQLite database and turns them into reports —
    cost, tools, turns, PRs, full-text search, and more, across every provider.
  actions:
    - theme: brand
      text: Get Started →
      link: /guide/
    - theme: alt
      text: Commands
      link: /commands/
    - theme: alt
      text: GitHub
      link: https://github.com/utensils/claudex

features:
  - title: Four providers, one index
    details: Claude Code (~/.claude/projects), OpenAI Codex (~/.codex), Pi
      (~/.pi/agent), and OpenClaw (~/.openclaw) are all first-class. Every
      report spans them by default; narrow with --provider claude|codex|pi|openclaw.
  - title: Additive & retentive
    details: Archive or delete a session from disk and its indexed data stays.
      Non-destructive schema migrations and per-provider incremental sync mean
      historical usage never disappears.
  - title: Reports out of the box
    details:
      summary, sessions, cost, tools, models, turns, files, prs, search, export
      — plus a live log watcher and a skill generator. Read commands support
      --json; Claude reports also support --no-index.
  - title: Proper filtering everywhere
    details:
      --provider, --project, --model, and --since / --until (dates, RFC3339, or
      spans like 7d / 2w) on every reporting command. --json always carries a
      provider key for unambiguous scripting.
  - title: Honest pricing math
    details:
      Opus / Sonnet / Haiku and OpenAI gpt-5 / gpt-4 tiers, applied per model.
      Pi and OpenClaw report their own per-message cost when available (local
      models = $0). Sub-cent values fall back to four decimals.
  - title: FTS5 full-text search
    details:
      The index ships with a messages_fts virtual table. Search across every
      user and assistant message in every session and provider, with SQLite's
      FTS5 ranking.
  - title: Worktree & subagent aware
    details: Worktree sessions aggregate to the parent project; Claude subagent
      transcripts roll up to their parent session. Group-by-project queries do
      the right thing automatically.
  - title: Single binary, no daemon
    details:
      Built with rusqlite (bundled), clap, and owo-colors. Runs on Linux and
      macOS. Nix flake included. cargo install or build straight from source.
---

## At a glance

<div class="terminal">
<span class="prompt">$</span> claudex summary<br>
<br>
<span class="comment"># How much have I spent on Codex this month?</span><br>
<span class="prompt">$</span> claudex cost --provider codex --since 30d<br>
<br>
<span class="comment"># Top 5 projects by cost across all providers</span><br>
<span class="prompt">$</span> claudex cost --limit 5<br>
<br>
<span class="comment"># Which files get touched most across all my projects?</span><br>
<span class="prompt">$</span> claudex files --limit 10<br>
<br>
<span class="comment"># Find every session where I discussed "migrations"</span><br>
<span class="prompt">$</span> claudex search migrations<br>
<br>
<span class="comment"># Export one session as Markdown</span><br>
<span class="prompt">$</span> claudex export &lt;session-prefix&gt; --output session.md<br>
</div>

## Install

```bash
# With Cargo
cargo install claudex-cli

# Or with Nix flakes
nix run github:utensils/claudex -- summary

# Or from a local checkout
git clone https://github.com/utensils/claudex
cd claudex && nix develop && cargo build --release -p claudex-cli --bin claudex
```

See the full [installation guide](/guide/installation) for Nix, devshell, and
shell-completion setup.

## Why claudex?

Claude Code, OpenAI Codex, Pi, and OpenClaw each persist every conversation as local
transcripts — every user turn, every assistant message, every tool call, every
token-usage block, every file modification — but those files are flat logs in
several different formats, not a queryable store.

claudex reads them once, normalizes them into one SQLite index, and gives
you a CLI that answers questions like:

- _How much have I spent on Codex versus Claude this month?_
- _Which project burned the most Opus tokens last week?_
- _What's my p95 turn duration in this repo?_
- _Show me every session that linked a PR._
- _Full-text search: where did I first discuss the schema migration?_
- _How many times have I edited `src/index.rs` across sessions?_

The index is **additive** — sessions you archive or delete from disk stay in
your history. No cloud. No daemon. No background service. Just a small Rust
binary and a SQLite file under `~/.claudex/`.

## Next steps

- **New here?** Start with [What is claudex?](/guide/) and
  [Quickstart](/guide/quickstart).
- **Hunting for a specific command?** Jump to
  [Commands overview](/commands/).
- **Piping to jq or building dashboards?** See
  [JSON output](/guide/json-output) and the
  [index schema](/reference/schema).

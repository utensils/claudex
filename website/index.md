---
layout: home

hero:
  name: claudex
  text: Query your Claude Code sessions
  tagline:
    A Rust CLI that indexes every JSONL transcript Claude Code writes under
    ~/.claude/projects/ and turns them into reports — cost, tools, turns, PRs,
    full-text search, and more.
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
  - title: Every session, indexed
    details: Scans ~/.claude/projects/ into a local SQLite database at
      ~/.claudex/index.db. Incremental sync keyed on (path, size, mtime);
      staleness window is 5 minutes.
  - title: Reports out of the box
    details:
      summary, sessions, cost, tools, models, turns, files, prs, search, export,
      codex — plus a live log watcher. Read commands support --json; most
      Claude Code reports also support --no-index.
  - title: Honest pricing math
    details:
      Separate Opus / Sonnet / Haiku tiers applied per-message from the model
      field in each record. Sub-cent values fall back to four decimals so tiny
      sessions don't round to $0.00.
  - title: Worktree-aware
    details:
      Sessions inside .claude/worktrees/&lt;branch&gt; aggregate to the parent
      project and render as "project (worktree)". Group-by-project queries do
      the right thing automatically.
  - title: FTS5 full-text search
    details:
      The index ships with a messages_fts virtual table. Search across every
      user and assistant message in every session, filtered by project, with
      SQLite's FTS5 ranking.
  - title: Live tail with structure
    details: claudex watch tails Claude Code's --debug-file log in real time,
      formats tool calls, detects new sessions, and separates them with a
      banner. --raw drops back to plain output.
  - title: JSON or TTY — your choice
    details: Human output uses a minimal comfy-table layout with dynamic width
      detection. --json emits a stable shape for pipelines, grep, and jq.
  - title: Single binary, no daemon
    details:
      Built with rusqlite (bundled), clap, and owo-colors. Runs on Linux and
      macOS. Nix flake included. cargo install or build straight from source.
---

## At a glance

<div class="terminal">
<span class="prompt">$</span> claudex summary<br>
<br>
<span class="comment"># Codex CLI activity from ~/.codex</span><br>
<span class="prompt">$</span> claudex codex<br>
<br>
<span class="comment"># Top 5 projects by cost, last 30 days</span><br>
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
cargo install --git https://github.com/utensils/claudex

# Or with Nix flakes
nix run github:utensils/claudex -- summary

# Or from a local checkout
git clone https://github.com/utensils/claudex
cd claudex && nix develop && cargo build --release
```

See the full [installation guide](/guide/installation) for Nix, devshell, and
shell-completion setup.

## Why claudex?

Claude Code persists every conversation as JSONL under
`~/.claude/projects/<encoded-path>/<session>.jsonl`. That's a gold mine — it
records every user turn, every assistant message, every tool call, every
token-usage block, every file modification — but those files are flat logs, not
a queryable store.

claudex reads them once, indexes the parts you actually want to ask questions
about, and gives you a CLI that answers questions like:

- _Which project burned the most Opus tokens last week?_
- _What's my p95 turn duration in this repo?_
- _Show me every session that linked a PR._
- _Full-text search: where did I first discuss the schema migration?_
- _How many times have I edited `src/index.rs` across sessions?_

No cloud. No daemon. No background service. Just a small Rust binary and a
SQLite file under `~/.claudex/`.

## Next steps

- **New here?** Start with [What is claudex?](/guide/) and
  [Quickstart](/guide/quickstart).
- **Hunting for a specific command?** Jump to
  [Commands overview](/commands/).
- **Piping to jq or building dashboards?** See
  [JSON output](/guide/json-output) and the
  [index schema](/reference/schema).

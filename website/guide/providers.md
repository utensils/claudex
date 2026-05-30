# Providers & filtering

claudex indexes three coding agents into one database and reports across all of
them. Every reporting command spans every provider by default; a shared set of
filter flags narrows the view.

## The three providers

| Provider                   | Source                                                     | Notes                                                                                                                                                                              |
| -------------------------- | ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Claude Code** (`claude`) | `~/.claude/projects/**.jsonl`                              | Worktrees aggregate to the parent project; subagent transcripts roll up to their parent session.                                                                                   |
| **OpenAI Codex** (`codex`) | `~/.codex/sessions/**` and `~/.codex/archived_sessions/**` | Project comes from the transcript's `cwd`; sessions under `archived_sessions/` are flagged archived. Token usage is the last cumulative `token_count` (cached input → cache read). |
| **Pi** (`pi`)              | `~/.pi/agent/sessions/**`                                  | Multi-provider under the hood; claudex trusts Pi's own per-message cost (local Ollama models report `$0`).                                                                         |

A provider is indexed only if its data directory exists, so you never need to
opt in — install Codex or Pi and claudex picks them up on the next sync.

## The index is additive

claudex's index is **retentive, not a cache**. When you archive or delete a
session from disk, its indexed data is kept — flagged as no longer on disk, but
still counted in cost, search, and history. Schema upgrades migrate in place
(they never drop tables), so your usage history survives across versions.

The only command that discards retained data is `claudex index --force`, which
wipes and rebuilds from whatever is currently on disk.

Use `--on-disk-only` on a report to exclude retained/archived sessions and see
just what's live on disk right now.

## Filtering flags

Every reporting command (`sessions`, `cost`, `search`, `tools`, `models`,
`turns`, `prs`, `files`) accepts the same cross-cutting filters:

| Flag                             | Effect                                                                          |
| -------------------------------- | ------------------------------------------------------------------------------- |
| `--provider <claude\|codex\|pi>` | Restrict to one or more providers. Repeatable or comma-separated. Default: all. |
| `--project <substr>`             | Filter by project path substring.                                               |
| `--model <substr>`               | Filter by model (e.g. `opus`, `gpt-5`).                                         |
| `--since <when>`                 | Only sessions at/after this time.                                               |
| `--until <when>`                 | Only sessions at/before this time.                                              |
| `--on-disk-only`                 | Exclude retained sessions whose file was archived or deleted.                   |
| `--json`                         | Machine-readable output; always includes a `provider` key per row.              |

### Time values

`--since` and `--until` accept three formats:

- **A date** — `2026-01-15` (start of day for `--since`, end of day for `--until`).
- **An RFC3339 timestamp** — `2026-01-15T08:30:00Z`.
- **A relative span** — `7d`, `12h`, `2w`, `30m` (interpreted as "now minus span").

So `--since 30d` means the last 30 days, and `--since 2026-01-01 --until 2026-01-31`
covers all of January.

## Examples

```bash
# How much have I spent on Codex this month?
claudex cost --provider codex --since 30d

# Compare Claude vs Codex cost, this week
claudex cost --provider claude,codex --since 7d

# List recent Pi sessions
claudex sessions --provider pi --since 14d

# GPT-5 usage across providers
claudex models --model gpt-5

# Only sessions still on disk, in one project
claudex sessions --project myrepo --on-disk-only
```

## Provider-aware output

- In tables, a **Provider** column appears only when the result set spans more
  than one provider — a Claude-only result stays narrow.
- In `--json`, every row carries a `"provider"` key so scripts are unambiguous.

```bash
claudex cost --per-session --json | jq 'group_by(.provider) | map({provider: .[0].provider, cost: (map(.cost_usd) | add)})'
```

## `--no-index`

The `--no-index` fallback scans Claude Code transcripts directly (no database).
It honors `--since`/`--until`/`--model` in memory, but is Claude-only — use the
default indexed path for Codex and Pi.

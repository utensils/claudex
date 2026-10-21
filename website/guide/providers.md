# Providers & filtering

claudex indexes eleven coding agents into one database and reports across all of
them. Every reporting command spans every provider by default; a shared set of
filter flags narrows the view.

## The providers

| Provider                                    | Source                                                                                                         | Notes                                                                                                                                                                                                                                      |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Claude Code** (`claude`)                  | `~/.claude/projects/**.jsonl`                                                                                  | Worktrees aggregate to the parent project; subagent transcripts roll up to their parent session.                                                                                                                                           |
| **OpenAI Codex** (`codex`)                  | `~/.codex/sessions/**` and `~/.codex/archived_sessions/**`                                                     | Project comes from the transcript's `cwd`; sessions under `archived_sessions/` are flagged archived. Token usage is the last cumulative `token_count` (cached input → cache read).                                                         |
| **GitHub Copilot CLI** (`copilot`)          | `${CLAUDEX_COPILOT_DIR:-~/.copilot}/session-state/*/events.jsonl`                                              | Project comes from the session's `cwd`. Token usage is read from the final `session.shutdown` metrics; cost is an API-equivalent estimate computed from the rate card (Copilot bills by premium requests — the count is kept in `extras`). |
| **VS Code Copilot Chat** (`copilot-vscode`) | VS Code `User/workspaceStorage/*/chatSessions/**` (stable + Insiders; override with `CLAUDEX_VSCODE_USER_DIR`) | Indexes chat text, tools, models, and timing. VS Code stores no token counts locally, so these sessions report zero tokens and `$0` cost.                                                                                                  |
| **Pi** (`pi`)                               | `~/.pi/agent/sessions/**`                                                                                      | Multi-provider under the hood; claudex trusts Pi's own per-message cost (local Ollama models report `$0`).                                                                                                                                 |
| **OpenClaw** (`openclaw`)                   | `${OPENCLAW_STATE_DIR:-~/.openclaw}/agents/*/sessions/**`                                                      | Reads classic transcripts plus trajectory sidecars. Provider-reported cost is trusted when present; trajectory-only sessions are de-duplicated with their transcript later.                                                                |

| **OMP** (`omp`) | `~/.omp/agent/sessions/**` | Oh My Pi: recursive session/subagent discovery, assistant usage and separate `model_usage` records. Honors OMP profiles and agent-directory settings. |
| **Gemini CLI** (`gemini`) | `~/.gemini/tmp/**/chats/session-*` | Reads JSON sessions and JSONL updates, including tools and token usage. |
| **OpenCode** (`opencode`) | `~/.local/share/opencode/opencode.db` or legacy `storage/` | Read-only SQLite access and legacy JSON sessions. Honors `XDG_DATA_HOME`; includes WAL changes when checking freshness. |

| **Cline** (`cline`) | `~/.cline/data/sessions/` and editor `globalStorage/saoudrizwan.claude-dev/tasks/` | SDK artifacts and legacy VS Code task history, UI-recorded usage/costs, tools, and text. |
| **Roo Code** (`roo`) | Editor `globalStorage/rooveterinaryinc.roo-cline/tasks/` | API conversation history, UI usage/costs, and model metadata. |

A provider is indexed only if its data directory exists, so you never need to
opt in — install Codex, Copilot, Pi, or OpenClaw and claudex picks them up on the next sync.

## Additional session directories

Extra roots are additive for every provider. Default discovery remains enabled.
Store persistent JSON configuration at `~/.claudex/config.json` (or
`$CLAUDEX_DIR/config.json`):

```json
{
  "providers": {
    "omp": { "session_dirs": ["/path/to/sessions", "~/other/sessions"] },
    "codex": { "session_dirs": ["/path/to/rollouts"] },
    "opencode": { "session_dirs": ["/path/to/opencode-data"] }
  }
}
```

Select another file with `--config FILE` or `CLAUDEX_CONFIG`. A CLI file selection
wins over the environment variable. Relative directories resolve beside the
configuration file; `~` expands to your home. Unknown providers and misspelled
configuration keys are rejected.

For one-off additions, repeat the global flag:

```bash
claudex --session-dir omp=/path/to/sessions --session-dir pi=/other/transcripts index
claudex --config ./sources.json sessions --provider omp,opencode --json
claudex providers --json
```

CLI directories resolve from the current working directory and add to configured
ones. `providers --json` includes every `root_dirs` entry. Changing the list
invalidates the five-minute discovery window immediately. Overlapping directories
and symlink aliases are deduplicated by canonical file path; symlink directory
cycles are skipped. Removing a source retains its indexed history.

Extra directories contain transcripts recursively, without requiring a provider's
usual parent-directory layout. Copilot CLI discovers `events.jsonl`; VS Code
Copilot accepts `.json` and `.jsonl` sessions. OpenCode directories contain
`opencode.db` or legacy `storage/` (a direct database path is also accepted).
Gemini accepts its data root or a directory containing sessions. Cline and Roo accept
individual task directories, task trees, or their data parent; Cline also reads SDK
session artifact trees. VS Code, Insiders, and VSCodium default locations are included. Set the provider
correctly: claudex uses that provider's parser rather than guessing file formats.
Extra directories apply to indexed commands; Claude `--no-index` continues to scan
its standard transcript storage.

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

| Flag                                                                                                   | Effect                                                                          |
| ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| `--provider <claude\|codex\|copilot\|copilot-vscode\|pi\|omp\|gemini\|opencode\|cline\|roo\|openclaw>` | Restrict to one or more providers. Repeatable or comma-separated. Default: all. |
| `--project <substr>`                                                                                   | Filter by project path substring.                                               |
| `--model <substr>`                                                                                     | Filter by model (e.g. `opus`, `gpt-5`).                                         |
| `--since <when>`                                                                                       | Only sessions at/after this time.                                               |
| `--until <when>`                                                                                       | Only sessions at/before this time.                                              |
| `--on-disk-only`                                                                                       | Exclude retained sessions whose file was archived or deleted.                   |
| `--json`                                                                                               | Machine-readable output; always includes a `provider` key per row.              |

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

# What would my Copilot usage cost at API rates?
claudex cost --provider copilot --since 30d

# List recent Pi sessions
claudex sessions --provider pi --since 14d

# Search OpenClaw trajectory-backed sessions
claudex search "tool timeout" --provider openclaw --json

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
default indexed path for other providers. Additional directories apply to indexed commands.

## Other harnesses

Cursor, Amp, Windsurf, and Aider are not automatically indexed. Their exported,
cloud-backed, optional-hook, or Markdown histories need separate format adapters;
pointing their files at another provider does not make them compatible.

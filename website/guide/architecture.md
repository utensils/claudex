# How it works

A quick tour of the pieces, for when you want to understand what claudex does
under the hood — or contribute to it.

## Data flow

```
~/.claude/projects/<encoded-path>/<session>.jsonl
        │   (Claude Code writes these; claudex never modifies them)
        ▼
store::SessionStore         — discover files, decode paths, canonicalize worktrees
        ▼
parser::stream_records      — streaming JSONL → SessionStats (O(1) memory)
        ▼
index::IndexStore           — rusqlite, bundled, schema_version = 3
        ▼
commands::<name>::run       — reads the index (or falls back to file scans)
        ▼
ui::table() / palette       — comfy-table with dynamic width + owo-colors
        │   or
        ▼
--json                      — stable shape for pipelines
```

## Modules

| Module              | Purpose                                                                                                                                                                                            |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/main.rs`       | clap parser, dispatches to `commands::*::run`. Pre-parses `--color` from argv before `Cli::parse()` so clap-generated help/errors honor the flag.                                                  |
| `src/store.rs`      | Locates session files, decodes project-directory names (`/.hidden` ↔ `--hidden`, `/seg` ↔ `-seg`), canonicalizes worktree paths (`…/.claude/worktrees/<branch>` aggregates to the parent project). |
| `src/parser.rs`     | `SessionStats` accumulator; `stream_records` reads JSONL one record at a time.                                                                                                                     |
| `src/types.rs`      | `TokenUsage`, `ModelPricing` (Opus/Sonnet/Haiku tiers). `cost_for_model` is the single source of truth for pricing math.                                                                           |
| `src/index.rs`      | `IndexStore` (SQLite). Relational report tables plus an FTS5 virtual table. Incremental sync keyed on `(file_path, file_size, file_mtime)`.                                                        |
| `src/ui.rs`         | Palette, `table()` builder, number formatters (`fmt_cost`, `fmt_count`), `Spinner`, `ColorChoice`. Everything presentation.                                                                        |
| `src/commands/*.rs` | One file per subcommand: `sessions`, `cost`, `search`, `tools`, `watch`, `summary`, `session`, `export`, `index`, `turns`, `prs`, `files`, `models`, `codex`, `update`, `completions`.             |

## Key invariants

### Staleness window

`STALE_SECS = 300` in `src/index.rs`. Read commands call `ensure_fresh()`, which
triggers an incremental sync only if the last sync is older than five minutes.
That's enough to feel fresh without re-scanning on every shell invocation.

Force an update: `claudex index`
Force a full rebuild: `claudex index --force`

### Indexed Claude Code read commands support `--no-index`

The fallback path reads JSONL files directly via `parser::parse_session`. **The
two paths must produce matching results.** If you add a metric to the index,
add it to the file-scan fallback too — the test suite exercises both.

### Schema migrations

Bumping `SCHEMA_VERSION` in `src/index.rs` triggers a full rebuild on next
open. Add new columns/tables inside the `CREATE TABLE IF NOT EXISTS` block and
bump the version. The current version is **3**.

### Codex stats are read-only and separate from the Claude Code index

`claudex codex` reads OpenAI Codex CLI state under `~/.codex` directly instead
of ingesting it into `~/.claudex/index.db`. It scans rollout JSONL files under
`~/.codex/sessions/**` and `~/.codex/archived_sessions/`, uses
`~/.codex/session_index.jsonl` for titles when present, and optionally opens
`~/.codex/state_5.sqlite` read-only for thread/token totals. This keeps the
Claude Code transcript index and Codex's state model independent.

### Worktree aggregation

`SessionStore::canonical_project_path` collapses
`…/.claude/worktrees/<branch>` back to the parent project, so a session you
started from a worktree rolls up into the project it belongs to. For display,
`display_project_name` renders worktree sessions as
`"projectname (worktree)"`.

### Pricing math

Lives in `src/types.rs`. Never inline per-token multipliers in a command —
always call `TokenUsage::cost_for_model(model)` so Opus / Sonnet / Haiku stay
consistent. Cost is computed per model _per message_, then summed — a session
that mixes models is priced correctly.

## Streaming parser, not slurping

`parser::stream_records` reads each JSONL file one line at a time and folds
into a `SessionStats` accumulator. Memory stays O(1) in the session size, so
multi-hundred-megabyte transcripts are fine. The index ingestion path uses the
same streaming reader.

## The UI layer

Every command module outputs through `src/ui.rs`:

- `ui::table()` — a `comfy-table` preset with minimal borders, no header
  separator, and dynamic width via `terminal_size`.
- `ui::header(...)`, `ui::right_align(...)` — header row styling + right-align
  numeric columns.
- `cell_*` helpers (`cell_project`, `cell_cost`, `cell_count`, `cell_dim`,
  `cell_model`) — semantic colors, so swapping the palette retints every
  report at once.
- `Spinner` — TTY-gated on stderr. Never shown when stdout is a pipe.
- `fmt_cost` — `$12,345.67`, falling back to four decimals for sub-cent values
  so tiny sessions don't round to `$0.00`.
- `fmt_count` — `326,297`, grouping separators honoring the locale-agnostic
  default.

Command modules **do not** reach for `comfy-table` or `owo-colors` directly.
The invariant is: every presentation choice lives in `ui.rs`.

## Want to add a subcommand?

1. Add a `Commands::Foo { … }` variant in `src/main.rs` and a dispatch arm.
2. Create `src/commands/foo.rs` with `pub fn run(...) -> anyhow::Result<()>`
   and register it in `src/commands/mod.rs`.
3. If the command reads aggregated data, add a query method to `IndexStore`
   and a `--no-index` fallback using `parser::parse_session` over
   `SessionStore::all_session_files`.
4. Support `--json`. For human output, use `ui::table()`, `ui::header(...)`,
   `ui::right_align(...)`, and the `cell_*` helpers.
5. Add a case to `tests/cli_tests.rs` covering both indexed + `--no-index`
   paths and JSON output.

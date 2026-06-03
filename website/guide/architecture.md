# How it works

A quick tour of the pieces, for when you want to understand what claudex does
under the hood — or contribute to it.

## Data flow

```
~/.claude/projects/**.jsonl   (Claude Code)  ┐
~/.codex/sessions|archived/**  (OpenAI Codex) ├─ claudex never modifies these
~/.pi/agent/sessions/**        (Pi)           │
~/.openclaw/agents/*/sessions  (OpenClaw)     ┘
        │
        ▼
providers::{claude,codex,pi,openclaw} — SessionProvider: enumerate + parse → ProviderRecord
        ▼
index::IndexStore             — rusqlite, bundled, schema_version = 7, additive/retentive
        ▼
api::Claudex                  — typed library facade for embedding
        │   or
        ▼
commands::<name>::run         — reads the index with a shared ResolvedFilter
        ▼
ui::table() / palette         — comfy-table with dynamic width + owo-colors
        │   or
        ▼
--json                        — stable shape for pipelines (provider key per row)
```

## Modules

claudex is a Cargo workspace:

- `crates/claudex` — reusable library crate (`claudex::api`, providers,
  parser, index, typed report rows).
- `crates/claudex-cli` — CLI package published as `claudex-cli`, installing the
  binary named `claudex`.

| Module                                 | Purpose                                                                                                                                                                                                                                                             |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/claudex-cli/src/main.rs`       | clap parser, dispatches to `commands::*::run`. Pre-parses `--color` from argv before `Cli::parse()` so clap-generated help/errors honor the flag.                                                                                                                   |
| `crates/claudex/src/providers/*.rs`    | The provider abstraction. `SessionProvider` trait + `Provider` enum (enum dispatch). Claude/Codex/Pi/OpenClaw each `enumerate` their transcripts and `parse` them into a normalized `ProviderRecord`. `enabled_default()` returns every provider whose root exists. |
| `crates/claudex-cli/src/cli.rs`        | Clap-only `FilterArgs` and `skills` argument types. It resolves CLI flags into the library's `filter::ResolvedFilter`.                                                                                                                                              |
| `crates/claudex/src/api.rs`            | Preferred library facade (`Claudex`, `ClaudexConfig`, `QueryFilter`, `ProviderKind`) returning typed structs.                                                                                                                                                       |
| `crates/claudex/src/filter.rs`         | Shared provider/model/date/on-disk filters used by the API, CLI, SQL queries, and Claude `--no-index` fallback.                                                                                                                                                     |
| `crates/claudex/src/store.rs`          | Claude file discovery, project-directory decoding (`/.hidden` ↔ `--hidden`, `/seg` ↔ `-seg`), worktree canonicalization.                                                                                                                                            |
| `crates/claudex/src/parser.rs`         | `SessionStats` accumulator; `stream_records` reads JSONL one record at a time.                                                                                                                                                                                      |
| `crates/claudex/src/types.rs`          | `TokenUsage`, `ModelPricing` (Opus/Sonnet/Haiku + gpt-5/gpt-4 tiers). `cost_for_model` is the single source of truth; providers can supply their own `embedded_cost`.                                                                                               |
| `crates/claudex/src/index.rs`          | `IndexStore` (SQLite). Relational report tables plus an FTS5 virtual table. Per-provider incremental sync; additive retention; non-destructive migrations.                                                                                                          |
| `crates/claudex-cli/src/skill/*.rs`    | `claudex skills` — generates the agent skill from the live clap tree.                                                                                                                                                                                               |
| `crates/claudex-cli/src/ui.rs`         | Palette, `table()` builder, number formatters (`fmt_cost`, `fmt_count`), `Spinner`, `ColorChoice`. Everything presentation.                                                                                                                                         |
| `crates/claudex-cli/src/commands/*.rs` | One file per subcommand: `sessions`, `cost`, `search`, `tools`, `watch`, `summary`, `session`, `export`, `index`, `turns`, `prs`, `files`, `models`, `update`. (`completions` and `skills` are dispatched in `main.rs`.)                                            |

## Key invariants

### Staleness window

`STALE_SECS = 300` in `crates/claudex/src/index.rs`. Read commands call `ensure_fresh()`, which
triggers an incremental sync only if the last sync is older than five minutes.
That's enough to feel fresh without re-scanning on every shell invocation.

Force an update: `claudex index`
Force a full rebuild: `claudex index --force`

### Providers are first-class; filtering happens at query time

All four providers flow through the same `IndexStore` pipeline. `ensure_fresh`
/ `sync` / `force_rebuild` take `&[Provider]`, and reports span every provider
whose data root exists. `--provider` narrows at query time — the index always
holds everything available, so switching providers never re-syncs.

### The index is additive, not a cache

A session whose source file is archived or deleted is **soft-deleted** (flagged
`present_on_disk = 0`, `archived_at` stamped) and retained with its derived rows
and FTS content. The only destructive path is `claudex index --force`. Each
provider's sync is scoped (`WHERE provider = ?`) so one provider's sync never
archives another's rows.

### Indexed read commands support `--no-index` (Claude only)

The fallback reads Claude JSONL files directly via `parser::parse_session` and
applies the shared filter in memory. The indexed path is the multi-provider one.

### Schema migrations are forward-only

Bumping `SCHEMA_VERSION` runs the `migrate_schema` ladder (guarded
`ALTER TABLE ADD COLUMN`) — never `DROP`, because retained data can't be rebuilt
from disk. Add a column to the `CREATE TABLE IF NOT EXISTS` block **and** an
additive migration step, then bump the version. The current version is **7**.

### Worktree aggregation

`SessionStore::canonical_project_path` collapses
`…/.claude/worktrees/<branch>` back to the parent project, so a session you
started from a worktree rolls up into the project it belongs to. For display,
`display_project_name` renders worktree sessions as
`"projectname (worktree)"`.

### Pricing math

Lives in `crates/claudex/src/types.rs`. Never inline per-token multipliers in a command —
always call `TokenUsage::cost_for_model(model)` so Opus / Sonnet / Haiku stay
consistent. Cost is computed per model _per message_, then summed — a session
that mixes models is priced correctly.

## Streaming parser, not slurping

`parser::stream_records` reads each JSONL file one line at a time and folds
into a `SessionStats` accumulator. Memory stays O(1) in the session size, so
multi-hundred-megabyte transcripts are fine. The index ingestion path uses the
same streaming reader.

## The UI layer

Every command module outputs through `crates/claudex-cli/src/ui.rs`:

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

1. Add a `Commands::Foo { … }` variant in `crates/claudex-cli/src/main.rs` and a dispatch arm.
2. Create `crates/claudex-cli/src/commands/foo.rs` with
   `pub fn run(...) -> anyhow::Result<()>` and register it in
   `crates/claudex-cli/src/commands/mod.rs`.
3. If the command reads aggregated data, add a query method to `IndexStore`
   and a `--no-index` fallback using `parser::parse_session` over
   `SessionStore::all_session_files`.
4. Support `--json`. For human output, use `ui::table()`, `ui::header(...)`,
   `ui::right_align(...)`, and the `cell_*` helpers.
5. Add a case to `crates/claudex-cli/tests/cli_tests.rs` covering both indexed + `--no-index`
   paths and JSON output.

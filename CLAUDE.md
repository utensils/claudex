# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# claudex — Architecture & Development Guide

> Query, search, and analyze Claude Code sessions from the command line.

claudex is a Rust CLI (edition 2024, MSRV 1.95) that reads the JSONL transcripts Claude Code writes under `~/.claude/projects/`, ingests them into a local SQLite index at `~/.claudex/index.db`, and exposes reports as subcommands.

## Build & Development Commands

### Nix (preferred)

```bash
nix build          # Build claudex (uses crane)
nix run            # Run claudex
nix develop        # Enter devshell (auto via direnv)
nix fmt            # Format Nix + Rust (nixfmt + rustfmt)
nix flake check    # Validate formatting + flake
```

### Devshell commands (inside `nix develop`)

| Category | Command | Description |
|----------|---------|-------------|
| build | `build` / `build-release` | `cargo build` / `cargo build --release` |
| check | `check` / `clippy` / `fmt` / `fmt-check` | Individual checks |
| check | `run-tests` | `cargo test` |
| check | `ci-local` | fmt-check → check → clippy → test → build (mirrors CI exactly) |
| check | `coverage` | `cargo llvm-cov --workspace --summary-only` (pass `--html` for browsable report) |
| run | `claudex` | `cargo run -- "$@"` |

### Running a single test

```bash
cargo test store::tests::decode_hidden_dir          # one unit test
cargo test --test index_tests -- name_of_test_fn    # one integration test in tests/
cargo test decode_                                  # all tests whose name contains decode_
```

### CI (GitHub Actions, `.github/workflows/ci.yml`)

`fmt`, `check`, `clippy -D warnings`, `test`, `build --release` must all pass. Run `ci-local` before pushing. A separate `coverage` job runs `cargo llvm-cov` and uploads to Codecov; it is non-blocking (`continue-on-error: true`, `fail_ci_if_error: false`).

## Architecture

### Data flow

```
~/.claude/projects/<encoded-path>/<session>.jsonl   ← source of truth (Claude Code writes these)
        │
        ▼   store::SessionStore (discovery + path decoding)
        ▼   parser::parse_session / stream_records (streaming JSONL → SessionStats)
        ▼
~/.claudex/index.db  (SQLite, schema_version=2, created on demand)
        │
        ▼   index::IndexStore::ensure_fresh / sync_now / force_rebuild
        ▼
commands::<name>::run  →  stdout (tables + palette via ui module, JSON via --json)
```

### Module layout

- `src/main.rs` — clap parser, dispatches to `commands::*::run`. Pre-parses `--color` from argv before `Cli::parse()` so clap-generated help/errors honor the flag too.
- `src/lib.rs` — re-exports `commands`, `index`, `parser`, `store`, `types`, `ui`. Also exposes `claudex_dir()` → `~/.claudex`.
- `src/store.rs` — locates session files, decodes project directory names (`/.hidden` ↔ `--hidden`, `/seg` ↔ `-seg`), and canonicalises worktree paths (`…/.claude/worktrees/<branch>` aggregates to the parent project). `SessionStore::at(path)` is a test-only constructor.
- `src/parser.rs` — `SessionStats` accumulator; `stream_records` reads JSONL one record at a time so large sessions don't balloon memory.
- `src/types.rs` — `TokenUsage` and `ModelPricing` (Opus/Sonnet/Haiku pricing tiers; default is Sonnet). `cost_for_model` is the single source of truth for pricing math.
- `src/index.rs` — `IndexStore` (SQLite via `rusqlite`, bundled). Tables: `sessions`, `token_usage`, `tool_calls`, `turn_durations`, `pr_links`, `file_modifications`, `thinking_usage`, `stop_reasons`, `attachments`, `permission_changes`, plus an FTS virtual table `messages_fts`. Incremental sync keys on `(file_path, file_size, file_mtime)`. `IndexStore::open_at(path)` is a test-only constructor.
- `src/ui.rs` — **single home for every presentation concern**: palette (semantic helpers like `project`, `cost`, `cell_project`, `cell_cost`), `table()` builder (minimal style, dynamic width via `terminal_size`), `Spinner` (TTY-gated, stderr), number formatters (`fmt_cost` → `$12,345.67` with sub-cent fallback to 4 decimals, `fmt_count` → `326,297`), and `ColorChoice` / `apply_color_choice`.
- `src/commands/*.rs` — one module per subcommand: `sessions`, `cost`, `search`, `tools`, `watch`, `summary`, `export`, `index`, `turns`, `prs`, `files`, `models`, `completions` (via helper in `main.rs`).
- `tests/index_tests.rs` — unit-style tests against parser/types/store.
- `tests/index_store_tests.rs` — integration tests against every `IndexStore` query method using `TempDir` + `open_at`/`at`.
- `tests/cli_tests.rs` — end-to-end subprocess tests against the compiled binary with a fixture `$HOME`. Exercises every subcommand's indexed and `--no-index` paths, JSON and text output, and the `--color` flag.
- `tests/completions_tests.rs` — shell-completion generation tests (clap_complete).

### Key invariants

- **Index staleness window = 300 s** (`STALE_SECS` in `src/index.rs`). Read commands call `ensure_fresh` which triggers an incremental sync only if the last sync is older than that. `claudex index` forces sync; `claudex index --force` wipes and rebuilds.
- **Every read command supports `--no-index`** and falls back to scanning JSONL files directly via `parser::parse_session`. Both paths must produce matching results — if you add a metric, add it to both the index query and the file-scan fallback.
- **Schema migrations**: bumping `SCHEMA_VERSION` in `src/index.rs` triggers a rebuild on next open. Add new columns/tables inside the `CREATE TABLE IF NOT EXISTS` block and bump the version.
- **Worktree aggregation**: always key on `canonical_project_path(&decoded)` when grouping by project, and use `display_project_name` for user-facing labels (renders worktree sessions as `"projectname (worktree)"`).
- **Pricing math lives in `types.rs`**. Do not inline per-token multipliers in commands — call `TokenUsage::cost_for_model` so the Opus/Sonnet/Haiku tiers stay consistent.

### Adding a new subcommand

1. Add a `Commands::Foo { … }` variant in `src/main.rs` and a dispatch arm.
2. Create `src/commands/foo.rs` with `pub fn run(...) -> anyhow::Result<()>` and register it in `src/commands/mod.rs`.
3. If the command reads aggregated data, add a query method to `IndexStore` and an `--no-index` fallback that uses `parser::parse_session` over `SessionStore::all_session_files`.
4. Support `--json` output for machine-readable results. For human output use `ui::table()`, `ui::header(...)`, `ui::right_align(...)`, and the `cell_*` / palette helpers — **never** call `comfy-table` or `owo-colors` directly from a command module.
5. Add an end-to-end case to `tests/cli_tests.rs` covering both the indexed path and (if applicable) the `--no-index` fallback, plus JSON output shape.

## Conventions

- Conventional Commits (`feat(scope):`, `fix(scope):`, `test:`, `refactor:`). Recent commits in `git log` are the authoritative style guide.
- Two-space indent for Nix (`nixfmt`); `rustfmt` defaults for Rust. `nix fmt` runs both.
- `clippy -D warnings` is enforced — no new clippy lints in CI.

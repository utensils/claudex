# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# claudex — Architecture & Development Guide

> Query, search, and analyze Claude Code sessions from the command line.

claudex is a Rust CLI (edition 2024, MSRV 1.85) that reads the JSONL transcripts Claude Code writes under `~/.claude/projects/`, ingests them into a local SQLite index at `~/.claudex/index.db`, and exposes reports as subcommands.

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
| run | `claudex` | `cargo run -- "$@"` |

### Running a single test

```bash
cargo test store::tests::decode_hidden_dir          # one unit test
cargo test --test index_tests -- name_of_test_fn    # one integration test in tests/
cargo test decode_                                  # all tests whose name contains decode_
```

### CI (GitHub Actions, `.github/workflows/ci.yml`)

`fmt`, `check`, `clippy -D warnings`, `test`, `build --release` must all pass. Run `ci-local` before pushing.

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
commands::<name>::run  →  stdout (tables via comfy-table / colors via owo-colors / JSON via --json)
```

### Module layout

- `src/main.rs` — clap parser, dispatches to `commands::*::run`.
- `src/lib.rs` — re-exports `commands`, `index`, `parser`, `store`, `types`.
- `src/store.rs` — locates session files, decodes project directory names (`/.hidden` ↔ `--hidden`, `/seg` ↔ `-seg`), and canonicalises worktree paths (`…/.claude/worktrees/<branch>` aggregates to the parent project).
- `src/parser.rs` — `SessionStats` accumulator; `stream_records` reads JSONL one record at a time so large sessions don't balloon memory.
- `src/types.rs` — `TokenUsage` and `ModelPricing` (Opus/Sonnet/Haiku pricing tiers; default is Sonnet). `cost_for_model` is the single source of truth for pricing math.
- `src/index.rs` — `IndexStore` (SQLite via `rusqlite`, bundled). Tables: `sessions`, `token_usage`, `tool_calls`, `turn_durations`, `pr_links`, `file_modifications`, `thinking_usage`, `stop_reasons`, `attachments`, `permission_changes`, plus an FTS virtual table `messages_fts`. Incremental sync keys on `(file_path, file_size, file_mtime)`.
- `src/commands/*.rs` — one module per subcommand: `sessions`, `cost`, `search`, `tools`, `watch`, `summary`, `export`, `index`, `turns`, `prs`, `files`, `models`.
- `tests/index_tests.rs` — integration tests against `IndexStore` using `tempfile`.

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
4. Support `--json` output for machine-readable results; use `comfy-table` + `owo-colors` for the human table.

## Conventions

- Conventional Commits (`feat(scope):`, `fix(scope):`, `test:`, `refactor:`). Recent commits in `git log` are the authoritative style guide.
- Two-space indent for Nix (`nixfmt`); `rustfmt` defaults for Rust. `nix fmt` runs both.
- `clippy -D warnings` is enforced — no new clippy lints in CI.

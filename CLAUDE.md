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

### CI (GitHub Actions, `.github/workflows/`)

Three workflows:

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `ci.yml` | push to `main`, pull_request to `main` | `docs` (bun fmt:check + build), `fmt`, `check`, `clippy -D warnings`, `test`, `build --release`. Plus non-blocking `coverage` (cargo llvm-cov → Codecov). |
| `pages.yml` | push to `main` touching `website/**` | Builds VitePress and deploys to GitHub Pages via `actions/deploy-pages@v4`. Base path `/claudex/`. |
| `release.yml` | tag push `v*`, or manual `workflow_dispatch` with required `tag` input | Matrix build of prebuilt binaries (4 targets), publishes a GitHub Release. |

Run `ci-local` (devshell) before pushing — mirrors the Rust-side checks
exactly.

## Release process

### Cutting a release

The `release.yml` workflow is the source of truth. To cut a new release:

1. **Bump version in all four surfaces** — `Cargo.toml` is authoritative;
   the flake re-reads it via `fromTOML`, so flake stays in sync
   automatically. Remaining touch-points:
   - `Cargo.toml` — `[package].version`
   - `Cargo.lock` — the `[[package]]` block named `claudex`
   - `website/.vitepress/config.ts` — the `text: 'vX.Y.Z'` nav entry
2. Update `README.md` install snippets if the tag is user-facing.
3. Commit on a `release/vX.Y.Z` branch, open a PR, land it.
4. `git tag vX.Y.Z && git push origin vX.Y.Z` — this fires `release.yml`.

### What `release.yml` does

Matrix targets (4):

- `aarch64-apple-darwin` on `macos-14`
- `x86_64-apple-darwin`  on `macos-13`
- `x86_64-unknown-linux-gnu`  on `ubuntu-22.04`
- `aarch64-unknown-linux-gnu` on `ubuntu-22.04-arm`

Per-target: `cargo build --release --target <t> --locked`, ad-hoc codesign
on macOS, strip, tar. Linux runners are pinned to `ubuntu-22.04` so the
glibc ABI floor stays stable across runner image upgrades. Release job
aggregates artifacts, generates `SHA256SUMS`, publishes via
`softprops/action-gh-release@v2`.

`make_latest` is **conditional on an actual tag push**
(`startsWith(github.ref, 'refs/tags/v')`). Manual `workflow_dispatch`
rebuilds of historical tags won't demote newer releases.

### The install script

`install.sh` in the repo root pulls the canonical
`/releases/latest/download/<asset>` redirect from GitHub — **no dependency
on `api.github.com`**, so it works in environments where the REST API is
blocked or rate-limited. Verifies against `SHA256SUMS` from the same
release, installs to `$CLAUDEX_INSTALL_DIR` (default `~/.local/bin`),
clears macOS quarantine. Override tag with `CLAUDEX_VERSION=v0.2.0`.

### Three supported install paths

All documented in `website/guide/installation.md`:

1. **`install.sh`** — prebuilt tarball from GitHub Releases (fastest).
2. **Cargo** — `cargo install --git https://github.com/utensils/claudex --tag vX.Y.Z`.
3. **Nix flake** — `nix run`, `nix profile install`, or as a flake input.
   `packages.default` and `apps.default` both carry populated `meta`
   sourced from `Cargo.toml` via `fromTOML`.

### Version bump — where it lands

| Surface | Field |
|---------|-------|
| `Cargo.toml` | `version`, `description`, `homepage`, `documentation` |
| `Cargo.lock` | auto on next `cargo` invocation; commit the update |
| `flake.nix` | nothing to edit — re-reads `Cargo.toml` |
| `website/.vitepress/config.ts` | nav entry `text: 'vX.Y.Z'` |
| `README.md` | install snippets referencing `--tag vX.Y.Z` |

### Docs deploy

`pages.yml` redeploys automatically on pushes to `main` that touch
`website/**`. No manual step. Canonical URL:
<https://utensils.io/claudex/> (org CNAME; `utensils.github.io/claudex/`
301-redirects here).

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

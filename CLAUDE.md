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
| `release-please.yml` | push to `main`, or manual `workflow_dispatch` with required `tag` input | Maintains the release PR; on merge cuts the tag, builds prebuilt binaries (4 targets), publishes the GitHub Release, and pushes to the AUR. See [Release process](#release-process). |

Run `ci-local` (devshell) before pushing — mirrors the Rust-side checks
exactly.

## Release process

Releases are driven by [release-please](https://github.com/googleapis/release-please)
(`.github/workflows/release-please.yml`). **There is no manual version
bump and no `release/vX.Y.Z` branch** — the version surfaces and the
CHANGELOG are maintained for you. `release-please-config.json` +
`.release-please-manifest.json` (repo root) hold the config and the
current version.

### Cutting a release

1. Land PRs to `main` using Conventional Commits (`feat:`, `fix:`,
   `feat!:`/`BREAKING CHANGE:` for major). release-please derives the next
   semver and changelog from those commit messages.
2. release-please keeps a standing **release PR** open against `main`. It
   bumps every version surface (below) and updates `CHANGELOG.md`. Review
   it like any PR.
3. **Merge the release PR.** release-please then tags `vX.Y.Z`, creates the
   GitHub Release (changelog body), and the same workflow builds the four
   target binaries, attaches them + `SHA256SUMS`, and publishes to the AUR.

`bump-minor-pre-major` is set, so pre-1.0 a `feat:` bumps the minor and a
breaking change bumps the minor (not major).

To **re-build/re-publish an existing tag** (e.g. a flaked runner), use the
workflow's `workflow_dispatch` with the `tag` input (`vX.Y.Z`). That path
rebuilds assets and refreshes the release but — like the old `make_latest`
guard — deliberately **does not republish to the AUR**.

### Version bump — where it lands (all automatic)

release-please rewrites these inside the release PR; do not hand-edit for a
release:

| Surface | Field | How |
|---------|-------|-----|
| `Cargo.toml` | `[package].version` | `rust` release-type (native) |
| `Cargo.lock` | the `claudex` `[[package]]` block | `rust` release-type (native) |
| `CHANGELOG.md` | new `## [X.Y.Z]` section prepended | release-please (native) |
| `flake.nix` | nothing — re-reads `Cargo.toml` via `fromTOML` | n/a |
| `website/.vitepress/config.ts` | `text: 'vX.Y.Z'` nav entry | `extra-files` + `// x-release-please-version` marker |
| `README.md` | `CLAUDEX_VERSION=vX.Y.Z` + `--tag vX.Y.Z` snippets | `extra-files` + `<!-- x-release-please-version -->` / start-end block markers |
| `packaging/aur/*/PKGBUILD` | `pkgver` + `sha256sums` | CI runs `scripts/aur/update-pkgbuild.sh` (`claudex-bin`/`claudex`); `claudex-git` hand-bumped only |

**The markers in `README.md` and `config.ts` are load-bearing** — if you
remove them, those surfaces silently stop tracking the version. The
`config.ts` marker is a trailing line comment that must survive
`bun run fmt:check` (prettier keeps it).

### What `release-please.yml` does

Jobs run in order: `release-please` (maintains the release PR / cuts the
tag + draft release on push to `main`) → `resolve-tag` (emits the tag, or
mints one from the `workflow_dispatch` input) → `build` → `publish-release`
→ `publish-aur`.

Build matrix targets (4):

- `aarch64-apple-darwin` on `macos-14`
- `x86_64-apple-darwin`  on `macos-14` (cross-compile from Apple Silicon)
- `x86_64-unknown-linux-gnu`  on `ubuntu-22.04`
- `aarch64-unknown-linux-gnu` on `ubuntu-22.04-arm`

Per-target: `cargo build --release --target <t> --locked`, strip, ad-hoc
codesign on macOS (strip first — it invalidates the signature; unsigned
Apple Silicon binaries get SIGKILLed at launch), tar. Linux runners are
pinned to `ubuntu-22.04` so the glibc ABI floor stays stable across runner
image upgrades.

`release-please` drafts the release immediately so users never see an
asset-less release; `publish-release` aggregates artifacts, generates
`SHA256SUMS`, sets the notes to the **release-please changelog body plus a
curated Install template** (idempotent via a `<!-- claudex-install-instructions -->`
marker), uploads assets, then lifts the draft — which makes it "latest"
(what `install.sh` / `claudex update` resolve via `/releases/latest`).

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

### AUR

PKGBUILDs live in [`packaging/aur/`](./packaging/aur/) as the source
of truth. The AUR git repos (`ssh://aur@aur.archlinux.org/<pkg>.git`)
are downstream mirrors that CI force-publishes to on every release,
via the `publish-aur` matrix job in `release-please.yml` and the
`KSXGitHub/github-actions-deploy-aur` action. See
[`packaging/aur/README.md`](./packaging/aur/README.md) for the full
release flow and one-time bootstrap.

Three invariants worth knowing:

- `scripts/aur/update-pkgbuild.sh` does **not** regenerate `.SRCINFO` —
  the deploy action does that inside its own Arch container after the
  PKGBUILD is refreshed. Local hand-bumps need
  `makepkg --printsrcinfo > .SRCINFO` (or just let the AUR push fail
  loudly).
- `claudex-git` is hand-bumped only. `update-pkgbuild.sh` refuses to
  touch it; the `publish-aur` matrix excludes it.
- The `publish-aur` job is gated on `AUR_SSH_PRIVATE_KEY` being set
  as a repo secret (the maintainer's primary ed25519 key,
  registered with the AUR account). Without it, the job logs a
  skip line and exits cleanly — no red Xs.

### Docs deploy

`pages.yml` redeploys automatically on pushes to `main` that touch
`website/**`. No manual step. Canonical URL:
<https://utensils.io/claudex/> (org CNAME; `utensils.github.io/claudex/`
301-redirects here).

## Architecture

### Data flow

```
~/.claude/projects/**.jsonl      (Claude Code)  ┐
~/.codex/sessions|archived/**     (OpenAI Codex) ├─ source transcripts
~/.pi/agent/sessions/**           (Pi)           ┘
        │
        ▼   providers::{claude,codex,pi} (SessionProvider: enumerate + parse → ProviderRecord)
        ▼
~/.claudex/index.db  (SQLite, schema_version=6, created on demand)
        │   additive/retentive: archived or deleted sessions are RETAINED (present_on_disk=0),
        │   non-destructive ALTER-TABLE migrations, per-provider incremental sync.
        ▼   index::IndexStore::ensure_fresh / sync_now / force_rebuild (take &[Provider])
        ▼
commands::<name>::run(&ResolvedFilter)  →  stdout (tables + palette via ui, JSON via --json)
```

### Module layout

- `src/main.rs` — clap parser, dispatches to `commands::*::run`. Pre-parses `--color` from argv before `Cli::parse()` so clap-generated help/errors honor the flag too.
- `src/lib.rs` — re-exports `cli`, `cli_help`, `commands`, `index`, `parser`, `plan`, `providers`, `skill`, `stats`, `store`, `types`, `ui`. Also exposes `claudex_dir()` → `~/.claudex`, overridable via the `CLAUDEX_DIR` env var (used by the subprocess tests in `tests/cli_tests.rs` and handy for sandboxed CI / parallel dev databases; the env var wins unconditionally when set).
- `src/providers/{mod,claude,codex,pi}.rs` — the provider abstraction. `SessionProvider` trait + `Provider` enum (enum dispatch) discover each agent's transcripts (`enumerate` → `DiscoveredFile`) and normalize them (`parse` → `ProviderRecord`, the type the index insert loop consumes). `enabled_default()` returns every provider whose root dir exists. Claude wraps `SessionStore` + the moved transcript parser; Codex reads `~/.codex` (last-cumulative `token_count`, cached input → cache read); Pi reads `~/.pi/agent` (per-model usage, trusts Pi's own `embedded_cost`, local models = $0).
- `src/cli.rs` — shared `FilterArgs` (flattened into every reporting command) → `ResolvedFilter`: `--provider`, `--model`, `--since`/`--until` (date / RFC3339 / `7d`,`2w` spans), `--on-disk-only`. `sql_predicates()` builds the indexed WHERE; `matches()` filters the `--no-index` fallback. Also houses the `skills` clap types (`SkillCommand`/`SkillArgs`/`SkillTarget`).
- `src/cli_help.rs` — **single home for CLI usage examples and parse-error hints.** Per-subcommand `*_EXAMPLES`/`*_HELP` string consts wired into clap via `#[command(after_long_help = ...)]` in `main.rs`, plus `error_help_for(bin)` which `main.rs` appends to custom usage errors (`uses_shared_filters` decides whether to also emit `FILTER_FORMATS`). Add a command's examples here, not inline in the command module — mirrors how `ui.rs` owns presentation and `cli.rs` owns filtering.
- `src/skill/{mod,templates}.rs` — `claudex skills generate|install`. A `Flavor` enum over one shared `body()`; `command_list()` is clap-derived so the skill never drifts. The committed `.claude/skills/claudex/SKILL.md` is a generated artifact (regenerate: `claudex skills generate --target claude-code --dir . --force`; a drift-guard test enforces it).
- `src/store.rs` — locates session files, decodes project directory names (`/.hidden` ↔ `--hidden`, `/seg` ↔ `-seg`), and canonicalises worktree paths (`…/.claude/worktrees/<branch>` aggregates to the parent project). `SessionStore::at(path)` is a test-only constructor.
- `src/parser.rs` — `SessionStats` accumulator; `stream_records` reads JSONL one record at a time so large sessions don't balloon memory.
- `src/types.rs` — `TokenUsage` and `ModelPricing` (Opus/Sonnet/Haiku + OpenAI `gpt-5`/`gpt-4` tiers; default is Sonnet). `cost_for_model` is the single source of truth for pricing math; providers that report their own cost set `ModelSessionStats::embedded_cost`, which the index trusts over the table.
- `src/stats.rs` — small numeric helpers shared across commands (e.g. `percentile_sorted` used by `turns` and the session drill-down).
- `src/plan.rs` — `Plan` enum (`Api` / `FlatMonthly { usd_per_month }`), `FromStr` parser for the `--plan` value, and `cost_fields` returning a `serde_json::Map` of plan-aware cost keys. Consumed only by the `summary` subcommand today; if you wire it into another command, add `--plan` to that command's clap definition (not as a global) so the flag never silently no-ops.
- `src/index.rs` — `IndexStore` (SQLite via `rusqlite`, bundled). Tables: `sessions` (now carries `provider`, `present_on_disk`, `archived_at`, `last_seen`, `extras`), `token_usage` (carries `cost_source` — `computed` vs `provider`), `tool_calls`, `turn_durations`, `pr_links`, `file_modifications`, `thinking_usage`, `stop_reasons`, `attachments`, `permission_changes`, plus an FTS virtual table `messages_fts`. Incremental sync keys on `(file_path, file_size, file_mtime)`, scoped per provider. `IndexStore::open_at(path)` is a test-only constructor.
- `src/ui.rs` — **single home for every presentation concern**: palette (semantic helpers like `project`, `cost`, `cell_project`, `cell_cost`), `table()` builder (minimal style, dynamic width via `terminal_size`), `Spinner` (TTY-gated, stderr), number formatters (`fmt_cost` → `$12,345.67` with sub-cent fallback to 4 decimals, `fmt_count` → `326,297`), and `ColorChoice` / `apply_color_choice`.
- `src/commands/*.rs` — one module per subcommand: `sessions`, `session`, `cost`, `search`, `tools`, `watch`, `summary`, `export`, `index`, `turns`, `prs`, `files`, `models`, `update`. (`completions` and `skills` are dispatched in `main.rs` to helpers/`skill::execute`, not modules here. The old scan-only `codex` subcommand was removed — Codex is now a first-class indexed provider reached via `--provider codex`.)
- `tests/index_tests.rs` — unit-style tests against parser/types/store.
- `tests/index_store_tests.rs` — integration tests against every `IndexStore` query method using `TempDir` + `open_at`/`at` (query methods take `&ResolvedFilter`).
- `tests/retention_tests.rs` — v4→v5 migration preserves data, deleted-file retention, in-place rowid reuse, restored-file un-archival (opens the on-disk DB with a `rusqlite` dev-dependency).
- `tests/providers_tests.rs` — Claude/Codex/Pi `enumerate`+`parse` unit tests (cumulative tokens, embedded cost, archived flags).
- `tests/cli_tests.rs` — end-to-end subprocess tests against the compiled binary with a fixture `$HOME` (including synthetic `~/.codex` / `~/.pi/agent`). Exercises indexed + `--no-index` paths, the shared filters, provider-aware output, and `skills`.
- `tests/skill_tests.rs` — skill-template unit tests (per-flavor frontmatter, plugin manifest).
- `tests/completions_tests.rs` — shell-completion generation tests (clap_complete).

### Key invariants

- **Providers are first-class and additive.** All three (Claude/Codex/Pi) flow through the same `IndexStore` pipeline. `ensure_fresh`/`sync`/`force_rebuild` take `&[Provider]`; default reporting spans every provider (`providers::enabled_default()`). Filtering happens at query time (`--provider`), never at sync time — the index always holds everything available.
- **The index is retentive, not a cache.** A session whose source file is archived or deleted is soft-deleted (`present_on_disk=0`, `archived_at` stamped) and RETAINED with its derived rows + FTS. The ONLY destructive path is `claudex index --force` (`force_rebuild`). Per-provider sync scoping (`WHERE provider = ?`) is mandatory so one provider's sync never archives another's rows.
- **Index staleness window = 300 s** (`STALE_SECS`), tracked per provider (`last_sync:<id>` / `sessions_root:<id>` meta keys). `claudex index` forces sync; `claudex index --force` wipes and rebuilds.
- **Schema migrations are forward-only and non-destructive.** Bumping `SCHEMA_VERSION` runs the `migrate_schema` ladder (guarded `ALTER TABLE ADD COLUMN`) — never `DROP`, because retained data can't be rebuilt from disk. Add columns to the `CREATE TABLE IF NOT EXISTS` block AND an additive migration step, then bump the version.
- **Every Claude read command still supports `--no-index`**, falling back to `parser::parse_session` with `ResolvedFilter::matches` applied in memory. The indexed path is the multi-provider one; `--no-index` is a Claude-only escape hatch.
- **Filtering is centralized in `src/cli.rs`.** Reporting commands flatten `FilterArgs` and pass `&ResolvedFilter` to the query methods, which append `sql_predicates(alias)`. Don't hand-roll provider/date predicates in a command.
- **Worktree aggregation**: always key on `canonical_project_path(&decoded)` when grouping by project, and use `display_project_name` for user-facing labels (renders worktree sessions as `"projectname (worktree)"`).
- **Pricing math lives in `types.rs`**. Do not inline per-token multipliers in commands — call `TokenUsage::cost_for_model` (Opus/Sonnet/Haiku/GPT tiers, latest vs legacy). Providers reporting their own cost set `embedded_cost`, which the insert loop stores with `cost_source='provider'` (everything else is `'computed'`).
- **Stored costs are repriced automatically.** `cost_usd` is materialized at ingest, so changing `cost_for_model` would leave old rows stale. When you change the rate card, bump `PRICING_REVISION` (`index.rs`): the next open reprices every `cost_source='computed'` row in place via `reprice_computed_costs` (keyed on the `pricing_revision` meta value, runs once per bump). `'provider'` rows are never touched. This is the non-destructive way to refresh retained/archived rows — unlike `index --force`, which drops them.

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
- `AGENTS.md` is a symlink to `CLAUDE.md` (so Codex/other agents read the same guide) — edit `CLAUDE.md`, never `AGENTS.md` directly.

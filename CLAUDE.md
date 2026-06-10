# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# claudex — Architecture & Development Guide

> Query, search, and analyze Claude Code sessions from the command line.

claudex is a Rust workspace (edition 2024, MSRV 1.95) with a reusable
`claudex` library crate and a `claudex-cli` package that installs a binary
named `claudex`. The project reads local agent transcripts, ingests them into
a SQLite index at `~/.claudex/index.db`, and exposes both typed library queries
and CLI reports.

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
| build | `build` / `build-release` | `cargo build -p claudex-cli --bin claudex` / release variant |
| check | `check` / `clippy` / `fmt` / `fmt-check` | Individual checks |
| check | `run-tests` | `cargo test --workspace` |
| check | `ci-local` | fmt-check → check → clippy → test → build (mirrors CI exactly) |
| check | `coverage` | `cargo llvm-cov --workspace --summary-only` (pass `--html` for browsable report) |
| run | `claudex` | `cargo run -p claudex-cli --bin claudex -- "$@"` |
| docs | `docs-dev` / `docs-build` / `docs-preview` | VitePress dev server / static build / preview of `website/` |
| docs | `docs-fmt` / `docs-fmt-check` | prettier format / check (`docs-fmt-check` matches the CI `docs` job) |

### Running a single test

```bash
cargo test store::tests::decode_hidden_dir          # one unit test
cargo test -p claudex --test index_tests -- name_of_test_fn
cargo test -p claudex-cli --test cli_tests -- name_of_test_fn
cargo test decode_                                  # all tests whose name contains decode_
```

### CI (GitHub Actions, `.github/workflows/`)

Three workflows:

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `ci.yml` | push to `main`, pull_request to `main` | `docs` (bun fmt:check + build), workspace `fmt`, `check`, `clippy -D warnings`, `test`, `cargo build --release -p claudex-cli --bin claudex`, and package checks. Plus non-blocking `coverage` (cargo llvm-cov → Codecov). |
| `pages.yml` | push to `main` touching `website/**` | Builds VitePress and deploys to GitHub Pages via `actions/deploy-pages@v4`. Base path `/claudex/`. |
| `release-please.yml` | push to `main`, or manual `workflow_dispatch` with required `tag` input | Maintains the release PR; on merge cuts the tag, builds prebuilt binaries (4 targets), publishes crates.io packages, publishes the GitHub Release, and pushes to the AUR. See [Release process](#release-process). |

Run `ci-local` (devshell) before pushing — mirrors the Rust-side checks
exactly.

## Release process

Releases are driven by [release-please](https://github.com/googleapis/release-please)
(`.github/workflows/release-please.yml`). **There is no manual version
bump and no `release/vX.Y.Z` branch** — the version surfaces and the
CHANGELOG are maintained for you. `release-please-config.json` +
`.release-please-manifest.json` (repo root) hold the config and the
current version.

Both workspace packages intentionally share the `claudex` release component
with `include-component-in-tag: false`. Together with the `cargo-workspace`
plugin, that preserves one unprefixed `vX.Y.Z` tag/release train while still
letting release-please update the library, CLI, and internal dependency version
surfaces. If you change release-please config, dry-run a library-only and a
CLI-only change before merging it.

### Cutting a release

1. Land PRs to `main` using Conventional Commits (`feat:`, `fix:`,
   `feat!:`/`BREAKING CHANGE:` for major). release-please derives the next
   semver and changelog from those commit messages.
2. release-please keeps a standing **release PR** open against `main`. It
   bumps every version surface (below) and updates `CHANGELOG.md`. Review
   it like any PR.
3. **Merge the release PR.** release-please then tags `vX.Y.Z`, creates the
   draft GitHub Release (changelog body), and the same workflow builds the
   four target binaries, publishes `claudex` then `claudex-cli` to crates.io,
   attaches assets + `SHA256SUMS`, lifts the GitHub Release, and publishes to
   the AUR.

`bump-minor-pre-major` is set, so pre-1.0 a `feat:` bumps the minor and a
breaking change bumps the minor (not major).

To **re-build/re-publish an existing tag** (e.g. a flaked runner), use the
workflow's `workflow_dispatch` with the `tag` input (`vX.Y.Z`). That path
rebuilds assets and refreshes the release but deliberately **does not
republish to crates.io or the AUR**.

### Version bump — where it lands (all automatic)

release-please rewrites these inside the release PR; do not hand-edit for a
release:

| Surface | Field | How |
|---------|-------|-----|
| root `Cargo.toml` | nothing — `[workspace.package]` intentionally carries **no** `version` (the plugin doesn't maintain it for non-inheriting crates, so the field drifted; removed in 0.10.x) | n/a |
| `crates/claudex*/Cargo.toml` | package versions and internal path dependency versions | `cargo-workspace` plugin |
| `Cargo.lock` | the `claudex` / `claudex-cli` `[[package]]` blocks | `cargo-workspace` plugin |
| `CHANGELOG.md` | new `## [X.Y.Z]` section prepended | release-please (native) |
| `flake.nix` | nothing — reads the CLI crate manifest (version, description) and root workspace (homepage) via `fromTOML` | n/a |
| `website/.vitepress/config.ts` | `text: 'vX.Y.Z'` nav entry | `extra-files` + `// x-release-please-version` marker |
| `README.md` and crate READMEs | `CLAUDEX_VERSION=vX.Y.Z`, `claudex = "X.Y.Z"`, and `--version X.Y.Z` snippets | `extra-files` + release-please markers |
| `website/reference/library.md` | library install snippet | `extra-files` + `# x-release-please-version` marker |
| `packaging/aur/*/PKGBUILD` | `pkgver` + `sha256sums` | CI runs `scripts/aur/update-pkgbuild.sh` (`claudex-bin`/`claudex`); `claudex-git` hand-bumped only |

**The release-please markers are load-bearing** — if you remove them, those
surfaces silently stop tracking the version. The `config.ts` marker is a
trailing line comment that must survive `bun run fmt:check` (prettier keeps it).

**`extra-files` paths are package-relative** — entries resolve against the
package directory (`crates/claudex*`), and files outside it need a leading `/`
(repo-root-relative). A wrong path does not error; the surface just silently
stops updating (this drifted README/website versions after the workspace
split). When touching `extra-files`, verify the next release PR's diff
actually includes every registered file.

### What `release-please.yml` does

Jobs run in order: `release-please` (maintains the release PR / cuts the
tag + draft release on push to `main`) → `resolve-tag` (emits the tag, or
mints one from the `workflow_dispatch` input) → `build` → `publish-crates`
→ `publish-release` → `publish-aur`. `publish-crates` is a successful no-op
for `workflow_dispatch` rebuilds, so manual asset refreshes never republish
crates.io packages.

Build matrix targets (4):

- `aarch64-apple-darwin` on `macos-14`
- `x86_64-apple-darwin`  on `macos-14` (cross-compile from Apple Silicon)
- `x86_64-unknown-linux-gnu`  on `ubuntu-22.04`
- `aarch64-unknown-linux-gnu` on `ubuntu-22.04-arm`

Per-target: `cargo build --release -p claudex-cli --bin claudex --target <t>
--locked`, strip, ad-hoc codesign on macOS (strip first — it invalidates the
signature; unsigned Apple Silicon binaries get SIGKILLed at launch), tar.
Linux runners are pinned to `ubuntu-22.04` so the glibc ABI floor stays stable
across runner image upgrades.

`release-please` drafts the release immediately so users never see an
asset-less release. On real releases, crates.io publishing must complete before
`publish-release` aggregates artifacts, generates `SHA256SUMS`, sets the notes
to the **release-please changelog body plus a curated Install template**
(idempotent via a `<!-- claudex-install-instructions -->` marker), uploads
assets, then lifts the draft — which makes it "latest" (what `install.sh` /
`claudex update` resolve via `/releases/latest`).

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
2. **Cargo** — `cargo install claudex-cli` or `cargo install claudex-cli --version X.Y.Z`.
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
~/.pi/agent/sessions/**           (Pi)           │
~/.trajectory/** + state files     (OpenClaw)     ┘
        │
        ▼   claudex::providers::* (SessionProvider: enumerate + parse → ProviderRecord)
        ▼
~/.claudex/index.db  (SQLite, created on demand)
        │   additive/retentive: archived or deleted sessions are RETAINED (present_on_disk=0),
        │   non-destructive ALTER-TABLE migrations, per-provider incremental sync.
        ▼   claudex::index::IndexStore or claudex::api::Claudex
        ▼
claudex-cli::commands::<name>::run(&ResolvedFilter)  →  stdout (tables + palette via ui, JSON via --json)
```

### Module layout

- Root `Cargo.toml` — virtual workspace only. Shared edition, MSRV, metadata,
  and dependency versions live under `workspace.package` /
  `workspace.dependencies`; the **package versions live in the crate
  manifests** (maintained by release-please), not in the workspace.
- `crates/claudex/src/lib.rs` — reusable library entrypoint. Re-exports
  `api`, `filter`, `index`, `parser`, `plan`, `providers`, `stats`, `store`,
  `time_utils`, and `types`, plus `claudex_dir()` → `~/.claudex`
  (`CLAUDEX_DIR` wins unconditionally when set).
- `crates/claudex/src/api.rs` — preferred public facade (`Claudex`,
  `ClaudexConfig`, `QueryFilter`, `ProviderKind`) returning typed report
  structs. Keep terminal rendering and progress UI out of this crate.
- `crates/claudex/src/filter.rs` — shared provider/model/date/on-disk filter
  logic. CLI clap types convert into these library filters instead of
  duplicating business rules.
- `crates/claudex/src/providers/{mod,claude,codex,openclaw,pi}.rs` — provider
  abstraction. `SessionProvider` + `Provider` enum discover each agent's
  transcripts and normalize them to `ProviderRecord`. `providers/pr.rs` is the
  crate-internal GitHub PR link extractor that populates
  `ProviderRecord::pr_links` (backs the `prs` command).
- `crates/claudex/src/store.rs` — Claude session file discovery, project path
  decoding, and worktree canonicalization. `SessionStore::at(path)` is a
  test-only constructor.
- `crates/claudex/src/parser.rs` — Claude JSONL transcript parser and
  `SessionStats` accumulator; `stream_records` reads one record at a time.
- `crates/claudex/src/types.rs` — token usage and pricing math. `cost_for_model`
  is the single source of truth for computed costs.
- `crates/claudex/src/index.rs` — `IndexStore` (SQLite via `rusqlite`,
  bundled), schema migrations, incremental sync, retention, repricing, and
  query methods. `IndexStore::open_at(path)` is a test-only constructor.
- `crates/claudex-cli/src/main.rs` — clap parser and dispatch. Pre-parses
  `--color` before `Cli::parse()` so clap-generated help/errors honor it too.
  `lib.rs` re-exports the CLI modules so integration tests can use them as a
  library.
- `crates/claudex-cli/src/cli.rs` — clap-only `FilterArgs` / `ProviderArg`,
  skill clap types, and conversion into `claudex::filter::ResolvedFilter`.
- `crates/claudex-cli/src/cli_help.rs` — single home for CLI usage examples and
  parse-error hints wired into clap.
- `crates/claudex-cli/src/skill/{mod,templates}.rs` — `claudex skills
  generate|install`. The committed `.claude/skills/claudex/SKILL.md` is a
  generated artifact (regenerate: `claudex skills generate --target
  claude-code --dir . --force`; a drift-guard test enforces it).
- `crates/claudex-cli/src/ui.rs` — single home for every presentation concern:
  palette, table builder, spinner, number formatters, and color choice.
- `crates/claudex-cli/src/commands/*.rs` — one module per subcommand:
  `activity`, `budget`, `sessions`, `session`, `cost`, `search`, `tools`,
  `watch`, `summary`, `export`, `index`, `timeline`, `providers`, `turns`,
  `prs`, `files`, `models`, and `update`.
- `crates/claudex/tests/*.rs` — library and index/provider integration tests,
  including API facade coverage and dependency hygiene.
- `crates/claudex-cli/tests/*.rs` — end-to-end subprocess, completions, and
  skill-template tests. `CARGO_BIN_EXE_claudex` stays valid because the binary
  target is still named `claudex`.

### Key invariants

- **Providers are first-class and additive.** Claude, Codex, Pi, and OpenClaw
  flow through the same `IndexStore` pipeline. `ensure_fresh` / `sync_now` /
  `force_rebuild` take provider lists; default reporting spans every provider
  whose root exists (`providers::enabled_default()`). Filtering happens at
  query time (`--provider`), never at sync time — the index always holds
  everything available.
- **The index is retentive, not a cache.** A session whose source file is archived or deleted is soft-deleted (`present_on_disk=0`, `archived_at` stamped) and RETAINED with its derived rows + FTS. The ONLY destructive path is `claudex index --force` (`force_rebuild`). Per-provider sync scoping (`WHERE provider = ?`) is mandatory so one provider's sync never archives another's rows.
- **Index staleness window = 300 s** (`STALE_SECS`), tracked per provider (`last_sync:<id>` / `sessions_root:<id>` meta keys). `claudex index` forces sync; `claudex index --force` wipes and rebuilds.
- **Schema migrations are forward-only and non-destructive.** Bumping `SCHEMA_VERSION` runs the `migrate_schema` ladder (guarded `ALTER TABLE ADD COLUMN`) — never `DROP`, because retained data can't be rebuilt from disk. Add columns to the `CREATE TABLE IF NOT EXISTS` block AND an additive migration step, then bump the version.
- **Every Claude read command still supports `--no-index`**, falling back to `parser::parse_session` with `ResolvedFilter::matches` applied in memory. The indexed path is the multi-provider one; `--no-index` is a Claude-only escape hatch.
- **Filtering is centralized in `crates/claudex/src/filter.rs`.** The CLI owns
  clap parsing in `crates/claudex-cli/src/cli.rs`, then converts to
  `ResolvedFilter`. Reporting commands pass `&ResolvedFilter` to query methods,
  which append `sql_predicates(alias)`. Don't hand-roll provider/date
  predicates in a command.
- **Worktree aggregation**: always key on `canonical_project_path(&decoded)` when grouping by project, and use `display_project_name` for user-facing labels (renders worktree sessions as `"projectname (worktree)"`).
- **Pricing math lives in `types.rs`**. Do not inline per-token multipliers in commands — call `TokenUsage::cost_for_model` (Opus/Sonnet/Haiku/GPT tiers, latest vs legacy). Providers reporting their own cost set `embedded_cost`, which the insert loop stores with `cost_source='provider'` (everything else is `'computed'`).
- **Stored costs are repriced automatically.** `cost_usd` is materialized at ingest, so changing `cost_for_model` would leave old rows stale. When you change the rate card, bump `PRICING_REVISION` (`index.rs`): the next open reprices every `cost_source='computed'` row in place via `reprice_computed_costs` (keyed on the `pricing_revision` meta value, runs once per bump). `'provider'` rows are never touched. This is the non-destructive way to refresh retained/archived rows — unlike `index --force`, which drops them.

### Adding a new subcommand

1. Add a `Commands::Foo { … }` variant in `crates/claudex-cli/src/main.rs` and a dispatch arm.
2. Create `crates/claudex-cli/src/commands/foo.rs` with `pub fn run(...) -> anyhow::Result<()>` and register it in `crates/claudex-cli/src/commands/mod.rs`.
3. If the command reads aggregated data, add a query method to `IndexStore` and an `--no-index` fallback that uses `parser::parse_session` over `SessionStore::all_session_files`.
4. Support `--json` output for machine-readable results. For human output use `ui::table()`, `ui::header(...)`, `ui::right_align(...)`, and the `cell_*` / palette helpers — **never** call `comfy-table` or `owo-colors` directly from a command module.
5. Add an end-to-end case to `crates/claudex-cli/tests/cli_tests.rs` covering both the indexed path and (if applicable) the `--no-index` fallback, plus JSON output shape.

## Conventions

- Conventional Commits (`feat(scope):`, `fix(scope):`, `test:`, `refactor:`). Recent commits in `git log` are the authoritative style guide.
- Two-space indent for Nix (`nixfmt`); `rustfmt` defaults for Rust. `nix fmt` runs both.
- `clippy -D warnings` is enforced — no new clippy lints in CI.
- `AGENTS.md` is a symlink to `CLAUDE.md` (so Codex/other agents read the same guide) — edit `CLAUDE.md`, never `AGENTS.md` directly.

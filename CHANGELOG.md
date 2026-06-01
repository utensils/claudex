# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are managed by [release-please](https://github.com/googleapis/release-please),
which prepends each new version below from the Conventional Commits landed on `main`.

## [0.8.0](https://github.com/utensils/claudex/compare/v0.7.0...v0.8.0) (2026-06-01)


### Features

* **providers:** add OpenClaw session indexing ([#42](https://github.com/utensils/claudex/issues/42)) ([cfd0fe5](https://github.com/utensils/claudex/commit/cfd0fe59d285c4a9030a53210f1cea2d7b4830f9))

## [0.7.0](https://github.com/utensils/claudex/compare/v0.6.0...v0.7.0) (2026-05-31)


### Features

* **prs:** extract provider PR links ([#39](https://github.com/utensils/claudex/issues/39)) ([451877e](https://github.com/utensils/claudex/commit/451877e85451634f13e69ee1af71aeb8b70f5614))

## [0.6.0](https://github.com/utensils/claudex/compare/v0.5.2...v0.6.0) (2026-05-30)


### Features

* **cost:** historical pricing accuracy + automatic in-place reprice ([#38](https://github.com/utensils/claudex/issues/38)) ([4a1971d](https://github.com/utensils/claudex/commit/4a1971de911b08fd76706f33e339b6cf3f6fba3e))
* index Codex and Pi sessions as first-class providers ([#33](https://github.com/utensils/claudex/issues/33)) ([71a7ccc](https://github.com/utensils/claudex/commit/71a7ccc22c767047fdba1f666d1a8a05a9eea6cd))


### Bug Fixes

* **cli:** improve usage errors and examples ([#36](https://github.com/utensils/claudex/issues/36)) ([b5d76bf](https://github.com/utensils/claudex/commit/b5d76bf1279743caa224595d30f67807c6e589b6))
* **cost:** grand-total TOTAL row + scoped CLI error usage ([#35](https://github.com/utensils/claudex/issues/35)) ([96dc7b5](https://github.com/utensils/claudex/commit/96dc7b52b54bf4df322cb38c7ee6c139256dab1e))

## [0.5.2](https://github.com/utensils/claudex/compare/v0.5.1...v0.5.2) (2026-05-30)


### Bug Fixes

* **session:** roll up subagents in the --no-index drill-down ([#31](https://github.com/utensils/claudex/issues/31)) ([1259c19](https://github.com/utensils/claudex/commit/1259c19817a43a6984359c9d35b73e8ecf3de371))

## [0.5.1](https://github.com/utensils/claudex/compare/v0.5.0...v0.5.1) (2026-05-30)


### Bug Fixes

* **aur:** preserve PKGBUILD perms when rewriting ([#29](https://github.com/utensils/claudex/issues/29)) ([6a4130d](https://github.com/utensils/claudex/commit/6a4130db4bc7222121ff70cecf5a69bfabcf2f4e))

## [0.5.0](https://github.com/utensils/claudex/compare/v0.4.0...v0.5.0) (2026-05-30)


### Features

* **aur:** add AUR packaging with auto-publish on release ([#23](https://github.com/utensils/claudex/issues/23)) ([2b76be2](https://github.com/utensils/claudex/commit/2b76be2433f7755aba7bf62317683d1a7c8b0c51))


### Bug Fixes

* **index:** include subagent transcripts in session accounting ([#26](https://github.com/utensils/claudex/issues/26)) ([219ed09](https://github.com/utensils/claudex/commit/219ed09bf7f6289efaf487539c5da8a6eb103df2))

## [0.4.0] — 2026-05-15

Third tagged release. Headline: a new `claudex codex` report that summarizes OpenAI Codex CLI activity from `~/.codex`, a `summary --plan flat-monthly:USD` flag that reframes the cost section for flat-fee subscribers (Pro / Pro Max / Team flat-fee), and a packaged Claude Code skill at `.claude/skills/claudex/` for end-user slash commands and autonomous-agent workflows.

### Added

- New `claudex codex` report ([#19](https://github.com/utensils/claudex/pull/19)): session and state-file stats for the OpenAI Codex CLI. Scans `~/.codex/sessions`, `~/.codex/archived_sessions`, the optional `~/.codex/state_5.sqlite` state DB, and `~/.codex/session_index.jsonl`. Surfaces session counts (today / week / total / archived / active files), message and tool-call totals, top projects, top tools, CLI versions, originators, sources, and optional state-DB thread/token totals. Supports `--json`. `codex` is intentionally outside the `~/.claudex/` index pipeline — it reads `~/.codex` directly on every invocation and so does not accept `--no-index`.
- New `claudex summary --plan <api|flat-monthly:USD>` flag ([#20](https://github.com/utensils/claudex/pull/20)) for users on flat Claude subscriptions. `--plan api` is the default and is bit-identical to v0.3.0. `--plan flat-monthly:250` keeps the historical `total_cost_usd` / `cost_this_week_usd` JSON keys (existing pipelines keep working) and additively emits `plan`, `actual_monthly_cost_usd`, `api_equivalent_total_usd`, `api_equivalent_week_usd`, and `leverage_this_week_multiple`. Leverage is computed against a calendar-accurate `365.25 / 12 / 7 ≈ 4.348` weeks-per-month; it serializes as JSON `null` when there's no usage this week, rather than a misleading `0.0`. The human-readable cost section becomes plan-aware (Plan / API equivalent / Leverage this week).
- New `src/plan.rs` module with shared `WEEKS_PER_MONTH` const and `Plan::leverage_this_week`, so the JSON output and the human-readable `summary` cost section share a single source of truth.
- Claude Code skill at `.claude/skills/claudex/SKILL.md` ([#18](https://github.com/utensils/claudex/pull/18)): a complete `/claudex` slash-command spec covering every subcommand, JSON shape, and agent-oriented `jq` pipeline. New `website/guide/skill.md` documents three installation paths (personal, project-local, repo clone) and updates the docs sidebar with an Integrations section.

### Docs

- `website/commands/codex.md` and `website/commands/summary.md` documented for the new report and flag. `website/commands/index.md` flag matrix lists `codex` and notes the `summary --plan` exception.
- `website/guide/json-output.md` documents the additive flat-monthly JSON keys explicitly, including the `null` leverage rule.
- `website/guide/quickstart.md` and `website/index.md` updated to feature `codex` alongside the Claude Code reports.
- `README.md` Quickstart shows `summary --plan flat-monthly:250`, and the subcommands table hyperlinks `codex`.
- `CLAUDE.md` records that `codex` bypasses the index pipeline (reads `~/.codex` directly), and adds `src/stats.rs` / `src/plan.rs` to the module layout. Documented schema version bumped 2 → 3 to match `src/index.rs`. ([#21](https://github.com/utensils/claudex/pull/21))
- `website/guide/architecture.md` and `website/guide/index.md` tightened so the "every read command supports `--no-index`" claim correctly carves out `codex` (reads `~/.codex` directly) and the index-only reports.

## [0.3.0] — 2026-04-23

Second tagged release. Headline: new `session` drill-down report, install-source-aware `claudex update` subcommand, per-model tracking, and a schema v3 rebuild of the index.

### Added

- New `claudex update` subcommand ([#16](https://github.com/utensils/claudex/pull/16)): in-place self-update for `install.sh` installs with SHA-256 verification, and source-aware upgrade hints for Nix (`nix profile upgrade claudex`), cargo (`cargo install … --tag vX.Y.Z --force claudex`), and Homebrew (`brew upgrade claudex`). Flags: `--check`, `--force`, `--version <tag>`. The latest tag is resolved via the `/releases/latest` redirect so the command never hits `api.github.com`.
- New `claudex session <selector>` drill-down report ([#15](https://github.com/utensils/claudex/pull/15)): overview, tokens, per-model usage, turn stats, tools, files, PR links, stop reasons, attachments, permission changes. Supports `--json` and `--no-index`.
- `claudex sessions --file <substring>` filter — only surface sessions that touched a matching file path.
- `claudex files --path <substring>` filter — limit the file-mods report by path substring.
- `claudex search --json` output with FTS5 `[[…]]` snippet markers and `bm25` rank.
- `cost`, `models`, and `summary` reports now surface Cache Write / Cache Read / avg-per-session / avg-tokens-per-session.
- `summary` gained Tokens, Top Stop Reasons, Model Distribution, and Metrics (thinking blocks, avg turn duration, PR links, files modified) sections; indexed and `--no-index` paths are at parity.
- `tools --per-session` shows the session date and sorts newest-first; NULL dates sort last to match SQLite.
- `files` table includes Modifications, Sessions, Last Touched, Top Project columns.
- Per-model tracking across parser and index: per-`(session, model)` token usage, inference_geo / service_tier / speed / iterations.
- Shared `src/stats.rs::percentile_sorted` helper; shared `src/store.rs::find_matching_sessions` with a UUID-prefix heuristic so short hex selectors don't fall back to project-name matching.

### Changed

- **Schema version 3.** Existing `~/.claudex/index.db` rebuilds on first open.
  - `token_usage` now stores one row per `(session, model)` with a new `assistant_message_count` column.
  - `token_usage.inference_geo` and `token_usage.service_tier` hold distinct reported values joined by ASCII Unit Separator (`\u001f`).
  - `sessions.model` now stores the sole model tag or `mixed` when a session switched models.
- `query_cost_per_session` aggregates with `GROUP BY s.id + SUM(...)` so mixed-model sessions sum correctly.
- `query_model_usage` aggregates in Rust with weighted speed averaging and deduped tier/geo sets.
- `summary.model_distribution` counts distinct sessions via a `HashSet`, avoiding double-counting on mixed-model sessions.
- `SessionCostRow.models` is sorted deterministically via `BTreeSet`.
- README restyled with a Quickstart block and a `session` subcommand entry.

### Fixed

- `claudex export --format json` now emits a proper JSON array when the selector resolves to multiple sessions (single match still returns an object) — previously concatenated objects produced invalid JSON.
- Zero-token per-model rows are no longer inserted into `token_usage`, preventing empty-signal models from polluting `query_model_usage` and the `session` drill-down.
- `tools --per-session --no-index` ordering matches the indexed path's `ORDER BY first_timestamp DESC NULLS LAST`.

### Docs

- New `website/commands/session.md` page.
- `website/reference/schema.md` updated for v3 (new column, multi-value columns).
- `website/commands/{cost,export,files,index,models,search,sessions,summary,tools}.md` and `website/guide/{json-output,recipes}.md` updated for the new fields, flags, and shapes.
- `website/commands/models.md` documents `avg_speed` as the mean of per-session-model averages (not throughput-weighted).

## [0.2.0] — 2026-04-19

First tagged release. Install paths: `install.sh`, `cargo install --git … --tag v0.2.0`, Nix flake.

### Added

- Shell completions via `clap_complete` ([#8](https://github.com/utensils/claudex/pull/8)).
- Terminal beautification, code coverage wiring, and CI ([#10](https://github.com/utensils/claudex/pull/10)).
- VitePress documentation site deployed via GitHub Pages ([#12](https://github.com/utensils/claudex/pull/12)).
- Release workflow, install script, and flake metadata ([#13](https://github.com/utensils/claudex/pull/13)).
- `CLAUDEX_DIR` override for index location ([#14](https://github.com/utensils/claudex/pull/14)).

### Fixed

- `watch` tails `--debug-file` path instead of the dead `~/.claude/debug/latest` ([#9](https://github.com/utensils/claudex/pull/9)).

### Changed

- Cleanup: untrack cruft, tighten `.gitignore`, sync docs, bump MSRV to 1.95 ([#11](https://github.com/utensils/claudex/pull/11)).
- Docs align recipes and command shapes with v0.2.0 ([#14](https://github.com/utensils/claudex/pull/14)).

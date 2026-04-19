# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- New `claudex session <selector>` drill-down report: overview, tokens, per-model usage, turn stats, tools, files, PR links, stop reasons, attachments, permission changes. Supports `--json` and `--no-index`.
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

[unreleased]: https://github.com/utensils/claudex/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/utensils/claudex/releases/tag/v0.2.0

# File layout

Where claudex reads from and writes to.

## Reads

### `~/.claude/projects/<encoded-path>/<session>.jsonl`

Claude Code's own session persistence — one JSONL file per session, under a
directory whose name is an encoded version of the project path.

**Path encoding:**

- Path separators (`/`) become `-`.
- Hidden directories (leading `.`) become `--`.
  - `/home/you/.config` → `-home-you--config`
- Trailing fragments are preserved verbatim where possible.

`store::decode_project_name` reverses this. `store::display_project_name` adds
the `(worktree)` suffix for worktree-based sessions.

### Worktrees

Sessions inside `~/.claude/worktrees/<branch>/…` are rolled up to the parent
project via `canonical_project_path`, so a worktree checkout of `claudex` ends
up grouped with the main `claudex` project for every report.

### `~/.codex/sessions/**/rollout-*.jsonl`

OpenAI Codex CLI active session transcripts. `claudex codex` scans these files
recursively, counts session metadata, user/agent messages, reasoning items, tool
calls/results, review events, compactions, aborts, CLI versions, and per-project
activity.

### `~/.codex/archived_sessions/rollout-*.jsonl`

Codex's archived rollout transcripts. `claudex codex` includes these in the
same totals while also reporting active vs archived session-file counts.

### `~/.codex/session_index.jsonl`

Optional Codex title index. When present, `claudex codex` uses it to attach
thread titles to the `most_recent` JSON object and text report.

### `~/.codex/state_5.sqlite`

Optional Codex state database. `claudex codex` opens this read-only and reports
thread counts, token totals, top projects, and model/provider counts when the
`threads` table is available.

## Writes

Claudex writes exclusively under `~/.claudex/` (or `$CLAUDEX_DIR`):

### `~/.claudex/index.db`

The SQLite index. See [Index schema](/reference/schema). Don't edit manually.

### `~/.claudex/debug/latest.log` (optional)

Only present if you use `claudex watch` together with
`claude --debug-file ~/.claudex/debug/latest.log`. Created lazily on first
use. Truncated on each new `claude` invocation.

Watch can read from any path you pass to `--follow`; the default path exists
so a two-terminal workflow works without configuration.

## Environment

- `CLAUDEX_DIR` — override the location of `~/.claudex/`.
- `NO_COLOR` — disable color output (honored when `--color` is `auto`).
- `COLUMNS` — override terminal width for table rendering.

## Nothing lives in `~/.claude/` or `~/.codex/` that claudex owns

Claudex is strictly a reader of `~/.claude/projects/` and `~/.codex/`. It never
writes there, never modifies transcripts, never interferes with Claude Code or
Codex state.

Uninstalling is `rm -rf ~/.claudex` plus `cargo uninstall claudex` (or deleting
the binary if installed another way).

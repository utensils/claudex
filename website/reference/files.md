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

## Nothing lives in `~/.claude/` that claudex owns

Claudex is strictly a reader of `~/.claude/projects/`. It never writes there,
never modifies transcripts, never interferes with Claude Code's state.

Uninstalling is `rm -rf ~/.claudex` plus `cargo uninstall claudex` (or deleting
the binary if installed another way).

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

### `~/.codex/sessions/**/rollout-*.jsonl` and `~/.codex/archived_sessions/**`

OpenAI Codex CLI session transcripts. The Codex provider ingests these into the
index like any other: project from the transcript's `cwd`, model from
`turn_context`, token usage from the last cumulative `token_count` record, tool
calls, reasoning, and message text for full-text search. Files under
`archived_sessions/` are indexed and flagged archived.

### `${CLAUDEX_COPILOT_DIR:-~/.copilot}/session-state/<uuid>/events.jsonl`

GitHub Copilot CLI event logs, one directory per session. The Copilot provider
reads the typed event stream: project from `session.start`'s `cwd`, messages
and tool calls from `user.message`/`assistant.message`, per-model token usage
from the final `session.shutdown` metrics (premium-request counts are kept in
`extras`). The sibling `workspace.yaml` supplies the session title and a cwd
fallback; `session.db`, `checkpoints/`, `files/`, and `research/` are ignored.

### VS Code `User/workspaceStorage/<hash>/chatSessions/*.{json,jsonl}`

VS Code Copilot Chat sessions (stable and Insiders; override the `User`
directory with `CLAUDEX_VSCODE_USER_DIR`). Older sessions are one JSON
document; current VS Code writes a delta log that claudex replays before
extraction. The workspace folder comes from the sibling `workspace.json`;
chats from empty windows live in `globalStorage/emptyWindowChatSessions/`.
VS Code stores no token counts locally, so these sessions index at `$0`.

### `~/.pi/agent/sessions/<cwd-encoded>/*.jsonl`

Pi session transcripts, one directory per working directory. The Pi provider
ingests per-model token usage and trusts Pi's own per-message cost (local Ollama
models report `$0`).

### `${OPENCLAW_STATE_DIR:-~/.openclaw}/agents/<agent-id>/sessions/`

OpenClaw session stores. claudex reads `sessions.json` for metadata enrichment,
classic `<session-id>.jsonl` transcripts for canonical message/usage data, and
`<session-id>.trajectory.jsonl` or pointer-resolved trajectory files for runtime
status and trajectory-only sessions. Trajectory-only rows are keyed by logical
OpenClaw session source, so they are reused when the classic transcript appears.

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

## Nothing in the provider directories belongs to claudex

Claudex is strictly a reader of `~/.claude/projects/`, `~/.codex/`,
`~/.copilot/`, VS Code `workspaceStorage`, `~/.pi/agent/`, and OpenClaw state.
It never writes there, never modifies transcripts, never interferes with any
agent's state.
(`claudex skills install` does
write `SKILL.md` files into harness config dirs, but only when you ask it to.)

Uninstalling is `rm -rf ~/.claudex` plus `cargo uninstall claudex` (or deleting
the binary if installed another way).

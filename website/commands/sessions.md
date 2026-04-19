# `sessions`

List sessions grouped by project, sorted by recency.

## Usage

```bash
claudex sessions [-p/--project <substr>] [--file <substr>]
                 [-l/--limit <n>] [--json] [--no-index]
```

## Flags

| Flag                       | Default | Description                                       |
| -------------------------- | ------- | ------------------------------------------------- |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path.    |
| `--file <substr>`          | —       | Only show sessions that touched a matching file.  |
| `-l`, `--limit <n>`        | `20`    | Maximum number of rows.                           |
| `--json`                   | off     | Emit JSON.                                        |
| `--no-index`               | off     | Scan JSONL files directly; don't touch the index. |

## Example

```bash
# 10 most recent sessions across all projects
claudex sessions --limit 10

# All sessions in one project
claudex sessions --project claudex --limit 100

# Sessions that touched one file
claudex sessions --file src/index.rs

# JSON for piping
claudex sessions --json --limit 5
```

## Columns

| Column   | Source                                                                          |
| -------- | ------------------------------------------------------------------------------- |
| Project  | Decoded project directory name (worktree sessions render as `name (worktree)`). |
| Session  | First 8 characters of the session UUID.                                         |
| Date     | First timestamp, rendered as `YYYY-MM-DD HH:MM`.                                |
| Messages | Count of user + assistant messages in the session.                              |
| Duration | Wall-clock duration from first to last message.                                 |
| Model    | Sole model tag, or `mixed` when the session switched models.                    |

## JSON shape

```json
[
  {
    "project": "claudex",
    "session_id": "e1a2f4e8-...",
    "file_path": "/Users/you/.claude/projects/.../e1a2f4e8....jsonl",
    "date": "2026-04-18T14:22:13+00:00",
    "message_count": 83,
    "duration_ms": 1283410,
    "model": "claude-sonnet-4-6"
  }
]
```

## Notes

- **Worktree aggregation.** Sessions from `~/.claude/worktrees/<branch>/…`
  display under the parent project with `(worktree)` appended.
- **Session-ID prefix.** The 8-character prefix is enough to disambiguate
  nearly all sessions. For [`session`](/commands/session) or
  [`export`](/commands/export), you can pass that prefix directly.

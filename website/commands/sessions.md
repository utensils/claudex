# `sessions`

List sessions grouped by project, sorted by recency.

## Usage

```bash
claudex sessions [-p/--project <substr>] [-l/--limit <n>] [--json] [--no-index]
```

## Flags

| Flag                       | Default | Description                                       |
| -------------------------- | ------- | ------------------------------------------------- |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path.    |
| `-l`, `--limit <n>`        | `20`    | Maximum number of rows.                           |
| `--json`                   | off     | Emit JSON.                                        |
| `--no-index`               | off     | Scan JSONL files directly; don't touch the index. |

## Example

```bash
# 10 most recent sessions across all projects
claudex sessions --limit 10

# All sessions in one project
claudex sessions --project claudex --limit 100

# JSON for piping
claudex sessions --json --limit 5
```

## Columns

| Column   | Source                                                                          |
| -------- | ------------------------------------------------------------------------------- |
| Project  | Decoded project directory name (worktree sessions render as `name (worktree)`). |
| Session  | First 8 characters of the session UUID.                                         |
| Date     | First timestamp, rendered as `YYYY-MM-DD`.                                      |
| Messages | Count of user + assistant messages in the session.                              |
| Duration | Wall-clock duration from first to last message.                                 |
| Model    | Most recent model tag seen in the session (Opus / Sonnet / Haiku).              |

## JSON shape

```json
[
  {
    "project": "claudex",
    "session_id": "e1a2f4e8-...",
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
  nearly all sessions. For [`export`](/commands/export), you can pass that
  prefix directly.

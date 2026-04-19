# `files`

Most frequently modified files across your sessions.

## Usage

```bash
claudex files [-p/--project <substr>] [-l/--limit <n>] [--json]
```

## Flags

| Flag                       | Default | Description                                    |
| -------------------------- | ------- | ---------------------------------------------- |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path. |
| `-l`, `--limit <n>`        | `20`    | Maximum files to show.                         |
| `--json`                   | off     | Emit JSON.                                     |

## Example

```bash
# Top 10 most-edited files overall
claudex files --limit 10

# In one project
claudex files --project claudex --limit 20

# Files you've edited in 5+ sessions
claudex files --json | jq '.[] | select(.session_count >= 5) | .file_path'
```

## Columns

| Column   | Source                                    |
| -------- | ----------------------------------------- |
| File     | File path (usually absolute).             |
| Edits    | Count of edit events across sessions.     |
| Sessions | Distinct sessions that modified the file. |

## JSON shape

```json
[
  {
    "file_path": "/Users/you/projects/claudex/src/index.rs",
    "edit_count": 92,
    "session_count": 14
  }
]
```

## Notes

- **What counts.** Any edit the assistant performs through Edit, Write, or
  NotebookEdit records a `file_modifications` row. Bash operations that happen
  to touch files (like `sed -i`) are not counted.
- **No `--no-index`.** File modifications are stored in the index; the
  file-scan path would re-parse everything to compute them.
- **Path shape.** Paths are stored verbatim — tilde expansion depends on how
  Claude Code recorded them in the session.

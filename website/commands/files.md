# `files`

Most frequently modified files across your sessions.

## Usage

```bash
claudex files [-p/--project <substr>] [--path <substr>]
              [-l/--limit <n>] [--json]
```

## Flags

| Flag                       | Default | Description                                    |
| -------------------------- | ------- | ---------------------------------------------- |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path. |
| `--path <substr>`          | —       | Filter matching file paths by substring.       |
| `-l`, `--limit <n>`        | `20`    | Maximum files to show.                         |
| `--json`                   | off     | Emit JSON.                                     |

## Example

```bash
# Top 10 most-edited files overall
claudex files --limit 10

# In one project
claudex files --project claudex --limit 20

# One path
claudex files --path src/index.rs

# Files with 20 or more edit events
claudex files --json | jq '.[] | select(.modification_count >= 20) | .file_path'
```

## Columns

| Column        | Source                                           |
| ------------- | ------------------------------------------------ |
| File          | File path (as recorded by Claude Code).          |
| Modifications | Count of edit events across all sessions.        |
| Sessions      | Distinct sessions that touched the file.         |
| Last Touched  | Most recent session timestamp touching the file. |
| Top Project   | Project with the most edit events for the file.  |

## JSON shape

```json
[
  {
    "file_path": "CLAUDE.md",
    "modification_count": 60,
    "distinct_session_count": 33,
    "last_touched_at": "2026-04-18T14:22:13+00:00",
    "top_project": "/Users/you/projects/claudex"
  }
]
```

## Notes

- **What counts.** Any edit the assistant performs through Edit, Write, or
  NotebookEdit records a `file_modifications` row. Bash operations that
  happen to touch files (like `sed -i`) are not counted.
- **Edit events and session reach.** `modification_count` is still the raw edit
  event count. `distinct_session_count` tells you how broadly the file is
  touched across sessions.
- **No `--no-index`.** File modifications are stored in the index only; the
  file-scan path would re-parse everything to compute them.
- **Path shape.** Paths are stored verbatim — tilde expansion depends on how
  Claude Code recorded them in the session.

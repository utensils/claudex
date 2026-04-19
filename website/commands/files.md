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

# Files with 20 or more edit events
claudex files --json | jq '.[] | select(.modification_count >= 20) | .file_path'
```

## Columns

| Column        | Source                                    |
| ------------- | ----------------------------------------- |
| File          | File path (as recorded by Claude Code).   |
| Modifications | Count of edit events across all sessions. |

## JSON shape

```json
[
  { "file_path": "CLAUDE.md", "modification_count": 60 },
  { "file_path": "README.md", "modification_count": 33 }
]
```

## Notes

- **What counts.** Any edit the assistant performs through Edit, Write, or
  NotebookEdit records a `file_modifications` row. Bash operations that
  happen to touch files (like `sed -i`) are not counted.
- **Edit events, not distinct sessions.** `modification_count` sums every
  edit event — editing the same file 10 times in one session adds 10. For a
  "distinct sessions that touched this file" metric, query
  `file_modifications` directly via SQL; see
  [Recipes → Files touched by the most distinct sessions](/guide/recipes#files-touched-by-the-most-distinct-sessions).
- **No `--no-index`.** File modifications are stored in the index only; the
  file-scan path would re-parse everything to compute them.
- **Path shape.** Paths are stored verbatim — tilde expansion depends on how
  Claude Code recorded them in the session.

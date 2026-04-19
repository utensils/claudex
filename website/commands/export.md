# `export`

Export a session transcript as Markdown or JSON.

## Usage

```bash
claudex export <selector> [--format markdown|json]
                           [-o/--output <path>]
                           [-p/--project <substr>]
```

## Flags

| Flag                       | Default    | Description                                                   |
| -------------------------- | ---------- | ------------------------------------------------------------- |
| `<selector>`               | —          | Session ID prefix _or_ project name substring. Required.      |
| `--format`                 | `markdown` | `markdown` or `json`.                                         |
| `-o`, `--output <path>`    | stdout     | Write to a file instead of stdout.                            |
| `-p`, `--project <substr>` | —          | Disambiguate when the selector matches more than one session. |

## Example

```bash
# List recent sessions to find a prefix
claudex sessions --limit 5

# Export as Markdown to stdout
claudex export e1a2f4

# Save to a file
claudex export e1a2f4 --output session.md

# JSON instead
claudex export e1a2f4 --format json --output session.json

# Ambiguous prefix? Add --project
claudex export ab --project claudex
```

## Markdown output

The Markdown export renders the full conversation as:

```markdown
# Session: <short-id>

**Project:** <path>
**Date:** <YYYY-MM-DD HH:MM UTC>
**Model:** <model>

---

## User

_<timestamp>_

<content>

## Assistant

_<timestamp>_

<content>
```

Each turn is a section. Tool calls and tool results are inlined. This is the
shape you want for pasting into docs, wikis, or PR descriptions.

## JSON output

If the selector resolves to one session, JSON export is one object. If it
resolves to multiple sessions, JSON export is an array of those objects.
Each object has top-level keys: `session_id`, `project`, `date`, `model`,
`message_count`, `messages`. The `messages` array preserves every raw JSONL
record from the source session file, in chronological order — no flattening.

```json
{
  "session_id": "<uuid>",
  "project": "<path>",
  "date": "<iso-8601>",
  "model": "<model>",
  "message_count": 14,
  "messages": [
    {
      "type": "user",
      "uuid": "...",
      "parentUuid": null,
      "sessionId": "<uuid>",
      "timestamp": "<iso-8601>",
      "cwd": "<path>",
      "gitBranch": "<branch>",
      "version": "<claude-code-version>",
      "userType": "external",
      "isSidechain": false,
      "permissionMode": "default",
      "entrypoint": "cli",
      "promptId": "...",
      "message": { "role": "user", "content": "..." }
    }
  ]
}
```

Each record's `message.role` is `user` or `assistant`. `message.content` is
either a string (user prompts) or an array of typed blocks (assistant
responses — text, tool_use, thinking, etc.) per the Anthropic API shape.

Useful jq:

```bash
# Count messages by role
claudex export <sid> --format json \
  | jq '[.messages[].message.role] | group_by(.) | map({role: .[0], count: length})'

# Extract only the text content
claudex export <sid> --format json \
  | jq -r '.messages[] | select(.message.role == "assistant") | .message.content | if type == "string" then . else (.[] | select(.type == "text") | .text) end'
```

## Notes

- **Selector precedence.** Session-ID prefix wins if there's an exact match,
  even when `--project` is set. Fall back to the project filter otherwise.
- **No `--json` flag.** Use `--format json` — it's the format of the export,
  not a summary layer.
- **Multiple matches.** Markdown writes the matching sessions sequentially.
  JSON returns a proper array instead of concatenated objects.
- **Large sessions.** Markdown export is streaming where possible, but huge
  sessions (100+ MB JSONL) can produce very long Markdown files. Use
  `--format json` if you want to slice the output yourself.

# `session`

Detailed drill-down for one session.

## Usage

```bash
claudex session <selector> [-p/--project <substr>] [--json] [--no-index]
```

## Flags

| Flag                       | Default | Description                                              |
| -------------------------- | ------- | -------------------------------------------------------- |
| `<selector>`               | —       | Session ID prefix or project-name substring. Required.   |
| `-p`, `--project <substr>` | —       | Narrow candidate sessions before resolving the selector. |
| `--json`                   | off     | Emit structured JSON.                                    |
| `--no-index`               | off     | Parse the JSONL directly instead of using the index.     |

## Example

```bash
# Inspect one session in text form
claudex session e1a2f4

# JSON for automation
claudex session e1a2f4 --json

# Ambiguous selector? Narrow it
claudex session ab --project claudex
```

## What it shows

- Overview: project, file path, timestamps, duration, messages, model, cost
- Tokens: input, output, cache write, cache read, total
- Per-model usage inside the session, including mixed-model sessions
- Turn stats, thinking blocks, tools, files touched, PR links
- Stop reasons, attachments, and permission-mode changes

## JSON shape

```json
{
  "project": "/Users/you/projects/claudex",
  "file_path": "/Users/you/.claude/projects/.../e1a2f4....jsonl",
  "session_id": "e1a2f4e8-...",
  "date": "2026-04-18T14:22:13+00:00",
  "last_activity": "2026-04-18T15:07:44+00:00",
  "duration_ms": 1283410,
  "message_count": 83,
  "model": "mixed",
  "input_tokens": 326297,
  "output_tokens": 6679149,
  "cache_creation_tokens": 80583078,
  "cache_read_tokens": 7157509259,
  "total_tokens": 7244707783,
  "cost_usd": 12735.6563118,
  "thinking_block_count": 14,
  "turn_stats": {
    "turn_count": 26,
    "avg_duration_ms": 321938.0,
    "p50_duration_ms": 66000.0,
    "p95_duration_ms": 572000.0,
    "max_duration_ms": 657000
  },
  "models": [
    {
      "model": "claude-opus-4-6",
      "model_family": "Opus",
      "assistant_message_count": 3,
      "input_tokens": 1000,
      "output_tokens": 500,
      "cache_creation_tokens": 200,
      "cache_read_tokens": 5000,
      "cost_usd": 0.09875,
      "inference_geos": ["us-east-1"],
      "service_tiers": ["default"],
      "avg_speed": 35.2,
      "iterations": 3
    }
  ],
  "tools": [{ "tool": "Edit", "count": 14 }],
  "files_modified": ["src/index.rs"],
  "pr_links": [
    {
      "pr_number": 42,
      "pr_url": "https://github.com/...",
      "pr_repository": "org/repo",
      "timestamp": "2026-04-18T10:03:00Z"
    }
  ],
  "stop_reasons": [{ "stop_reason": "end_turn", "count": 8 }],
  "attachments": [{ "filename": "bug.png", "mime_type": "image/png" }],
  "permission_changes": [
    { "mode": "bypassPermissions", "timestamp": "2026-04-18T10:00:00Z" }
  ]
}
```

## Notes

- **Selector resolution.** Session-ID prefix wins if it matches. Project-name
  matching is a fallback for when you only know the project.
- **Exactly one session.** `session` is not a batch command. If the selector
  matches more than one session, claudex stops and asks you to refine it.
- **Use `export` for transcripts.** `session` is an analysis report; it does
  not dump the full message history. [`export`](/commands/export) does.

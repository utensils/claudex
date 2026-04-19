# `summary`

One-screen dashboard of your Claude Code usage. This is the command you run
first.

## Usage

```bash
claudex summary [--json] [--no-index]
```

## What it shows

- **Sessions** — total, today, this week.
- **Cost (estimated)** — all time, this week.
- **Tokens** — sum of input, output, cache-write, cache-read.
- **Thinking** — number of extended-thinking blocks.
- **Turns** — average turn duration in milliseconds.
- **PRs** — count of sessions linked to pull requests.
- **Files** — total distinct files modified across all sessions.
- **Top projects** — up to 5 by session count.
- **Top tools** — up to 5 by call count (`Bash`, `Edit`, `Read`, etc).
- **Top stop reasons** — up to 5 by assistant stop reason.
- **Model distribution** — sessions and cost per model family.
- **Most recent session** — project, session ID, timestamp, model, message
  count.

## Flags

| Flag         | Description                                       |
| ------------ | ------------------------------------------------- |
| `--json`     | Emit JSON (see shape below).                      |
| `--no-index` | Scan JSONL files directly; don't touch the index. |

## Example

```bash
claudex summary
```

```bash
claudex summary --json | jq '.total_cost_usd'
```

## JSON shape

```json
{
  "total_sessions": 372,
  "sessions_today": 4,
  "sessions_this_week": 28,
  "total_cost_usd": 512.34,
  "cost_this_week_usd": 41.22,
  "total_input_tokens": 93847123,
  "total_output_tokens": 18123456,
  "total_cache_creation_tokens": 301122,
  "total_cache_read_tokens": 5512201,
  "total_tokens": 116784902,
  "thinking_block_count": 1289,
  "avg_turn_duration_ms": 4132,
  "pr_count": 14,
  "files_modified_count": 842,
  "top_projects": [{ "project": "claudex", "sessions": 41 }],
  "top_tools": [{ "tool": "Edit", "calls": 1240 }],
  "top_stop_reasons": [{ "stop_reason": "end_turn", "count": 812 }],
  "model_distribution": [
    { "model": "claude-opus-4-7", "sessions": 12, "cost_usd": 210.44 }
  ],
  "most_recent": {
    "project": "claudex",
    "session_id": "e1a2f4...",
    "date": "2026-04-18T14:22:13+00:00",
    "model": "claude-sonnet-4-6",
    "message_count": 83
  }
}
```

## Notes

- **Week boundary.** "This week" starts Monday 00:00 in the local time zone.
- **Cost is approximate.** See [Pricing model](/reference/pricing).
- **Missing `most_recent`.** If `~/.claude/projects/` is empty, `most_recent`
  is `null`.

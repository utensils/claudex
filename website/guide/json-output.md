# JSON output

Every read command supports `--json`, emitting a pretty-printed, stable shape.
This is the recommended contract for automation — it doesn't change between
patch releases.

## Examples

```bash
# Top projects by cost
claudex cost --json --limit 5

# All sessions as JSON
claudex sessions --json

# Summary as JSON
claudex summary --json
```

## Piping to jq

```bash
# Sum cost across all projects
claudex cost --json | jq '[.[].cost_usd] | add'

# Project names only, sorted
claudex sessions --json | jq -r '.[].project' | sort -u

# p95 turn duration for one project
claudex turns --project claudex --json | jq '.[0].p95_ms'

# Every PR this week
claudex prs --json | jq '.[] | select(.first_timestamp > (now - 604800) * 1000)'
```

## Shapes, by command

### `summary`

```json
{
  "total_sessions": 372,
  "sessions_today": 4,
  "sessions_this_week": 28,
  "total_cost_usd": 512.34,
  "cost_this_week_usd": 41.22,
  "total_tokens": 93847123,
  "thinking_block_count": 1289,
  "avg_turn_duration_ms": 4132,
  "pr_count": 14,
  "files_modified_count": 842,
  "top_projects": [{ "project": "claudex", "sessions": 41 }],
  "top_tools": [{ "tool": "Edit", "calls": 1240 }],
  "model_distribution": [
    { "model": "claude-opus-4-7", "sessions": 12, "cost_usd": 210.44 }
  ],
  "most_recent": {
    "project": "claudex",
    "session_id": "e1a2f4…",
    "date": "2026-04-18T14:22:13+00:00",
    "model": "claude-sonnet-4-6",
    "message_count": 83
  }
}
```

### `sessions`, `cost --per-session`, `tools --per-session`

Array of objects. Always includes `project` and `session_id`. Cost / token /
tool fields depend on the command.

### `cost`

Array sorted by `cost_usd` descending. Each entry includes all four token
counts plus the model that produced them.

### `turns`, `prs`, `files`, `models`

Arrays. Field names mirror the column headers you see in the table output.

## Why not CSV?

JSON round-trips nested structures (token breakdowns, model distributions)
without flattening. For a CSV equivalent, `jq -r` has you covered:

```bash
claudex cost --json \
  | jq -r '.[] | [.project, .cost_usd] | @csv'
```

## Stability

The JSON shape is the public contract. Fields may be added; existing fields
aren't removed or renamed without a major-version bump. Field order is
preserved within objects because `serde_json` uses insertion order.

If you need richer access, see the [index schema](/reference/schema) — you can
query the SQLite database directly. But JSON output is the stable surface.

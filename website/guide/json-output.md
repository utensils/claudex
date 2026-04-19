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
# Total cost across everything
claudex summary --json | jq '.total_cost_usd'

# Project names only, sorted
claudex sessions --json | jq -r '.[].project' | sort -u

# p95 turn duration for one project
claudex turns --project claudex --json | jq '.[0].p95_duration_ms'

# Every PR this month
claudex prs --json \
  | jq --arg m "$(date +%Y-%m)" '.[] | select(.timestamp | startswith($m))'
```

## Shapes, by command

Per-command pages have the authoritative shape; this section is a cheat
sheet.

### `summary`

Single object. Keys: `total_sessions`, `sessions_today`,
`sessions_this_week`, `total_cost_usd`, `cost_this_week_usd`,
`total_tokens`, `thinking_block_count`, `avg_turn_duration_ms`, `pr_count`,
`files_modified_count`, `top_projects`, `top_tools`, `model_distribution`,
`most_recent`. See [`summary`](/commands/summary) for the full shape.

### `sessions`

Array. Each entry: `project`, `session_id`, `date`, `duration_ms`,
`message_count`, `model`.

### `cost` (aggregated) / `cost --per-session`

- Aggregated: `project`, `sessions`, `input_tokens`, `output_tokens`,
  `cache_creation_tokens`, `cache_read_tokens`, `models` (array of
  families), `cost_usd`.
- Per-session: `project`, `session_id`, `date`, `model`, `input_tokens`,
  `output_tokens`, `cache_creation_tokens`, `cache_read_tokens`, `cost_usd`.

### `tools` (aggregated) / `tools --per-session`

- Aggregated: `tool`, `count`.
- Per-session: `project`, `session_id`, `date`, `tools` (object —
  `{name: count}`).

### `models`

Array. Each entry: `model`, `model_family`, `session_count`, `input_tokens`,
`output_tokens`, `cost_usd`.

### `turns`

Array. Each entry: `project`, `turn_count`, `avg_duration_ms`,
`p50_duration_ms`, `p95_duration_ms`, `max_duration_ms`.

### `prs`

Array. Each entry: `project`, `session_id`, `timestamp`, `pr_number`,
`pr_repository`, `pr_url`.

### `files`

Array. Each entry: `file_path`, `modification_count`.

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

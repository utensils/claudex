# JSON output

Every read command supports `--json`, emitting a pretty-printed, stable shape.
This is the recommended contract for automation — it doesn't change between
patch releases. Every per-row report includes a `"provider"` key
(`claude` / `codex` / `pi`) so results are unambiguous across providers.

## Examples

```bash
# Top projects by cost
claudex cost --json --limit 5

# All sessions as JSON
claudex sessions --json

# Summary as JSON
claudex summary --json

# Just Codex sessions, as JSON
claudex sessions --provider codex --json
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
`total_input_tokens`, `total_output_tokens`, `total_cache_creation_tokens`,
`total_cache_read_tokens`, `total_tokens`, `thinking_block_count`,
`avg_turn_duration_ms`, `pr_count`, `files_modified_count`, `top_projects`,
`top_tools`, `top_stop_reasons`, `model_distribution`, `most_recent`. See
[`summary`](/commands/summary) for the full shape.

When invoked with `--plan flat-monthly:USD`, `summary --json` is **additive**:
the historical `total_cost_usd` and `cost_this_week_usd` keys are still
emitted (script authors who already grep for them keep working), and the
following keys are added alongside:

- `plan` — discriminator, `"flat-monthly"` when set; absent under `--plan api`.
- `actual_monthly_cost_usd` — the flat fee passed in.
- `api_equivalent_total_usd` — alias of `total_cost_usd`.
- `api_equivalent_week_usd` — alias of `cost_this_week_usd`.
- `leverage_this_week_multiple` — `api_equivalent_week_usd ÷ weekly_plan_cost`
  where `weekly_plan_cost = actual_monthly_cost_usd / 4.348`. Emitted as
  `null` when there's no usage this week (a brand-new account would
  otherwise show a misleading "0× leverage").

### `sessions`

Array. Each entry: `provider`, `project`, `session_id`, `file_path`, `date`,
`duration_ms`, `message_count`, `model`.

### `session`

Single object. Keys include `project`, `file_path`, `session_id`, `date`,
`last_activity`, token totals, `cost_usd`, `turn_stats`, `models`, `tools`,
`files_modified`, `pr_links`, `stop_reasons`, `attachments`,
`permission_changes`.

### `cost` (aggregated) / `cost --per-session`

- Aggregated: `project`, `sessions`, `input_tokens`, `output_tokens`,
  `cache_creation_tokens`, `cache_read_tokens`, `avg_cost_per_session_usd`,
  `models` (array of families), `cost_usd`.
- Per-session: `provider`, `project`, `session_id`, `date`, `model`, `models`,
  `input_tokens`, `output_tokens`, `cache_creation_tokens`,
  `cache_read_tokens`, `cost_usd`.

### `search`

Array. Each entry: `provider`, `project`, `session_id`, `message_timestamp`,
`message_type`, `snippet`, `rank`.

### `tools` (aggregated) / `tools --per-session`

- Aggregated: `tool`, `count`.
- Per-session: `project`, `session_id`, `date`, `tools` (object —
  `{name: count}`).

### `models`

Array. Each entry: `model`, `model_family`, `session_count`, `input_tokens`,
`output_tokens`, `cache_creation_tokens`, `cache_read_tokens`,
`avg_cost_per_session_usd`, `avg_tokens_per_session`, `service_tiers`,
`inference_geos`, `avg_speed`, `total_iterations`, `cost_usd`.

### `turns`

Array. Each entry: `project`, `turn_count`, `avg_duration_ms`,
`p50_duration_ms`, `p95_duration_ms`, `max_duration_ms`.

### `prs`

Array. Each entry: `provider`, `project`, `session_id`, `timestamp`,
`pr_number`, `pr_repository`, `pr_url`.

### `files`

Array. Each entry: `file_path`, `modification_count`,
`distinct_session_count`, `last_touched_at`, `top_project`.

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

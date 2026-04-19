# `models`

Call counts, token usage, and cost broken down per model.

## Usage

```bash
claudex models [-p/--project <substr>] [--json]
```

## Flags

| Flag                       | Description                                    |
| -------------------------- | ---------------------------------------------- |
| `-p`, `--project <substr>` | Filter by substring match on the project path. |
| `--json`                   | Emit JSON.                                     |

## Example

```bash
# Global breakdown
claudex models

# One project
claudex models --project claudex

# All Opus rows (any variant)
claudex models --json | jq '.[] | select(.model_family == "Opus")'

# Top model by cost
claudex models --json | jq 'sort_by(-.cost_usd)[0] | {model, cost_usd}'
```

## Columns

| Column      | Source                                                                         |
| ----------- | ------------------------------------------------------------------------------ |
| Model       | Full model tag from Claude Code (e.g. `claude-opus-4-7`, `claude-sonnet-4-6`). |
| Family      | Opus / Sonnet / Haiku (derived from the name).                                 |
| Sessions    | Distinct sessions that used the model.                                         |
| Input       | Total input tokens.                                                            |
| Output      | Total output tokens.                                                           |
| Cache Write | Total cache-creation tokens.                                                   |
| Cache Read  | Total cache-read tokens.                                                       |
| Avg/Session | Average spend per session using the model.                                     |
| Avg Tokens  | Average total tokens per session using the model.                              |
| Cost (USD)  | Model-specific cost.                                                           |

## JSON shape

```json
[
  {
    "model": "claude-opus-4-6",
    "model_family": "Opus",
    "session_count": 467,
    "input_tokens": 1050304,
    "output_tokens": 21225014,
    "cache_creation_tokens": 4027298,
    "cache_read_tokens": 893758965,
    "avg_cost_per_session_usd": 64.35,
    "avg_tokens_per_session": 1958324.4,
    "service_tiers": ["default", "priority"],
    "inference_geos": ["us-east-1", "eu-west-1"],
    "avg_speed": 35.2,
    "total_iterations": 982,
    "cost_usd": 30051.131094
  }
]
```

## Notes

- **Family detection.** `model_family` is a substring match on the model tag:
  anything containing `opus` is `Opus`, `haiku` is `Haiku`, anything else is
  `Sonnet`. See [Pricing model](/reference/pricing).
- **Mixed-model sessions.** A session that switched models appears under
  every model it used; `session_count` counts each model-session pair once.
- **Runtime metadata.** `service_tiers`, `inference_geos`, `avg_speed`, and
  `total_iterations` come from the usage blocks Claude Code records on
  assistant messages. `avg_speed` is the mean of per-session model averages,
  not a throughput-weighted global average. These fields are best-effort;
  missing values stay empty/null.

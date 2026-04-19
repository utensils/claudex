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

| Column     | Source                                                                         |
| ---------- | ------------------------------------------------------------------------------ |
| Model      | Full model tag from Claude Code (e.g. `claude-opus-4-7`, `claude-sonnet-4-6`). |
| Family     | Opus / Sonnet / Haiku (derived from the name).                                 |
| Sessions   | Distinct sessions that used the model.                                         |
| Input      | Total input tokens.                                                            |
| Output     | Total output tokens.                                                           |
| Cost (USD) | Model-specific cost.                                                           |

## JSON shape

```json
[
  {
    "model": "claude-opus-4-6",
    "model_family": "Opus",
    "session_count": 467,
    "input_tokens": 1050304,
    "output_tokens": 21225014,
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
- **Cache tokens.** Not broken out here — see [`cost`](/commands/cost) or
  [`summary`](/commands/summary) for cache-read / cache-creation detail.

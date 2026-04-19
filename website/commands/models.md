# `models`

Call counts, token usage, and cost broken down per model.

## Usage

```bash
claudex models [-p/--project <substr>] [--json]
```

## Flags

| Flag | Description |
|------|-------------|
| `-p`, `--project <substr>` | Filter by substring match on the project path. |
| `--json` | Emit JSON. |

## Example

```bash
# Global breakdown
claudex models

# One project
claudex models --project claudex

# Most-used Opus variant across everything
claudex models --json | jq '.[] | select(.model | test("opus"; "i"))'
```

## Columns

| Column | Source |
|--------|--------|
| Model | Full model tag from Claude Code (e.g. `claude-opus-4-7`, `claude-sonnet-4-6`). |
| Tier | Opus / Sonnet / Haiku (derived from the name). |
| Sessions | Distinct sessions that used the model. |
| Messages | Assistant messages attributed to the model. |
| Input | Total input tokens. |
| Output | Total output tokens. |
| Cache W | Cache-creation tokens. |
| Cache R | Cache-read tokens. |
| Cost (USD) | Model-specific cost. |

## JSON shape

```json
[
  {
    "model": "claude-opus-4-7",
    "tier": "Opus",
    "sessions": 12,
    "messages": 421,
    "input_tokens": 88000,
    "output_tokens": 212000,
    "cache_creation_tokens": 120000,
    "cache_read_tokens": 8_400_000,
    "cost_usd": 210.44
  }
]
```

## Notes

- **Tier detection.** The tier is derived from a substring match on the model
  name — any model tag containing `opus`, `haiku`, or (by default) otherwise
  is treated as Sonnet-class. See [Pricing model](/reference/pricing).
- **Mixed-model sessions** show up under multiple rows; the `sessions` column
  counts each model-session pair distinctly.

# `cost`

Token usage and approximate cost, aggregated per project or per session.

## Usage

```bash
claudex cost [-p/--project <substr>] [--per-session]
             [-l/--limit <n>] [--json] [--no-index]
```

## Flags

| Flag                       | Default | Description                                               |
| -------------------------- | ------- | --------------------------------------------------------- |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path.            |
| `--per-session`            | off     | Break down per session instead of aggregating by project. |
| `-l`, `--limit <n>`        | `20`    | Maximum number of rows.                                   |
| `--json`                   | off     | Emit JSON.                                                |
| `--no-index`               | off     | Scan JSONL files directly.                                |

## Example

```bash
# Top 10 projects by cost
claudex cost --limit 10

# Per-session breakdown for one project
claudex cost --project claudex --per-session --limit 20

# Total cost across everything, as a single number
claudex cost --json --limit 1000 | jq '[.[].cost_usd] | add'
```

## Columns (aggregated)

| Column     | Source                                   |
| ---------- | ---------------------------------------- |
| Project    | Decoded project name.                    |
| Sessions   | Number of sessions counted.              |
| Input      | Total input tokens.                      |
| Output     | Total output tokens.                     |
| Cache Read | Cache-read tokens.                       |
| Model(s)   | Model families seen (Opus/Sonnet/Haiku). |
| Cost (USD) | Sum of per-message costs.                |

## Columns (per-session)

| Column     | Source                          |
| ---------- | ------------------------------- |
| Project    | Decoded project name.           |
| Session    | 8-character session ID prefix.  |
| Date       | First timestamp.                |
| Model      | Full model tag for the session. |
| Input      | Input tokens for the session.   |
| Output     | Output tokens for the session.  |
| Cost (USD) | Cost for the session.           |

## JSON shape

### Aggregated (default)

```json
[
  {
    "project": "/Users/you/projects/claudex",
    "sessions": 123,
    "input_tokens": 326297,
    "output_tokens": 6679149,
    "cache_creation_tokens": 80583078,
    "cache_read_tokens": 7157509259,
    "models": ["Opus", "Sonnet"],
    "cost_usd": 12735.6563118
  }
]
```

Note: `models` is an **array of model-family names** (Opus / Sonnet / Haiku)
for any model that contributed to the project's cost. Sorted by `cost_usd`
descending.

### Per-session (`--per-session`)

```json
[
  {
    "project": "/Users/you/projects/claudex",
    "session_id": "f69d4985-f914-4968-81c0-009ea004fbc5",
    "date": "2026-04-01T16:36:41.451+00:00",
    "model": "claude-opus-4-6",
    "input_tokens": 34960,
    "output_tokens": 483151,
    "cache_creation_tokens": 4027298,
    "cache_read_tokens": 893758965,
    "cost_usd": 1452.91101
  }
]
```

## Notes

- **Per-message pricing.** Each message is priced by _its own_ model. A
  session that mixes Opus and Sonnet messages is priced correctly.
- **Cache reads dominate long sessions.** Don't be surprised to see huge
  `cache_read_tokens` — prompt caching means the same context is read from
  cache repeatedly.
- **Sub-cent values.** Costs below one cent render with four decimals so they
  don't round to `$0.00`.
- **See also:** [Pricing model](/reference/pricing), [`models`](/commands/models).

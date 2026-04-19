# `cost`

Token usage and approximate cost, aggregated per project or per session.

## Usage

```bash
claudex cost [-p/--project <substr>] [--per-session]
             [-l/--limit <n>] [--json] [--no-index]
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-p`, `--project <substr>` | — | Filter by substring match on the project path. |
| `--per-session` | off | Break down per session instead of aggregating by project. |
| `-l`, `--limit <n>` | `20` | Maximum number of rows. |
| `--json` | off | Emit JSON. |
| `--no-index` | off | Scan JSONL files directly. |

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

| Column | Source |
|--------|--------|
| Project | Decoded project name. |
| Sessions | Number of sessions counted. |
| Input | Total input tokens. |
| Output | Total output tokens. |
| Cache W | Cache-creation tokens. |
| Cache R | Cache-read tokens. |
| Cost (USD) | Sum of per-message costs. |

## Columns (per-session)

| Column | Source |
|--------|--------|
| Project | Decoded project name. |
| Session | 8-character session ID prefix. |
| Date | First timestamp. |
| Model | Model tag (Opus / Sonnet / Haiku). |
| Input | Input tokens for the session. |
| Output | Output tokens for the session. |
| Cost (USD) | Cost for the session. |

## JSON shape

```json
[
  {
    "project": "claudex",
    "session_id": "e1a2f4...",
    "date": "2026-04-18T14:22:13+00:00",
    "model": "claude-sonnet-4-6",
    "input_tokens": 4120,
    "output_tokens": 18330,
    "cache_creation_tokens": 2048,
    "cache_read_tokens": 94210,
    "cost_usd": 0.441
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

# `turns`

Per-turn timing analysis: average, median (p50), 95th percentile, and max
duration per project.

## Usage

```bash
claudex turns [-p/--project <substr>] [-l/--limit <n>] [--json]
```

## Flags

| Flag                       | Default | Description                                    |
| -------------------------- | ------- | ---------------------------------------------- |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path. |
| `-l`, `--limit <n>`        | `20`    | Maximum projects to show.                      |
| `--json`                   | off     | Emit JSON.                                     |

## Example

```bash
# Global p95 leaderboard
claudex turns --limit 10

# One project's timing profile
claudex turns --project claudex

# p95 as a single number
claudex turns --project claudex --json | jq '.[0].p95_ms'
```

## Columns

| Column  | Source                         |
| ------- | ------------------------------ |
| Project | Decoded project name.          |
| Turns   | Number of turns measured.      |
| Avg     | Arithmetic mean duration (ms). |
| p50     | Median duration (ms).          |
| p95     | 95th-percentile duration (ms). |
| Max     | Longest single turn (ms).      |

## JSON shape

```json
[
  {
    "project": "claudex",
    "turns": 1420,
    "avg_ms": 4132,
    "p50_ms": 2811,
    "p95_ms": 17420,
    "max_ms": 98233
  }
]
```

## What counts as a "turn"?

A turn is the wall-clock interval between one user message and the next
assistant message in the same session. Long tool-calling round-trips within
one turn add to that turn's duration.

## Notes

- **No `--no-index`.** Turn durations are derived during ingest; the file-scan
  fallback would re-parse every session just to compute them. Run
  `claudex index` if you want a fresh measurement.
- **Outliers.** A single 10-minute thinking turn will spike the `max` but
  barely move `p50`. Report both.

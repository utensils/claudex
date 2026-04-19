# Recipes

Copy-paste one-liners for common questions. Every example here was run
against the shipped binary and the output shape shown is what you'll see.

Two conventions in use throughout:

- **Human-readable output** on the page — dollars are rounded to two
  decimals, turn durations are rendered in seconds, and SQL queries use
  `sqlite3 .mode box` for a real table.
- **Pipe-friendly raw values** are called out where relevant — they're
  single-number jq queries you can embed in scripts without post-processing.

## Spend

### Grand total (formatted)

```bash
claudex summary --json \
  | jq '.total_cost_usd' \
  | awk '{printf "$%\047.2f\n", $1}'
```

```
$33,187.40
```

(Raw number for piping: `claudex summary --json | jq '.total_cost_usd'`.)

### Cost this week

```bash
claudex summary --json \
  | jq '.cost_this_week_usd' \
  | awk '{printf "$%\047.2f\n", $1}'
```

```
$7,847.78
```

### Top 10 projects by cost

```bash
claudex cost --limit 10
```

The text table already formats dollars and token counts — no jq needed.

### Which Opus variant cost the most?

```bash
claudex models --json \
  | jq '[.[] | select(.model_family == "Opus")]
        | sort_by(-.cost_usd)[0]
        | {model, sessions: .session_count, cost_usd: (.cost_usd * 100 | round / 100)}'
```

```json
{
  "model": "claude-opus-4-6",
  "sessions": 467,
  "cost_usd": 30051.13
}
```

## Sessions

### Most recent five

```bash
claudex sessions --limit 5
```

### All sessions in one project (JSON)

```bash
claudex sessions --project claudex --json --limit 100
```

### Sessions that touched a specific file

```bash
claudex sessions --file src/index.rs --limit 10
```

## Search

### Where did I first discuss a topic

```bash
claudex search "foreign key" --limit 1
```

### Case-sensitive search in one project

```bash
claudex search CamelCaseThing --project my-app --case-sensitive
```

## Turns

The index stores turn durations in **milliseconds**. The recipes below
convert to seconds so the output is legible at a glance.

### One-line timing profile for a project

```bash
claudex turns --project claudex --json \
  | jq -r '.[0] | "p50: \(.p50_duration_ms/1000 | round)s   p95: \(.p95_duration_ms/1000 | round)s   max: \(.max_duration_ms/1000 | round)s   (n=\(.turn_count))"'
```

```
p50: 66s   p95: 572s   max: 657s   (n=26)
```

### Projects with the slowest average turn

```bash
claudex turns --json \
  | jq -r 'sort_by(-.avg_duration_ms)[:5][]
           | "\(.avg_duration_ms/1000 | round | tostring + "s") \t \(.project)"'
```

```
2838s    /Users/you/projects/comfyui-nix
373s     /Users/you/projects/quantierra-ui
324s     /Users/you/projects/mold
321s     /Users/you/projects/site
316s     /Users/you/projects/nixos
```

## Files

### Top 10 most-edited files

```bash
claudex files --limit 10
```

### Files with 20+ edit events

```bash
claudex files --json | jq -r '.[] | select(.modification_count >= 20) | .file_path'
```

### Files touched by the most distinct sessions

`claudex files --json` includes `distinct_session_count`, so you no longer
need raw SQL for this:

```bash
claudex files --json \
  | jq -r '.[] | select(.distinct_session_count >= 5) | [.file_path, .distinct_session_count] | @tsv'
```

## Session drill-down

### Inspect one session

```bash
claudex session <session-id-prefix>
```

### Pull just the files, tools, and stop reasons

```bash
claudex session <session-id-prefix> --json \
  | jq '{files: .files_modified, tools: .tools, stop_reasons: .stop_reasons}'
```

## Export

### Export every session in a project to a directory

```bash
mkdir -p exports
claudex sessions --project claudex --json --limit 1000 \
  | jq -r '.[].session_id' \
  | while read sid; do
      claudex export "$sid" --output "exports/$sid.md"
    done
```

### Export a session as JSON and count messages per role

```bash
claudex export <session-id-prefix> --format json \
  | jq '[.messages[].message.role] | group_by(.) | map({role: .[0], count: length})'
```

```json
[
  { "role": "assistant", "count": 9 },
  { "role": "user", "count": 5 }
]
```

## PRs

`claudex prs` already deduplicates by `pr_url` — you get one row per unique
PR, with the most recent mention's timestamp.

### PRs this month

```bash
claudex prs --json \
  | jq -r --arg m "$(date +%Y-%m)" \
       '.[] | select(.timestamp | startswith($m))
              | "#\(.pr_number)  \(.pr_repository)  \(.pr_url)"'
```

```
#14  utensils/claudex  https://github.com/utensils/claudex/pull/14
#13  utensils/claudex  https://github.com/utensils/claudex/pull/13
#12  utensils/claudex  https://github.com/utensils/claudex/pull/12
```

### PR count per project

```bash
claudex prs --json \
  | jq 'group_by(.project)
        | map({project: .[0].project, prs: length})
        | sort_by(-.prs)'
```

## Summary

### Markdown block for standup notes

```bash
COST=$(claudex summary --json | jq '.cost_this_week_usd' \
         | awk '{printf "%\047.2f", $1}')
claudex summary --json | jq -r --arg cost "$COST" '
  "## Claude Code this week\n\n" +
  "- Sessions: \(.sessions_this_week)\n" +
  "- Cost:     $\($cost)\n" +
  "- Tokens:   \(.total_tokens / 1e9 * 100 | round / 100)B\n"'
```

```
## Claude Code this week

- Sessions: 286
- Cost:     $7,847.78
- Tokens:   18.01B
```

Paste straight into your standup notes.

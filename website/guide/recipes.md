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
sqlite3 ~/.claudex/index.db <<'SQL'
.mode box
SELECT s.project_name,
       substr(s.session_id, 1, 8)                              AS sid,
       datetime(s.first_timestamp/1000, 'unixepoch')           AS started
FROM   sessions s
JOIN   file_modifications fm ON fm.session_rowid = s.id
WHERE  fm.file_path LIKE '%src/index.rs%'
GROUP BY s.id
ORDER BY s.first_timestamp DESC
LIMIT 10;
SQL
```

```
┌───────────────────────────────────────────┬──────────┬─────────────────────┐
│               project_name                │   sid    │       started       │
├───────────────────────────────────────────┼──────────┼─────────────────────┤
│ /Users/you/projects/claudex               │ 3b50e273 │ 2026-04-19 00:47:36 │
│ /Users/you/projects/claudex               │ 0e119824 │ 2026-04-19 00:40:30 │
└───────────────────────────────────────────┴──────────┴─────────────────────┘
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

`claudex files` counts edit _events_, not distinct sessions. For "edited
across N sessions" use the index directly:

```bash
sqlite3 ~/.claudex/index.db <<'SQL'
.mode box
SELECT fm.file_path,
       COUNT(DISTINCT fm.session_rowid) AS sessions
FROM   file_modifications fm
GROUP BY fm.file_path
HAVING sessions >= 5
ORDER BY sessions DESC
LIMIT 20;
SQL
```

```
┌─────────────────────────────────────────┬──────────┐
│                file_path                │ sessions │
├─────────────────────────────────────────┼──────────┤
│ CLAUDE.md                               │ 60       │
│ README.md                               │ 33       │
│ flake.nix                               │ 33       │
│ CHANGELOG.md                            │ 31       │
│ .gitignore                              │ 22       │
│ app/views/dealflow/emails/show.html.erb │ 19       │
└─────────────────────────────────────────┴──────────┘
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

### Unique PRs this month

A single PR can be referenced by many sessions; dedupe by URL:

```bash
claudex prs --json \
  | jq -r --arg m "$(date +%Y-%m)" '
      map(select(.timestamp | startswith($m)))
      | unique_by(.pr_url)
      | .[] | "#\(.pr_number)  \(.pr_repository)  \(.pr_url)"'
```

```
#12  utensils/claudex  https://github.com/utensils/claudex/pull/12
#13  utensils/claudex  https://github.com/utensils/claudex/pull/13
```

### PR count per project

```bash
claudex prs --json \
  | jq 'group_by(.project)
        | map({project: .[0].project, prs: (map(.pr_url) | unique | length)})
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

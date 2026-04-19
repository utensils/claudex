# Recipes

One-liners for common questions. Copy, paste, adapt.

## Spend

### Total cost across all sessions

```bash
claudex cost --json --limit 1000 | jq '[.[].cost_usd] | add'
```

### Cost this week, per project

```bash
claudex summary --json | jq '.cost_this_week_usd'
```

### Which project burned the most Opus tokens?

```bash
claudex models --json \
  | jq '[.[] | select(.model | test("opus"; "i"))] | sort_by(-.cost_usd)[0]'
```

## Sessions

### Most recent five sessions

```bash
claudex sessions --limit 5
```

### All sessions in one project

```bash
claudex sessions --project claudex --limit 100
```

### Sessions that touched a specific file

```bash
sqlite3 ~/.claudex/index.db <<'SQL'
  SELECT s.project_name, s.session_id
  FROM   sessions s
  JOIN   file_modifications fm ON fm.session_rowid = s.id
  WHERE  fm.file_path LIKE '%src/index.rs%'
  GROUP BY s.id
  ORDER BY s.first_timestamp DESC
  LIMIT 10;
SQL
```

(The JSON output covers almost everything — falling back to SQL is fine for
one-offs.)

## Search

### Find where you first discussed a topic

```bash
claudex search "foreign key" --limit 1
```

### Case-sensitive search in one project

```bash
claudex search CamelCaseThing --project my-app --case-sensitive
```

## Turns

### p95 turn duration for a project

```bash
claudex turns --project claudex --json | jq '.[0].p95_ms'
```

### Projects with the slowest average turn

```bash
claudex turns --json | jq 'sort_by(-.avg_ms)[:5] | .[] | {project, avg_ms}'
```

## Files

### Top 10 most-edited files

```bash
claudex files --limit 10
```

### Files you've edited across 5+ sessions

```bash
claudex files --json \
  | jq '.[] | select(.session_count >= 5) | .file_path'
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

### Export a session as JSON and pipe elsewhere

```bash
claudex export e1a2f4 --format json | your-tool
```

## PRs

### Every session with a linked PR, this month

```bash
claudex prs --json \
  | jq --arg m "$(date +%Y-%m)" '.[] | select(.date | startswith($m))'
```

## Summary, formatted

### Markdown-ish summary block

```bash
claudex summary --json | jq -r '
  "## Claude Code this week\n\n" +
  "- Sessions: \(.sessions_this_week)\n" +
  "- Cost:     $\(.cost_this_week_usd)\n" +
  "- Tokens:   \(.total_tokens)\n"
'
```

Paste the output into your standup notes.

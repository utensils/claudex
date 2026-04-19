# Recipes

One-liners for common questions. Every example here was run against the
shipped binary before publishing — if one stops working after an upgrade,
open an issue.

## Spend

### Total cost across all sessions

```bash
claudex summary --json | jq '.total_cost_usd'
```

### Cost this week

```bash
claudex summary --json | jq '.cost_this_week_usd'
```

### Cost by project (top 10)

```bash
claudex cost --limit 10
```

### Which model family burned the most?

```bash
claudex models --json \
  | jq '[.[] | select(.model_family == "Opus")] | sort_by(-.cost_usd)[0]'
```

## Sessions

### Most recent five sessions

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
  SELECT s.project_name, s.session_id
  FROM   sessions s
  JOIN   file_modifications fm ON fm.session_rowid = s.id
  WHERE  fm.file_path LIKE '%src/index.rs%'
  GROUP BY s.id
  ORDER BY s.first_timestamp DESC
  LIMIT 10;
SQL
```

The JSON output from `claudex` covers most questions; falling back to SQL on
the index is fine for ad-hoc shapes.

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
claudex turns --project claudex --json | jq '.[0].p95_duration_ms'
```

### Projects with the slowest average turn

```bash
claudex turns --json \
  | jq 'sort_by(-.avg_duration_ms)[:5] | .[] | {project, avg_duration_ms}'
```

## Files

### Top 10 most-edited files

```bash
claudex files --limit 10
```

### Files with 20+ edit events

```bash
claudex files --json \
  | jq '.[] | select(.modification_count >= 20) | .file_path'
```

### Files touched by the most distinct sessions

`claudex files` counts edit _events_, not distinct sessions. For the
"edited across N sessions" question, query the index directly:

```bash
sqlite3 ~/.claudex/index.db <<'SQL'
  SELECT fm.file_path, COUNT(DISTINCT fm.session_rowid) AS sessions
  FROM   file_modifications fm
  GROUP BY fm.file_path
  HAVING sessions >= 5
  ORDER BY sessions DESC
  LIMIT 20;
SQL
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

### Export a session as JSON

```bash
claudex export e1a2f4 --format json > session.json
jq '.message_count' session.json
```

## PRs

### Every PR-linked session this month

```bash
claudex prs --json \
  | jq --arg m "$(date +%Y-%m)" \
       '.[] | select(.timestamp | startswith($m)) | {project, pr_number, pr_url}'
```

### Count PRs per project

```bash
claudex prs --json \
  | jq 'group_by(.project) | map({project: .[0].project, prs: length})'
```

## Summary

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

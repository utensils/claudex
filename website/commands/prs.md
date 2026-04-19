# `prs`

Sessions linked to pull requests — either ones you opened during the session
or PR URLs that appeared in the conversation.

## Usage

```bash
claudex prs [-p/--project <substr>] [-l/--limit <n>] [--json]
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-p`, `--project <substr>` | — | Filter by substring match on the project path. |
| `-l`, `--limit <n>` | `20` | Maximum rows. |
| `--json` | off | Emit JSON. |

## Example

```bash
# Every PR-linked session, most recent first
claudex prs --limit 20

# Just one repo
claudex prs --project claudex

# Count PRs per project
claudex prs --json | jq 'group_by(.project) | map({project: .[0].project, count: length})'
```

## Columns

| Column | Source |
|--------|--------|
| Project | Decoded project name. |
| Session | 8-character session ID prefix. |
| Date | First timestamp of the session. |
| PR | PR number. |
| Repo | `owner/repo`. |
| URL | Full PR URL. |

## JSON shape

```json
[
  {
    "project": "claudex",
    "session_id": "e1a2f4...",
    "first_timestamp": 1744000000000,
    "date": "2026-04-18T14:22:13+00:00",
    "pr_number": 11,
    "pr_repository": "utensils/claudex",
    "pr_url": "https://github.com/utensils/claudex/pull/11"
  }
]
```

## Notes

- **Detection.** PR URLs are pulled out of message text during ingest
  (`pr_links` table). If you open a PR via `gh pr create` in a Bash tool call,
  the URL shows up in the tool output and gets captured.
- **One session → many PRs.** A session that opens multiple PRs produces
  multiple rows.
- **No `--no-index`.** PR links are only stored in the index; the file-scan
  path would need to re-parse every session's text.

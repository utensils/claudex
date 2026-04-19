# `prs`

Sessions linked to pull requests — either ones you opened during the session
or PR URLs that appeared in the conversation.

## Usage

```bash
claudex prs [-p/--project <substr>] [-l/--limit <n>] [--json]
```

## Flags

| Flag                       | Default | Description                                    |
| -------------------------- | ------- | ---------------------------------------------- |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path. |
| `-l`, `--limit <n>`        | `20`    | Maximum rows.                                  |
| `--json`                   | off     | Emit JSON.                                     |

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

| Column    | Source                            |
| --------- | --------------------------------- |
| Project   | Decoded project name.             |
| Session   | 8-character session ID prefix.    |
| Timestamp | When the PR reference was logged. |
| PR        | PR number.                        |
| Repo      | `owner/repo`.                     |
| URL       | Full PR URL.                      |

## JSON shape

```json
[
  {
    "project": "/Users/you/projects/claudex",
    "session_id": "7671fcc3-0a7d-49bf-9996-e6c98129f005",
    "timestamp": "2026-04-19T02:58:27.597Z",
    "pr_number": 13,
    "pr_repository": "utensils/claudex",
    "pr_url": "https://github.com/utensils/claudex/pull/13"
  }
]
```

Note: `timestamp` is the moment the PR URL appeared in the conversation —
not the session's first message. Format is ISO-8601 with millisecond
precision.

## Deduplication

Output is **one row per unique `pr_url`**. A single PR commonly gets
referenced from many sessions (the original session that opened it, plus
any later ones that mentioned the URL); rather than emit one row per
session, `prs` picks the row with the most recent `timestamp` per URL and
shows that. `session_id`, `project`, and `timestamp` describe the most
recent mention.

## Notes

- **Detection.** PR URLs are pulled out of message text during ingest
  (`pr_links` table). If you open a PR via `gh pr create` in a Bash tool call,
  the URL shows up in the tool output and gets captured.
- **One session → many PRs.** A session that opens multiple PRs produces
  multiple rows.
- **No `--no-index`.** PR links are only stored in the index; the file-scan
  path would need to re-parse every session's text.

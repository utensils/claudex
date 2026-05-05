# codex

Summarize OpenAI Codex CLI activity from `~/.codex`.

```bash
claudex codex
claudex codex --json
```

`claudex codex` scans Codex rollout JSONL files under:

- `~/.codex/sessions/**/rollout-*.jsonl`
- `~/.codex/archived_sessions/rollout-*.jsonl`

It also reads `~/.codex/session_index.jsonl` for session titles when present and
queries `~/.codex/state_5.sqlite` for thread/token totals when the state DB is
available.

## Reports

The text report includes:

- total, today, and this-week sessions
- active vs archived session-file counts
- user and agent message counts
- reasoning item counts
- tool call/result counts
- aborted turns, context compactions, and review events
- top projects by session count
- top tools by call count
- Codex CLI versions
- most recent session
- optional state DB totals: thread count, user-event thread count, tokens used

## JSON

```bash
claudex codex --json
```

Shape:

```json
{
  "total_sessions": 12,
  "archived_sessions": 3,
  "active_session_files": 9,
  "sessions_today": 2,
  "sessions_this_week": 5,
  "user_messages": 40,
  "agent_messages": 38,
  "reasoning_items": 120,
  "tool_calls": 91,
  "tool_results": 91,
  "aborted_turns": 1,
  "compacted_events": 2,
  "review_events": 0,
  "top_projects": [{ "name": "/repo", "count": 4 }],
  "top_tools": [{ "name": "shell", "count": 50 }],
  "cli_versions": [{ "name": "0.99.0", "count": 8 }],
  "originators": [{ "name": "codex_cli_rs", "count": 12 }],
  "sources": [{ "name": "cli", "count": 12 }],
  "most_recent": {
    "session_id": "019...",
    "title": "Fix parser",
    "project": "/repo",
    "date": "2026-05-05T00:00:00+00:00",
    "cli_version": "0.99.0",
    "source": "cli"
  },
  "state": {
    "thread_count": 12,
    "threads_with_user_event": 12,
    "total_tokens_used": 123456,
    "top_projects": [{ "name": "/repo", "count": 4 }],
    "top_models": [{ "name": "openai", "count": 12 }]
  }
}
```

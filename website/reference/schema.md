# Index schema

The SQLite index at `~/.claudex/index.db`. **Schema version: 3.**

::: warning Not a stable surface
Table and column names may change between releases. Use `claudex <cmd> --json`
for automation. This page is for curiosity and debugging.
:::

## `meta`

Single-row key/value scratchpad.

| Column  | Type    | Notes                               |
| ------- | ------- | ----------------------------------- |
| `key`   | TEXT PK | e.g. `schema_version`, `last_sync`. |
| `value` | TEXT    | Stringified value.                  |

`schema_version` is the source of truth for rebuild-on-mismatch logic.

## `sessions`

One row per JSONL file.

| Column            | Type        | Notes                                                          |
| ----------------- | ----------- | -------------------------------------------------------------- |
| `id`              | INTEGER PK  | Surrogate key.                                                 |
| `project_name`    | TEXT        | Decoded project name, with `(worktree)` for worktree sessions. |
| `file_path`       | TEXT UNIQUE | Absolute path to the JSONL.                                    |
| `file_size`       | INTEGER     | Bytes. Part of the incremental-sync key.                       |
| `file_mtime`      | INTEGER     | Unix seconds. Part of the sync key.                            |
| `session_id`      | TEXT        | Session UUID from Claude Code.                                 |
| `first_timestamp` | INTEGER     | Unix ms.                                                       |
| `last_timestamp`  | INTEGER     | Unix ms.                                                       |
| `duration_ms`     | INTEGER     | Last minus first.                                              |
| `message_count`   | INTEGER     | User + assistant.                                              |
| `model`           | TEXT        | Sole model tag, or `mixed` when a session switched models.     |
| `indexed_at`      | INTEGER     | Unix seconds.                                                  |

Indexes: `idx_sessions_project`, `idx_sessions_timestamp`.

## `token_usage`

One row per `(session, model)` pair. A session that switched models has
multiple rows.

| Column                                                                        | Notes                                       |
| ----------------------------------------------------------------------------- | ------------------------------------------- |
| `session_id`                                                                  | FK → `sessions.id` (ON DELETE CASCADE).     |
| `model`                                                                       | Model tag.                                  |
| `assistant_message_count`                                                     | Assistant messages contributing to the row. |
| `input_tokens`, `output_tokens`, `cache_creation_tokens`, `cache_read_tokens` | Four counters.                              |
| `cost_usd`                                                                    | Pre-computed cost for this row.             |
| `inference_geo`                                                               | Region, if reported.                        |
| `speed`                                                                       | Tokens/sec, if reported.                    |
| `service_tier`                                                                | `standard`, `priority`, etc.                |
| `iterations`                                                                  | Count of messages contributing to the row.  |

Index: `idx_token_usage_session`.

## `tool_calls`

One row per `(session, tool_name)` pair.

| Column       | Notes                                                     |
| ------------ | --------------------------------------------------------- |
| `session_id` | FK.                                                       |
| `tool_name`  | Tool name as reported (e.g. `Edit`, `Bash`, `mcp__*__*`). |
| `count`      | Invocations.                                              |

Index: `idx_tool_calls_session`.

## `turn_durations`

One row per turn.

| Column          | Notes                                            |
| --------------- | ------------------------------------------------ |
| `session_rowid` | FK.                                              |
| `turn_number`   | 1-based.                                         |
| `duration_ms`   | Wall-clock from user message to assistant reply. |
| `timestamp`     | ISO-8601 string of the user message.             |

Index: `idx_turn_durations_session`.

## `pr_links`

One row per PR URL detected in the session.

| Column          | Notes                  |
| --------------- | ---------------------- |
| `session_rowid` | FK.                    |
| `pr_number`     | Parsed from URL.       |
| `pr_url`        | Full URL.              |
| `pr_repository` | `owner/repo`.          |
| `timestamp`     | When the URL appeared. |

## `file_modifications`

One row per file edit event.

| Column               | Notes                                           |
| -------------------- | ----------------------------------------------- |
| `session_rowid`      | FK.                                             |
| `file_path`          | As recorded.                                    |
| `is_snapshot_update` | 1 for standard Edit/Write; 0 for special cases. |

Indexes: `idx_file_mods_session`, `idx_file_mods_path`.

## `thinking_usage`

| Column            | Notes                              |
| ----------------- | ---------------------------------- |
| `session_rowid`   | FK.                                |
| `thinking_blocks` | Count of extended-thinking blocks. |
| `thinking_tokens` | Token count, if available.         |

## `stop_reasons`

| Column          | Notes                                      |
| --------------- | ------------------------------------------ |
| `session_rowid` | FK.                                        |
| `stop_reason`   | `end_turn`, `tool_use`, `max_tokens`, etc. |
| `count`         | Number of messages with this reason.       |

## `attachments`

| Column          | Notes        |
| --------------- | ------------ |
| `session_rowid` | FK.          |
| `filename`      | As attached. |
| `mime_type`     | If recorded. |

## `permission_changes`

| Column          | Notes                          |
| --------------- | ------------------------------ |
| `session_rowid` | FK.                            |
| `mode`          | Permission mode switched into. |
| `timestamp`     | ISO-8601.                      |

## `messages_fts` (virtual)

FTS5 virtual table over every user + assistant message.

```sql
CREATE VIRTUAL TABLE messages_fts USING fts5(
    session_id   UNINDEXED,
    message_type,
    content,
    timestamp    UNINDEXED,
    tokenize     = 'porter unicode61'
);
```

Used by [`search`](/commands/search). The `porter` stemmer means `migrat`
matches `migration`, `migrated`, `migrates`.

## Migration strategy

Schema changes follow one rule: bump `SCHEMA_VERSION`. A version mismatch on
open triggers a full rebuild. Additive changes (new column defaulting to 0,
new table) go inside the same `CREATE TABLE IF NOT EXISTS` block; destructive
changes still need a version bump.

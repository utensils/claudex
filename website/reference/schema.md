# Index schema

The SQLite index at `~/.claudex/index.db`. **Schema version: 6.**

::: warning Not a stable surface
Table and column names may change between releases. Use `claudex <cmd> --json`
for automation. This page is for curiosity and debugging.
:::

## `meta`

Single-row key/value scratchpad.

| Column  | Type    | Notes                                          |
| ------- | ------- | ---------------------------------------------- |
| `key`   | TEXT PK | e.g. `schema_version`, `last_sync:<provider>`. |
| `value` | TEXT    | Stringified value.                             |

`schema_version` drives the forward-only migration ladder; `last_sync:<id>` and
`sessions_root:<id>` track each provider's incremental sync independently;
`pricing_revision` records the rate-card version this index was last priced at
(drives the automatic one-off reprice — see [Pricing](/reference/pricing#repricing-existing-data)).

## `sessions`

One row per transcript file, across every provider.

| Column              | Type        | Notes                                                              |
| ------------------- | ----------- | ------------------------------------------------------------------ |
| `id`                | INTEGER PK  | Surrogate key.                                                     |
| `project_name`      | TEXT        | Decoded project name, with `(worktree)` for worktree sessions.     |
| `file_path`         | TEXT UNIQUE | Absolute path to the transcript.                                   |
| `file_size`         | INTEGER     | Bytes. Part of the incremental-sync key.                           |
| `file_mtime`        | INTEGER     | Unix seconds. Part of the sync key.                                |
| `session_id`        | TEXT        | Session id from the provider.                                      |
| `parent_session_id` | TEXT        | Set for Claude subagent transcripts (roll up to parent).           |
| `first_timestamp`   | INTEGER     | Unix ms.                                                           |
| `last_timestamp`    | INTEGER     | Unix ms.                                                           |
| `duration_ms`       | INTEGER     | Last minus first.                                                  |
| `message_count`     | INTEGER     | User + assistant.                                                  |
| `model`             | TEXT        | Sole model tag, or `mixed` when a session switched models.         |
| `indexed_at`        | INTEGER     | Unix seconds.                                                      |
| `provider`          | TEXT        | `claude` / `codex` / `pi`.                                         |
| `present_on_disk`   | INTEGER     | `1` if the source file still exists, `0` if retained-after-delete. |
| `archived_at`       | INTEGER     | Unix seconds when the file was archived/removed (NULL if live).    |
| `last_seen`         | INTEGER     | Unix seconds of the last sync that observed the file.              |
| `extras`            | TEXT        | Provider-specific metadata as a JSON object (cli_version, git, …). |

Indexes: `idx_sessions_project`, `idx_sessions_timestamp`, `idx_sessions_parent`,
`idx_sessions_provider`, `idx_sessions_present`.

## `token_usage`

One row per `(session, model)` pair. A session that switched models has
multiple rows.

| Column                                                                        | Notes                                                                                                                              |
| ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `session_id`                                                                  | FK → `sessions.id` (ON DELETE CASCADE).                                                                                            |
| `model`                                                                       | Model tag.                                                                                                                         |
| `assistant_message_count`                                                     | Assistant messages contributing to the row.                                                                                        |
| `input_tokens`, `output_tokens`, `cache_creation_tokens`, `cache_read_tokens` | Four counters.                                                                                                                     |
| `cost_usd`                                                                    | Pre-computed cost for this row.                                                                                                    |
| `cost_source`                                                                 | `computed` (priced from the tier table) or `provider` (a provider-reported cost, e.g. Pi). Repricing only touches `computed` rows. |
| `inference_geo`                                                               | Distinct reported regions for the row, joined with ASCII Unit Separator (`\u001f`) in the raw DB value.                            |
| `speed`                                                                       | Average tokens/sec for the session-model row, if reported.                                                                         |
| `service_tier`                                                                | Distinct reported service tiers for the row, joined with ASCII Unit Separator (`\u001f`) in the raw DB value.                      |
| `iterations`                                                                  | Count of messages contributing to the row.                                                                                         |

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

Migrations are **forward-only and non-destructive**. Bumping `SCHEMA_VERSION`
runs a migration ladder of guarded `ALTER TABLE ADD COLUMN` steps — it never
drops tables, because the index retains sessions that have left disk and those
rows can't be rebuilt. Add a column to the `CREATE TABLE IF NOT EXISTS` block
**and** an additive migration step, then bump the version. The only destructive
path is `claudex index --force`.

Cost values are versioned separately by `pricing_revision`: when the rate card
(`ModelPricing::for_model`) changes, that constant is bumped and the next open
reprices every `cost_source = 'computed'` row in place — non-destructively, so
retained/archived rows are corrected too. See
[Pricing → Repricing](/reference/pricing#repricing-existing-data).

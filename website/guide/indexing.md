# The index

Claudex keeps a SQLite index at `~/.claudex/index.db`. You don't manage it
directly — it's an implementation detail — but a few things are worth knowing.

## Location

```
~/.claudex/
├── index.db          # SQLite, schema_version = 2
└── debug/            # only when you use `claudex watch`
    └── latest.log
```

Override with the `CLAUDEX_DIR` environment variable. The directory is created
on first use.

## Sync semantics

| Trigger | What happens |
|---------|--------------|
| First run of any read command | Full scan of `~/.claude/projects/`, populate every table. |
| Subsequent runs, last sync ≤ 5 min | Use the cached index as-is. |
| Subsequent runs, last sync > 5 min | Incremental sync — add new sessions, update changed ones. |
| `claudex index` | Force a sync right now (incremental). |
| `claudex index --force` | Wipe the database and rebuild from scratch. |

The staleness window is `STALE_SECS = 300` in `src/index.rs`.

## How incremental sync works

Each session file is keyed on the tuple `(file_path, file_size, file_mtime)`.
If any of those change, the file is re-ingested. That's why sync is fast —
claudex never re-parses a file whose bytes haven't changed.

Deletes are detected too: if a file in the index no longer exists on disk, its
rows are removed.

## When to force a rebuild

- You bumped `SCHEMA_VERSION` (during development). A version mismatch triggers
  an automatic rebuild on next open, so you rarely need `--force` for this.
- You suspect the index is corrupted (interrupted write, disk full mid-sync).
- You moved `~/.claude/projects/` around and want to start clean.

```bash
claudex index --force
```

## Bypassing the index

Every read command accepts `--no-index` to skip the index and scan JSONL files
directly. This is the authoritative path — the index is a cache built from it.

Use `--no-index` when:

- You're debugging a discrepancy between indexed and raw output (and you want
  to file a bug).
- You want a one-off read with no database mutation.
- You're running on a system where `~/.claudex/` is read-only.

```bash
claudex cost --no-index
claudex sessions --no-index --limit 5
claudex search "TODO" --no-index
```

## What's inside the index?

See [Index schema](/reference/schema) for the table-by-table breakdown. In
short: `sessions`, `token_usage`, `tool_calls`, `turn_durations`, `pr_links`,
`file_modifications`, `thinking_usage`, `stop_reasons`, `attachments`,
`permission_changes`, plus an FTS5 virtual table `messages_fts`.

## Can I query it myself?

Yes. It's a normal SQLite database:

```bash
sqlite3 ~/.claudex/index.db

sqlite> .tables
sqlite> SELECT project_name, COUNT(*) FROM sessions GROUP BY 1 ORDER BY 2 DESC;
```

Just be aware that:

- Schema changes with each `SCHEMA_VERSION` bump. Don't build long-lived
  tooling on the schema — use `claudex <cmd> --json` instead, which is the
  stable contract.
- Writing to the database outside of claudex is unsupported. Claudex assumes
  it's the only writer.

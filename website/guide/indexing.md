# The index

Claudex keeps a SQLite index at `~/.claudex/index.db`. You don't manage it
directly — it's an implementation detail — but a few things are worth knowing.

## Location

```
~/.claudex/
├── index.db          # SQLite, schema_version = 7
└── debug/            # only when you use `claudex watch`
    └── latest.log
```

Override with the `CLAUDEX_DIR` environment variable. The directory is created
on first use.

## Sync semantics

The index covers all four providers. Each is synced independently with its own
staleness window and data-root stamp.

| Trigger                            | What happens                                                |
| ---------------------------------- | ----------------------------------------------------------- |
| First run of any read command      | Full scan of every provider's transcripts, populate tables. |
| Subsequent runs, last sync ≤ 5 min | Use the cached index as-is.                                 |
| Subsequent runs, last sync > 5 min | Incremental per-provider sync — add new, update changed.    |
| `claudex index`                    | Force a sync of every provider right now (incremental).     |
| `claudex index --force`            | Wipe the database and rebuild from scratch.                 |

The staleness window is `STALE_SECS = 300` in `crates/claudex/src/index.rs`.

## How incremental sync works

Each session file is keyed on the tuple `(file_path, file_size, file_mtime)`.
If any of those change, the file is re-ingested in place. That's why sync is
fast — claudex never re-parses a file whose bytes haven't changed.

OpenClaw also records a provider-scoped `source_key`, so a runtime trajectory
captured before its canonical transcript appears can be replaced in place later
instead of double-counting the same session.

## Retention: the index is additive

When a session file disappears from disk (you archive or delete it), its rows
are **not** removed. They're flagged as no-longer-on-disk (`present_on_disk = 0`,
`archived_at` stamped) and retained — so cost, search, and history still include
them. Use `--on-disk-only` on a report to scope to live files. The only command
that discards retained data is `claudex index --force`.

## When to force a rebuild

- You suspect the index is corrupted (interrupted write, disk full mid-sync).
- You want to drop retained/archived data and start clean from what's on disk.

A `SCHEMA_VERSION` bump no longer triggers a rebuild — migrations are
additive and non-destructive, so upgrades preserve your retained history.

```bash
claudex index --force
```

## Bypassing the index

Most Claude Code read commands accept `--no-index` to skip the index and scan
Claude JSONL files directly. It's a Claude-only escape hatch — Codex, Pi, and OpenClaw are
served from the index. (Note the index is no longer just a cache: it retains
sessions that have left disk, so `--no-index` can show _fewer_ rows than the
indexed path.)

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

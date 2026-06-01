# `index`

Manage the SQLite index at `~/.claudex/index.db`. Normally you don't run this
— read commands keep the index fresh automatically — but it's here when you
want to.

## Usage

```bash
claudex index [--force] [--status] [--prune-retained-days N] [--vacuum]
```

## Flags

| Flag                        | Description                                                                         |
| --------------------------- | ----------------------------------------------------------------------------------- |
| `--force`                   | Wipe the database and rebuild from scratch. Otherwise performs an incremental sync. |
| `--status`                  | Print live/retained/archive counts after maintenance.                               |
| `--prune-retained-days <n>` | Delete retained off-disk sessions older than `n` days.                              |
| `--vacuum`                  | Run SQLite `VACUUM` after maintenance or pruning.                                   |

## Example

```bash
# Force a sync right now
claudex index

# Full rebuild (same as deleting ~/.claudex/index.db and running any command)
claudex index --force

# Inspect retention counts
claudex index --status

# Prune retained sessions older than six months and compact the DB
claudex index --prune-retained-days 180 --vacuum
```

## When to use it

- **Before a report when you expect fresh data.** Read commands sync on a
  5-minute staleness window; `claudex index` forces a sync outside that
  window.
- **After moving `~/.claude/projects/` around.** Stale `file_path`s will get
  pruned on the next sync, but `--force` is faster.
- **Suspected corruption** (disk full, interrupted write). `--force`
  rebuilds cleanly.
- **Schema-version bumps.** An automatic rebuild happens on version mismatch,
  so you rarely need `--force` for this — but running it explicitly is
  harmless.

## What incremental sync does

Each session file is keyed on `(file_path, file_size, file_mtime)`. Files
whose tuple hasn't changed are skipped. New files are ingested. Files that
vanished from disk are retained and marked off-disk; only `--force` or
`--prune-retained-days` removes historical rows.

This is why sync is fast: a typical run touches only a handful of sessions
even across hundreds of project histories.

## Output

Human output shows a spinner on **stderr** while syncing. When finished, it
prints a one-line summary (sessions indexed, rows updated). There's no
`--json` output for this command.

## Notes

- **Concurrency.** Claudex assumes it's the only writer to `~/.claudex/index.db`.
  Running two `claudex index` commands simultaneously is not supported — one
  will block on the SQLite lock.
- **Location.** Override with `CLAUDEX_DIR=/path/to/dir`.
- **Bundled SQLite.** No system SQLite is required; rusqlite ships its own.

# `watch`

Tail Claude Code's debug log in real time with formatted output.

## Usage

```bash
claudex watch [--follow <path>] [--raw]
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--follow <path>` | `~/.claudex/debug/latest.log` | Tail this file instead. |
| `--raw` | off | Disable formatting; emit raw log lines. |

## The two-terminal pattern

```bash
# Terminal 1 — starts watching
claudex watch

# Terminal 2 — starts Claude Code pointing at the same log
claude --debug-file ~/.claudex/debug/latest.log
```

**Claude Code does not write to `~/.claudex/debug/latest.log` on its own.**
You have to point it there with `--debug-file`.

Each new `claude` invocation truncates the file. `claudex watch` detects this
(size-shrink or inode change) and prints a banner so new sessions are obvious.

The directory is created on first run, so you can start `claudex watch` before
launching `claude`.

## Custom path

```bash
claudex watch --follow /tmp/my-session.log
claude        --debug-file /tmp/my-session.log
```

## Raw mode

```bash
claudex watch --raw
```

Passes log lines through unchanged. Useful when the formatter is hiding
something you need to debug.

## What gets formatted

- **Tool calls** — tool name + compact argument preview.
- **Session starts** — banner with timestamp.
- **Plain log lines** — passed through lightly.

## Relationship to the index

Watch reads the log _file_ directly; it does not update
`~/.claudex/index.db`. The index picks up the completed session the next time
you run a read command (summary, cost, sessions, …) and staleness has
elapsed.

## Notes

- **No `--json`.** Watch is a live view, not a data feed.
- **No network transport.** If `claude` is running on a remote host, either
  mount the log over SSHFS/NFS or run `claudex watch` on the remote host.
- **Terminal width.** The formatter adapts to `COLUMNS`; long tool args wrap.
- **See also:** [Watch mode guide](/guide/watch) for the broader ergonomics.

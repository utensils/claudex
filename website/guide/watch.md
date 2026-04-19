# Watch mode

`claudex watch` tails Claude Code's debug log in real time and formats it for
a terminal. It's the closest thing to "live session output" that claudex
offers.

## The two-terminal setup

```bash
# Terminal 1
claudex watch

# Terminal 2
claude --debug-file ~/.claudex/debug/latest.log
```

That's it. claudex tails the log, formats tool calls as they happen, and
inserts a session banner every time `claude` truncates the file (which happens
on every new invocation).

## Why does claude need `--debug-file`?

Claude Code **does not write `~/.claudex/debug/latest.log` on its own.** You
have to point it there per-invocation with `--debug-file`. claudex just
happens to watch that path by default.

The directory is created on first run, so you can start `claudex watch`
_before_ launching `claude` — it'll sit and wait for the file to appear.

## Starting fresh each invocation

Each new `claude --debug-file path` _truncates_ that file. `claudex watch`
detects this (via an inode or size-shrink change) and prints a visual
separator so you can tell a new session has started mid-stream.

## Customizing the path

```bash
# Watch an arbitrary file
claudex watch --follow /tmp/my-session.log

# Match the claude command
claude --debug-file /tmp/my-session.log
```

## Raw output

Formatting can obscure weird events. Drop back to raw tail:

```bash
claudex watch --raw
```

## What gets formatted?

- **Tool calls** — shown with a tool name + trimmed argument preview.
- **Session starts** — banner with timestamp.
- **Plain log lines** — pass through mostly unchanged.

The formatting is intentionally light — claudex watch is a pretty tail, not a
session visualizer. For post-hoc analysis, use [export](/commands/export) to
turn a completed session into Markdown.

## Trade-offs

- **No scrollback analysis.** Once a line streams past, it's gone from the
  view (though it's still in the log file).
- **Not the index.** Watch reads the log directly; it doesn't update
  `~/.claudex/index.db`. The index picks up the completed session the next
  time you run a read command.
- **File-based only.** There's no network transport. If `claude` is running on
  a remote host, either mount the log file over SSHFS or run `claudex watch`
  on the remote host.

## Shell ergonomics

A common pattern: spawn `claude` into a log and tail it in a split pane.

```bash
# In tmux / zellij / cmux — in one pane
claudex watch

# In another pane
claude --debug-file ~/.claudex/debug/latest.log
```

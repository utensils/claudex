# Color & terminal

claudex uses color for scannability — project names, costs, counts, and
timestamps each have a distinct color so a report reads at a glance. The
palette is defined in `src/ui.rs`; commands never reach for colors directly.

## Controlling color

```bash
claudex cost --color auto     # default — color iff stdout is a TTY
claudex cost --color always   # force color on, even through pipes
claudex cost --color never    # no ANSI codes ever
```

The flag is global — put it anywhere on the command line.

`NO_COLOR=1` (the [widely-adopted convention](https://no-color.org)) disables
color automatically when `--color` is at its default `auto`.

## Why "auto" does the right thing

`Auto` checks whether stdout is a terminal _and_ whether `NO_COLOR` is set.
When you pipe into `less` or `jq`, ANSI escape codes would show up as garbage;
claudex detects that and drops them.

## Forcing color through a pipe

`less -R` preserves ANSI codes. Combine with `--color always`:

```bash
claudex sessions --color always | less -R
```

## clap help also honors `--color`

claudex peeks at argv before clap parses, so `claudex --help --color never`
produces plain help text. Handy when piping help into a file for docs.

## Palette roles

| Role    | Used for                                             |
| ------- | ---------------------------------------------------- |
| project | Project names, headlines                             |
| cost    | Dollar amounts (bolded for readability)              |
| count   | Numeric counts with thousands separators             |
| dim     | Timestamps, session ID prefixes, neutral metadata    |
| model   | Model name (Opus / Sonnet / Haiku)                   |
| role    | Message roles (`user`, `assistant`) in search output |

On the Claude-branded docs site you're reading right now, the palette echoes
these roles — coral for brand / cost, cream for surfaces, deep ink for text.

## Tables

Human output uses a minimal `comfy-table` layout with no header separator and
dynamic width detection via `terminal_size`. Columns auto-wrap on narrow
terminals.

If your terminal lies about its width (common with certain multiplexers), set
`COLUMNS` manually:

```bash
COLUMNS=180 claudex cost
```

## Spinners

`ensure_fresh` shows a spinner on **stderr** during the initial sync so you
don't think the command hung. The spinner is TTY-gated — you never see it in
pipelines, and it doesn't pollute `--json` output.

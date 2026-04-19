# Troubleshooting

Common issues and how to diagnose them.

## "No sessions found"

```bash
claudex sessions
# (empty)
```

- Did you run `claude` locally at least once? claudex only reads what Claude
  Code has written.
- Does `ls ~/.claude/projects/` show directories? If not, Claude Code hasn't
  persisted anything yet.
- If you point `CLAUDE_HOME` or similar elsewhere, claudex still looks at
  `~/.claude/` — there's no override yet.

## Costs look too low / too high

Costs are _estimated_, not invoiced. They come from the token-usage blocks
Claude Code records, multiplied by published Opus / Sonnet / Haiku pricing
tiers.

- **Too low?** Check for mixed-model sessions. Each message is priced by its
  own model, so a session that started with Opus and finished with Sonnet
  shows a blended cost.
- **Too high?** Cache-read tokens dominate long sessions. See
  [Pricing model](/reference/pricing).
- **Always rounding to $0.00?** It doesn't — sub-cent values fall back to four
  decimal places. If you still see `$0.00`, check the token counts with
  `--json`.

## `claudex watch` shows nothing

Watch tails a file that Claude Code hasn't written to. You need `--debug-file`:

```bash
claude --debug-file ~/.claudex/debug/latest.log
```

Without that flag, Claude Code doesn't dump anything to the watch path. See
[Watch mode](/guide/watch).

## Search returns no hits for a term you know exists

- FTS5 tokenizes words with the `porter unicode61` analyzer. Symbols like `{`,
  `[`, `.`, `/` aren't tokens — search for the word next to them instead.
- Case-sensitive search falls back to a full file scan. Slower but authoritative.
- Make sure the index is fresh: `claudex index`.

## Index seems stuck / wrong

Nuke and rebuild:

```bash
claudex index --force
```

If that still looks wrong, run with `--no-index` to bypass the cache:

```bash
claudex cost --no-index
```

If the `--no-index` output disagrees with the indexed output, that's a bug —
please [open an issue](https://github.com/utensils/claudex/issues).

## Spinner keeps appearing on every run

The staleness window is 5 minutes. If your system clock is wrong or you
frequently skip across time zones, the index may think it's always stale.
Check `date` and your timezone.

## `COLUMNS` is wrong

Inside some terminal multiplexers, `terminal_size` can misread. Override:

```bash
COLUMNS=180 claudex cost
```

## Colors show as garbage in output

You probably piped color-enabled output into a tool that doesn't speak ANSI.
Use `--color never`:

```bash
claudex cost --color never | your-tool
```

Or rely on `Auto` to drop color when stdout isn't a TTY.

## Build fails with "rustc 1.94 is not supported"

claudex requires Rust 1.95 or newer. Upgrade:

```bash
rustup update stable
```

If you use Nix, the devshell pins the correct toolchain automatically.

## I see permission errors writing to `~/.claudex/`

Make sure the directory is writable. If you want to redirect it, use
`CLAUDEX_DIR`:

```bash
export CLAUDEX_DIR=/tmp/claudex
claudex summary
```

## Still stuck?

- Run `claudex <cmd> --no-index --json` and compare against the indexed
  output. Divergence = bug.
- File an issue with a reproducible snippet:
  <https://github.com/utensils/claudex/issues>

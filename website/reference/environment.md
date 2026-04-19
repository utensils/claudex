# Environment

Environment variables and paths claudex reads.

## Variables

| Var | Effect |
|-----|--------|
| `CLAUDEX_DIR` | Override the location of `~/.claudex/` (index, debug logs). |
| `NO_COLOR` | Disable color when `--color` is `auto`. [no-color.org](https://no-color.org). |
| `COLUMNS` | Override terminal width for table rendering. Useful in CI and multiplexers. |
| `RUST_BACKTRACE` | Rust panic behavior. `1` for a backtrace, `full` for a symbol-rich one. |

## Paths

| Path | Purpose |
|------|---------|
| `~/.claude/projects/` | Source of truth — Claude Code's session transcripts. **Read-only for claudex.** |
| `~/.claudex/index.db` | SQLite index. |
| `~/.claudex/debug/latest.log` | Default target for `claudex watch` + `claude --debug-file`. |
| `~/.cargo/bin/claudex` | Binary, if installed via `cargo install`. |

## Running under direnv

The project ships a Nix flake. With direnv installed and `use flake` in
`.envrc`, the devshell activates automatically on `cd` into the project
directory — no `nix develop` invocation required.

## CI flags to know

- `--color auto` (default) drops color when stdout isn't a TTY. CI pipelines
  that capture stdout get plain text by default.
- `NO_COLOR=1` in the CI environment forces plain text everywhere.
- `--json` is the recommended output for CI assertions; it's stable across
  patch releases.

## Cross-platform notes

- **macOS (Apple Silicon / Intel).** First-class target.
- **Linux (x86_64).** First-class target.
- **Windows.** Not tested. Paths assume `/`.

## Rust toolchain

- **MSRV: 1.95.** Set in `rust-toolchain.toml`.
- Nix devshell pins the correct toolchain automatically.

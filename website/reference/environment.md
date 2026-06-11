# Environment

Environment variables and paths claudex reads.

## Variables

| Var                       | Effect                                                                                                                                         |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `CLAUDEX_DIR`             | Override the location of `~/.claudex/` (index, debug logs).                                                                                    |
| `CLAUDEX_COPILOT_DIR`     | GitHub Copilot CLI data root to read for the `copilot` provider. Default `~/.copilot`.                                                         |
| `CLAUDEX_VSCODE_USER_DIR` | VS Code `User` directory to read for the `copilot-vscode` provider. Default: the platform config dir's `Code/User` and `Code - Insiders/User`. |
| `NO_COLOR`                | Disable color when `--color` is `auto`. [no-color.org](https://no-color.org).                                                                  |
| `OPENCLAW_STATE_DIR`      | OpenClaw state root to read for the `openclaw` provider. Default `~/.openclaw`.                                                                |
| `OPENCLAW_TRAJECTORY_DIR` | Optional OpenClaw runtime trajectory directory to merge when session metadata points to it.                                                    |
| `RUST_BACKTRACE`          | Rust panic behavior. `1` for a backtrace, `full` for a symbol-rich one.                                                                        |

Table width is detected automatically from the terminal (via the
`terminal_size` crate); claudex does not read `$COLUMNS`.

### Install script (`install.sh`) only

These are read by the [install script](/guide/installation#install-script), not
by the `claudex` binary:

| Var                   | Effect                                                              |
| --------------------- | ------------------------------------------------------------------- |
| `CLAUDEX_VERSION`     | Release tag to install (default: `latest`), e.g. `v0.5.2`.          |
| `CLAUDEX_INSTALL_DIR` | Install directory (default: `~/.local/bin`), e.g. `/usr/local/bin`. |

## Paths

| Path                                                    | Purpose                                                                                    |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `~/.claude/projects/`                                   | Source of truth — Claude Code's session transcripts. **Read-only for claudex.**            |
| `${OPENCLAW_STATE_DIR:-~/.openclaw}/agents/*/sessions/` | Source of truth — OpenClaw transcripts and trajectory sidecars. **Read-only for claudex.** |
| `${CLAUDEX_COPILOT_DIR:-~/.copilot}/session-state/`     | Source of truth — GitHub Copilot CLI event logs. **Read-only for claudex.**                |
| VS Code `User/workspaceStorage/*/chatSessions/`         | Source of truth — VS Code Copilot Chat sessions. **Read-only for claudex.**                |
| `~/.claudex/index.db`                                   | SQLite index.                                                                              |
| `~/.claudex/debug/latest.log`                           | Default target for `claudex watch` + `claude --debug-file`.                                |
| `~/.cargo/bin/claudex`                                  | Binary, if installed via `cargo install`.                                                  |

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

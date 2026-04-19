# Installation

claudex is a single Rust binary. It builds on Linux and macOS, x86_64 and
aarch64. It needs no system dependencies at runtime — rusqlite is bundled.

Minimum supported Rust version: **1.95**.

## Cargo (from source)

```bash
cargo install --git https://github.com/utensils/claudex claudex
```

The binary lands in `~/.cargo/bin/claudex`. Make sure that directory is on your
`PATH`.

## Nix flake

The repo ships a flake with a reproducible build (crane) and a devshell. If you
have Nix with flakes enabled:

```bash
# Run it without installing
nix run github:utensils/claudex -- --help

# Build the binary into ./result
nix build github:utensils/claudex

# Enter a dev shell (auto-activates via direnv if use_flake is on)
nix develop github:utensils/claudex
```

## From a local clone

```bash
git clone https://github.com/utensils/claudex
cd claudex

# Inside the Nix devshell (preferred)
nix develop
ci-local            # fmt-check → check → clippy → test → build

# Or straight cargo
cargo build --release
./target/release/claudex --help
```

The devshell exposes convenience commands (`build`, `build-release`, `check`,
`clippy`, `fmt`, `fmt-check`, `run-tests`, `ci-local`, `coverage`, `claudex`).
See the project [CLAUDE.md](https://github.com/utensils/claudex/blob/main/CLAUDE.md)
for the full table.

## Verify the install

```bash
claudex --version
claudex --help
claudex summary        # first run will index ~/.claude/projects/
```

If `~/.claude/projects/` is empty (you've never run Claude Code), you'll see an
empty summary — that's expected.

## Shell completions

Completions are generated on demand by the `completions` subcommand. See the
[Shell completions](/guide/completions) guide for setup in zsh, bash, fish,
elvish, and PowerShell.

## Uninstall

claudex writes only to `~/.claudex/` (the index and, if you use watch,
`~/.claudex/debug/`). To wipe it:

```bash
# Drop the index and debug logs
rm -rf ~/.claudex

# Remove the binary
rm ~/.cargo/bin/claudex   # if installed via cargo
```

Nothing lives in `~/.claude/` that claudex owns — it only reads from there.

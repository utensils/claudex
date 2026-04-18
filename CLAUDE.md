# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# claudex — Architecture & Development Guide

> Query, search, and analyze Claude Code sessions from the command line.

claudex is a CLI tool for working with Claude Code session data. It is a bare skeleton at v0.1.0 — only the help menu is wired up. Features will be added incrementally.

## Build & Development Commands

### Nix (preferred)

```bash
nix build          # Build claudex
nix run            # Run claudex
nix develop        # Enter devshell (auto via direnv)
nix fmt            # Format Nix + Rust (nixfmt + rustfmt)
nix flake check    # Validate formatting + flake
```

### Devshell commands (available inside `nix develop`)

| Category | Command | Description |
|----------|---------|-------------|
| build | `build` | `cargo build` (debug) |
| build | `build-release` | `cargo build --release` |
| check | `check` | `cargo check` |
| check | `clippy` | `cargo clippy -- -D warnings` |
| check | `fmt` | `cargo fmt` |
| check | `fmt-check` | `cargo fmt --check` |
| check | `run-tests` | `cargo test` |
| check | `ci-local` | Run the full CI sequence locally |
| run | `claudex` | Run claudex |

### Cargo (direct)

```bash
cargo build          # Debug build
cargo build --release
cargo check
cargo clippy -- -D warnings
cargo fmt --check
cargo test
cargo run -- --help
```

### CI (GitHub Actions)

CI runs on every push and PR (`.github/workflows/ci.yml`). All jobs must pass:

| Job | What it checks |
|-----|----------------|
| `fmt` | `cargo fmt --all -- --check` |
| `check` | `cargo check` |
| `clippy` | `cargo clippy -- -D warnings` |
| `test` | `cargo test` |
| `build` | `cargo build --release` |

## Project Structure

```
src/
└── main.rs    # CLI entry point (clap)
```

**MSRV**: 1.85

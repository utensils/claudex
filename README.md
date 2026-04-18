# claudex

Query, search, and analyze Claude Code sessions from the command line.

> **Status**: early skeleton — help menu only, features coming soon.

## What is claudex?

claudex is a CLI tool for working with Claude Code session data stored locally on your machine. It will let you search conversation history, extract insights, and analyze how you use Claude Code across projects.

## Build

```bash
# Preferred: Nix devshell (auto-activates with direnv)
nix develop

# Then build
cargo build

# Or build directly with Nix
nix build
```

## Run

```bash
# From devshell
claudex --help

# Via cargo
cargo run -- --help

# Via nix
nix run
```

## Development

Requires Rust 1.85+. See [CLAUDE.md](CLAUDE.md) for the full development guide.

## License

MIT — see [LICENSE](LICENSE).

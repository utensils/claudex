# claudex

[![CI](https://github.com/utensils/claudex/actions/workflows/ci.yml/badge.svg)](https://github.com/utensils/claudex/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/utensils/claudex/graph/badge.svg)](https://codecov.io/gh/utensils/claudex)

Query, search, and analyze Claude Code sessions from the command line.

## What is claudex?

claudex is a Rust CLI that reads the JSONL transcripts Claude Code writes under `~/.claude/projects/`, indexes them into a local SQLite database at `~/.claudex/index.db`, and exposes reports as subcommands. Every read command supports `--json` for machine-readable output and `--no-index` to bypass the index and scan files directly.

## Subcommands

| Command | What it does |
|--------|--------------|
| `sessions` | List sessions grouped by project |
| `cost` | Token usage and approximate cost per project or per session |
| `search <query>` | Full-text search across session messages (FTS5) |
| `tools` | Tool usage frequency |
| `summary` | Dashboard overview — sessions, cost, top projects/tools, model mix |
| `models` | Per-model call counts, token usage, and cost |
| `turns` | Per-turn timing (avg / p50 / p95 / max) |
| `prs` | Sessions linked to pull requests |
| `files` | Most frequently modified files across sessions |
| `export <selector>` | Export a session transcript as Markdown or JSON |
| `watch` | Tail Claude Code's debug log in real time (`claude --debug-file ...`) |
| `index` | Manage the session index (normally updated automatically) |
| `completions <shell>` | Generate shell completions (bash, zsh, fish, elvish, powershell) |

Global flags: `--color auto|always|never` (respects `NO_COLOR`).

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

Requires Rust 1.95+. See [CLAUDE.md](CLAUDE.md) for the full development guide. `ci-local` in the devshell mirrors CI; `coverage` (pass `--html` for a browsable report) runs `cargo llvm-cov`.

## License

MIT — see [LICENSE](LICENSE).

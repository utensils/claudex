# claudex

[![CI](https://github.com/utensils/claudex/actions/workflows/ci.yml/badge.svg)](https://github.com/utensils/claudex/actions/workflows/ci.yml)
[![Deploy Docs](https://github.com/utensils/claudex/actions/workflows/pages.yml/badge.svg)](https://github.com/utensils/claudex/actions/workflows/pages.yml)
[![codecov](https://codecov.io/gh/utensils/claudex/graph/badge.svg)](https://codecov.io/gh/utensils/claudex)

Query, search, and analyze Claude Code sessions from the command line.

**Docs:** https://utensils.io/claudex/ — full guide, per-command reference, index schema, pricing model.

## What is claudex?

claudex is a Rust CLI that reads the JSONL transcripts Claude Code writes under `~/.claude/projects/`, indexes them into a local SQLite database at `~/.claudex/index.db`, and exposes reports as subcommands. Most read commands support `--json` for machine-readable output and `--no-index` to bypass the index and scan files directly — see the [flag support matrix](https://utensils.io/claudex/commands/) for per-command details.

## Install

Three supported paths. See the [installation guide](https://utensils.io/claudex/guide/installation) for pinning versions, module inputs, and verification steps.

### Install script (macOS + Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/utensils/claudex/main/install.sh | sh
```

Prebuilt, stripped, SHA256-verified binary into `~/.local/bin/claudex`. Override with `CLAUDEX_VERSION=v0.2.0` or `CLAUDEX_INSTALL_DIR=/usr/local/bin`.

### Cargo

```bash
cargo install --git https://github.com/utensils/claudex --tag v0.2.0 claudex
```

### Nix flake

```bash
# Run without installing
nix run github:utensils/claudex -- summary

# Install into the user profile
nix profile install github:utensils/claudex

# Build the binary
nix build github:utensils/claudex
./result/bin/claudex --help
```

As a flake input:

```nix
inputs.claudex.url = "github:utensils/claudex";
```

Requires Rust 1.95+ for source builds. Prebuilt binaries have no runtime dependencies.

## Subcommands

| Command | What it does | Docs |
|--------|--------------|------|
| `summary` | Dashboard — sessions, cost, top projects/tools, model mix | [→](https://utensils.io/claudex/commands/summary) |
| `sessions` | List sessions grouped by project | [→](https://utensils.io/claudex/commands/sessions) |
| `cost` | Token usage and approximate cost per project or per session | [→](https://utensils.io/claudex/commands/cost) |
| `search <query>` | Full-text search across session messages (FTS5) | [→](https://utensils.io/claudex/commands/search) |
| `tools` | Tool usage frequency | [→](https://utensils.io/claudex/commands/tools) |
| `models` | Per-model call counts, token usage, and cost | [→](https://utensils.io/claudex/commands/models) |
| `turns` | Per-turn timing (avg / p50 / p95 / max) | [→](https://utensils.io/claudex/commands/turns) |
| `prs` | Sessions linked to pull requests | [→](https://utensils.io/claudex/commands/prs) |
| `files` | Most frequently modified files across sessions | [→](https://utensils.io/claudex/commands/files) |
| `export <selector>` | Export a session transcript as Markdown or JSON | [→](https://utensils.io/claudex/commands/export) |
| `watch` | Tail Claude Code's debug log in real time | [→](https://utensils.io/claudex/commands/watch) |
| `index` | Manage the session index (normally updated automatically) | [→](https://utensils.io/claudex/commands/index-cmd) |
| `completions <shell>` | Generate shell completions (bash, zsh, fish, elvish, powershell) | [→](https://utensils.io/claudex/commands/completions) |

Global flags: `--color auto|always|never` (respects `NO_COLOR`).

## Documentation

- [Quickstart](https://utensils.io/claudex/guide/quickstart) — the first five minutes.
- [How it works](https://utensils.io/claudex/guide/architecture) — data flow, modules, key invariants.
- [The index](https://utensils.io/claudex/guide/indexing) — sync semantics, staleness window.
- [JSON output](https://utensils.io/claudex/guide/json-output) — stable shapes for pipelines.
- [Recipes](https://utensils.io/claudex/guide/recipes) — copy-paste one-liners.
- [Reference](https://utensils.io/claudex/reference/) — file layout, index schema, pricing.

## Development

From a local clone:

```bash
git clone https://github.com/utensils/claudex
cd claudex
nix develop        # (auto via direnv + use_flake)
ci-local           # fmt-check → check → clippy → test → build
```

See [CLAUDE.md](CLAUDE.md) for the full development guide. `coverage` (pass `--html` for a browsable report) runs `cargo llvm-cov`.

## License

MIT — see [LICENSE](LICENSE).

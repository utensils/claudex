# claudex

[![CI](https://github.com/utensils/claudex/actions/workflows/ci.yml/badge.svg)](https://github.com/utensils/claudex/actions/workflows/ci.yml)
[![Deploy Docs](https://github.com/utensils/claudex/actions/workflows/pages.yml/badge.svg)](https://github.com/utensils/claudex/actions/workflows/pages.yml)
[![codecov](https://codecov.io/gh/utensils/claudex/graph/badge.svg)](https://codecov.io/gh/utensils/claudex)

**Query, search, and analyze Claude Code sessions from the command line.**

claudex reads the JSONL transcripts Claude Code writes under `~/.claude/projects/`, indexes them into a local SQLite database at `~/.claudex/index.db`, and exposes reports as subcommands. Every read command supports `--json` for machine-readable output; most also support `--no-index` to bypass the index and scan files directly.

📚 **Docs:** <https://utensils.io/claudex/> — guide, per-command reference, index schema, pricing.

---

## Quickstart

```bash
claudex summary                  # dashboard: sessions, cost, top projects, model mix
claudex sessions --limit 10      # recent sessions
claudex session 3f2a1b          # drill into one session (ID prefix or project name)
claudex search "migration"       # full-text search across all transcripts
claudex cost --per-session       # token & cost breakdown
claudex export 3f2a1b --format markdown > session.md
```

See the [flag support matrix](https://utensils.io/claudex/commands/) for per-command `--json` / `--no-index` coverage.

## Install

Pick one. All three paths are covered in depth in the [installation guide](https://utensils.io/claudex/guide/installation) — pinning, module inputs, verification.

### Install script — macOS + Linux

```bash
curl -fsSL https://raw.githubusercontent.com/utensils/claudex/main/install.sh | sh
```

Fetches a prebuilt, stripped, SHA256-verified binary into `~/.local/bin/claudex`. Override with `CLAUDEX_VERSION=v0.3.0` or `CLAUDEX_INSTALL_DIR=/usr/local/bin`.

### Cargo

```bash
cargo install --git https://github.com/utensils/claudex --tag v0.3.0 claudex
```

### Nix flake

```bash
nix run     github:utensils/claudex -- summary    # run without installing
nix profile install github:utensils/claudex       # install into user profile
nix build   github:utensils/claudex               # build locally → ./result/bin/claudex
```

As a flake input:

```nix
inputs.claudex.url = "github:utensils/claudex";
```

Source builds require Rust 1.95+. Prebuilt binaries have no runtime dependencies.

## Subcommands

| Command                                                                   | What it does                                                         |
| ------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| [`summary`](https://utensils.io/claudex/commands/summary)                 | Dashboard — sessions, cost, top projects/tools, model mix            |
| [`sessions`](https://utensils.io/claudex/commands/sessions)               | List sessions grouped by project                                     |
| [`session <selector>`](https://utensils.io/claudex/commands/session)      | Drill into one session: cost, tools, files, PRs, turns, stop reasons |
| [`cost`](https://utensils.io/claudex/commands/cost)                       | Token usage and approximate cost per project or per session          |
| [`search <query>`](https://utensils.io/claudex/commands/search)           | Full-text search across session messages (FTS5), with JSON hits      |
| [`tools`](https://utensils.io/claudex/commands/tools)                     | Tool usage frequency                                                 |
| [`models`](https://utensils.io/claudex/commands/models)                   | Per-model call counts, token usage, and cost                         |
| [`turns`](https://utensils.io/claudex/commands/turns)                     | Per-turn timing (avg / p50 / p95 / max)                              |
| [`prs`](https://utensils.io/claudex/commands/prs)                         | Sessions linked to pull requests                                     |
| [`files`](https://utensils.io/claudex/commands/files)                     | Most frequently modified files across sessions                       |
| [`export <selector>`](https://utensils.io/claudex/commands/export)        | Export a session transcript as Markdown or JSON                      |
| [`watch`](https://utensils.io/claudex/commands/watch)                     | Tail Claude Code's debug log in real time                            |
| [`index`](https://utensils.io/claudex/commands/index-cmd)                 | Manage the session index (normally updated automatically)            |
| [`update`](https://utensils.io/claudex/commands/update)                   | Self-update claudex, or print the right upgrade recipe for Nix/cargo/brew |
| [`completions <shell>`](https://utensils.io/claudex/commands/completions) | Generate shell completions (bash, zsh, fish, elvish, powershell)     |

Global flag: `--color auto|always|never` (respects `NO_COLOR`).

## Documentation

- [Quickstart](https://utensils.io/claudex/guide/quickstart) — first five minutes.
- [How it works](https://utensils.io/claudex/guide/architecture) — data flow, modules, key invariants.
- [The index](https://utensils.io/claudex/guide/indexing) — sync semantics, staleness window.
- [JSON output](https://utensils.io/claudex/guide/json-output) — stable shapes for pipelines.
- [Recipes](https://utensils.io/claudex/guide/recipes) — copy-paste one-liners.
- [Reference](https://utensils.io/claudex/reference/) — file layout, index schema, pricing.

## Development

```bash
git clone https://github.com/utensils/claudex
cd claudex
nix develop        # auto via direnv + use_flake
ci-local           # fmt-check → check → clippy → test → build
```

Additional commands: `coverage` runs `cargo llvm-cov` (pass `--html` for a browsable report). See [CLAUDE.md](CLAUDE.md) for the full development guide.

## License

MIT — see [LICENSE](LICENSE).

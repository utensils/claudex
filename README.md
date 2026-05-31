# claudex

[![CI](https://github.com/utensils/claudex/actions/workflows/ci.yml/badge.svg)](https://github.com/utensils/claudex/actions/workflows/ci.yml)
[![Deploy Docs](https://github.com/utensils/claudex/actions/workflows/pages.yml/badge.svg)](https://github.com/utensils/claudex/actions/workflows/pages.yml)
[![codecov](https://codecov.io/gh/utensils/claudex/graph/badge.svg)](https://codecov.io/gh/utensils/claudex)

**Query, search, and analyze your Claude Code, OpenAI Codex, Pi, and OpenClaw sessions from the command line.**

claudex reads the local transcripts of four coding agents — Claude Code (`~/.claude/projects/`), OpenAI Codex (`~/.codex/`), Pi (`~/.pi/agent/`), and OpenClaw (`${OPENCLAW_STATE_DIR:-~/.openclaw}`) — indexes them into a single SQLite database at `~/.claudex/index.db`, and exposes reports as subcommands. Every report spans all four providers by default; narrow with `--provider` and the shared `--since`/`--until`/`--model` filters. The index is **additive**: sessions you archive or delete from disk stay in your history. Every read command supports `--json` (with a `provider` key per row); Claude reports also support `--no-index`.

📚 **Docs:** <https://utensils.io/claudex/> — guide, per-command reference, index schema, pricing.

---

## Quickstart

```bash
claudex summary                              # dashboard: sessions, cost, top projects, model mix
claudex cost --provider codex --since 30d    # Codex spend over the last 30 days
claudex sessions --limit 10                  # recent sessions across every provider
claudex session 3f2a1b                       # drill into one session (ID prefix or project name)
claudex search "migration"                   # full-text search across all transcripts
claudex cost --per-session                   # token & cost breakdown
claudex export 3f2a1b --format markdown > session.md
claudex skills install                       # install the agent skill for Claude Code/Codex/Pi/OpenClaw
```

See the [flag support matrix](https://utensils.io/claudex/commands/) for per-command `--json` / `--no-index` coverage.

## Install

Pick one. All four paths are covered in depth in the [installation guide](https://utensils.io/claudex/guide/installation) — pinning, module inputs, verification.

### Install script — macOS + Linux

```bash
curl -fsSL https://raw.githubusercontent.com/utensils/claudex/main/install.sh | sh
```

Fetches a prebuilt, stripped, SHA256-verified binary into `~/.local/bin/claudex`. Override with `CLAUDEX_VERSION=v0.7.0` or `CLAUDEX_INSTALL_DIR=/usr/local/bin`. <!-- x-release-please-version -->

### Cargo

<!-- x-release-please-start-version -->

```bash
cargo install --git https://github.com/utensils/claudex --tag v0.7.0 claudex
```

<!-- x-release-please-end-version -->

### AUR — Arch Linux

```bash
paru -S claudex-bin      # prebuilt binary (fastest)
paru -S claudex          # build from source
paru -S claudex-git      # track main HEAD
```

Maintained in-tree at [`packaging/aur/`](./packaging/aur/) and auto-published on every release.

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

| Command                                                                   | What it does                                                              |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| [`summary`](https://utensils.io/claudex/commands/summary)                 | Dashboard — sessions, cost, top projects/tools, model mix                 |
| [`sessions`](https://utensils.io/claudex/commands/sessions)               | List sessions grouped by project (all providers)                          |
| [`session <selector>`](https://utensils.io/claudex/commands/session)      | Drill into one session: cost, tools, files, PRs, turns, stop reasons      |
| [`cost`](https://utensils.io/claudex/commands/cost)                       | Token usage and approximate cost per project or per session               |
| [`search <query>`](https://utensils.io/claudex/commands/search)           | Full-text search across session messages (FTS5), with JSON hits           |
| [`tools`](https://utensils.io/claudex/commands/tools)                     | Tool usage frequency                                                      |
| [`models`](https://utensils.io/claudex/commands/models)                   | Per-model call counts, token usage, and cost                              |
| [`turns`](https://utensils.io/claudex/commands/turns)                     | Per-turn timing (avg / p50 / p95 / max)                                   |
| [`prs`](https://utensils.io/claudex/commands/prs)                         | Sessions linked to pull requests                                          |
| [`files`](https://utensils.io/claudex/commands/files)                     | Most frequently modified files across sessions                            |
| [`export <selector>`](https://utensils.io/claudex/commands/export)        | Export a session transcript as Markdown or JSON                           |
| [`watch`](https://utensils.io/claudex/commands/watch)                     | Tail Claude Code's debug log in real time                                 |
| [`index`](https://utensils.io/claudex/commands/index-cmd)                 | Manage the session index (normally updated automatically)                 |
| [`update`](https://utensils.io/claudex/commands/update)                   | Self-update claudex, or print the right upgrade recipe for Nix/cargo/brew |
| [`completions <shell>`](https://utensils.io/claudex/commands/completions) | Generate shell completions (bash, zsh, fish, elvish, powershell)          |
| [`skills`](https://utensils.io/claudex/commands/skills)                   | Generate or install the agent skill for Claude Code, Codex, or Pi         |

Global flag: `--color auto|always|never` (respects `NO_COLOR`). Every report accepts the shared `--provider` / `--model` / `--since` / `--until` / `--on-disk-only` [filters](https://utensils.io/claudex/guide/providers); `--project` is a separate per-command filter.

## Documentation

- [Quickstart](https://utensils.io/claudex/guide/quickstart) — first five minutes.
- [Providers & filtering](https://utensils.io/claudex/guide/providers) — the four providers, retention, and the shared filter flags.
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

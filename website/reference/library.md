# Library API

The `claudex` crate exposes the same provider discovery, indexing, retention,
and report queries used by the CLI, but returns typed Rust structs instead of
terminal-rendered strings.

## Install

```toml
[dependencies]
claudex = "0.14.1" # x-release-plz-version
```

## Facade

Use `claudex::api` for the supported embedding surface:

```rust
use claudex::api::{Claudex, Filter};

fn main() -> anyhow::Result<()> {
    let mut claudex = Claudex::new()?;
    let summary = claudex.summary(Filter::default())?;

    println!("{} sessions", summary.total_sessions);
    Ok(())
}
```

Key types:

- `Claudex` — owns an index handle and the configured providers.
- `ClaudexConfig` — optional state directory and provider list.
- `QueryFilter` / `Filter` — provider, model, date, and on-disk filters.
- `ProviderKind` / `Provider` — Claude, Codex, Copilot (CLI and VS Code), Pi, OMP, Gemini CLI, OpenCode, Cline, Roo Code, and OpenClaw selectors.

## Additional sources

`Claudex::new()` and `Claudex::with_config()` load the same source configuration
as the CLI. For explicit embedding, supply `SourceConfig` directly:

```rust
use claudex::api::{Claudex, ClaudexConfig};
use claudex::config::SourceConfig;

let mut sources = SourceConfig::default();
sources.add_directory("omp=/path/to/sessions")?;
let mut cx = Claudex::with_sources(ClaudexConfig::default(), &sources)?;
cx.sync_now()?;
```

Sources add to standard discovery. Relative paths in JSON files resolve beside
the configuration file; programmatic/CLI additions resolve from cwd.

## Query Pattern

Report methods ensure the index is fresh, resolve the filter, and return typed
rows:

```rust
use claudex::api::{Claudex, Filter, Provider};

let mut cx = Claudex::new()?;
let filter = Filter {
    providers: vec![Provider::Codex],
    since: Some("30d".to_string()),
    ..Default::default()
};

let sessions = cx.sessions(None, None, filter, 20)?;
```

The facade includes summary, sessions, search, costs, tools, models, timeline,
activity, provider status, retention stats, PRs, files, turn timing, session
matches, and session detail.

## Sync Behavior

`ensure_fresh()` uses the same five-minute provider staleness window as the CLI.
`sync_now()` forces an incremental sync, and `force_rebuild()` is the destructive
rebuild path that drops retained rows before re-indexing.

The library does not draw spinners, tables, colors, shell completions, or update
prompts. Those terminal concerns intentionally live in `claudex-cli`.

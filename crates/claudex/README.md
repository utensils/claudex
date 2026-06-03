# claudex

Reusable Rust library for indexing and querying local agent coding sessions.

The crate reads Claude Code, OpenAI Codex, Pi, and OpenClaw transcripts through
the same provider/index pipeline used by the CLI, then returns typed report
structs instead of terminal-rendered tables.

## Install

```toml
[dependencies]
claudex = "0.9.2" # x-release-please-version
```

## Example

```rust
use claudex::api::{Claudex, Filter};

fn main() -> anyhow::Result<()> {
    let mut claudex = Claudex::new()?;
    let summary = claudex.summary(Filter::default())?;

    println!("sessions: {}", summary.total_sessions);
    println!("cost: ${:.2}", summary.total_cost);
    Ok(())
}
```

Use `claudex::api` for the supported facade. `claudex::index::IndexStore`
remains public for callers that need lower-level query control.

use std::path::PathBuf;

use anyhow::{Context, Result};

pub mod commands;
pub mod index;
pub mod parser;
pub mod plan;
pub mod stats;
pub mod store;
pub mod types;
pub mod ui;

/// Returns the claudex state directory, creating it if missing.
///
/// Defaults to `~/.claudex`. Override with the `CLAUDEX_DIR` environment
/// variable — useful for sandboxed CI, read-only `$HOME`, or parallel
/// databases during development. The env var wins unconditionally when set.
pub fn claudex_dir() -> Result<PathBuf> {
    let dir = if let Some(val) = std::env::var_os("CLAUDEX_DIR")
        && !val.is_empty()
    {
        PathBuf::from(val)
    } else {
        let home = dirs::home_dir().context("could not find home directory")?;
        home.join(".claudex")
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

// Env-var override tests live in tests/cli_tests.rs — they spawn a subprocess
// with `CLAUDEX_DIR` set, which avoids racing with other lib-level tests that
// call `claudex_dir()` in the same process.

use std::path::PathBuf;

use anyhow::{Context, Result};

pub mod commands;
pub mod index;
pub mod parser;
pub mod store;
pub mod types;
pub mod ui;

/// Returns `~/.claudex`, creating it if missing.
pub fn claudex_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not find home directory")?;
    let dir = home.join(".claudex");
    std::fs::create_dir_all(&dir).context("creating ~/.claudex")?;
    Ok(dir)
}

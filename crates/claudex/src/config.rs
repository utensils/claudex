//! Additive transcript discovery shared by the CLI and library.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Extra transcript directories, grouped by provider identifier.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderPaths>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderPaths {
    #[serde(default)]
    pub session_dirs: Vec<PathBuf>,
}

static CLI_CONFIG: OnceLock<SourceConfig> = OnceLock::new();

impl SourceConfig {
    /// Read JSON configuration. Relative directories resolve beside this file.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading configuration {}", path.display()))?;
        let mut config: Self = serde_json::from_str(&text)
            .with_context(|| format!("parsing configuration {}", path.display()))?;
        let absolute = std::fs::canonicalize(path)?;
        config.resolve(absolute.parent().unwrap_or(Path::new(".")))?;
        Ok(config)
    }

    pub(crate) fn resolve(&mut self, base: &Path) -> Result<()> {
        for (id, paths) in &mut self.providers {
            if !crate::filter::ProviderKind::ALL
                .iter()
                .any(|kind| kind.id() == id)
            {
                bail!("unknown session provider {id:?}");
            }
            for path in &mut paths.session_dirs {
                if path.as_os_str().is_empty() {
                    bail!("empty session directory for {id}");
                }
                *path = crate::providers::expand_home(&path.to_string_lossy())?;
                if path.is_relative() {
                    *path = base.join(&*path);
                }
            }
            paths.session_dirs.sort();
            paths.session_dirs.dedup();
        }
        Ok(())
    }

    /// Add a CLI-style `PROVIDER=PATH` directory; does not replace defaults.
    pub fn add_directory(&mut self, value: &str) -> Result<()> {
        let (id, path) = value
            .split_once('=')
            .context("expected PROVIDER=PATH for --session-dir")?;
        self.providers
            .entry(id.to_string())
            .or_default()
            .session_dirs
            .push(PathBuf::from(path));
        self.resolve(&std::env::current_dir()?)
    }
}

/// Default configuration is `$CLAUDEX_CONFIG` or `<state directory>/config.json`.
/// An absent default file is fine; an explicitly selected missing file is an error.
pub fn source_config() -> Result<SourceConfig> {
    if let Some(config) = CLI_CONFIG.get() {
        return Ok(config.clone());
    }
    load_selected(None)
}

fn load_selected(path: Option<&Path>) -> Result<SourceConfig> {
    if let Some(path) = path {
        return SourceConfig::load(path);
    }
    if let Some(path) = std::env::var_os("CLAUDEX_CONFIG") {
        return SourceConfig::load(Path::new(&path));
    }
    let path = crate::claudex_dir()?.join("config.json");
    if path.exists() {
        SourceConfig::load(&path)
    } else {
        Ok(SourceConfig::default())
    }
}

/// Initialize immutable CLI overrides before running a command.
pub fn configure_cli(path: Option<&Path>, directories: &[String]) -> Result<()> {
    let mut config = load_selected(path)?;
    for directory in directories {
        config.add_directory(directory)?;
    }
    CLI_CONFIG
        .set(config)
        .map_err(|_| anyhow::anyhow!("CLI sources already configured"))
}

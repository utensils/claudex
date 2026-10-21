//! Combines all roots before provider-scoped index reconciliation.
use super::{DiscoveredFile, Provider, ProviderRecord, SessionProvider};
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub struct AdditionalProvider {
    pub base: Provider,
    pub directories: Vec<PathBuf>,
}

impl SessionProvider for AdditionalProvider {
    fn id(&self) -> &'static str {
        self.base.id()
    }
    fn root_dir(&self) -> &Path {
        self.base.root_dir()
    }
    fn enabled(&self) -> bool {
        self.base.enabled() || !self.directories.is_empty()
    }
    fn source_fingerprint(&self, file: &DiscoveredFile) -> Result<(i64, i64)> {
        if self.id() == "opencode" {
            for dir in &self.directories {
                if file.path.starts_with(dir) {
                    return super::opencode::OpenCodeProvider::at(dir.clone())
                        .source_fingerprint(file);
                }
            }
        }
        self.base.source_fingerprint(file)
    }
    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let mut files = self.base.enumerate()?;
        let mut visited = BTreeSet::new();
        for dir in &self.directories {
            if self.id() == "opencode" {
                files.extend(super::opencode::OpenCodeProvider::at(dir.clone()).enumerate()?);
            } else if self.id() == "cline" {
                files.extend(super::cline::ClineProvider::at(dir.clone()).enumerate()?);
            } else if self.id() == "roo" {
                files.extend(super::cline::RooProvider::at(dir.clone()).enumerate()?);
            } else if self.id() == "gemini" {
                files.extend(super::gemini::GeminiProvider::at(dir.clone()).enumerate()?);
            } else {
                collect(dir, self.id(), &mut visited, &mut files)?;
            }
        }
        let mut seen = BTreeSet::new();
        files.retain(|file| {
            seen.insert(std::fs::canonicalize(&file.path).unwrap_or_else(|_| file.path.clone()))
        });
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }
    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        if self.id() == "opencode" {
            for dir in &self.directories {
                if file.path.starts_with(dir) {
                    return super::opencode::OpenCodeProvider::at(dir.clone()).parse(file);
                }
            }
        }
        let mut record = self.base.parse(file)?;
        if self.id() == "openclaw" && file.source_key.is_some() {
            record.source_key = file.source_key.clone();
        }
        Ok(record)
    }
}

fn collect(
    dir: &Path,
    id: &str,
    visited: &mut BTreeSet<PathBuf>,
    out: &mut Vec<DiscoveredFile>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let canonical =
        std::fs::canonicalize(dir).with_context(|| format!("resolving {}", dir.display()))?;
    if !visited.insert(canonical) {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, id, visited, out)?;
        } else if path.is_file() && id == "openclaw" {
            if let Some(file) = super::openclaw::additional_file(path) {
                out.push(file);
            }
        } else if path.is_file()
            && (path.extension().is_some_and(|ext| ext == "jsonl")
                || (id == "copilot-vscode" && path.extension().is_some_and(|ext| ext == "json")))
        {
            if id == "copilot" && path.file_name().is_none_or(|name| name != "events.jsonl") {
                continue;
            }
            let parent_session_id = if id == "claude" {
                path.parent()
                    .filter(|p| p.file_name().is_some_and(|name| name == "subagents"))
                    .and_then(Path::parent)
                    .and_then(Path::file_name)
                    .map(|s| s.to_string_lossy().into_owned())
            } else {
                None
            };
            out.push(DiscoveredFile {
                path,
                project_display: String::new(),
                parent_session_id,
                archived: false,
                source_key: None,
            });
        }
    }
    Ok(())
}

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub struct SessionStore {
    pub base_dir: PathBuf,
}

impl SessionStore {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        Ok(Self {
            base_dir: home.join(".claude").join("projects"),
        })
    }

    pub fn project_dirs(&self) -> Result<Vec<(String, PathBuf)>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let mut projects = Vec::new();
        let entries = std::fs::read_dir(&self.base_dir)
            .with_context(|| format!("reading {}", self.base_dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                projects.push((name, path));
            }
        }
        projects.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(projects)
    }

    pub fn session_files(&self, project_path: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(project_path)?.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }

    pub fn all_session_files(
        &self,
        project_filter: Option<&str>,
    ) -> Result<Vec<(String, PathBuf)>> {
        let mut result = Vec::new();
        for (project, dir) in self.project_dirs()? {
            if let Some(filter) = project_filter {
                let decoded = decode_project_name(&project);
                if !project.contains(filter) && !decoded.contains(filter) {
                    continue;
                }
            }
            for file in self.session_files(&dir)? {
                result.push((project.clone(), file));
            }
        }
        Ok(result)
    }
}

/// Decode a Claude Code project directory name into a filesystem path.
///
/// Claude Code encodes CWD paths with two rules applied in order:
///   `/.hidden` → `--hidden`  (slash + dot for hidden dirs → double dash)
///   `/segment` → `-segment`  (path separator → single dash)
///
/// Decoding reverses this left-to-right: `--` → `/.`, `-` → `/`
pub fn decode_project_name(encoded: &str) -> String {
    let mut result = String::with_capacity(encoded.len() + 1);
    let bytes = encoded.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                result.push_str("/.");
                i += 2;
            } else {
                result.push('/');
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

/// Return the canonical project path used as an index key.
///
/// Worktree paths (`…/.claude/worktrees/branch`) are resolved to their parent
/// so worktree sessions aggregate with the parent project automatically.
pub fn canonical_project_path(decoded_path: &str) -> &str {
    const MARKER: &str = "/.claude/worktrees/";
    if let Some(idx) = decoded_path.find(MARKER) {
        &decoded_path[..idx]
    } else {
        decoded_path
    }
}

/// Convert a decoded project path to a human-readable display name.
///
/// Paths containing `/.claude/worktrees/` are shown as "projectname (worktree)"
/// to avoid long branch-hash suffixes obscuring the actual project name.
pub fn display_project_name(decoded_path: &str) -> String {
    const MARKER: &str = "/.claude/worktrees/";
    if let Some(idx) = decoded_path.find(MARKER) {
        let base = &decoded_path[..idx];
        let proj = base.rsplit('/').next().unwrap_or(base);
        return format!("{} (worktree)", proj);
    }
    decoded_path.to_string()
}

/// Shorten a decoded project path to at most 55 bytes for display.
pub fn short_name(path: &str) -> String {
    const MAX: usize = 55;
    // '…' is 3 bytes; reserve space for it so total stays within MAX
    const SUFFIX_MAX: usize = MAX - 3;
    if path.len() <= MAX {
        return path.to_string();
    }
    let suffix_start = path.len() - SUFFIX_MAX;
    // Advance to the next '/' so we don't cut mid-component
    let adjusted = path[suffix_start..]
        .find('/')
        .map_or(suffix_start, |p| suffix_start + p);
    format!("…{}", &path[adjusted..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_simple() {
        assert_eq!(
            decode_project_name("-Users-jamesbrink"),
            "/Users/jamesbrink"
        );
    }

    #[test]
    fn decode_hidden_dir() {
        assert_eq!(
            decode_project_name("-Users-jamesbrink-Projects-claudex--claude-worktrees"),
            "/Users/jamesbrink/Projects/claudex/.claude/worktrees"
        );
    }

    #[test]
    fn display_name_worktree() {
        assert_eq!(
            display_project_name(
                "/Users/jamesbrink/Projects/claudex/.claude/worktrees/strange-borg-c09522"
            ),
            "claudex (worktree)"
        );
    }

    #[test]
    fn display_name_normal() {
        assert_eq!(
            display_project_name("/Users/jamesbrink/Projects/foo"),
            "/Users/jamesbrink/Projects/foo"
        );
    }

    #[test]
    fn short_name_short() {
        assert_eq!(short_name("/Users/foo"), "/Users/foo");
    }

    #[test]
    fn short_name_long() {
        let long = "/Users/jamesbrink/Projects/utensils/claudex/.claude/worktrees/something";
        let s = short_name(long);
        assert!(s.starts_with('…'));
        assert!(s.len() <= 55);
    }
}

//! Oh My Pi sessions, including nested subagents and out-of-band model calls.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::{DiscoveredFile, ProviderRecord, SessionProvider, expand_home};
use crate::parser::stream_records;

pub struct OmpProvider {
    sessions_dir: PathBuf,
}

impl OmpProvider {
    /// Locate OMP's session storage, respecting its agent, profile and XDG settings.
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not find home directory")?;
        let profile = std::env::var("OMP_PROFILE")
            .or_else(|_| std::env::var("PI_PROFILE"))
            .unwrap_or_default();
        let profile = profile.trim();
        let profile = (!profile.is_empty() && profile != "default").then_some(profile);
        if let Some(profile) = profile
            && (profile.len() > 64
                || !profile.bytes().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'-' | b'_')
                })
                || !profile.as_bytes()[0].is_ascii_lowercase()
                    && !profile.as_bytes()[0].is_ascii_digit()
                || profile.ends_with('.')
                || is_reserved_profile(profile))
        {
            bail!("invalid OMP profile name");
        }
        let mut config_root = home.join(
            std::env::var("PI_CONFIG_DIR")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| ".omp".into()),
        );
        if let Some(profile) = profile {
            config_root = config_root.join("profiles").join(profile);
        } else if let Ok(agent_dir) = std::env::var("PI_CODING_AGENT_DIR")
            && !agent_dir.is_empty()
        {
            let agent_dir = expand_home(&agent_dir)?;
            if agent_dir != config_root.join("agent") {
                return Ok(Self::at(agent_dir));
            }
        }
        if cfg!(any(target_os = "linux", target_os = "macos"))
            && let Ok(data) = std::env::var("XDG_DATA_HOME")
            && !data.is_empty()
        {
            let mut app_root = PathBuf::from(data).join("omp");
            if let Some(profile) = profile {
                app_root = app_root.join("profiles").join(profile);
            }
            if app_root.exists() {
                return Ok(Self::sessions_at(app_root.join("sessions")));
            }
        }
        Ok(Self::at(config_root.join("agent")))
    }

    /// Root at an explicit OMP agent directory.
    pub fn at(agent_dir: PathBuf) -> Self {
        Self::sessions_at(agent_dir.join("sessions"))
    }

    /// Root directly at a session directory, including custom `--session-dir` storage.
    pub fn sessions_at(sessions_dir: PathBuf) -> Self {
        Self { sessions_dir }
    }
}

impl SessionProvider for OmpProvider {
    fn id(&self) -> &'static str {
        "omp"
    }

    fn root_dir(&self) -> &Path {
        &self.sessions_dir
    }

    fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        let mut files = Vec::new();
        let mut pending = vec![self.sessions_dir.clone()];
        while let Some(dir) = pending.pop() {
            if !dir.exists() {
                continue;
            }
            for item in
                std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
            {
                let item = item?;
                let kind = item.file_type()?;
                if kind.is_dir() {
                    pending.push(item.path());
                } else if kind.is_file() && item.path().extension().is_some_and(|e| e == "jsonl") {
                    files.push(DiscoveredFile {
                        path: item.path(),
                        project_display: String::new(),
                        parent_session_id: None,
                        archived: false,
                        source_key: None,
                    });
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        let mut entry = super::pi::parse_pi_session(&file.path)?;
        stream_records(&file.path, |record| {
            match record["type"].as_str() {
                Some("session") => {
                    entry.parent_session_id = record["parentSession"]
                        .as_str()
                        .filter(|p| !p.is_empty())
                        .map(parent_session_id);
                }
                Some("model_usage") => accumulate_model_call(&mut entry, record),
                _ => {}
            }
            true
        })?;
        Ok(entry)
    }
}

fn parent_session_id(reference: &str) -> String {
    let path = Path::new(reference);
    if path.extension().is_some_and(|ext| ext == "jsonl") {
        let mut id = None;
        let _ = stream_records(path, |record| {
            if record["type"].as_str() == Some("session") {
                id = record["id"].as_str().map(str::to_string);
                return false;
            }
            true
        });
        id.unwrap_or_else(|| {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            stem.rsplit_once('_')
                .map_or(stem.as_ref(), |(_, id)| id)
                .to_string()
        })
    } else {
        reference.to_string()
    }
}

fn accumulate_model_call(entry: &mut ProviderRecord, record: &Value) {
    let provider = record["provider"].as_str().unwrap_or_default();
    let model = record["model"].as_str().unwrap_or_default();
    let key = if provider.is_empty() {
        model.to_string()
    } else {
        format!("{provider}/{model}")
    };
    let usage = &record["usage"];
    let stats = entry.model_usage.entry(key).or_default();
    stats.usage.input_tokens += usage["input"].as_u64().unwrap_or_default();
    stats.usage.output_tokens += usage["output"].as_u64().unwrap_or_default();
    stats.usage.cache_read_tokens += usage["cacheRead"].as_u64().unwrap_or_default();
    stats.usage.cache_creation_tokens += usage["cacheWrite"].as_u64().unwrap_or_default();
    stats.assistant_message_count += 1;
    if let Some(cost) = usage["cost"]["total"].as_f64() {
        stats.embedded_cost = Some(stats.embedded_cost.unwrap_or_default() + cost);
        entry.embedded_cost = Some(entry.embedded_cost.unwrap_or_default() + cost);
    }
    if let Some(reason) = record["stopReason"].as_str().filter(|s| !s.is_empty()) {
        *entry
            .stop_reason_counts
            .entry(reason.to_string())
            .or_default() += 1;
    }
}

fn is_reserved_profile(profile: &str) -> bool {
    let base = profile
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit())
}

//! Provider abstraction over the agent CLIs whose sessions claudex indexes.
//!
//! Each provider knows how to find its on-disk transcripts (`enumerate`) and
//! turn one into a normalized [`ProviderRecord`] (`parse`). The index sync loop
//! is provider-agnostic: it reconciles the enumerated files against the rows it
//! already holds for that provider and writes the parsed records, tagging every
//! row with [`SessionProvider::id`]. Claude, Codex, Pi, and OpenClaw all share
//! this contract.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::parser::ModelSessionStats;
use crate::types::TokenUsage;

pub mod claude;
pub mod codex;
pub mod openclaw;
pub mod pi;
pub(crate) mod pr;

pub use claude::ClaudeProvider;
pub use codex::CodexProvider;
pub use openclaw::OpenClawProvider;
pub use pi::PiProvider;

/// A transcript file discovered by a provider, with the metadata the sync loop
/// needs that does not come from parsing the file's contents.
pub struct DiscoveredFile {
    /// Absolute path to the transcript on disk.
    pub path: PathBuf,
    /// User-facing project label (canonical project path / cwd).
    pub project_display: String,
    /// Parent session id when this file is a subagent transcript (Claude only).
    pub parent_session_id: Option<String>,
    /// True when the source lives in the provider's archive location.
    pub archived: bool,
    /// Provider-scoped logical source id. OpenClaw uses this to merge a
    /// trajectory-only row with the canonical transcript row once it appears.
    pub source_key: Option<String>,
}

/// One assistant/user message's text, destined for the FTS index.
pub struct MessageForFts {
    pub msg_type: String,
    pub content: String,
    pub timestamp_ms: Option<i64>,
}

/// Normalized, provider-independent view of a single session, mapping directly
/// onto the index's `sessions` row and its derived tables. Every provider's
/// `parse` produces one of these; the index insert loop consumes it without
/// knowing which provider it came from.
#[derive(Default)]
pub struct ProviderRecord {
    pub session_id: Option<String>,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub duration_ms: u64,
    pub message_count: usize,
    pub model: Option<String>,
    pub usage: TokenUsage,
    pub model_usage: BTreeMap<String, ModelSessionStats>,
    pub tool_names: Vec<String>,
    pub messages: Vec<MessageForFts>,
    pub turn_durations: Vec<(u64, String)>,
    pub pr_links: Vec<(i64, String, String, String)>,
    pub file_paths_modified: Vec<String>,
    pub thinking_block_count: u64,
    pub stop_reason_counts: HashMap<String, u64>,
    pub attachments: Vec<(String, String)>,
    pub permission_modes: Vec<(String, String)>,
    pub inference_geo: Option<String>,
    pub speed: Option<f64>,
    pub service_tier: Option<String>,
    pub iterations: u64,
    /// Project label derived from the transcript's contents (Codex stores its
    /// `cwd` inside the file). When non-empty it overrides the
    /// [`DiscoveredFile::project_display`] the enumerator guessed from the path.
    pub project_display: String,
    /// Provider-scoped logical source id. Existing providers leave this empty.
    pub source_key: Option<String>,
    /// Cost already computed by the provider (Pi reports per-message cost). When
    /// `Some`, the index trusts it instead of deriving from a pricing table.
    pub embedded_cost: Option<f64>,
    /// Provider-specific metadata as a JSON object string, stored verbatim in
    /// `sessions.extras` (e.g. Codex cli_version / git branch, Pi api/provider).
    pub extras: Option<String>,
}

/// The contract every provider implements. Implementations are concrete structs
/// held by the [`Provider`] enum, which dispatches without trait objects.
pub trait SessionProvider {
    /// Stable identifier persisted in `sessions.provider` (e.g. "claude").
    fn id(&self) -> &'static str;
    /// Root directory the provider reads transcripts from.
    fn root_dir(&self) -> &Path;
    /// Whether the provider has data to index (its root exists).
    fn enabled(&self) -> bool {
        self.root_dir().exists()
    }
    /// Discover every transcript the provider currently has on disk.
    fn enumerate(&self) -> Result<Vec<DiscoveredFile>>;
    /// Parse one discovered transcript into a normalized record.
    fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord>;
}

/// Enum dispatch over the compiled-in providers. Adding a provider means adding
/// a variant and a match arm — no boxing, exhaustive at every call site.
pub enum Provider {
    Claude(ClaudeProvider),
    Codex(CodexProvider),
    OpenClaw(OpenClawProvider),
    Pi(PiProvider),
}

impl Provider {
    fn inner(&self) -> &dyn SessionProvider {
        match self {
            Provider::Claude(p) => p,
            Provider::Codex(p) => p,
            Provider::OpenClaw(p) => p,
            Provider::Pi(p) => p,
        }
    }

    pub fn id(&self) -> &'static str {
        self.inner().id()
    }

    pub fn root_dir(&self) -> &Path {
        self.inner().root_dir()
    }

    pub fn enabled(&self) -> bool {
        self.inner().enabled()
    }

    pub fn enumerate(&self) -> Result<Vec<DiscoveredFile>> {
        self.inner().enumerate()
    }

    pub fn parse(&self, file: &DiscoveredFile) -> Result<ProviderRecord> {
        self.inner().parse(file)
    }
}

/// The default set of providers to index: every provider whose data root exists
/// on disk. Spanning all available providers is intentional — claudex reports
/// agent usage everywhere, not just Claude.
pub fn enabled_default() -> Result<Vec<Provider>> {
    let candidates = vec![
        Provider::Claude(ClaudeProvider::new()?),
        Provider::Codex(CodexProvider::new()?),
        Provider::OpenClaw(OpenClawProvider::new()?),
        Provider::Pi(PiProvider::new()?),
    ];
    Ok(candidates.into_iter().filter(|p| p.enabled()).collect())
}

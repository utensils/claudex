//! Preferred high-level API for embedding claudex.
//!
//! The lower-level [`crate::index::IndexStore`] remains available for callers
//! that need direct control over SQL-backed queries. This module keeps common
//! library usage small: configure providers, keep the index fresh, and ask for
//! typed reports.

use std::path::PathBuf;

use anyhow::Result;

use crate::filter::{ProviderKind, QueryFilter, ResolvedFilter};
use crate::index::{
    ActivityData, CostSummary, FileModRow, IndexStore, IndexedSession, ModelUsageRow, PrLinkRow,
    ProviderStatusRow, RetentionStats, SearchFtsOptions, SearchHit, SessionCostRow, SessionDetail,
    SessionToolRow, SummaryData, TimelineRow, ToolRow, TurnStatsRow,
};
use crate::providers::{
    ClaudeProvider, CodexProvider, OpenClawProvider, PiProvider, Provider as ProviderImpl,
};

pub use crate::filter::{ProviderKind as Provider, QueryFilter as Filter};

/// Configuration for [`Claudex`].
#[derive(Debug, Clone, Default)]
pub struct ClaudexConfig {
    /// Directory containing `index.db`. Defaults to `CLAUDEX_DIR` or
    /// `~/.claudex`.
    pub state_dir: Option<PathBuf>,
    /// Providers to consider. Empty means every provider with an existing data
    /// root, matching CLI behavior.
    pub providers: Vec<ProviderKind>,
}

/// Reusable claudex client.
pub struct Claudex {
    index: IndexStore,
    providers: Vec<ProviderImpl>,
}

impl Claudex {
    pub fn new() -> Result<Self> {
        Self::with_config(ClaudexConfig::default())
    }

    pub fn with_config(config: ClaudexConfig) -> Result<Self> {
        let index = match config.state_dir {
            Some(dir) => IndexStore::open_at(&dir.join("index.db"))?,
            None => IndexStore::open()?,
        };
        let providers = providers_for(&config.providers)?;
        Ok(Self { index, providers })
    }

    pub fn index(&self) -> &IndexStore {
        &self.index
    }

    pub fn index_mut(&mut self) -> &mut IndexStore {
        &mut self.index
    }

    pub fn ensure_fresh(&mut self) -> Result<()> {
        self.index.ensure_fresh(&self.providers)
    }

    pub fn sync_now(&mut self) -> Result<usize> {
        self.index.sync_now(&self.providers)
    }

    pub fn force_rebuild(&mut self) -> Result<usize> {
        self.index.force_rebuild(&self.providers)
    }

    pub fn summary(&mut self, filter: QueryFilter) -> Result<SummaryData> {
        let filter = self.prepare(filter)?;
        self.index.query_summary(&filter)
    }

    pub fn sessions(
        &mut self,
        project: Option<&str>,
        file: Option<&str>,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<IndexedSession>> {
        let filter = self.prepare(filter)?;
        self.index.query_sessions(project, file, &filter, limit)
    }

    pub fn search(
        &mut self,
        query: &str,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        let filter = self.prepare(filter)?;
        self.index.search_fts(SearchFtsOptions {
            query,
            project_filter: None,
            filter: &filter,
            role_filter: None,
            tool_filter: None,
            file_filter: None,
            pr_filter: None,
            context: 0,
            limit,
        })
    }

    pub fn costs_by_project(
        &mut self,
        project: Option<&str>,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<crate::index::ProjectCostRow>> {
        let filter = self.prepare(filter)?;
        self.index.query_cost_by_project(project, &filter, limit)
    }

    pub fn costs_per_session(
        &mut self,
        project: Option<&str>,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<SessionCostRow>> {
        let filter = self.prepare(filter)?;
        self.index.query_cost_per_session(project, &filter, limit)
    }

    pub fn cost_summary(
        &mut self,
        project: Option<&str>,
        filter: QueryFilter,
    ) -> Result<CostSummary> {
        let filter = self.prepare(filter)?;
        self.index.query_cost_summary(project, &filter)
    }

    pub fn tools(
        &mut self,
        project: Option<&str>,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<ToolRow>> {
        let filter = self.prepare(filter)?;
        self.index.query_tools_aggregate(project, &filter, limit)
    }

    pub fn tools_per_session(
        &mut self,
        project: Option<&str>,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<SessionToolRow>> {
        let filter = self.prepare(filter)?;
        self.index.query_tools_per_session(project, &filter, limit)
    }

    pub fn models(
        &mut self,
        project: Option<&str>,
        filter: QueryFilter,
    ) -> Result<Vec<ModelUsageRow>> {
        let filter = self.prepare(filter)?;
        self.index.query_model_usage(project, &filter)
    }

    pub fn timeline(
        &mut self,
        filter: QueryFilter,
        weekly: bool,
        limit: usize,
    ) -> Result<Vec<TimelineRow>> {
        let filter = self.prepare(filter)?;
        self.index.query_timeline(&filter, weekly, limit)
    }

    pub fn activity(&mut self, filter: QueryFilter, limit: usize) -> Result<ActivityData> {
        let filter = self.prepare(filter)?;
        self.index.query_activity(&filter, limit)
    }

    pub fn provider_status(
        &mut self,
        filter: QueryFilter,
        deep: bool,
    ) -> Result<Vec<ProviderStatusRow>> {
        let filter = self.prepare(filter)?;
        self.index.provider_status(&self.providers, deep, &filter)
    }

    pub fn retention_stats(&mut self, filter: QueryFilter) -> Result<RetentionStats> {
        let filter = self.prepare(filter)?;
        self.index.retention_stats(&filter)
    }

    pub fn session_detail(&mut self, file_path: &str) -> Result<Option<SessionDetail>> {
        self.ensure_fresh()?;
        self.index.query_session_detail(file_path)
    }

    pub fn session_matches(
        &mut self,
        selector: &str,
        project_filter: Option<&str>,
    ) -> Result<Vec<IndexedSession>> {
        self.ensure_fresh()?;
        self.index.query_session_matches(selector, project_filter)
    }

    pub fn prs(
        &mut self,
        project: Option<&str>,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<PrLinkRow>> {
        let filter = self.prepare(filter)?;
        self.index.ensure_pr_links_fresh(&self.providers)?;
        self.index.query_pr_links(project, &filter, limit)
    }

    pub fn files(
        &mut self,
        project: Option<&str>,
        path: Option<&str>,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<FileModRow>> {
        let filter = self.prepare(filter)?;
        self.index.query_file_mods(project, path, &filter, limit)
    }

    pub fn turns(
        &mut self,
        project: Option<&str>,
        filter: QueryFilter,
        limit: usize,
    ) -> Result<Vec<TurnStatsRow>> {
        let filter = self.prepare(filter)?;
        self.index.query_turn_stats(project, &filter, limit)
    }

    fn prepare(&mut self, filter: QueryFilter) -> Result<ResolvedFilter> {
        self.ensure_fresh()?;
        filter.resolve()
    }
}

fn providers_for(kinds: &[ProviderKind]) -> Result<Vec<ProviderImpl>> {
    if kinds.is_empty() {
        return crate::providers::enabled_default();
    }

    let mut providers = Vec::new();
    for kind in kinds {
        let provider = match kind {
            ProviderKind::Claude => ProviderImpl::Claude(ClaudeProvider::new()?),
            ProviderKind::Codex => ProviderImpl::Codex(CodexProvider::new()?),
            ProviderKind::OpenClaw => ProviderImpl::OpenClaw(OpenClawProvider::new()?),
            ProviderKind::Pi => ProviderImpl::Pi(PiProvider::new()?),
        };
        providers.push(provider);
    }
    Ok(providers)
}

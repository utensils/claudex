//! Shared command-line filtering: the cross-cutting `--provider/--model/
//! --since/--until/--on-disk-only` flags every reporting command accepts,
//! resolved into a [`ResolvedFilter`] that the index queries (and the
//! `--no-index` fallback) apply uniformly.

use std::path::PathBuf;

use anyhow::{Result, bail};
use chrono::{DateTime, Duration, NaiveTime, Utc};
use clap::{Args, Subcommand, ValueEnum};
use rusqlite::types::Value as SqlValue;

use crate::parser::SessionStats;

/// Provider selector accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProviderArg {
    Claude,
    Codex,
    Pi,
}

impl ProviderArg {
    pub fn id(self) -> &'static str {
        match self {
            ProviderArg::Claude => "claude",
            ProviderArg::Codex => "codex",
            ProviderArg::Pi => "pi",
        }
    }
}

/// Cross-cutting filter flags shared by every reporting command. Flattened into
/// each command alongside its own options (`--project`, `--limit`, …).
#[derive(Args, Clone, Debug, Default)]
pub struct FilterArgs {
    /// Restrict to one or more providers (repeatable or comma-separated).
    /// Default: all indexed providers.
    #[arg(long, value_enum, value_delimiter = ',')]
    pub provider: Vec<ProviderArg>,
    /// Only sessions whose model matches this substring (e.g. `opus`, `gpt-5`).
    #[arg(long)]
    pub model: Option<String>,
    /// Only sessions at/after this time — a date (`2026-01-01`), an RFC3339
    /// timestamp, or a relative span (`7d`, `12h`, `2w`).
    #[arg(long, value_parser = validate_when_arg)]
    pub since: Option<String>,
    /// Only sessions at/before this time (same formats as `--since`).
    #[arg(long, value_parser = validate_when_arg)]
    pub until: Option<String>,
    /// Exclude sessions whose source file has been archived or deleted from
    /// disk (retained in the index by default).
    #[arg(long)]
    pub on_disk_only: bool,
}

impl FilterArgs {
    pub fn resolve(&self) -> Result<ResolvedFilter> {
        let mut providers: Vec<String> = self.provider.iter().map(|p| p.id().to_string()).collect();
        providers.sort();
        providers.dedup();
        Ok(ResolvedFilter {
            providers,
            model: self.model.clone(),
            since_ms: self
                .since
                .as_deref()
                .map(|s| parse_when(s, false))
                .transpose()?,
            until_ms: self
                .until
                .as_deref()
                .map(|s| parse_when(s, true))
                .transpose()?,
            on_disk_only: self.on_disk_only,
        })
    }
}

/// A resolved filter: epoch-millisecond bounds and concrete provider ids, ready
/// to be turned into SQL predicates or matched against parsed sessions.
#[derive(Debug, Clone, Default)]
pub struct ResolvedFilter {
    /// Provider ids to include; empty means all.
    pub providers: Vec<String>,
    pub model: Option<String>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub on_disk_only: bool,
}

impl ResolvedFilter {
    /// Whether `provider` is in scope (true when no provider filter is set).
    pub fn includes_provider(&self, provider: &str) -> bool {
        self.providers.is_empty() || self.providers.iter().any(|p| p == provider)
    }

    /// Whether any filter is active (lets callers skip work when unfiltered).
    pub fn is_unfiltered(&self) -> bool {
        self.providers.is_empty()
            && self.model.is_none()
            && self.since_ms.is_none()
            && self.until_ms.is_none()
            && !self.on_disk_only
    }

    /// Build the additional `AND …` SQL predicates for a `sessions` row aliased
    /// `alias`, plus the parameters to bind, in order. Appended after a query's
    /// existing `WHERE`.
    pub fn sql_predicates(&self, alias: &str) -> (String, Vec<SqlValue>) {
        let mut sql = String::new();
        let mut params: Vec<SqlValue> = Vec::new();

        if !self.providers.is_empty() {
            let placeholders = vec!["?"; self.providers.len()].join(", ");
            sql.push_str(&format!(" AND {alias}.provider IN ({placeholders})"));
            params.extend(self.providers.iter().map(|p| SqlValue::Text(p.clone())));
        }
        if let Some(since) = self.since_ms {
            sql.push_str(&format!(" AND {alias}.first_timestamp >= ?"));
            params.push(SqlValue::Integer(since));
        }
        if let Some(until) = self.until_ms {
            sql.push_str(&format!(" AND {alias}.first_timestamp <= ?"));
            params.push(SqlValue::Integer(until));
        }
        if let Some(model) = &self.model {
            // Match against the session's per-model token_usage rows, not the
            // `sessions.model` label — that label is "mixed" for sessions that
            // switched models, which would otherwise drop them from a
            // `--model opus` / `--model gpt-5` filter.
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM token_usage tu WHERE tu.session_id = {alias}.id AND tu.model LIKE ?)"
            ));
            params.push(SqlValue::Text(format!("%{model}%")));
        }
        if self.on_disk_only {
            // Archived transcripts (e.g. Codex's archived_sessions/) still live
            // on disk, so `present_on_disk = 1` alone keeps them; require they
            // are not archived too.
            sql.push_str(&format!(
                " AND {alias}.present_on_disk = 1 AND {alias}.archived_at IS NULL"
            ));
        }
        (sql, params)
    }

    /// Apply the filter to an in-memory [`SessionStats`] for the `--no-index`
    /// fallback. `provider` is the source provider id, `archived` whether the
    /// file is archived/off-disk.
    pub fn matches(&self, provider: &str, stats: &SessionStats, archived: bool) -> bool {
        if !self.providers.is_empty() && !self.providers.iter().any(|p| p == provider) {
            return false;
        }
        if self.on_disk_only && archived {
            return false;
        }
        let first_ms = stats.first_timestamp.map(|d| d.timestamp_millis());
        if let Some(since) = self.since_ms
            && first_ms.is_none_or(|ts| ts < since)
        {
            return false;
        }
        if let Some(until) = self.until_ms
            && first_ms.is_none_or(|ts| ts > until)
        {
            return false;
        }
        if let Some(model) = &self.model {
            let needle = model.to_lowercase();
            let hit = stats
                .model_names()
                .iter()
                .any(|m| m.to_lowercase().contains(&needle));
            if !hit {
                return false;
            }
        }
        true
    }
}

// --- `claudex skills` ---

/// Generate or install the agent skill that describes claudex.
#[derive(Subcommand, Debug)]
pub enum SkillCommand {
    /// Write skill files to a directory for review (default ./claudex-skills)
    #[command(after_long_help = crate::cli_help::SKILLS_GENERATE_EXAMPLES)]
    Generate(SkillArgs),
    /// Write skill files into live harness configuration locations
    #[command(after_long_help = crate::cli_help::SKILLS_INSTALL_EXAMPLES)]
    Install(SkillArgs),
}

/// Options shared by `skills generate` and `skills install`.
#[derive(Args, Debug, Clone)]
pub struct SkillArgs {
    /// Harness target(s) to write for (repeatable or comma-separated).
    #[arg(long, value_enum, value_delimiter = ',', default_value = "all")]
    pub target: Vec<SkillTarget>,
    /// Output root (generate) or base directory override (install).
    #[arg(long)]
    pub dir: Option<PathBuf>,
    /// Install to user-level config (~/) instead of the current project.
    #[arg(long)]
    pub global: bool,
    /// Overwrite existing files.
    #[arg(long)]
    pub force: bool,
    /// Output the summary as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Harness flavor a skill is generated for.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillTarget {
    /// Claude Code — `.claude/skills/claudex/SKILL.md`
    ClaudeCode,
    /// OpenAI Codex — `.agents/skills/claudex/SKILL.md`
    Codex,
    /// Pi — `.pi/skills/claudex/SKILL.md`
    Pi,
    /// Idempotent block spliced into `AGENTS.md`
    AgentsMd,
    /// Claude Code plugin — `.claude-plugin/plugin.json` + skill
    Plugin,
    /// Expand to claude-code + codex + pi + agents-md
    All,
}

/// Parse a `--since`/`--until` value into epoch milliseconds. A bare date
/// resolves to the start of that UTC day for `--since` and the end of the day
/// for `--until`, so an inclusive `--since D --until D` covers all of day D.
fn parse_when(value: &str, end_of_day: bool) -> Result<i64> {
    let v = value.trim();

    // Relative span: <N>[d|h|w|m] where m = minutes.
    if let Some(ms) = parse_relative(v) {
        return Ok(ms);
    }
    // RFC3339 timestamp.
    if let Ok(dt) = DateTime::parse_from_rfc3339(v) {
        return Ok(dt.with_timezone(&Utc).timestamp_millis());
    }
    // Bare date.
    if let Ok(date) = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d") {
        let time = if end_of_day {
            NaiveTime::from_hms_milli_opt(23, 59, 59, 999).unwrap()
        } else {
            NaiveTime::from_hms_opt(0, 0, 0).unwrap()
        };
        return Ok(date.and_time(time).and_utc().timestamp_millis());
    }
    bail!("invalid date/time '{value}' (use YYYY-MM-DD, RFC3339, or a span like 7d/12h/2w)")
}

pub fn validate_when_arg(value: &str) -> std::result::Result<String, String> {
    parse_when(value, false)
        .map(|_| value.to_string())
        .map_err(|e| e.to_string())
}

/// Parse a relative span like `7d`, `12h`, `2w`, `30m` into "now minus span" in
/// epoch milliseconds. Returns `None` if the input isn't a recognised span.
fn parse_relative(v: &str) -> Option<i64> {
    let (num, unit) = v.split_at(v.find(|c: char| c.is_alphabetic())?);
    let n: i64 = num.parse().ok()?;
    let span = match unit {
        "m" | "min" | "mins" => Duration::minutes(n),
        "h" | "hr" | "hrs" => Duration::hours(n),
        "d" | "day" | "days" => Duration::days(n),
        "w" | "wk" | "wks" | "week" | "weeks" => Duration::weeks(n),
        _ => return None,
    };
    Some((Utc::now() - span).timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_date_since_is_start_of_day() {
        let ms = parse_when("2026-01-15", false).unwrap();
        let dt = DateTime::from_timestamp_millis(ms).unwrap();
        assert_eq!(
            dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-01-15T00:00:00"
        );
    }

    #[test]
    fn bare_date_until_is_end_of_day() {
        let ms = parse_when("2026-01-15", true).unwrap();
        let dt = DateTime::from_timestamp_millis(ms).unwrap();
        assert_eq!(dt.format("%Y-%m-%dT%H:%M").to_string(), "2026-01-15T23:59");
    }

    #[test]
    fn rfc3339_is_parsed() {
        let ms = parse_when("2026-01-15T08:30:00Z", false).unwrap();
        let dt = DateTime::from_timestamp_millis(ms).unwrap();
        assert_eq!(dt.format("%H:%M").to_string(), "08:30");
    }

    #[test]
    fn relative_spans_are_in_the_past() {
        let now = Utc::now().timestamp_millis();
        let seven_d = parse_when("7d", false).unwrap();
        assert!(seven_d < now);
        // ~7 days in ms, with generous slack for test execution time.
        let delta = now - seven_d;
        assert!((delta - 7 * 86_400_000).abs() < 60_000);
    }

    #[test]
    fn invalid_value_errors() {
        assert!(parse_when("not-a-date", false).is_err());
    }

    #[test]
    fn predicates_bind_providers_and_dates() {
        let f = ResolvedFilter {
            providers: vec!["codex".into(), "pi".into()],
            since_ms: Some(1000),
            on_disk_only: true,
            ..Default::default()
        };
        let (sql, params) = f.sql_predicates("s");
        assert!(sql.contains("s.provider IN (?, ?)"));
        assert!(sql.contains("s.first_timestamp >= ?"));
        assert!(sql.contains("s.present_on_disk = 1"));
        // 2 providers + 1 since bound = 3 params (on_disk_only is inline).
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn unfiltered_is_detected() {
        assert!(ResolvedFilter::default().is_unfiltered());
        assert!(
            !ResolvedFilter {
                on_disk_only: true,
                ..Default::default()
            }
            .is_unfiltered()
        );
    }
}

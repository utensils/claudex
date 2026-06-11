//! Shared query filters used by the library API and the CLI.

use anyhow::{Result, bail};
use chrono::{DateTime, Duration, NaiveTime, Utc};
use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};

use crate::parser::SessionStats;

/// Provider selector for library callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    Claude,
    Codex,
    Copilot,
    CopilotVscode,
    OpenClaw,
    Pi,
}

impl ProviderKind {
    pub fn id(self) -> &'static str {
        match self {
            ProviderKind::Claude => "claude",
            ProviderKind::Codex => "codex",
            ProviderKind::Copilot => "copilot",
            ProviderKind::CopilotVscode => "copilot-vscode",
            ProviderKind::OpenClaw => "openclaw",
            ProviderKind::Pi => "pi",
        }
    }
}

/// User-facing filter builder for API calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryFilter {
    pub providers: Vec<ProviderKind>,
    pub model: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub on_disk_only: bool,
}

impl QueryFilter {
    pub fn resolve(&self) -> Result<ResolvedFilter> {
        let mut providers: Vec<String> =
            self.providers.iter().map(|p| p.id().to_string()).collect();
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedFilter {
    /// Provider ids to include; empty means all.
    pub providers: Vec<String>,
    pub model: Option<String>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub on_disk_only: bool,
}

impl ResolvedFilter {
    /// The `--no-index` fallback reads Claude Code transcript files directly.
    /// Other providers require the unified index because their on-disk formats
    /// are normalized through provider parsers during sync.
    pub fn ensure_no_index_supported(&self) -> Result<()> {
        if self.providers.iter().any(|p| p != "claude") {
            bail!(
                "--no-index only scans Claude transcripts; remove --no-index to use indexed provider data, or use --provider claude"
            );
        }
        Ok(())
    }

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

    /// Build the additional `AND ...` SQL predicates for a `sessions` row
    /// aliased `alias`, plus the parameters to bind, in order.
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
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM token_usage tu WHERE tu.session_id = {alias}.id AND tu.model LIKE ?)"
            ));
            params.push(SqlValue::Text(format!("%{model}%")));
        }
        if self.on_disk_only {
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

/// Parse a `--since`/`--until` value into epoch milliseconds. A bare date
/// resolves to the start of that UTC day for `--since` and the end of the day
/// for `--until`, so an inclusive `--since D --until D` covers all of day D.
pub fn parse_when(value: &str, end_of_day: bool) -> Result<i64> {
    let v = value.trim();

    if let Some(ms) = parse_relative(v) {
        return Ok(ms);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(v) {
        return Ok(dt.with_timezone(&Utc).timestamp_millis());
    }
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

//! Subscription plan handling for cost reporting.
//!
//! By default, claudex prices every token at Anthropic API rates and emits
//! `total_cost_usd` / `cost_this_week_usd` in JSON output. For users on a flat
//! subscription (Claude Pro, Pro Max, Team flat-fee tiers), the API-rate
//! number is informationally interesting but is not their actual recurring
//! cost.
//!
//! `Plan::FlatMonthly { usd_per_month }` lets the user pass `--plan
//! flat-monthly:250` and get plan-relative reporting: actual flat cost,
//! API-equivalent value (preserved), and a "leverage this week" multiple.
//!
//! Backward compat: under both `Plan::Api` (the default) and
//! `Plan::FlatMonthly`, the historical `total_cost_usd` /
//! `cost_this_week_usd` keys are still emitted, so existing pipelines keep
//! working. Flat-monthly *adds* a `plan` discriminator and the new
//! `actual_monthly_cost_usd`, `api_equivalent_*`, and
//! `leverage_this_week_multiple` keys alongside them.

use std::str::FromStr;

use serde_json::{Map, Value, json};

/// Average days per month (365.25 / 12) divided by 7 days/week. Used to
/// extrapolate "this week" API value to a monthly-cost comparison without
/// the ~8% under-reporting bias of treating a month as exactly 4 weeks.
const WEEKS_PER_MONTH: f64 = 365.25 / 12.0 / 7.0; // ≈ 4.348

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Plan {
    /// Default — emit token-based API-priced costs unchanged.
    #[default]
    Api,
    /// Flat monthly subscription. `usd_per_month` is the user's recurring fee.
    /// Outputs add plan-relative keys alongside the historical API-priced
    /// totals.
    FlatMonthly { usd_per_month: f64 },
}

impl FromStr for Plan {
    type Err = String;

    /// Parse from CLI value. Recognized forms:
    ///   - `api` (default)
    ///   - `flat-monthly:USD` where USD is a positive decimal (e.g. `250` or `19.99`)
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "api" {
            return Ok(Plan::Api);
        }
        if let Some(rest) = s.strip_prefix("flat-monthly:") {
            let usd: f64 = rest
                .parse()
                .map_err(|e| format!("invalid USD value in --plan ({rest:?}): {e}"))?;
            if !usd.is_finite() || usd <= 0.0 {
                return Err(format!("--plan flat-monthly USD must be > 0, got {usd}"));
            }
            return Ok(Plan::FlatMonthly { usd_per_month: usd });
        }
        Err(format!(
            "unknown --plan value {s:?}; expected `api` or `flat-monthly:USD`"
        ))
    }
}

impl Plan {
    /// JSON cost fields for the given plan. Returns a `Map` (not a `Value`)
    /// so callers can `extend` it into a parent object without an
    /// `as_object()` downcast that could silently drop fields on type
    /// mismatch.
    ///
    /// `Plan::Api` returns the historical two-key shape:
    ///   `{ "total_cost_usd": .., "cost_this_week_usd": .. }`
    ///
    /// `Plan::FlatMonthly` keeps the historical keys (so existing pipelines
    /// don't break) and adds:
    ///   `plan`, `actual_monthly_cost_usd`, `api_equivalent_total_usd`,
    ///   `api_equivalent_week_usd`, `leverage_this_week_multiple`.
    pub fn cost_fields(self, api_total: f64, api_week: f64) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("total_cost_usd".into(), json!(api_total));
        m.insert("cost_this_week_usd".into(), json!(api_week));
        if let Plan::FlatMonthly { usd_per_month } = self {
            let weekly_plan_cost = usd_per_month / WEEKS_PER_MONTH;
            let leverage_this_week = if weekly_plan_cost > 0.0 {
                api_week / weekly_plan_cost
            } else {
                0.0
            };
            m.insert("plan".into(), json!("flat-monthly"));
            m.insert("actual_monthly_cost_usd".into(), json!(usd_per_month));
            m.insert("api_equivalent_total_usd".into(), json!(api_total));
            m.insert("api_equivalent_week_usd".into(), json!(api_week));
            m.insert(
                "leverage_this_week_multiple".into(),
                json!(leverage_this_week),
            );
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Parsing ---

    #[test]
    fn parse_api_default() {
        assert_eq!(Plan::from_str("api").unwrap(), Plan::Api);
    }

    #[test]
    fn parse_flat_monthly_integer() {
        assert_eq!(
            Plan::from_str("flat-monthly:250").unwrap(),
            Plan::FlatMonthly {
                usd_per_month: 250.0
            },
        );
    }

    #[test]
    fn parse_flat_monthly_decimal() {
        assert_eq!(
            Plan::from_str("flat-monthly:19.99").unwrap(),
            Plan::FlatMonthly {
                usd_per_month: 19.99
            },
        );
    }

    #[test]
    fn reject_zero_or_negative() {
        assert!(Plan::from_str("flat-monthly:0").is_err());
        assert!(Plan::from_str("flat-monthly:-50").is_err());
    }

    #[test]
    fn reject_garbage() {
        assert!(Plan::from_str("flat-monthly:abc").is_err());
        assert!(Plan::from_str("flat-monthly:").is_err());
        assert!(Plan::from_str("monthly:250").is_err());
        assert!(Plan::from_str("").is_err());
    }

    // --- JSON shape: Plan::Api preserves historical keys ---

    #[test]
    fn cost_fields_api_default_shape() {
        let m = Plan::Api.cost_fields(18188.40, 3136.21);
        assert_eq!(m["total_cost_usd"], 18188.40);
        assert_eq!(m["cost_this_week_usd"], 3136.21);
        // Plan::Api must NOT emit flat-monthly keys (backward compat).
        assert!(m.get("plan").is_none());
        assert!(m.get("actual_monthly_cost_usd").is_none());
        assert!(m.get("leverage_this_week_multiple").is_none());
        assert_eq!(m.len(), 2);
    }

    // --- JSON shape: Plan::FlatMonthly is additive ---

    #[test]
    fn cost_fields_flat_monthly_keeps_historical_keys() {
        // Pipelines that grep `.total_cost_usd` must keep working when a user
        // switches to a flat-monthly plan.
        let plan = Plan::FlatMonthly {
            usd_per_month: 250.0,
        };
        let m = plan.cost_fields(18188.40, 3136.21);
        assert_eq!(m["total_cost_usd"], 18188.40);
        assert_eq!(m["cost_this_week_usd"], 3136.21);
    }

    #[test]
    fn cost_fields_flat_monthly_emits_discriminator_and_aliases() {
        let plan = Plan::FlatMonthly {
            usd_per_month: 250.0,
        };
        let m = plan.cost_fields(18188.40, 3136.21);
        assert_eq!(m["plan"], "flat-monthly");
        assert_eq!(m["actual_monthly_cost_usd"], 250.0);
        assert_eq!(m["api_equivalent_total_usd"], 18188.40);
        assert_eq!(m["api_equivalent_week_usd"], 3136.21);
    }

    #[test]
    fn cost_fields_flat_monthly_leverage_uses_calendar_weeks() {
        // With the calendar-accurate WEEKS_PER_MONTH = 365.25/12/7 ≈ 4.348,
        // weekly_plan_cost(250) ≈ 57.50 and leverage = 3136.21 / 57.50 ≈ 54.54.
        // The naive (week × 4) / monthly used previously would give ≈ 50.18 —
        // an ~8% under-report.
        let plan = Plan::FlatMonthly {
            usd_per_month: 250.0,
        };
        let m = plan.cost_fields(18188.40, 3136.21);
        let lev = m["leverage_this_week_multiple"].as_f64().unwrap();
        let expected = 3136.21 / (250.0 / WEEKS_PER_MONTH);
        assert!((lev - expected).abs() < 1e-9, "got {lev}, want {expected}");
        // Sanity-check the rough magnitude so a future change can't silently
        // drift back to the buggy 4-weeks-per-month math.
        assert!(
            (lev - 54.54).abs() < 0.05,
            "calendar-week leverage should be ~54.54, got {lev}"
        );
    }

    #[test]
    fn cost_fields_flat_monthly_zero_week_yields_zero_leverage() {
        let plan = Plan::FlatMonthly {
            usd_per_month: 250.0,
        };
        let m = plan.cost_fields(18188.40, 0.0);
        assert_eq!(m["leverage_this_week_multiple"], 0.0);
    }
}

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
//! API-equivalent value (preserved), and a leverage multiple.
//!
//! The default `Plan::Api` is wire-compatible with previous claudex versions —
//! same JSON keys, same human-readable output.

use std::str::FromStr;

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Plan {
    /// Default — emit token-based API-priced costs unchanged.
    #[default]
    Api,
    /// Flat monthly subscription. `usd_per_month` is the user's recurring fee.
    /// Outputs reframe API-priced totals as "API-equivalent" + leverage.
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
    /// Returns the JSON cost fields for the given plan.
    ///
    /// For `Plan::Api`, returns the historical two-key shape:
    ///   `{ "total_cost_usd": .., "cost_this_week_usd": .. }`
    ///
    /// For `Plan::FlatMonthly`, returns plan-relative fields:
    ///   `{ "actual_monthly_cost_usd": ..,
    ///      "api_equivalent_total_usd": ..,
    ///      "api_equivalent_week_usd": ..,
    ///      "leverage_total_multiple": ..,
    ///      "leverage_monthly_multiple": .. }`
    pub fn cost_fields(self, api_total: f64, api_week: f64) -> Value {
        match self {
            Plan::Api => json!({
                "total_cost_usd": api_total,
                "cost_this_week_usd": api_week,
            }),
            Plan::FlatMonthly { usd_per_month } => {
                let leverage_total = api_total / usd_per_month;
                let leverage_monthly = (api_week * 4.0) / usd_per_month;
                json!({
                    "actual_monthly_cost_usd": usd_per_month,
                    "api_equivalent_total_usd": api_total,
                    "api_equivalent_week_usd": api_week,
                    "leverage_total_multiple": leverage_total,
                    "leverage_monthly_multiple": leverage_monthly,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn cost_fields_api_default_shape() {
        let v = Plan::Api.cost_fields(18188.40, 3136.21);
        assert_eq!(v["total_cost_usd"], 18188.40);
        assert_eq!(v["cost_this_week_usd"], 3136.21);
        // Plan::Api must NOT emit flat-monthly keys (backward compat).
        assert!(v.get("actual_monthly_cost_usd").is_none());
        assert!(v.get("leverage_total_multiple").is_none());
    }

    #[test]
    fn cost_fields_flat_monthly_shape_and_math() {
        let plan = Plan::FlatMonthly {
            usd_per_month: 250.0,
        };
        let v = plan.cost_fields(18188.40, 3136.21);
        assert_eq!(v["actual_monthly_cost_usd"], 250.0);
        assert_eq!(v["api_equivalent_total_usd"], 18188.40);
        assert_eq!(v["api_equivalent_week_usd"], 3136.21);
        // 18188.40 / 250 = 72.7536
        let lev_total = v["leverage_total_multiple"].as_f64().unwrap();
        assert!((lev_total - 72.7536).abs() < 1e-9, "got {lev_total}");
        // (3136.21 * 4) / 250 = 50.17936
        let lev_month = v["leverage_monthly_multiple"].as_f64().unwrap();
        assert!((lev_month - 50.17936).abs() < 1e-9, "got {lev_month}");
        // Plan::FlatMonthly must NOT emit api-default keys (clean separation).
        assert!(v.get("total_cost_usd").is_none());
        assert!(v.get("cost_this_week_usd").is_none());
    }
}

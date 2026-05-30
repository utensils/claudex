use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

pub struct ModelPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_write_per_mtok: f64,
    pub cache_read_per_mtok: f64,
}

impl ModelPricing {
    /// Per-million-token pricing for a model id. Unknown models fall back to
    /// Claude Sonnet — Pi's local/free models never reach this path because they
    /// carry a provider-supplied cost instead (see `ModelSessionStats::embedded_cost`).
    pub fn for_model(model: Option<&str>) -> Self {
        let m = model.unwrap_or("").to_lowercase();
        if is_claude_opus_latest(&m) {
            Self {
                input_per_mtok: 5.0,
                output_per_mtok: 25.0,
                cache_write_per_mtok: 6.25,
                cache_read_per_mtok: 0.50,
            }
        } else if m.contains("opus") {
            Self {
                input_per_mtok: 15.0,
                output_per_mtok: 75.0,
                cache_write_per_mtok: 18.75,
                cache_read_per_mtok: 1.50,
            }
        } else if is_claude_haiku_latest(&m) {
            Self {
                input_per_mtok: 1.0,
                output_per_mtok: 5.0,
                cache_write_per_mtok: 1.25,
                cache_read_per_mtok: 0.10,
            }
        } else if m.contains("haiku") {
            Self {
                input_per_mtok: 0.80,
                output_per_mtok: 4.0,
                cache_write_per_mtok: 1.00,
                cache_read_per_mtok: 0.08,
            }
        } else if has_any(&m, &["gpt-5.5-pro"]) {
            openai_pricing(30.0, 30.0, 180.0)
        } else if has_any(&m, &["gpt-5.5"]) {
            openai_pricing(5.0, 0.50, 30.0)
        } else if has_any(&m, &["gpt-5.4-pro"]) {
            openai_pricing(30.0, 30.0, 180.0)
        } else if has_any(&m, &["gpt-5.4-mini"]) {
            openai_pricing(0.75, 0.075, 4.50)
        } else if has_any(&m, &["gpt-5.4-nano"]) {
            openai_pricing(0.20, 0.02, 1.25)
        } else if has_any(&m, &["gpt-5.4"]) {
            openai_pricing(2.50, 0.25, 15.0)
        } else if has_any(&m, &["gpt-5.3-codex", "gpt-5.2-codex", "gpt-5.2"]) {
            openai_pricing(1.75, 0.175, 14.0)
        } else if has_any(&m, &["gpt-5-pro"]) {
            openai_pricing(15.0, 15.0, 120.0)
        } else if is_gpt5(&m) {
            Self {
                input_per_mtok: 1.25,
                output_per_mtok: 10.0,
                cache_write_per_mtok: 1.25,
                cache_read_per_mtok: 0.125,
            }
        } else if has_any(&m, &["gpt-4.1-mini"]) {
            openai_pricing(0.40, 0.10, 1.60)
        } else if has_any(&m, &["gpt-4.1-nano"]) {
            openai_pricing(0.10, 0.025, 0.40)
        } else if has_any(&m, &["gpt-4.1"]) {
            openai_pricing(2.0, 0.50, 8.0)
        } else if has_any(&m, &["gpt-4o-mini"]) {
            openai_pricing(0.15, 0.075, 0.60)
        } else if has_any(&m, &["gpt-4o-2024-05-13"]) {
            openai_pricing(5.0, 5.0, 15.0)
        } else if is_gpt4(&m) {
            openai_pricing(2.50, 1.25, 10.0)
        } else {
            // Claude Sonnet — also the default for an unspecified model.
            Self {
                input_per_mtok: 3.0,
                output_per_mtok: 15.0,
                cache_write_per_mtok: 3.75,
                cache_read_per_mtok: 0.30,
            }
        }
    }

    /// Short display label for a model's family.
    pub fn name(model: Option<&str>) -> &'static str {
        let m = model.unwrap_or("").to_lowercase();
        if m.contains("opus") {
            "Opus"
        } else if m.contains("haiku") {
            "Haiku"
        } else if is_gpt5(&m) {
            "GPT-5"
        } else if is_gpt4(&m) {
            "GPT-4"
        } else {
            "Sonnet"
        }
    }
}

fn is_gpt5(m: &str) -> bool {
    m.contains("gpt-5") || m.contains("gpt5")
}

fn is_gpt4(m: &str) -> bool {
    m.contains("gpt-4") || m.contains("gpt4")
}

fn is_claude_opus_latest(m: &str) -> bool {
    has_any(
        m,
        &[
            "opus-4-5", "opus-4.5", "opus-4-6", "opus-4.6", "opus-4-7", "opus-4.7", "opus-4-8",
            "opus-4.8",
        ],
    )
}

fn is_claude_haiku_latest(m: &str) -> bool {
    has_any(m, &["haiku-4-5", "haiku-4.5"])
}

fn has_any(m: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| m.contains(needle))
}

fn openai_pricing(input: f64, cached_input: f64, output: f64) -> ModelPricing {
    ModelPricing {
        input_per_mtok: input,
        output_per_mtok: output,
        cache_write_per_mtok: input,
        cache_read_per_mtok: cached_input,
    }
}

impl TokenUsage {
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
    }

    /// Model-aware cost in USD.
    pub fn cost_for_model(&self, model: Option<&str>) -> f64 {
        let p = ModelPricing::for_model(model);
        (self.input_tokens as f64 * p.input_per_mtok
            + self.cache_creation_tokens as f64 * p.cache_write_per_mtok
            + self.output_tokens as f64 * p.output_per_mtok
            + self.cache_read_tokens as f64 * p.cache_read_per_mtok)
            / 1_000_000.0
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub project: String,
    pub session_id: String,
    pub file_path: Option<String>,
    pub date: Option<DateTime<Utc>>,
    pub message_count: usize,
    pub duration_ms: u64,
    pub model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Zero / default ---

    #[test]
    fn cost_zero_usage() {
        let u = TokenUsage::default();
        assert_eq!(u.cost_for_model(None), 0.0);
        assert_eq!(u.cost_for_model(Some("claude-opus-4-6")), 0.0);
        assert_eq!(u.cost_for_model(Some("claude-haiku-4-5")), 0.0);
        assert_eq!(u.total_tokens(), 0);
    }

    // --- Sonnet pricing (default fallback) ---

    #[test]
    fn sonnet_input_1m() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            ..Default::default()
        };
        // $3/MTok
        assert!((u.cost_for_model(None) - 3.0).abs() < 0.0001);
    }

    #[test]
    fn sonnet_output_1m() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        // $15/MTok
        assert!((u.cost_for_model(None) - 15.0).abs() < 0.0001);
    }

    #[test]
    fn sonnet_cache_write_1m() {
        let u = TokenUsage {
            cache_creation_tokens: 1_000_000,
            ..Default::default()
        };
        // $3.75/MTok
        assert!((u.cost_for_model(None) - 3.75).abs() < 0.0001);
    }

    #[test]
    fn sonnet_cache_read_1m() {
        let u = TokenUsage {
            cache_read_tokens: 1_000_000,
            ..Default::default()
        };
        // $0.30/MTok
        assert!((u.cost_for_model(None) - 0.30).abs() < 0.0001);
    }

    #[test]
    fn sonnet_all_token_types() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
        };
        // $3 + $15 + $3.75 + $0.30 = $22.05
        assert!((u.cost_for_model(Some("claude-sonnet-4-6")) - 22.05).abs() < 0.0001);
    }

    // --- Opus pricing ---

    #[test]
    fn opus_input_1m() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            ..Default::default()
        };
        // $5/MTok
        assert!((u.cost_for_model(Some("claude-opus-4-6")) - 5.0).abs() < 0.0001);
    }

    #[test]
    fn opus_output_1m() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        // $25/MTok
        assert!((u.cost_for_model(Some("claude-opus-4-6")) - 25.0).abs() < 0.0001);
    }

    #[test]
    fn opus_cache_write_1m() {
        let u = TokenUsage {
            cache_creation_tokens: 1_000_000,
            ..Default::default()
        };
        // $6.25/MTok
        assert!((u.cost_for_model(Some("claude-opus-4-6")) - 6.25).abs() < 0.0001);
    }

    #[test]
    fn opus_cache_read_1m() {
        let u = TokenUsage {
            cache_read_tokens: 1_000_000,
            ..Default::default()
        };
        // $0.50/MTok
        assert!((u.cost_for_model(Some("claude-opus-4-6")) - 0.50).abs() < 0.0001);
    }

    #[test]
    fn opus_all_token_types() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
        };
        // $5 + $25 + $6.25 + $0.50 = $36.75
        assert!((u.cost_for_model(Some("claude-opus-4-7")) - 36.75).abs() < 0.0001);
    }

    // --- Haiku pricing ---

    #[test]
    fn haiku_input_1m() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            ..Default::default()
        };
        // $1/MTok
        assert!((u.cost_for_model(Some("claude-haiku-4-5-20251001")) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn haiku_output_1m() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        // $5/MTok
        assert!((u.cost_for_model(Some("claude-haiku-4-5-20251001")) - 5.0).abs() < 0.0001);
    }

    #[test]
    fn haiku_cache_write_1m() {
        let u = TokenUsage {
            cache_creation_tokens: 1_000_000,
            ..Default::default()
        };
        // $1.25/MTok
        assert!((u.cost_for_model(Some("claude-haiku-4-5")) - 1.25).abs() < 0.0001);
    }

    #[test]
    fn haiku_cache_read_1m() {
        let u = TokenUsage {
            cache_read_tokens: 1_000_000,
            ..Default::default()
        };
        // $0.10/MTok
        assert!((u.cost_for_model(Some("claude-haiku-4-5")) - 0.10).abs() < 0.0001);
    }

    #[test]
    fn haiku_all_token_types() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
        };
        // $1.00 + $5.00 + $1.25 + $0.10 = $7.35
        assert!((u.cost_for_model(Some("claude-haiku-4-5")) - 7.35).abs() < 0.0001);
    }

    // --- OpenAI GPT pricing (Codex) ---

    #[test]
    fn gpt5_all_token_types() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
        };
        // $1.25 + $10.00 + $1.25 + $0.125 = $12.625
        assert!((u.cost_for_model(Some("gpt-5")) - 12.625).abs() < 0.0001);
        assert!((u.cost_for_model(Some("gpt-5-codex")) - 12.625).abs() < 0.0001);
        assert!((u.cost_for_model(Some("gpt-5.5")) - 40.5).abs() < 0.0001);
    }

    #[test]
    fn gpt4_all_token_types() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
        };
        // $2.50 + $10.00 + $2.50 + $1.25 = $16.25
        assert!((u.cost_for_model(Some("gpt-4o")) - 16.25).abs() < 0.0001);
    }

    #[test]
    fn gpt_models_are_not_priced_as_sonnet() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        // Sonnet output is $15/MTok; GPT-5 is $10/MTok — they must differ.
        let gpt = u.cost_for_model(Some("gpt-5-codex"));
        let sonnet = u.cost_for_model(Some("claude-sonnet-4-6"));
        assert!(
            (gpt - 10.0).abs() < 0.0001,
            "gpt-5 output should be $10/MTok"
        );
        assert!(
            gpt < sonnet,
            "gpt-5 ({gpt}) must not be priced as sonnet ({sonnet})"
        );
    }

    #[test]
    fn gpt_name_labels() {
        assert_eq!(ModelPricing::name(Some("gpt-5.5")), "GPT-5");
        assert_eq!(ModelPricing::name(Some("gpt-5-codex")), "GPT-5");
        assert_eq!(ModelPricing::name(Some("gpt-4o")), "GPT-4");
    }

    // --- Cross-model ordering ---

    #[test]
    fn cost_ordering_all_token_types() {
        let u = TokenUsage {
            input_tokens: 500_000,
            output_tokens: 500_000,
            cache_creation_tokens: 500_000,
            cache_read_tokens: 500_000,
        };
        let opus = u.cost_for_model(Some("claude-opus-4-6"));
        let sonnet = u.cost_for_model(Some("claude-sonnet-4-6"));
        let haiku = u.cost_for_model(Some("claude-haiku-4-5"));
        assert!(opus > sonnet, "opus ({opus}) should > sonnet ({sonnet})");
        assert!(sonnet > haiku, "sonnet ({sonnet}) should > haiku ({haiku})");
    }

    // --- Realistic scenario: cache-heavy Opus session ---

    #[test]
    fn realistic_opus_session_cost() {
        let u = TokenUsage {
            input_tokens: 5_000,
            output_tokens: 100_000,
            cache_creation_tokens: 50_000,
            cache_read_tokens: 500_000_000, // 500M cache reads
        };
        let cost = u.cost_for_model(Some("claude-opus-4-6"));
        // input:  5K * 5 / 1M = $0.025
        // output: 100K * 25 / 1M = $2.50
        // cache_w: 50K * 6.25 / 1M = $0.3125
        // cache_r: 500M * 0.50 / 1M = $250.00
        // total: $252.8375
        assert!((cost - 252.8375).abs() < 0.001, "got {cost}");
    }

    // --- add() ---

    #[test]
    fn add_all_fields() {
        let mut a = TokenUsage {
            input_tokens: 100,
            output_tokens: 200,
            cache_creation_tokens: 300,
            cache_read_tokens: 400,
        };
        let b = TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
            cache_creation_tokens: 30,
            cache_read_tokens: 40,
        };
        a.add(&b);
        assert_eq!(a.input_tokens, 110);
        assert_eq!(a.output_tokens, 220);
        assert_eq!(a.cache_creation_tokens, 330);
        assert_eq!(a.cache_read_tokens, 440);
    }

    #[test]
    fn add_preserves_cost_linearity() {
        let a = TokenUsage {
            output_tokens: 500_000,
            ..Default::default()
        };
        let b = TokenUsage {
            output_tokens: 500_000,
            ..Default::default()
        };
        let mut combined = a.clone();
        combined.add(&b);
        let separate = a.cost_for_model(None) + b.cost_for_model(None);
        assert!((combined.cost_for_model(None) - separate).abs() < 0.0001);
    }

    // --- total_tokens() ---

    #[test]
    fn total_tokens_sums_all() {
        let u = TokenUsage {
            input_tokens: 1,
            output_tokens: 2,
            cache_creation_tokens: 3,
            cache_read_tokens: 4,
        };
        assert_eq!(u.total_tokens(), 10);
    }

    // --- Model name detection ---

    #[test]
    fn model_pricing_name() {
        assert_eq!(ModelPricing::name(Some("claude-opus-4-7")), "Opus");
        assert_eq!(ModelPricing::name(Some("claude-opus-4-6")), "Opus");
        assert_eq!(ModelPricing::name(Some("claude-haiku-4-5")), "Haiku");
        assert_eq!(
            ModelPricing::name(Some("claude-haiku-4-5-20251001")),
            "Haiku"
        );
        assert_eq!(ModelPricing::name(Some("claude-sonnet-4-6")), "Sonnet");
        assert_eq!(ModelPricing::name(None), "Sonnet");
        assert_eq!(ModelPricing::name(Some("")), "Sonnet");
        assert_eq!(ModelPricing::name(Some("<synthetic>")), "Sonnet");
        assert_eq!(ModelPricing::name(Some("unknown-model")), "Sonnet");
    }

    // --- Pricing constants verification ---

    #[test]
    fn pricing_constants_opus() {
        let p = ModelPricing::for_model(Some("claude-opus-4-6"));
        assert_eq!(p.input_per_mtok, 5.0);
        assert_eq!(p.output_per_mtok, 25.0);
        assert_eq!(p.cache_write_per_mtok, 6.25);
        assert_eq!(p.cache_read_per_mtok, 0.50);
    }

    #[test]
    fn pricing_constants_sonnet() {
        let p = ModelPricing::for_model(Some("claude-sonnet-4-6"));
        assert_eq!(p.input_per_mtok, 3.0);
        assert_eq!(p.output_per_mtok, 15.0);
        assert_eq!(p.cache_write_per_mtok, 3.75);
        assert_eq!(p.cache_read_per_mtok, 0.30);
    }

    #[test]
    fn pricing_constants_haiku() {
        let p = ModelPricing::for_model(Some("claude-haiku-4-5"));
        assert_eq!(p.input_per_mtok, 1.0);
        assert_eq!(p.output_per_mtok, 5.0);
        assert_eq!(p.cache_write_per_mtok, 1.25);
        assert_eq!(p.cache_read_per_mtok, 0.10);
    }

    #[test]
    fn pricing_fallback_is_sonnet() {
        let default = ModelPricing::for_model(None);
        let sonnet = ModelPricing::for_model(Some("claude-sonnet-4-6"));
        assert_eq!(default.input_per_mtok, sonnet.input_per_mtok);
        assert_eq!(default.output_per_mtok, sonnet.output_per_mtok);
        assert_eq!(default.cache_write_per_mtok, sonnet.cache_write_per_mtok);
        assert_eq!(default.cache_read_per_mtok, sonnet.cache_read_per_mtok);
    }

    // --- Opus:Sonnet ratio verification ---

    #[test]
    fn opus_is_5x_sonnet_input() {
        let p_opus = ModelPricing::for_model(Some("opus"));
        let p_sonnet = ModelPricing::for_model(Some("sonnet"));
        assert!((p_opus.input_per_mtok / p_sonnet.input_per_mtok - 5.0).abs() < 0.001);
    }

    #[test]
    fn opus_is_5x_sonnet_output() {
        let p_opus = ModelPricing::for_model(Some("opus"));
        let p_sonnet = ModelPricing::for_model(Some("sonnet"));
        assert!((p_opus.output_per_mtok / p_sonnet.output_per_mtok - 5.0).abs() < 0.001);
    }

    #[test]
    fn opus_is_5x_sonnet_cache_read() {
        let p_opus = ModelPricing::for_model(Some("opus"));
        let p_sonnet = ModelPricing::for_model(Some("sonnet"));
        assert!((p_opus.cache_read_per_mtok / p_sonnet.cache_read_per_mtok - 5.0).abs() < 0.001);
    }
}

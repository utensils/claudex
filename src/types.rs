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
    pub fn for_model(model: Option<&str>) -> Self {
        let m = model.unwrap_or("").to_lowercase();
        if m.contains("opus") {
            Self {
                input_per_mtok: 15.0,
                output_per_mtok: 75.0,
                cache_write_per_mtok: 18.75,
                cache_read_per_mtok: 1.50,
            }
        } else if m.contains("haiku") {
            Self {
                input_per_mtok: 0.80,
                output_per_mtok: 4.0,
                cache_write_per_mtok: 1.00,
                cache_read_per_mtok: 0.08,
            }
        } else {
            // Sonnet (default)
            Self {
                input_per_mtok: 3.0,
                output_per_mtok: 15.0,
                cache_write_per_mtok: 3.75,
                cache_read_per_mtok: 0.30,
            }
        }
    }

    pub fn name(model: Option<&str>) -> &'static str {
        let m = model.unwrap_or("").to_lowercase();
        if m.contains("opus") {
            "Opus"
        } else if m.contains("haiku") {
            "Haiku"
        } else {
            "Sonnet"
        }
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
    pub date: Option<DateTime<Utc>>,
    pub message_count: usize,
    pub duration_ms: u64,
    pub model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_zero_usage() {
        let u = TokenUsage::default();
        assert_eq!(u.cost_for_model(None), 0.0);
        assert_eq!(u.total_tokens(), 0);
    }

    #[test]
    fn cost_one_million_output() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        assert!((u.cost_for_model(None) - 15.0).abs() < 0.001);
    }

    #[test]
    fn cost_add() {
        let mut a = TokenUsage {
            input_tokens: 100,
            ..Default::default()
        };
        let b = TokenUsage {
            input_tokens: 200,
            output_tokens: 50,
            ..Default::default()
        };
        a.add(&b);
        assert_eq!(a.input_tokens, 300);
        assert_eq!(a.output_tokens, 50);
    }

    #[test]
    fn cost_opus_higher_than_sonnet() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        assert!(u.cost_for_model(Some("claude-opus-4")) > u.cost_for_model(Some("claude-sonnet-4")));
    }

    #[test]
    fn cost_haiku_lower_than_sonnet() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        assert!(u.cost_for_model(Some("claude-haiku-4")) < u.cost_for_model(Some("claude-sonnet-4")));
    }

    #[test]
    fn model_pricing_name() {
        assert_eq!(ModelPricing::name(Some("claude-opus-4-7")), "Opus");
        assert_eq!(ModelPricing::name(Some("claude-haiku-4-5")), "Haiku");
        assert_eq!(ModelPricing::name(Some("claude-sonnet-4-6")), "Sonnet");
        assert_eq!(ModelPricing::name(None), "Sonnet");
    }
}

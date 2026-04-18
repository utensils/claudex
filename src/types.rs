use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

impl TokenUsage {
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
    }

    /// Cost in USD with model-aware pricing.
    /// Sonnet: $3/$15/$0.30 per MTok (input/output/cache-read)
    /// Opus:   $15/$75/$1.50 per MTok
    /// Haiku:  $0.80/$4/$0.08 per MTok
    pub fn cost_for_model(&self, model: Option<&str>) -> f64 {
        let (input, output, cache_write, cache_read) = pricing_rates(model);
        (self.input_tokens as f64 * input
            + self.cache_creation_tokens as f64 * cache_write
            + self.output_tokens as f64 * output
            + self.cache_read_tokens as f64 * cache_read)
            / 1_000_000.0
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }
}

/// Returns (input, output, cache_write, cache_read) rates per MTok.
pub fn pricing_rates(model: Option<&str>) -> (f64, f64, f64, f64) {
    match model.unwrap_or("") {
        m if m.contains("opus") => (15.0, 75.0, 18.75, 1.50),
        m if m.contains("haiku") => (0.80, 4.0, 1.0, 0.08),
        _ => (3.0, 15.0, 3.75, 0.30),
    }
}

/// Human-readable model family label.
pub fn model_label(model: Option<&str>) -> &'static str {
    match model.unwrap_or("") {
        m if m.contains("opus") => "opus",
        m if m.contains("haiku") => "haiku",
        m if m.contains("sonnet") => "sonnet",
        _ => "unknown",
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
}

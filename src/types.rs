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

    /// Approximate cost in USD using Sonnet pricing:
    /// $3/MTok input, $3.75/MTok cache-write, $15/MTok output, $0.30/MTok cache-read
    pub fn approx_cost_usd(&self) -> f64 {
        (self.input_tokens as f64 * 3.0
            + self.cache_creation_tokens as f64 * 3.75
            + self.output_tokens as f64 * 15.0
            + self.cache_read_tokens as f64 * 0.30)
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
        assert_eq!(u.approx_cost_usd(), 0.0);
        assert_eq!(u.total_tokens(), 0);
    }

    #[test]
    fn cost_one_million_output() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        assert!((u.approx_cost_usd() - 15.0).abs() < 0.001);
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

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
    /// Per-million-token pricing for a model id.
    ///
    /// Missing/empty model ids preserve the historical Claude Sonnet fallback
    /// for old Claude transcripts that did not record a model. Non-empty model
    /// ids are only priced when we recognize a priced family. Unknown, local,
    /// and open-weight models are treated as free unless the provider supplied
    /// an embedded cost (see `ModelSessionStats::embedded_cost`). This avoids
    /// fabricating Sonnet charges for Ollama/MLX/vLLM/OpenRouter-style models.
    pub fn for_model(model: Option<&str>) -> Self {
        let m = model.unwrap_or("").to_lowercase();
        if m.trim().is_empty() {
            sonnet_pricing()
        } else if is_claude_fable_tier(&m) {
            // Claude Fable 5 / Mythos 5 — the frontier tier above Opus
            // (GA June 9, 2026): $10/$50, standard 1.25x/0.1x cache multipliers.
            Self {
                input_per_mtok: 10.0,
                output_per_mtok: 50.0,
                cache_write_per_mtok: 12.50,
                cache_read_per_mtok: 1.00,
            }
        } else if is_claude_opus_fast(&m) {
            // Fast mode (research preview) carries premium rates over standard
            // Opus: $30/$150 on Opus 4.6/4.7, $10/$50 on Opus 4.8. The cache
            // multipliers (1.25x write / 0.1x read) stack on the fast input rate.
            if has_any(&m, &["opus-4-8", "opus-4.8"]) {
                Self {
                    input_per_mtok: 10.0,
                    output_per_mtok: 50.0,
                    cache_write_per_mtok: 12.50,
                    cache_read_per_mtok: 1.00,
                }
            } else {
                Self {
                    input_per_mtok: 30.0,
                    output_per_mtok: 150.0,
                    cache_write_per_mtok: 37.50,
                    cache_read_per_mtok: 3.00,
                }
            }
        } else if is_claude_opus_latest(&m) {
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
        } else if is_claude_haiku_3(&m) {
            // Claude 3 Haiku — the cheapest legacy Haiku tier ($0.25/$1.25).
            Self {
                input_per_mtok: 0.25,
                output_per_mtok: 1.25,
                cache_write_per_mtok: 0.3125,
                cache_read_per_mtok: 0.025,
            }
        } else if m.contains("haiku") {
            // Claude 3.5 Haiku (revised Dec 2024) and any other legacy Haiku.
            Self {
                input_per_mtok: 0.80,
                output_per_mtok: 4.0,
                cache_write_per_mtok: 1.00,
                cache_read_per_mtok: 0.08,
            }
        } else if m.contains("sonnet") {
            sonnet_pricing()
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
        } else if has_any(&m, &["gpt-4.5"]) {
            // GPT-4.5 preview ("Orion") — premium research-preview pricing.
            openai_pricing(75.0, 37.50, 150.0)
        } else if has_any(&m, &["gpt-4o-mini"]) {
            openai_pricing(0.15, 0.075, 0.60)
        } else if has_any(&m, &["gpt-4o-2024-05-13"]) {
            openai_pricing(5.0, 5.0, 15.0)
        } else if has_any(&m, &["gpt-4-turbo", "gpt-4-1106", "gpt-4-0125"]) {
            // GPT-4 Turbo — predates prompt caching, so cache rates = input.
            openai_pricing(10.0, 10.0, 30.0)
        } else if has_any(&m, &["gpt-4-32k"]) {
            openai_pricing(60.0, 60.0, 120.0)
        } else if is_gpt4_classic(&m) {
            // Original GPT-4 (8k) — far pricier than GPT-4o / GPT-4.1.
            openai_pricing(30.0, 30.0, 60.0)
        } else if is_gpt4(&m) {
            // GPT-4o (base) and anything else in the gpt-4 family.
            openai_pricing(2.50, 1.25, 10.0)
        } else {
            free_pricing()
        }
    }

    /// Short display label for a model's family.
    pub fn name(model: Option<&str>) -> &'static str {
        let m = model.unwrap_or("").to_lowercase();
        if m.trim().is_empty() {
            "Sonnet"
        } else if m.contains("fable") {
            "Fable"
        } else if m.contains("mythos") {
            "Mythos"
        } else if m.contains("opus") {
            "Opus"
        } else if m.contains("haiku") {
            "Haiku"
        } else if m.contains("sonnet") {
            "Sonnet"
        } else if has_any(&m, &["gpt-oss", "gpt_oss", "gptoss"]) {
            "GPT-OSS"
        } else if is_gpt5(&m) {
            "GPT-5"
        } else if is_gpt4(&m) {
            "GPT-4"
        } else if has_any(&m, &["qwen", "qwq"]) {
            "Qwen"
        } else if has_any(&m, &["deepseek"]) {
            "DeepSeek"
        } else if has_any(&m, &["gemma"]) {
            "Gemma"
        } else if has_any(&m, &["llama", "codellama"]) {
            "Llama"
        } else if has_any(
            &m,
            &[
                "mistral",
                "mixtral",
                "codestral",
                "devstral",
                "ministral",
                "magistral",
            ],
        ) {
            "Mistral"
        } else if has_any(&m, &["phi-", "phi_", "phi3", "phi4"]) {
            "Phi"
        } else if has_any(&m, &["chatglm", "glm-"]) {
            "GLM"
        } else if has_any(&m, &["granite"]) {
            "Granite"
        } else if has_any(&m, &["falcon"]) {
            "Falcon"
        } else if has_any(&m, &["olmo"]) {
            "OLMo"
        } else if has_any(&m, &["bloom"]) {
            "BLOOM"
        } else if has_any(&m, &["starcoder"]) {
            "StarCoder"
        } else if has_any(&m, &["gemini"]) {
            "Gemini"
        } else if has_any(&m, &["grok"]) {
            "Grok"
        } else if has_any(&m, &["command-r", "command_r", "command r", "cohere"]) {
            "Command R"
        } else if has_any(&m, &["nemotron"]) {
            "Nemotron"
        } else if has_any(&m, &["kimi", "moonshot"]) {
            "Kimi"
        } else if has_any(&m, &["ernie"]) {
            "ERNIE"
        } else if has_any(&m, &["hunyuan"]) {
            "Hunyuan"
        } else if has_any(&m, &["internlm"]) {
            "InternLM"
        } else if has_any(&m, &["baichuan"]) {
            "Baichuan"
        } else if has_any(&m, &["minimax"]) {
            "MiniMax"
        } else if has_any(&m, &["doubao", "seed"]) {
            "Seed"
        } else if has_any(&m, &["yi-"]) || m == "yi" {
            "Yi"
        } else if has_any(&m, &["nova"]) {
            "Nova"
        } else if has_any(&m, &["titan"]) {
            "Titan"
        } else if has_any(&m, &["sonar", "perplexity"]) {
            "Sonar"
        } else if has_any(&m, &["solar"]) {
            "SOLAR"
        } else if has_any(&m, &["dbrx"]) {
            "DBRX"
        } else if has_any(&m, &["jamba"]) {
            "Jamba"
        } else if has_any(&m, &["llava"]) {
            "LLaVA"
        } else {
            "Other"
        }
    }
}

fn is_gpt5(m: &str) -> bool {
    m.contains("gpt-5") || m.contains("gpt5")
}

fn is_gpt4(m: &str) -> bool {
    m.contains("gpt-4") || m.contains("gpt4")
}

fn is_claude_opus_fast(m: &str) -> bool {
    // Fast-mode Opus ids (`claude-opus-4-6-fast`, `claude-opus-4-8-fast`).
    // Fast mode only exists on Opus 4.6+, so anchor on the modern-Opus match
    // rather than `fast` alone to avoid false positives.
    is_claude_opus_latest(m) && m.contains("fast")
}

fn is_claude_fable_tier(m: &str) -> bool {
    // Claude Fable 5 (`claude-fable-5`) and the limited-availability Claude
    // Mythos 5 / Mythos Preview share the same published $10/$50 rate card.
    has_any(m, &["fable", "mythos"])
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

fn is_claude_haiku_3(m: &str) -> bool {
    // Claude 3 Haiku ids look like `claude-3-haiku-20240307`. Exclude the 3.5
    // Haiku ids (`claude-3-5-haiku-*`), which keep the legacy $0.80/$4 rate.
    m.contains("3-haiku") && !m.contains("3-5-haiku")
}

fn is_gpt4_classic(m: &str) -> bool {
    // Original GPT-4 family (gpt-4, gpt-4-0613, gpt-4-8k) — but NOT the far
    // cheaper gpt-4o, the gpt-4.1 family, or the premium gpt-4.5 preview, each
    // of which carries its own tier.
    (m.contains("gpt-4") || m.contains("gpt4"))
        && !m.contains("gpt-4o")
        && !m.contains("gpt4o")
        && !m.contains("gpt-4.1")
        && !m.contains("gpt4.1")
        && !m.contains("gpt-4.5")
        && !m.contains("gpt4.5")
}

fn has_any(m: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| m.contains(needle))
}

fn sonnet_pricing() -> ModelPricing {
    ModelPricing {
        input_per_mtok: 3.0,
        output_per_mtok: 15.0,
        cache_write_per_mtok: 3.75,
        cache_read_per_mtok: 0.30,
    }
}

fn free_pricing() -> ModelPricing {
    ModelPricing {
        input_per_mtok: 0.0,
        output_per_mtok: 0.0,
        cache_write_per_mtok: 0.0,
        cache_read_per_mtok: 0.0,
    }
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

    // --- Fable pricing (frontier tier) ---

    #[test]
    fn fable_all_token_types() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
        };
        // $10 + $50 + $12.50 + $1.00 = $73.50
        assert!((u.cost_for_model(Some("claude-fable-5")) - 73.50).abs() < 0.0001);
    }

    #[test]
    fn mythos_priced_as_fable_tier() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            ..Default::default()
        };
        // Claude Mythos 5 / Mythos Preview share Fable's $10/MTok input rate.
        assert!((u.cost_for_model(Some("claude-mythos-5")) - 10.0).abs() < 0.0001);
        assert!((u.cost_for_model(Some("claude-mythos-preview")) - 10.0).abs() < 0.0001);
    }

    #[test]
    fn fable_is_not_free_or_sonnet() {
        let u = TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        };
        let fable = u.cost_for_model(Some("claude-fable-5"));
        assert!((fable - 50.0).abs() < 0.0001, "fable output is $50/MTok");
        assert!(
            fable > u.cost_for_model(Some("claude-sonnet-4-6")),
            "fable must not fall through to the Sonnet or free tier"
        );
    }

    // --- Opus fast mode (premium rates) ---

    #[test]
    fn opus_fast_mode_is_premium() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        // Opus 4.6 / 4.7 fast: $30 + $150 = $180
        assert!((u.cost_for_model(Some("claude-opus-4-6-fast")) - 180.0).abs() < 0.0001);
        assert!((u.cost_for_model(Some("claude-opus-4-7-fast")) - 180.0).abs() < 0.0001);
        // Opus 4.8 fast: $10 + $50 = $60
        assert!((u.cost_for_model(Some("claude-opus-4-8-fast")) - 60.0).abs() < 0.0001);
        // Standard Opus ids must stay on the $5/$25 card.
        assert!((u.cost_for_model(Some("claude-opus-4-8")) - 30.0).abs() < 0.0001);
        // Fast ids still display as the Opus family.
        assert_eq!(ModelPricing::name(Some("claude-opus-4-6-fast")), "Opus");
    }

    #[test]
    fn opus_fast_cache_rates_stack_on_fast_input() {
        let p = ModelPricing::for_model(Some("claude-opus-4-6-fast"));
        assert_eq!(p.cache_write_per_mtok, 37.50); // 1.25x of $30
        assert_eq!(p.cache_read_per_mtok, 3.00); // 0.1x of $30
        let p8 = ModelPricing::for_model(Some("claude-opus-4-8-fast"));
        assert_eq!(p8.cache_write_per_mtok, 12.50);
        assert_eq!(p8.cache_read_per_mtok, 1.00);
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
        let fable = u.cost_for_model(Some("claude-fable-5"));
        let opus = u.cost_for_model(Some("claude-opus-4-6"));
        let sonnet = u.cost_for_model(Some("claude-sonnet-4-6"));
        let haiku = u.cost_for_model(Some("claude-haiku-4-5"));
        assert!(fable > opus, "fable ({fable}) should > opus ({opus})");
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
        assert_eq!(ModelPricing::name(Some("claude-fable-5")), "Fable");
        assert_eq!(ModelPricing::name(Some("claude-mythos-5")), "Mythos");
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
        assert_eq!(ModelPricing::name(Some("<synthetic>")), "Other");
        assert_eq!(ModelPricing::name(Some("unknown-model")), "Other");
    }

    #[test]
    fn local_and_open_weight_models_are_not_sonnet() {
        let u = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };

        for (model, family) in [
            ("qwen3.6-35b-a3b-ud-mlx", "Qwen"),
            ("ollama/qwen3.6:35b", "Qwen"),
            ("ollama/gemma4:31b", "Gemma"),
            ("llama3.3:70b", "Llama"),
            ("deepseek-r1:70b", "DeepSeek"),
            ("mistral-small:24b", "Mistral"),
            ("mixtral:8x22b", "Mistral"),
            ("phi4:14b", "Phi"),
            ("glm-5.1", "GLM"),
            ("granite-code:34b", "Granite"),
            ("falcon3:10b", "Falcon"),
            ("olmo-2:13b", "OLMo"),
            ("bloom:176b", "BLOOM"),
            ("starcoder2:15b", "StarCoder"),
            ("gemini-3.1-pro", "Gemini"),
            ("grok-4.1", "Grok"),
            ("command-r-plus", "Command R"),
            ("nemotron-3-super", "Nemotron"),
            ("kimi-k2.5-thinking", "Kimi"),
            ("ernie-4.5", "ERNIE"),
            ("hunyuan-large", "Hunyuan"),
            ("internlm2.5:20b", "InternLM"),
            ("baichuan2:13b", "Baichuan"),
            ("minimax-m2.5", "MiniMax"),
            ("doubao-seed-2.0", "Seed"),
            ("yi-34b", "Yi"),
            ("nova-pro", "Nova"),
            ("titan-text", "Titan"),
            ("sonar-pro", "Sonar"),
            ("solar-10.7b", "SOLAR"),
            ("dbrx-instruct", "DBRX"),
            ("jamba-large", "Jamba"),
            ("llava-1.6", "LLaVA"),
            ("gpt-oss-120b", "GPT-OSS"),
            ("unknown-local-model", "Other"),
        ] {
            assert_eq!(ModelPricing::name(Some(model)), family, "{model}");
            assert_eq!(
                u.cost_for_model(Some(model)),
                0.0,
                "{model} should not be charged at Sonnet fallback rates"
            );
        }
    }

    // --- Pricing constants verification ---

    #[test]
    fn pricing_constants_fable() {
        let p = ModelPricing::for_model(Some("claude-fable-5"));
        assert_eq!(p.input_per_mtok, 10.0);
        assert_eq!(p.output_per_mtok, 50.0);
        assert_eq!(p.cache_write_per_mtok, 12.50);
        assert_eq!(p.cache_read_per_mtok, 1.00);
    }

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

    // --- Historical / legacy tiers ---

    fn input_1m(model: &str) -> f64 {
        TokenUsage {
            input_tokens: 1_000_000,
            ..Default::default()
        }
        .cost_for_model(Some(model))
    }

    fn output_1m(model: &str) -> f64 {
        TokenUsage {
            output_tokens: 1_000_000,
            ..Default::default()
        }
        .cost_for_model(Some(model))
    }

    #[test]
    fn legacy_opus_3_is_full_price() {
        // Claude 3 / Opus 4 / 4.1 predate the 4.5+ price cut.
        let p = ModelPricing::for_model(Some("claude-3-opus-20240229"));
        assert_eq!(p.input_per_mtok, 15.0);
        assert_eq!(p.output_per_mtok, 75.0);
        assert_eq!(p.cache_write_per_mtok, 18.75);
        assert_eq!(p.cache_read_per_mtok, 1.50);
        assert_eq!(
            ModelPricing::for_model(Some("claude-opus-4-1")).input_per_mtok,
            15.0
        );
    }

    #[test]
    fn claude_3_haiku_is_cheapest_tier() {
        let p = ModelPricing::for_model(Some("claude-3-haiku-20240307"));
        assert_eq!(p.input_per_mtok, 0.25);
        assert_eq!(p.output_per_mtok, 1.25);
        assert_eq!(p.cache_write_per_mtok, 0.3125);
        assert_eq!(p.cache_read_per_mtok, 0.025);
        assert!((input_1m("claude-3-haiku-20240307") - 0.25).abs() < 0.0001);
    }

    #[test]
    fn claude_3_5_haiku_stays_legacy_not_claude_3() {
        // `claude-3-5-haiku-*` must NOT match the Claude 3 Haiku tier.
        let p = ModelPricing::for_model(Some("claude-3-5-haiku-20241022"));
        assert_eq!(
            p.input_per_mtok, 0.80,
            "3.5 Haiku keeps the $0.80 legacy rate"
        );
        assert_eq!(p.output_per_mtok, 4.0);
    }

    #[test]
    fn gpt4_turbo_pricing() {
        for id in [
            "gpt-4-turbo",
            "gpt-4-turbo-2024-04-09",
            "gpt-4-1106-preview",
        ] {
            assert!((input_1m(id) - 10.0).abs() < 0.0001, "{id} input");
            assert!((output_1m(id) - 30.0).abs() < 0.0001, "{id} output");
        }
    }

    #[test]
    fn gpt4_32k_pricing() {
        assert!((input_1m("gpt-4-32k") - 60.0).abs() < 0.0001);
        assert!((output_1m("gpt-4-32k") - 120.0).abs() < 0.0001);
    }

    #[test]
    fn gpt4_classic_8k_pricing() {
        for id in ["gpt-4", "gpt-4-0613", "gpt-4-0314"] {
            assert!((input_1m(id) - 30.0).abs() < 0.0001, "{id} input");
            assert!((output_1m(id) - 60.0).abs() < 0.0001, "{id} output");
        }
    }

    #[test]
    fn gpt4_family_disambiguation_unaffected() {
        // The new classic-GPT-4 tier must not steal gpt-4o / gpt-4.1 ids.
        assert!((input_1m("gpt-4o") - 2.50).abs() < 0.0001, "gpt-4o base");
        assert!(
            (input_1m("gpt-4o-mini") - 0.15).abs() < 0.0001,
            "gpt-4o-mini"
        );
        assert!((input_1m("gpt-4.1") - 2.0).abs() < 0.0001, "gpt-4.1");
        assert!(
            (input_1m("gpt-4.1-mini") - 0.40).abs() < 0.0001,
            "gpt-4.1-mini"
        );
    }

    #[test]
    fn gpt4_5_preview_is_premium_not_classic() {
        // `gpt-4.5-preview` must NOT fall into the classic GPT-4 8k tier ($30/$60);
        // it has its own premium rate ($75/$150).
        assert!(
            (input_1m("gpt-4.5-preview") - 75.0).abs() < 0.0001,
            "gpt-4.5 input"
        );
        assert!(
            (output_1m("gpt-4.5-preview") - 150.0).abs() < 0.0001,
            "gpt-4.5 output"
        );
        assert!(
            !is_gpt4_classic("gpt-4.5-preview"),
            "4.5 excluded from classic"
        );
    }
}

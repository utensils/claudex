# Pricing model

All costs in claudex are **approximate**. They come from published per-model
pricing tiers applied to the token-usage blocks recorded in each session — or,
for providers that report their own cost, from that figure directly.

Source of truth: `src/types.rs`, `ModelPricing::for_model`.

## Tiers

| Model tier | Input         | Output        | Cache write   | Cache read    |
| ---------- | ------------- | ------------- | ------------- | ------------- |
| **Opus**   | $15.00 / MTok | $75.00 / MTok | $18.75 / MTok | $1.50 / MTok  |
| **Sonnet** | $3.00 / MTok  | $15.00 / MTok | $3.75 / MTok  | $0.30 / MTok  |
| **Haiku**  | $0.80 / MTok  | $4.00 / MTok  | $1.00 / MTok  | $0.08 / MTok  |
| **GPT-5**  | $1.25 / MTok  | $10.00 / MTok | $1.25 / MTok  | $0.125 / MTok |
| **GPT-4**  | $2.50 / MTok  | $10.00 / MTok | $2.50 / MTok  | $1.25 / MTok  |

(MTok = million tokens. Claude tiers are Anthropic's published rates; the OpenAI
`gpt-*` tiers are list rates and approximate.)

## Tier detection

The tier is chosen from a substring of the model name:

- Contains `opus` → Opus; contains `haiku` → Haiku.
- Contains `gpt-5`/`gpt5` → GPT-5; contains `gpt-4`/`gpt4` → GPT-4.
- Anything else → Sonnet (the safe fallback, including Claude's `<synthetic>`).

So `claude-opus-4-7` maps to Opus, `gpt-5-codex` and `gpt-5.5` map to GPT-5, and
unknown names map to Sonnet.

## Provider-supplied cost

Pi computes a cost for every assistant message (and reports `$0` for local
Ollama models). claudex **trusts that figure** rather than re-deriving it from
the tier table — so a Pi session's cost reflects exactly what Pi billed,
including free local inference. Internally this is `ModelSessionStats::embedded_cost`,
which the index uses in place of `cost_for_model` when present.

## Computation

For each `(session, model)` row in the `token_usage` table, when no
provider-supplied cost is present:

```
cost = (input  × input_per_mtok
      + output × output_per_mtok
      + cache_write × cache_write_per_mtok
      + cache_read  × cache_read_per_mtok) / 1_000_000
```

For Codex, `input`/`cache_read` come from the last cumulative `token_count`
record (the cached portion of the prompt is billed as a cache read). Sessions
that switched models accumulate multiple rows; totals sum across them.

## Why it's approximate

- **No volume discounts.** Priority throughput, batch pricing, etc. aren't
  reflected.
- **No historical pricing.** If tiers change, old sessions are priced at
  _current_ rates. Claudex doesn't store a rate card.
- **No free tier / promo credits.** These are invoicing concerns; they don't
  show up in the API response.
- **Cache-read estimate.** Cache reads don't always correspond to billable
  tokens 1:1 in every context. Claudex prices them at the posted rate, which
  is a close upper bound.

For authoritative billing, use Anthropic's console. Claudex is for relative
comparisons — "which project costs more", "which model tier am I leaning on",
"how does this week compare to last" — where the model-agnostic math is
accurate enough.

## Opus:Sonnet ratio

Opus is exactly 5× Sonnet on input, output, and cache reads, and exactly 5×
Sonnet on cache writes as well. If you see an Opus session that claims cost
less than 5× what you'd expect from the same session run on Sonnet, check for
model switching mid-session.

## Rendering

- `fmt_cost` renders `$12,345.67` with thousands separators.
- Values below one cent fall back to four decimals: `$0.0042`. Tiny sessions
  don't disappear into `$0.00`.
- JSON output always uses raw `cost_usd` floats — no formatting.

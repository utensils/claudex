# Pricing model

All costs in claudex are **approximate**. They come from published Anthropic
pricing tiers applied to the token-usage blocks recorded in each session.

Source of truth: `src/types.rs`, `ModelPricing::for_model`.

## Tiers

| Model tier | Input         | Output        | Cache write   | Cache read   |
| ---------- | ------------- | ------------- | ------------- | ------------ |
| **Opus**   | $15.00 / MTok | $75.00 / MTok | $18.75 / MTok | $1.50 / MTok |
| **Sonnet** | $3.00 / MTok  | $15.00 / MTok | $3.75 / MTok  | $0.30 / MTok |
| **Haiku**  | $0.80 / MTok  | $4.00 / MTok  | $1.00 / MTok  | $0.08 / MTok |

(MTok = million tokens.)

## Tier detection

The tier is chosen from a substring of the model name:

- Name contains `opus` → Opus.
- Name contains `haiku` → Haiku.
- Anything else → Sonnet (the safe fallback).

So `claude-opus-4-7`, `claude-opus-4-6`, and `opus` all map to Opus.
`claude-haiku-4-5-20251001` maps to Haiku. Unknown or missing names map to
Sonnet — the middle tier — which underestimates Opus and overestimates Haiku
by a small amount.

## Computation

For each `(session, model)` row in the `token_usage` table:

```
cost = (input  × input_per_mtok
      + output × output_per_mtok
      + cache_write × cache_write_per_mtok
      + cache_read  × cache_read_per_mtok) / 1_000_000
```

The four token counts come from the `usage` block on each assistant message
(Claude Code records them verbatim from the API response).

Sessions that switched models accumulate multiple rows; totals sum across them.

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

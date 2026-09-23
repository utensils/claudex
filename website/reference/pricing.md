# Pricing model

All costs in claudex are **approximate**. They come from published per-model
pricing tiers applied to the token-usage blocks recorded in each session — or,
for providers that report their own cost, from that figure directly.

Source of truth: `crates/claudex/src/types.rs`, `ModelPricing::for_model`.

## Anthropic (Claude) tiers

Each Claude family carries one or more rate cards. Fable/Mythos 5.1 and Opus
5.5 have cheaper cache reads than their predecessors. Fast-mode Opus carries a
premium. Anthropic made Sonnet 5's launch rate permanent.

| Model tier                      | Input         | Output        | Cache write    | Cache read    |
| ------------------------------- | ------------- | ------------- | -------------- | ------------- |
| **Fable 5.1 / Mythos 5.1**      | $10.00 / MTok | $50.00 / MTok | $12.50 / MTok  | $0.25 / MTok  |
| **Fable 5 / Mythos 5**          | $10.00 / MTok | $50.00 / MTok | $12.50 / MTok  | $1.00 / MTok  |
| **Opus 5.5**                    | $4.00 / MTok  | $20.00 / MTok | $5.00 / MTok   | $0.20 / MTok  |
| **Opus 5.5 fast**               | $8.00 / MTok  | $40.00 / MTok | $10.00 / MTok  | $0.40 / MTok  |
| **Opus 5 / Opus 4.5–4.8**       | $5.00 / MTok  | $25.00 / MTok | $6.25 / MTok   | $0.50 / MTok  |
| **Opus 5 / Opus 4.8 fast**      | $10.00 / MTok | $50.00 / MTok | $12.50 / MTok  | $1.00 / MTok  |
| **Opus** (legacy 3/4)           | $15.00 / MTok | $75.00 / MTok | $18.75 / MTok  | $1.50 / MTok  |
| **Sonnet 5**                    | $2.00 / MTok  | $10.00 / MTok | $2.50 / MTok   | $0.20 / MTok  |
| **Sonnet 4.x / legacy default** | $3.00 / MTok  | $15.00 / MTok | $3.75 / MTok   | $0.30 / MTok  |
| **Haiku 4.5** (latest)          | $1.00 / MTok  | $5.00 / MTok  | $1.25 / MTok   | $0.10 / MTok  |
| **Haiku 3.5** (legacy)          | $0.80 / MTok  | $4.00 / MTok  | $1.00 / MTok   | $0.08 / MTok  |
| **Haiku 3**                     | $0.25 / MTok  | $1.25 / MTok  | $0.3125 / MTok | $0.025 / MTok |

(MTok = million tokens. These are Anthropic's published rates.)

Cache-write figures use Anthropic's five-minute cache-write rate because
transcripts do not distinguish five-minute from one-hour cache creation. See Anthropic's
[model pricing](https://platform.claude.com/docs/en/about-claude/pricing) and
[model overview](https://platform.claude.com/docs/en/about-claude/models/overview).

## OpenAI (`gpt-*`) tiers

OpenAI models carry many sub-tiers. claudex matches the **most specific** name
first, falling back to a base `gpt-5` / `gpt-4o` rate. Claudex currently uses
the **Standard short-context** schedule because provider transcripts do not
record the billing context-length band. For OpenAI models the
**cache-read** rate is the posted "cached input" rate. For GPT-5.6 and later,
cache writes cost 1.25x uncached input; earlier OpenAI rows use the input rate.
The GPT-5.6 rates and cache policy come from OpenAI's
[current model pages](https://developers.openai.com/api/docs/models/gpt-5.6-sol)
and [pricing table](https://developers.openai.com/api/docs/pricing). The Sol
rate is promotional at least through November 21, 2026.

| Model match                                 | Input         | Output         | Cache write    | Cache read    |
| ------------------------------------------- | ------------- | -------------- | -------------- | ------------- |
| `gpt-6-astra`                               | $10.00 / MTok | $50.00 / MTok  | $12.50 / MTok  | $1.00 / MTok  |
| `gpt-6-sol`                                 | $2.00 / MTok  | $10.00 / MTok  | $2.50 / MTok   | $0.20 / MTok  |
| `gpt-6-luna`                                | $0.10 / MTok  | $0.50 / MTok   | $0.125 / MTok  | $0.01 / MTok  |
| `gpt-5.6-sol`, `gpt-5.6` alias              | $4.00 / MTok  | $20.00 / MTok  | $5.00 / MTok   | $0.40 / MTok  |
| `gpt-5.6-terra`                             | $2.00 / MTok  | $12.00 / MTok  | $2.50 / MTok   | $0.20 / MTok  |
| `gpt-5.6-luna`                              | $0.20 / MTok  | $1.20 / MTok   | $0.25 / MTok   | $0.02 / MTok  |
| `gpt-5.6-cyber`                             | $12.50 / MTok | $75.00 / MTok  | $15.625 / MTok | $1.25 / MTok  |
| `gpt-5.5-pro`, `gpt-5.4-pro`                | $30.00 / MTok | $180.00 / MTok | $30.00 / MTok  | $30.00 / MTok |
| `gpt-5-pro`                                 | $15.00 / MTok | $120.00 / MTok | $15.00 / MTok  | $15.00 / MTok |
| `gpt-5.5`                                   | $5.00 / MTok  | $30.00 / MTok  | $5.00 / MTok   | $0.50 / MTok  |
| `gpt-5.4`                                   | $2.50 / MTok  | $15.00 / MTok  | $2.50 / MTok   | $0.25 / MTok  |
| `gpt-5.4-mini`                              | $0.75 / MTok  | $4.50 / MTok   | $0.75 / MTok   | $0.075 / MTok |
| `gpt-5.4-nano`                              | $0.20 / MTok  | $1.25 / MTok   | $0.20 / MTok   | $0.02 / MTok  |
| `gpt-5.3-codex`, `gpt-5.2-codex`, `gpt-5.2` | $1.75 / MTok  | $14.00 / MTok  | $1.75 / MTok   | $0.175 / MTok |
| `gpt-5` (base / other `gpt-5*`)             | $1.25 / MTok  | $10.00 / MTok  | $1.25 / MTok   | $0.125 / MTok |
| `gpt-4.1`                                   | $2.00 / MTok  | $8.00 / MTok   | $2.00 / MTok   | $0.50 / MTok  |
| `gpt-4.1-mini`                              | $0.40 / MTok  | $1.60 / MTok   | $0.40 / MTok   | $0.10 / MTok  |
| `gpt-4.1-nano`                              | $0.10 / MTok  | $0.40 / MTok   | $0.10 / MTok   | $0.025 / MTok |
| `gpt-4.5-preview`                           | $75.00 / MTok | $150.00 / MTok | $75.00 / MTok  | $37.50 / MTok |
| `gpt-4o-mini`                               | $0.15 / MTok  | $0.60 / MTok   | $0.15 / MTok   | $0.075 / MTok |
| `gpt-4o-2024-05-13`                         | $5.00 / MTok  | $15.00 / MTok  | $5.00 / MTok   | $5.00 / MTok  |
| `gpt-4-turbo`, `gpt-4-1106`, `gpt-4-0125`   | $10.00 / MTok | $30.00 / MTok  | $10.00 / MTok  | $10.00 / MTok |
| `gpt-4-32k`                                 | $60.00 / MTok | $120.00 / MTok | $60.00 / MTok  | $60.00 / MTok |
| `gpt-4` (classic 8k / 0613)                 | $30.00 / MTok | $60.00 / MTok  | $30.00 / MTok  | $30.00 / MTok |
| `gpt-4o` (base / other `gpt-4*`)            | $2.50 / MTok  | $10.00 / MTok  | $2.50 / MTok   | $1.25 / MTok  |

GPT-6 rates are from the [official pricing table](https://developers.openai.com/api/docs/pricing).
For GPT-6 requests above 272K input tokens, OpenAI doubles input and cache
rates and multiplies output rates by 1.5. Batch and Flex cost 50% of Standard;
Fast mode costs 2x the applicable rates. Claudex uses Standard short-context
estimates because its aggregated usage does not reliably retain these billing
conditions; it does not apply a request threshold to session totals.

OpenAI also publishes these higher **Standard long-context** rates:

| Model match     | Input         | Output        | Cache write   | Cache read   |
| --------------- | ------------- | ------------- | ------------- | ------------ |
| `gpt-6-astra`   | $20.00 / MTok | $75.00 / MTok | $25.00 / MTok | $2.00 / MTok |
| `gpt-6-sol`     | $4.00 / MTok  | $15.00 / MTok | $5.00 / MTok  | $0.40 / MTok |
| `gpt-6-luna`    | $0.20 / MTok  | $0.75 / MTok  | $0.25 / MTok  | $0.02 / MTok |
| `gpt-5.6-sol`   | $8.00 / MTok  | $30.00 / MTok | $10.00 / MTok | $0.80 / MTok |
| `gpt-5.6-terra` | $4.00 / MTok  | $18.00 / MTok | $5.00 / MTok  | $0.40 / MTok |
| `gpt-5.6-luna`  | $0.40 / MTok  | $1.80 / MTok  | $0.50 / MTok  | $0.04 / MTok |

These are documented for completeness but cannot be selected reliably from
the local transcript data, so claudex estimates long-context sessions at the
short-context rates above. Batch, Flex, Priority, and regional-processing
adjustments are likewise outside the transcript-derived estimate.

(OpenAI tiers are list rates and approximate. The classic GPT-4 / Turbo / 32k
tiers predate prompt caching, so their cache-read rate falls back to the input
rate. Pi-reported sessions use Pi's own cost instead — see below.)

## Google Gemini Flash tiers

For Gemini Flash model IDs, claudex estimates paid-tier Standard **text** token
costs using [Google's current rate card](https://ai.google.dev/gemini-api/docs/pricing).
Gemini 3.6–3.8 Flash rates are promotional through December 31, 2026.

| Model match                                  | Input        | Output       | Cache read    |
| -------------------------------------------- | ------------ | ------------ | ------------- |
| `gemini-3.8-flash`, `3.7-flash`, `3.6-flash` | $0.75 / MTok | $3.75 / MTok | $0.075 / MTok |
| `gemini-3.5-flash`                           | $1.50 / MTok | $9.00 / MTok | $0.15 / MTok  |
| `gemini-3.5-flash-lite`                      | $0.30 / MTok | $2.50 / MTok | $0.03 / MTok  |

Cache creation is estimated at the uncached input rate: transcripts lack the
cache storage duration needed for Google's hourly storage charge. Gemini free
tier, audio-specific rates, batch, grounding, and tool fees are not represented.

## xAI Grok 4.7

The [Grok 4.7 rate card](https://docs.x.ai/developers/release-notes) lists
$2.00 input, $0.50 cached input, and $6.00 output per million tokens on the
global endpoint for prompts below 200K tokens. Claudex uses the uncached input
rate for cache creation. Requests at or above 200K use higher rates; regional,
fast, and tool charges are outside the transcript-derived estimate.

## Tier detection

The tier is chosen by substring-matching the model name, **most specific first**:

- `fable-5-1` / `mythos-5-1` → the 5.1 cache-read rate; other `fable` /
  `mythos` → the earlier frontier rate card.
- `opus-5-5` + `fast` → Opus 5.5 fast rates; `opus-5-5` → Opus 5.5 rates.
- `opus-5` / `opus-4-8` + `fast` → the supported fast-mode premium card
  ($10/$50).
- `opus-5` or `opus-4-5`/`4.6`/`4.7`/`4.8` → current Opus rates; any other
  `opus` → legacy Opus.
- `sonnet-5` → the permanent Sonnet 5 card;
  other `sonnet` ids and missing/empty legacy model ids → standard Sonnet.
- `haiku-4-5` → Haiku 4.5; `3-haiku` (but not `3-5-haiku`) → the cheapest
  Claude 3 Haiku tier; any other `haiku` → Haiku 3.5 legacy.
- `gpt-6-astra` / `sol` / `luna` → dedicated rates and family labels.
- `gpt-5.6` alias / `gpt-5.6-sol` / `terra` / `luna` → dedicated rates and family labels;
  `gpt-5.6-cyber` → its specialized rate card.
- Supported `gemini-3.x-flash` IDs → the matching paid text rates above.
- `grok-4.7` → the xAI Standard short-context card above.
- Other `gpt-5*` / `gpt-4*` → the matching OpenAI row above (specific variants —
  including `gpt-4-turbo`/`-32k` and classic `gpt-4` — win over the `gpt-4o`
  base rate).
- Anything else — local, open-weight, and unrecognized models (including
  Claude's `<synthetic>`) → **$0**, unless the provider reported its own cost
  (see below). This avoids fabricating Sonnet charges for Ollama/MLX/vLLM-style
  models.

So `claude-fable-5-1` maps to **Fable 5.1** ($10/$50 with $0.25 cache reads),
`claude-opus-5-5` maps to **Opus 5.5** ($4/$20), `claude-sonnet-5` maps to
its permanent $2/$10 card, an older `claude-opus-3` maps to **legacy Opus** ($15/$75), and
unrecognized names are not charged. Note that the display
**family label** (`models` command) is `Astra`/`Sol`/`Luna` for GPT-6, `Sol`/`Terra`/`Luna` for GPT-5.6, or
`Fable`/`Mythos`/`Opus`/`Haiku`/`Sonnet`/`GPT-5`/`GPT-4`/etc. — it does not distinguish latest from legacy
(or fast from standard), but the **cost** does.

## Provider-supplied cost

Pi computes a cost for every assistant message (and reports `$0` for local
Ollama models), and OpenClaw records a running total per trajectory. claudex
**trusts those figures** rather than re-deriving them from the tier table — so
a Pi or OpenClaw session's cost reflects exactly what the provider billed,
including free local inference. Internally this is `ModelSessionStats::embedded_cost`,
which the index uses in place of `cost_for_model` when present.

## GitHub Copilot

Copilot is subscription-billed by **premium requests**, not per-token USD, so
claudex prices Copilot CLI sessions from the rate card like Claude/Codex — the
USD figure is an **API-equivalent estimate** of what the same tokens would cost
at list price, not what GitHub billed. The premium-request count is preserved
in the session's `extras`. VS Code Copilot Chat stores no token counts locally
(they live server-side), so `copilot-vscode` sessions report zero tokens and
`$0` cost while still counting for activity, search, and model reports.

## Repricing existing data

Every `token_usage` row records a **`cost_source`**: `computed` (priced from the
tiers above) or `provider` (a figure the provider reported — Pi and OpenClaw
today). The binary also carries a **`PRICING_REVISION`** that is bumped whenever
the rate card changes. On the next run after an upgrade, claudex reprices every
`computed` row **in place** with the current tiers and stamps the new revision,
so the one-off pass runs exactly once. `provider` rows are never touched, so
provider-billed figures (including `$0` local models) are preserved.

This is the non-destructive counterpart to `claudex index --force`: that command
deletes and rebuilds from disk and so **cannot** recover archived/retained
sessions, whereas the reprice updates retained rows too. No action is required —
new and existing indexes converge on the same rates automatically.

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

**Legacy** Opus is exactly 5× Sonnet on every dimension ($15 vs $3 input, $75 vs
$15 output, etc.). **Current** Opus 5.5 is $4/$20 input/output, 2× Sonnet 5
on base input and output — so do _not_ assume a 5× multiple for present-day Opus
sessions. If an Opus cost looks lower than you expect, that's usually the
current Opus rate card, not an error; mid-session model switching can also
lower it.

## Rendering

- `fmt_cost` renders `$12,345.67` with thousands separators.
- Values below one cent fall back to four decimals: `$0.0042`. Tiny sessions
  don't disappear into `$0.00`.
- JSON output always uses raw `cost_usd` floats — no formatting.

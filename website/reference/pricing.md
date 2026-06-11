# Pricing model

All costs in claudex are **approximate**. They come from published per-model
pricing tiers applied to the token-usage blocks recorded in each session — or,
for providers that report their own cost, from that figure directly.

Source of truth: `crates/claudex/src/types.rs`, `ModelPricing::for_model`.

## Anthropic (Claude) tiers

Each Claude family carries one or more rate cards: Fable 5 sits above Opus as
the frontier tier, the current Opus 4.5+ generation is priced well below older
Opus models, fast-mode Opus carries a premium, and the original Claude 3 Haiku
is cheaper still — so claudex routes each to a dedicated branch.

| Model tier              | Input         | Output         | Cache write    | Cache read    |
| ----------------------- | ------------- | -------------- | -------------- | ------------- |
| **Fable 5 / Mythos 5**  | $10.00 / MTok | $50.00 / MTok  | $12.50 / MTok  | $1.00 / MTok  |
| **Opus 4.5+** (4.5–4.8) | $5.00 / MTok  | $25.00 / MTok  | $6.25 / MTok   | $0.50 / MTok  |
| **Opus fast** (4.6/4.7) | $30.00 / MTok | $150.00 / MTok | $37.50 / MTok  | $3.00 / MTok  |
| **Opus 4.8 fast**       | $10.00 / MTok | $50.00 / MTok  | $12.50 / MTok  | $1.00 / MTok  |
| **Opus** (legacy 3/4)   | $15.00 / MTok | $75.00 / MTok  | $18.75 / MTok  | $1.50 / MTok  |
| **Sonnet** (default)    | $3.00 / MTok  | $15.00 / MTok  | $3.75 / MTok   | $0.30 / MTok  |
| **Haiku 4.5** (latest)  | $1.00 / MTok  | $5.00 / MTok   | $1.25 / MTok   | $0.10 / MTok  |
| **Haiku 3.5** (legacy)  | $0.80 / MTok  | $4.00 / MTok   | $1.00 / MTok   | $0.08 / MTok  |
| **Haiku 3**             | $0.25 / MTok  | $1.25 / MTok   | $0.3125 / MTok | $0.025 / MTok |

(MTok = million tokens. These are Anthropic's published rates.)

## OpenAI (`gpt-*`) tiers

OpenAI models carry many sub-tiers. claudex matches the **most specific** name
first, falling back to a base `gpt-5` / `gpt-4o` rate. For OpenAI models the
**cache-write** rate equals the input rate, and the **cache-read** rate is the
posted "cached input" rate.

| Model match                                 | Input         | Output         | Cache read    |
| ------------------------------------------- | ------------- | -------------- | ------------- |
| `gpt-5.5-pro`, `gpt-5.4-pro`                | $30.00 / MTok | $180.00 / MTok | $30.00 / MTok |
| `gpt-5-pro`                                 | $15.00 / MTok | $120.00 / MTok | $15.00 / MTok |
| `gpt-5.5`                                   | $5.00 / MTok  | $30.00 / MTok  | $0.50 / MTok  |
| `gpt-5.4`                                   | $2.50 / MTok  | $15.00 / MTok  | $0.25 / MTok  |
| `gpt-5.4-mini`                              | $0.75 / MTok  | $4.50 / MTok   | $0.075 / MTok |
| `gpt-5.4-nano`                              | $0.20 / MTok  | $1.25 / MTok   | $0.02 / MTok  |
| `gpt-5.3-codex`, `gpt-5.2-codex`, `gpt-5.2` | $1.75 / MTok  | $14.00 / MTok  | $0.175 / MTok |
| `gpt-5` (base / other `gpt-5*`)             | $1.25 / MTok  | $10.00 / MTok  | $0.125 / MTok |
| `gpt-4.1`                                   | $2.00 / MTok  | $8.00 / MTok   | $0.50 / MTok  |
| `gpt-4.1-mini`                              | $0.40 / MTok  | $1.60 / MTok   | $0.10 / MTok  |
| `gpt-4.1-nano`                              | $0.10 / MTok  | $0.40 / MTok   | $0.025 / MTok |
| `gpt-4.5-preview`                           | $75.00 / MTok | $150.00 / MTok | $37.50 / MTok |
| `gpt-4o-mini`                               | $0.15 / MTok  | $0.60 / MTok   | $0.075 / MTok |
| `gpt-4o-2024-05-13`                         | $5.00 / MTok  | $15.00 / MTok  | $5.00 / MTok  |
| `gpt-4-turbo`, `gpt-4-1106`, `gpt-4-0125`   | $10.00 / MTok | $30.00 / MTok  | $10.00 / MTok |
| `gpt-4-32k`                                 | $60.00 / MTok | $120.00 / MTok | $60.00 / MTok |
| `gpt-4` (classic 8k / 0613)                 | $30.00 / MTok | $60.00 / MTok  | $30.00 / MTok |
| `gpt-4o` (base / other `gpt-4*`)            | $2.50 / MTok  | $10.00 / MTok  | $1.25 / MTok  |

(OpenAI tiers are list rates and approximate. The classic GPT-4 / Turbo / 32k
tiers predate prompt caching, so their cache-read rate falls back to the input
rate. Pi-reported sessions use Pi's own cost instead — see below.)

## Tier detection

The tier is chosen by substring-matching the model name, **most specific first**:

- `fable` / `mythos` → the Fable 5 frontier tier ($10/$50).
- `opus-4-5`–`4.8` + `fast` → the fast-mode premium card ($30/$150 on 4.6/4.7,
  $10/$50 on 4.8).
- `opus-4-5`/`4.6`/`4.7`/`4.8` → Opus 4.5+ rates; any other `opus` → legacy Opus.
- `haiku-4-5` → Haiku 4.5; `3-haiku` (but not `3-5-haiku`) → the cheapest
  Claude 3 Haiku tier; any other `haiku` → Haiku 3.5 legacy.
- `gpt-5*` / `gpt-4*` → the matching OpenAI row above (specific variants —
  including `gpt-4-turbo`/`-32k` and classic `gpt-4` — win over the `gpt-4o`
  base rate).
- `sonnet`, or a **missing/empty** model id (old Claude transcripts) → Sonnet.
- Anything else — local, open-weight, and unrecognized models (including
  Claude's `<synthetic>`) → **$0**, unless the provider reported its own cost
  (see below). This avoids fabricating Sonnet charges for Ollama/MLX/vLLM-style
  models.

So `claude-fable-5` maps to **Fable 5** ($10/$50), `claude-opus-4-8` maps to
**Opus 4.5+** ($5/$25), an older `claude-opus-3` maps to **legacy Opus**
($15/$75), and unrecognized names are not charged. Note that the display
**family label** (`models` command) is just `Fable`/`Mythos`/`Opus`/`Haiku`/
`Sonnet`/`GPT-5`/`GPT-4`/etc. — it does not distinguish latest from legacy
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
$15 output, etc.). **Current** Opus 4.5+ is much cheaper — $5/$25 input/output,
roughly 1.7× Sonnet — so do _not_ assume a 5× multiple for present-day Opus
sessions. If an Opus cost looks lower than you expect, that's usually the 4.5+
rate card, not an error; mid-session model switching can also lower it.

## Rendering

- `fmt_cost` renders `$12,345.67` with thousands separators.
- Values below one cent fall back to four decimals: `$0.0042`. Tiny sessions
  don't disappear into `$0.00`.
- JSON output always uses raw `cost_usd` floats — no formatting.

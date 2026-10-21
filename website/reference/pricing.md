# Pricing model

All costs in claudex are **approximate**. They come from published per-model
pricing tiers applied to the token-usage blocks recorded in each session — or,
for providers that report their own cost, from that figure directly.

Anthropic and OpenAI text-token rates last verified October 21, 2026. These
are API-equivalent list-price estimates, not ChatGPT or Claude subscription
charges.

Source of truth: `crates/claudex/src/types.rs`, `ModelPricing::for_model`.

## Anthropic (Claude) tiers

Each Claude family carries one or more rate cards. Fable/Mythos 5.1, Opus
5.5, and Sonnet 5.5 have cheaper cache reads than their predecessors. Fast-mode
Opus carries a premium. Anthropic made Sonnet 5's launch rate permanent.

| Model tier                        | Input         | Output        | Cache write    | Cache read    |
| --------------------------------- | ------------- | ------------- | -------------- | ------------- |
| **Fable 5.1 / Mythos 5.1**        | $10.00 / MTok | $50.00 / MTok | $12.50 / MTok  | $0.25 / MTok  |
| **Fable 5 / Mythos 5**            | $10.00 / MTok | $50.00 / MTok | $12.50 / MTok  | $1.00 / MTok  |
| **Opus 5.5**                      | $4.00 / MTok  | $20.00 / MTok | $5.00 / MTok   | $0.20 / MTok  |
| **Opus 5.5 fast**                 | $8.00 / MTok  | $40.00 / MTok | $10.00 / MTok  | $0.40 / MTok  |
| **Opus 5 / Opus 4.5–4.8**         | $5.00 / MTok  | $25.00 / MTok | $6.25 / MTok   | $0.50 / MTok  |
| **Opus 5 / Opus 4.8 fast**        | $10.00 / MTok | $50.00 / MTok | $12.50 / MTok  | $1.00 / MTok  |
| **Opus** (legacy 3/4)             | $15.00 / MTok | $75.00 / MTok | $18.75 / MTok  | $1.50 / MTok  |
| **Sonnet 5.5**                    | $2.00 / MTok  | $10.00 / MTok | $2.50 / MTok   | $0.10 / MTok  |
| **Sonnet 5**                      | $2.00 / MTok  | $10.00 / MTok | $2.50 / MTok   | $0.20 / MTok  |
| **Sonnet 4.x / legacy default**   | $3.00 / MTok  | $15.00 / MTok | $3.75 / MTok   | $0.30 / MTok  |
| **Haiku 5.5** (up to 100K prompt) | $0.10 / MTok  | $0.50 / MTok  | $0.125 / MTok  | $0.01 / MTok  |
| **Haiku 4.5**                     | $1.00 / MTok  | $5.00 / MTok  | $1.25 / MTok   | $0.10 / MTok  |
| **Haiku 3.5** (legacy)            | $0.80 / MTok  | $4.00 / MTok  | $1.00 / MTok   | $0.08 / MTok  |
| **Haiku 3**                       | $0.25 / MTok  | $1.25 / MTok  | $0.3125 / MTok | $0.025 / MTok |

(MTok = million tokens. These are Anthropic's published rates.)

Cache-write figures use Anthropic's five-minute cache-write rate because
transcripts do not distinguish five-minute from one-hour cache creation. See Anthropic's
[model pricing](https://platform.claude.com/docs/en/about-claude/pricing) and
[model overview](https://platform.claude.com/docs/en/about-claude/models/overview).

Haiku 5.5 requests above 100K prompt tokens cost $0.50 input, $2.50 output,
$0.625 five-minute cache write, and $0.05 cache read per MTok. Claudex uses
the short-context estimate: session totals do not retain request boundaries
reliably enough to choose a band. A million-token session can contain many
short requests. Provider-reported costs remain authoritative when available.

## OpenAI (`gpt-*`) tiers

OpenAI models carry many sub-tiers. claudex matches the **most specific** name
first, falling back to a base `gpt-5` / `gpt-4o` rate. Claudex currently uses
the **Standard short-context** schedule because provider transcripts do not
record the billing context-length band. For OpenAI models the
**cache-read** rate is the posted "cached input" rate. For GPT-5.6 and later,
cache writes cost 1.25x uncached input (except Rosalind); earlier OpenAI rows
use the input rate.
The GPT-5.6 rates and cache policy come from OpenAI's
[current model pages](https://developers.openai.com/api/docs/models/gpt-5.6-sol)
and [pricing table](https://developers.openai.com/api/docs/pricing). The Sol
rate is promotional at least through November 21, 2026; review it after that date.

| Model match                                       | Input         | Output         | Cache write    | Cache read    |
| ------------------------------------------------- | ------------- | -------------- | -------------- | ------------- |
| `gpt-6-astra`                                     | $10.00 / MTok | $50.00 / MTok  | $12.50 / MTok  | $1.00 / MTok  |
| `gpt-6.1-sol`                                     | $2.00 / MTok  | $10.00 / MTok  | $2.50 / MTok   | $0.10 / MTok  |
| `gpt-6-sol`                                       | $2.00 / MTok  | $10.00 / MTok  | $2.50 / MTok   | $0.20 / MTok  |
| `gpt-6-luna`                                      | $0.10 / MTok  | $0.50 / MTok   | $0.125 / MTok  | $0.01 / MTok  |
| `gpt-5.6-sol`, `gpt-5.6` alias                    | $4.00 / MTok  | $20.00 / MTok  | $5.00 / MTok   | $0.40 / MTok  |
| `gpt-5.6-terra`                                   | $2.00 / MTok  | $12.00 / MTok  | $2.50 / MTok   | $0.20 / MTok  |
| `gpt-5.6-luna`                                    | $0.20 / MTok  | $1.20 / MTok   | $0.25 / MTok   | $0.02 / MTok  |
| `gpt-daybreak-blue-latest`                        | $4.00 / MTok  | $20.00 / MTok  | $5.00 / MTok   | $0.40 / MTok  |
| `gpt-rosalind-research`, `gpt-rosalind-discovery` | $5.00 / MTok  | $25.00 / MTok  | $5.00 / MTok   | $0.50 / MTok  |
| `gpt-5.6-cyber`                                   | $12.50 / MTok | $75.00 / MTok  | $15.625 / MTok | $1.25 / MTok  |
| `gpt-5.5-pro`, `gpt-5.4-pro`                      | $30.00 / MTok | $180.00 / MTok | $30.00 / MTok  | $30.00 / MTok |
| `gpt-5-pro`                                       | $15.00 / MTok | $120.00 / MTok | $15.00 / MTok  | $15.00 / MTok |
| `gpt-5.5`                                         | $5.00 / MTok  | $30.00 / MTok  | $5.00 / MTok   | $0.50 / MTok  |
| `gpt-5.4`                                         | $2.50 / MTok  | $15.00 / MTok  | $2.50 / MTok   | $0.25 / MTok  |
| `gpt-5.4-mini`                                    | $0.75 / MTok  | $4.50 / MTok   | $0.75 / MTok   | $0.075 / MTok |
| `gpt-5.4-nano`                                    | $0.20 / MTok  | $1.25 / MTok   | $0.20 / MTok   | $0.02 / MTok  |
| `gpt-5.3-codex`, `gpt-5.2-codex`, `gpt-5.2`       | $1.75 / MTok  | $14.00 / MTok  | $1.75 / MTok   | $0.175 / MTok |
| `gpt-5` (base / other `gpt-5*`)                   | $1.25 / MTok  | $10.00 / MTok  | $1.25 / MTok   | $0.125 / MTok |
| `gpt-4.1`                                         | $2.00 / MTok  | $8.00 / MTok   | $2.00 / MTok   | $0.50 / MTok  |
| `gpt-4.1-mini`                                    | $0.40 / MTok  | $1.60 / MTok   | $0.40 / MTok   | $0.10 / MTok  |
| `gpt-4.1-nano`                                    | $0.10 / MTok  | $0.40 / MTok   | $0.10 / MTok   | $0.025 / MTok |
| `gpt-4.5-preview`                                 | $75.00 / MTok | $150.00 / MTok | $75.00 / MTok  | $37.50 / MTok |
| `gpt-4o-mini`                                     | $0.15 / MTok  | $0.60 / MTok   | $0.15 / MTok   | $0.075 / MTok |
| `gpt-4o-2024-05-13`                               | $5.00 / MTok  | $15.00 / MTok  | $5.00 / MTok   | $5.00 / MTok  |
| `gpt-4-turbo`, `gpt-4-1106`, `gpt-4-0125`         | $10.00 / MTok | $30.00 / MTok  | $10.00 / MTok  | $10.00 / MTok |
| `gpt-4-32k`                                       | $60.00 / MTok | $120.00 / MTok | $60.00 / MTok  | $60.00 / MTok |
| `gpt-4` (classic 8k / 0613)                       | $30.00 / MTok | $60.00 / MTok  | $30.00 / MTok  | $30.00 / MTok |
| `gpt-4o` (base / other `gpt-4*`)                  | $2.50 / MTok  | $10.00 / MTok  | $2.50 / MTok   | $1.25 / MTok  |

Additional Standard text tiers from the same official rate card:

| Model                                  | Input / MTok | Output / MTok | Cached input / MTok |
| -------------------------------------- | ------------ | ------------- | ------------------- |
| `gpt-5-mini`                           | $0.25        | $2.00         | $0.025              |
| `gpt-5-nano`                           | $0.05        | $0.40         | $0.005              |
| `gpt-5.2-pro`                          | $21.00       | $168.00       | unavailable         |
| `gpt-5.5-cyber`                        | $12.50       | $75.00        | $1.25               |
| `o1`                                   | $15.00       | $60.00        | $7.50               |
| `o1-pro`                               | $150.00      | $600.00       | unavailable         |
| `o3`                                   | $2.00        | $8.00         | $0.50               |
| `o3-pro`                               | $20.00       | $80.00        | unavailable         |
| `o3-mini`                              | $1.10        | $4.40         | $0.55               |
| `o4-mini`                              | $1.10        | $4.40         | $0.275              |
| `gpt-3.5-turbo` / `gpt-3.5-turbo-0125` | $0.50        | $1.50         | unavailable         |

These tiers have no separate cache-write fee; recorded cache creation is
estimated at ordinary input price. Models without prompt caching also use
ordinary input price for any cached tokens reported by a transcript.
Dated `o1`/`o3`/`o4-mini` snapshots use their base model's card.

GPT-6.1 Sol's cache reads cost 5% of input; its predecessor GPT-6 Sol retains
10%. See the [GPT-6.1 Sol model card](https://developers.openai.com/api/docs/models/gpt-6.1-sol).
The deprecated [Daybreak Blue alias](https://developers.openai.com/api/docs/models/gpt-daybreak-blue-latest)
currently resolves to GPT-5.6 Sol, so it uses that card. Rosalind rates are in
the official pricing table; separate cache-write pricing does not apply, so
recorded cache creation is estimated at ordinary input price.

GPT-6 rates are from the [official pricing table](https://developers.openai.com/api/docs/pricing).
For GPT-6 requests above 272K input tokens, OpenAI doubles input and cache
rates and multiplies output rates by 1.5. Batch and Flex cost 50% of Standard;
Fast mode costs 2x the applicable rates; GPT-6.1 Sol Ultrafast costs 6x. Claudex uses Standard short-context
estimates because its aggregated usage does not reliably retain these billing
conditions; it does not apply a request threshold to session totals.

OpenAI also publishes these higher **Standard long-context** rates:

| Model match     | Input         | Output        | Cache write   | Cache read   |
| --------------- | ------------- | ------------- | ------------- | ------------ |
| `gpt-6-astra`   | $20.00 / MTok | $75.00 / MTok | $25.00 / MTok | $2.00 / MTok |
| `gpt-6.1-sol`   | $4.00 / MTok  | $15.00 / MTok | $5.00 / MTok  | $0.20 / MTok |
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
Gemini 3.6–3.8 Flash rates are promotional through December 31, 2026; review
them before the scheduled increase.

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
- `sonnet-5-5` / `sonnet-5.5` → Sonnet 5.5, including cheaper cache reads.
- `haiku-5-5` / `haiku-5.5` → Haiku 5.5 short-context card.
- `sonnet-5` → the permanent Sonnet 5 card;
  other `sonnet` ids and missing/empty legacy model ids → standard Sonnet.
- `haiku-4-5` → Haiku 4.5; `3-haiku` (but not `3-5-haiku`) → the cheapest
  Claude 3 Haiku tier; any other `haiku` → Haiku 3.5 legacy.
- `gpt-6-astra` / `gpt-6.1-sol` / `gpt-6-sol` / `gpt-6-luna` → dedicated rates and family labels.
- `gpt-5.6` alias / `gpt-5.6-sol` / `terra` / `luna` → dedicated rates and family labels;
  `gpt-5.6-cyber` → its specialized rate card and `Cyber` label. Unknown
  `gpt-5.6-*` suffixes stay unpriced until their rates are verified.
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
**family label** (`models` command) is `Astra`/`Sol`/`Luna` for GPT-6, `Sol`/`Terra`/`Luna`/`Cyber` for GPT-5.6, or
`Fable`/`Mythos`/`Opus`/`Haiku`/`Sonnet`/`GPT-5`/`GPT-4`/etc. — it does not distinguish latest from legacy
(or fast from standard), but the **cost** does.

## Recorded speed and service tiers

Research verified October 21, 2026. A requested speed, current configuration,
subscription allowance, and the tier actually used are different evidence.
Claudex never applies today's speed setting to historical requests.

| Harness / record                    | What can be recovered                                                                                                                                   | Claudex calculation                                                                                                                  |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Codex CLI and desktop rollout JSONL | Token counts and model; sampled desktop `turn_context` and `token_count` records have no service tier. The upstream request to persist it remains open. | Standard API-equivalent estimate; no inferred fast/ultrafast surcharge.                                                              |
| Claude Code                         | Assistant `message.usage.speed` records `fast` or `standard`; `service_tier` is a separate field.                                                       | Explicit fast-token subsets use the supported Opus fast rate, including mixed-speed sessions. Missing speed stays at standard rates. |
| Pi / OMP                            | Assistant `usage.cost.total` when recorded.                                                                                                             | Preserve the harness-supplied cost, including zero. A harness estimate does not prove the server billed that tier.                   |
| OpenCode                            | Assistant message `cost` when recorded.                                                                                                                 | Preserve the supplied cost.                                                                                                          |
| Cline / Roo Code                    | Per-request cost or task `totalCost`, depending on source format.                                                                                       | Preserve the supplied cost; no independent speed inference.                                                                          |
| OpenClaw                            | Trajectory cost and optional service-tier metadata; session-state `fastMode` may also exist.                                                            | Preserve supplied cost. A session-level `fastMode` flag is not per-request billing evidence.                                         |
| Copilot CLI / VS Code               | CLI premium-request metadata; VS Code transcripts lack token usage.                                                                                     | CLI USD is an API-equivalent estimate; VS Code is unpriced. Speed is not reconstructed.                                              |
| Gemini CLI                          | Token usage; no normalized per-request speed billing evidence in the supported record shape.                                                            | Standard rate-card estimate.                                                                                                         |

OpenAI's [Fast mode API documentation](https://developers.openai.com/api/docs/guides/fast-mode)
identifies the **response** `service_tier` as evidence of the tier used; the
request can specify `fast` or its `priority` alias. Codex's
[upstream logging issue](https://github.com/openai/codex/issues/20311) describes
the missing rollout field. Local state database schemas inspected for this
investigation did not provide an authoritative historical tier either.
Debug logs may expose it temporarily, but are not a stable source for backfilling
requests. Neither current `config.toml` nor model names establish past speed.

Anthropic explicitly documents
[`usage.speed`](https://platform.claude.com/docs/en/build-with-claude/fast-mode#checking-which-speed-was-used)
as the actual speed. Claudex stores the fast-token subset alongside computed
costs, so automatic repricing preserves mixed-speed estimates even after the
source transcript is deleted. Existing readable Claude transcripts are reparsed
on upgrade; retained rows without a stored speed breakdown remain standard
estimates because the missing evidence cannot be recovered.

For future custom harness integrations, persist the response ID, model,
request timestamp, per-request token buckets, actual response tier/speed,
and provider cost when available. Keep response tier separate from requested
speed, numerical throughput, and reasoning effort. OpenAI tier metadata absent
from the supported Codex rollout shape is currently not consumed by claudex;
adding a tier to a custom log alone does not enable tier-aware OpenAI pricing.

The [Codex pricing documentation](https://learn.chatgpt.com/docs/pricing)
lists different multipliers for different accounting units: Fast consumes
included subscription usage at **2.5x**, but purchased credits and Enterprise
pay-as-you-go usage at **2x**. Supported Astra/Sol Ultrafast modes use **8x**
included usage and **6x** purchased-credit/pay-as-you-go usage. These are not
multipliers on raw recorded token counts. Claudex's USD estimate does not
represent subscription dollars spent or a percentage of an account allowance.

## Provider-supplied cost

Pi computes a cost for every assistant message (and reports `$0` for local
Ollama models), and OpenClaw records a running total per trajectory. claudex
**trusts those figures** rather than re-deriving them from the tier table — so
a Pi or OpenClaw session's cost reflects the figure the harness recorded,
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

For Codex, `last_token_usage` values are summed when available, with the last
cumulative `total_token_usage` as a legacy fallback. Cached input is billed as
a cache read. Each rollout UUID contributes once; independently billed forked
children are additive. Archive moves and `.jsonl.zst` compression preserve
rollout identity, and compressed sources can be indexed directly. Legacy copies
remain in the physical tables with `superseded_by` set; reports exclude these
aliases. Unfiltered raw SQL sums over physical rows still include copies. Alias
messages and other derived rows remain recoverable through raw SQLite queries
or the library's `query_session_detail` with the exact physical path; normal
search and session listings use the canonical copy. The current
Codex parser uses the first recorded model for the rollout; model switches
within one rollout remain an estimation limitation.

## Why it's approximate

- **Incomplete billing conditions.** OpenAI speed tiers, long-context bands,
  regional surcharges, batch pricing, and discounts are not reconstructed.
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
$15 output, etc.). Opus 5 / 4.5–4.8 is $5/$25, or 2.5× Sonnet 5 on base input
and output. **Current** Opus 5.5 is $4/$20, or 2× Sonnet 5 — so do _not_ assume a 5× multiple for present-day Opus
sessions. If an Opus cost looks lower than you expect, that's usually the
current Opus rate card, not an error; mid-session model switching can also
lower it.

## Rendering

- `fmt_cost` renders `$12,345.67` with thousands separators.
- Values below one cent fall back to four decimals: `$0.0042`. Tiny sessions
  don't disappear into `$0.00`.
- JSON output always uses raw `cost_usd` floats — no formatting.

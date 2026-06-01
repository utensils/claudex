# `timeline`

Daily or weekly usage trend.

```bash
claudex timeline [--weekly] [--limit N] [--json] [filters]
```

Each bucket includes sessions, cost, token totals, tool calls, PR count, and
average turn duration.

Examples:

```bash
claudex timeline --since 30d
claudex timeline --weekly --limit 12
claudex timeline --provider codex --json
```

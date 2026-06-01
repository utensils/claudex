# `budget`

Monthly budget burn and projection.

```bash
claudex budget --monthly USD [--json] [filters]
```

By default the period starts on the first day of the current local month. Pass
`--since` / `--until` to scope the calculation differently.

Examples:

```bash
claudex budget --monthly 250
claudex budget --monthly 50 --provider codex --json
```

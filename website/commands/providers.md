# `providers`

Provider health and sync status.

```bash
claudex providers [--json] [--deep] [filters]
```

Shows each enabled provider root, discovered transcript count, indexed rows,
live vs retained history, archived rows, and last sync time. `--deep` also
parses every discovered transcript and counts failures; it can be slower on
large histories.

Examples:

```bash
claudex providers
claudex providers --provider codex,openclaw --json
claudex providers --deep
```

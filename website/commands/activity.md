# `activity`

One report for recent agent work.

```bash
claudex activity [--limit N] [--json] [filters]
```

Combines a summary with recent sessions, recent PRs, hot files, and slow
projects. It is meant for quick check-ins like "what happened today?"

Examples:

```bash
claudex activity --since 24h
claudex activity --limit 10
claudex activity --provider claude,codex --json
```

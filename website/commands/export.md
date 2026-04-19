# `export`

Export a session transcript as Markdown or JSON.

## Usage

```bash
claudex export <selector> [--format markdown|json]
                           [-o/--output <path>]
                           [-p/--project <substr>]
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `<selector>` | — | Session ID prefix _or_ project name substring. Required. |
| `--format` | `markdown` | `markdown` or `json`. |
| `-o`, `--output <path>` | stdout | Write to a file instead of stdout. |
| `-p`, `--project <substr>` | — | Disambiguate when the selector matches more than one session. |

## Example

```bash
# List recent sessions to find a prefix
claudex sessions --limit 5

# Export as Markdown to stdout
claudex export e1a2f4

# Save to a file
claudex export e1a2f4 --output session.md

# JSON instead
claudex export e1a2f4 --format json --output session.json

# Ambiguous prefix? Add --project
claudex export ab --project claudex
```

## Markdown output

The Markdown export renders the full conversation with:

- A frontmatter block (session ID, project, first/last timestamps, model,
  message count).
- Each user and assistant turn as a section.
- Tool-call blocks rendered as fenced code blocks with the tool name and input.
- Tool-result blocks rendered inline.

This is the shape you want for pasting into docs, wikis, or PR descriptions.

## JSON output

The JSON export is the full parsed session — every record, preserving
timestamps, message IDs, and tool-call structure. Use this when you want to
post-process a session with your own tooling.

## Notes

- **Selector precedence.** Session-ID prefix wins if there's an exact match,
  even when `--project` is set. Fall back to the project filter otherwise.
- **No `--json` flag.** Use `--format json` — it's the format of the export,
  not a summary layer.
- **Large sessions.** Markdown export is streaming where possible, but huge
  sessions (100+ MB JSONL) can produce very long Markdown files. Use
  `--format json` if you want to slice the output yourself.

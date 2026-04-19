# `search`

Full-text search across every user and assistant message in every session.

## Usage

```bash
claudex search <query> [-p/--project <substr>]
                        [-l/--limit <n>] [--json]
                        [--case-sensitive]
                        [--no-index]
```

## Flags

| Flag                       | Default | Description                                          |
| -------------------------- | ------- | ---------------------------------------------------- |
| `<query>`                  | —       | The text to search for. Positional, required.        |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path.       |
| `-l`, `--limit <n>`        | `20`    | Maximum hits to print.                               |
| `--json`                   | off     | Emit structured hits instead of highlighted text.    |
| `--case-sensitive`         | off     | Drop back to a file scan (FTS5 is case-insensitive). |
| `--no-index`               | off     | Scan JSONL files directly.                           |

## Example

```bash
# Where did I first talk about schema migrations?
claudex search "schema migration" --limit 1

# All hits in one project
claudex search serde --project claudex --limit 50

# Case-sensitive (slower)
claudex search CamelCaseThing --case-sensitive
```

## How it works

The index has an FTS5 virtual table `messages_fts` over every message. It uses
the `porter unicode61` analyzer (stemming, unicode-aware tokenization). Queries
accept FTS5 syntax:

| Syntax       | Meaning                      |
| ------------ | ---------------------------- |
| `foo bar`    | Match both words, any order. |
| `"foo bar"`  | Phrase match.                |
| `foo OR bar` | Either.                      |
| `foo -bar`   | `foo` but not `bar`.         |

Special characters (`{`, `[`, `.`, `/`) aren't tokens — search the word next
to them.

## Output

Each hit prints:

```
<project> <session-id-prefix> [YYYY-MM-DD] <role>
<matching line, query highlighted>
```

Only lines that contain the query are printed — not the entire message. This
keeps output scannable.

## JSON shape

```json
[
  {
    "project": "/Users/you/projects/claudex",
    "session_id": "0272bcdb-aea1-4d8c-b80c-809b07154b8a",
    "message_timestamp": "2026-04-19T02:50:53.545+00:00",
    "message_type": "assistant",
    "snippet": "…fixing the [[migration]] query path…",
    "rank": -7.28401
  }
]
```

## Notes

- **Highlight markers.** JSON `snippet` values wrap each match in `[[…]]`
  so consumers can reproduce highlighting. Strip them with a `s/\[\[|\]\]//g`
  if you want plain text. Both the indexed and `--no-index` paths emit the
  same markers.
- **Case sensitivity.** FTS5 always lowercases tokens. `--case-sensitive`
  falls through to a file-scan path that checks the raw text.
- **Stemming.** `migrat` matches `migration`, `migrated`, `migrates`. The
  porter stemmer is aggressive — you may get hits that look like near-misses.
- **Freshness.** Search always calls `ensure_fresh` first, so hits reflect
  state from at most 5 minutes ago. Run `claudex index` to force a sync.

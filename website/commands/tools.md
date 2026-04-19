# `tools`

Tool-usage frequency — how often you reach for Bash, Edit, Read, Grep, etc.

## Usage

```bash
claudex tools [-p/--project <substr>] [--per-session]
              [-l/--limit <n>] [--json] [--no-index]
```

## Flags

| Flag                       | Default | Description                                    |
| -------------------------- | ------- | ---------------------------------------------- |
| `-p`, `--project <substr>` | —       | Filter by substring match on the project path. |
| `--per-session`            | off     | Break down per session instead of aggregating. |
| `-l`, `--limit <n>`        | `20`    | Maximum rows.                                  |
| `--json`                   | off     | Emit JSON.                                     |
| `--no-index`               | off     | Scan JSONL files directly.                     |

## Example

```bash
# Global tool usage
claudex tools --limit 15

# Tool usage inside one project
claudex tools --project claudex

# Per-session breakdown for one project
claudex tools --project claudex --per-session --limit 50
```

## Columns (aggregated)

| Column | Source                                     |
| ------ | ------------------------------------------ |
| Tool   | Tool name as reported by Claude Code.      |
| Count  | Total invocations across matched sessions. |

## Columns (per-session)

The per-session table lists each session with a `tool=count` cell for every
tool the session touched.

| Column  | Source                          |
| ------- | ------------------------------- |
| Project | Decoded project name.           |
| Session | 8-character session ID prefix.  |
| Date    | First timestamp of the session. |
| Tools   | Tool invocations as a map.      |

## JSON shape

### Aggregated (default)

```json
[
  { "tool": "Bash", "count": 25963 },
  { "tool": "Read", "count": 12870 }
]
```

### Per-session (`--per-session`)

```json
[
  {
    "project": "/Users/you/projects/claudex",
    "session_id": "0272bcdb-aea1-4d8c-b80c-809b07154b8a",
    "date": "2026-04-19T02:50:53.545+00:00",
    "tools": {
      "Bash": 2,
      "Edit": 14,
      "Read": 7
    }
  }
]
```

Note the per-session `tools` is a **nested object** (`tool_name → count`),
not a flat array. To flatten in jq:

```bash
claudex tools --per-session --json \
  | jq '.[] | {session: .session_id, tools: (.tools | to_entries | map({tool: .key, count: .value}))}'
```

## Notes

- **MCP tools.** Tools exposed by MCP servers show up under their real names
  (e.g. `mcp__playwright__browser_click`). That's useful signal if you want to
  see how much you lean on a given server.
- **Zero-use tools.** Tools that exist in Claude Code but you haven't used
  don't appear — there's nothing to count.

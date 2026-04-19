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

| Column   | Source                                     |
| -------- | ------------------------------------------ |
| Tool     | Tool name as reported by Claude Code.      |
| Calls    | Total invocations across matched sessions. |
| Sessions | Distinct sessions that used the tool.      |

## Columns (per-session)

| Column  | Source                         |
| ------- | ------------------------------ |
| Project | Decoded project name.          |
| Session | 8-character session ID prefix. |
| Tool    | Tool name.                     |
| Calls   | Invocations in that session.   |

## JSON shape

Aggregated:

```json
[
  { "tool": "Edit", "calls": 1240, "sessions": 92 },
  { "tool": "Bash", "calls": 988, "sessions": 110 }
]
```

Per-session:

```json
[
  {
    "project": "claudex",
    "session_id": "e1a2f4...",
    "tool": "Edit",
    "calls": 14
  }
]
```

## Notes

- **MCP tools.** Tools exposed by MCP servers show up under their real names
  (e.g. `mcp__playwright__browser_click`). That's useful signal if you want to
  see how much you lean on a given server.
- **Zero-use tools.** Tools that exist in Claude Code but you haven't used
  don't appear — there's nothing to count.

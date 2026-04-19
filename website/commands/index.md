# Commands overview

Every subcommand is listed here with a one-line summary. Click through for
flags, examples, and the JSON shape.

Global flag: `--color auto|always|never` (respects `NO_COLOR`).

## Read-only reports

| Command                          | What it does                                                   |
| -------------------------------- | -------------------------------------------------------------- |
| [`summary`](/commands/summary)   | Dashboard — sessions, cost, top projects/tools, model mix.     |
| [`sessions`](/commands/sessions) | List sessions grouped by project.                              |
| [`cost`](/commands/cost)         | Token usage and approximate cost per project (or per session). |
| [`search`](/commands/search)     | Full-text search across session messages (FTS5).               |
| [`tools`](/commands/tools)       | Tool-usage frequency, optionally per session.                  |
| [`models`](/commands/models)     | Call counts, token usage, and cost per model.                  |
| [`turns`](/commands/turns)       | Per-turn timing (avg / p50 / p95 / max).                       |
| [`prs`](/commands/prs)           | Sessions linked to pull requests.                              |
| [`files`](/commands/files)       | Most frequently modified files across sessions.                |

### Flag support matrix

Not every report accepts every flag. Consult the per-command page for exact
usage; the matrix below is the quick overview.

| Command    | `--project` | `--limit` | `--json` | `--no-index` |
| ---------- | :---------: | :-------: | :------: | :----------: |
| `summary`  |      —      |     —     |    ✓     |      ✓       |
| `sessions` |      ✓      |     ✓     |    ✓     |      ✓       |
| `cost`     |      ✓      |     ✓     |    ✓     |      ✓       |
| `search`   |      ✓      |     ✓     |    —     |      ✓       |
| `tools`    |      ✓      |     ✓     |    ✓     |      ✓       |
| `models`   |      ✓      |     —     |    ✓     |      —       |
| `turns`    |      ✓      |     ✓     |    ✓     |      —       |
| `prs`      |      ✓      |     ✓     |    ✓     |      —       |
| `files`    |      ✓      |     ✓     |    ✓     |      —       |

Notes:

- `search` is output-oriented (highlighted hits); it has no `--json` shape.
  Case-sensitive queries fall back to a file scan automatically.
- `turns`, `prs`, `files`, and `models` derive their data from the index
  only — there's no file-scan fallback path, so `--no-index` isn't accepted.
- `summary` is a whole-tree dashboard; filtering by project or row limit
  doesn't apply.

## Actions

| Command                                | What it does                                          |
| -------------------------------------- | ----------------------------------------------------- |
| [`export`](/commands/export)           | Dump a session transcript as Markdown or JSON.        |
| [`watch`](/commands/watch)             | Tail Claude Code's `--debug-file` log in real time.   |
| [`index`](/commands/index-cmd)         | Manage the SQLite index — force sync or full rebuild. |
| [`completions`](/commands/completions) | Generate shell completion scripts.                    |

## Conventions

- **Project filter.** `--project foo` matches any session whose decoded project
  path contains `foo`. Worktree sessions roll up to their parent project.
- **Session selector.** Commands that take a session (currently just
  [`export`](/commands/export)) match on session-ID prefix or project name.
- **Limit default.** Most commands default to `--limit 20`. Pass a higher
  number for more rows.
- **Thousands separators.** Token counts and message counts render as
  `326,297`. Costs render as `$12,345.67`, falling back to `$0.0042` for
  sub-cent values.

## Quick alphabetical index

- [cost](/commands/cost)
- [completions](/commands/completions)
- [export](/commands/export)
- [files](/commands/files)
- [index](/commands/index-cmd)
- [models](/commands/models)
- [prs](/commands/prs)
- [search](/commands/search)
- [sessions](/commands/sessions)
- [summary](/commands/summary)
- [tools](/commands/tools)
- [turns](/commands/turns)
- [watch](/commands/watch)

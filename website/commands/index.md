# Commands overview

Every subcommand is listed here with a one-line summary. Click through for
flags, examples, and the JSON shape.

Global flag: `--color auto|always|never` (respects `NO_COLOR`).

Reports span Claude Code, OpenAI Codex, GitHub Copilot (CLI and VS Code), Pi, and OpenClaw by default. Every report accepts
the shared [filter flags](/guide/providers) — `--provider`, `--model`,
`--since`/`--until`, `--on-disk-only`. `--project` is a separate per-command
filter (it has its own column in the matrix below).

## Read-only reports

| Command                            | What it does                                                     |
| ---------------------------------- | ---------------------------------------------------------------- |
| [`summary`](/commands/summary)     | Dashboard — sessions, cost, top projects/tools, model mix.       |
| [`sessions`](/commands/sessions)   | List sessions grouped by project (all providers).                |
| [`session`](/commands/session)     | Drill into one session: spend, files, tools, PRs, turns.         |
| [`cost`](/commands/cost)           | Token usage and approximate cost per project (or per session).   |
| [`search`](/commands/search)       | Full-text search across session messages (FTS5), with JSON hits. |
| [`tools`](/commands/tools)         | Tool-usage frequency, optionally per session.                    |
| [`models`](/commands/models)       | Call counts, token usage, and cost per model.                    |
| [`turns`](/commands/turns)         | Per-turn timing (avg / p50 / p95 / max).                         |
| [`prs`](/commands/prs)             | Sessions linked to pull requests.                                |
| [`files`](/commands/files)         | Most frequently modified files across sessions.                  |
| [`providers`](/commands/providers) | Provider roots, sync status, retention, and parse diagnostics.   |
| [`timeline`](/commands/timeline)   | Daily or weekly usage trend.                                     |
| [`budget`](/commands/budget)       | Monthly budget burn and projection.                              |
| [`activity`](/commands/activity)   | Recent sessions, PRs, files, and slow projects.                  |

### Flag support matrix

Not every report accepts every flag. Consult the per-command page for exact
usage; the matrix below is the quick overview.

| Command     | filters | `--project` | `--limit` | `--json` | `--no-index` |
| ----------- | :-----: | :---------: | :-------: | :------: | :----------: |
| `summary`   |    ✓    |      —      |     —     |    ✓     |      ✓       |
| `sessions`  |    ✓    |      ✓      |     ✓     |    ✓     |      ✓       |
| `session`   |    —    |      ✓      |     —     |    ✓     |      ✓       |
| `cost`      |    ✓    |      ✓      |     ✓     |    ✓     |      ✓       |
| `search`    |    ✓    |      ✓      |     ✓     |    ✓     |      ✓       |
| `tools`     |    ✓    |      ✓      |     ✓     |    ✓     |      ✓       |
| `models`    |    ✓    |      ✓      |     —     |    ✓     |      —       |
| `turns`     |    ✓    |      ✓      |     ✓     |    ✓     |      —       |
| `prs`       |    ✓    |      ✓      |     ✓     |    ✓     |      —       |
| `files`     |    ✓    |      ✓      |     ✓     |    ✓     |      —       |
| `providers` |    ✓    |      —      |     —     |    ✓     |      —       |
| `timeline`  |    ✓    |      —      |     ✓     |    ✓     |      —       |
| `budget`    |    ✓    |      —      |     —     |    ✓     |      —       |
| `activity`  |    ✓    |      —      |     ✓     |    ✓     |      —       |

Notes:

- **filters** = the shared [`--provider` / `--model` / `--since` / `--until` /
  `--on-disk-only`](/guide/providers) set.
- `search` supports `--json`; case-sensitive queries still fall back to a
  file scan automatically.
- `turns`, `prs`, `files`, and `models` derive their data from the index
  only — there's no file-scan fallback path, so `--no-index` isn't accepted.
- `summary` is a whole-index dashboard spanning all providers; the shared
  filters apply, but there is no row limit.
- `summary` also accepts `--plan <api|flat-monthly:USD>` to reframe the cost
  section for flat-fee subscribers. See [`summary`](/commands/summary).

## Actions

| Command                                | What it does                                                                            |
| -------------------------------------- | --------------------------------------------------------------------------------------- |
| [`export`](/commands/export)           | Dump a session transcript as Markdown or JSON.                                          |
| [`watch`](/commands/watch)             | Tail Claude Code's `--debug-file` log in real time.                                     |
| [`index`](/commands/index-cmd)         | Manage the SQLite index — force sync or full rebuild.                                   |
| [`update`](/commands/update)           | Self-update claudex, or print the right upgrade recipe for Nix / cargo / brew installs. |
| [`completions`](/commands/completions) | Generate shell completion scripts.                                                      |
| [`skills`](/commands/skills)           | Generate or install the claudex agent skill for Claude Code, Codex, Pi, or OpenClaw.    |

## Conventions

- **Project filter.** `--project foo` matches any session whose decoded project
  path contains `foo`. Worktree sessions roll up to their parent project.
- **Session selector.** Commands that take a session (currently
  [`session`](/commands/session) and [`export`](/commands/export)) match on
  session-ID prefix or project name.
- **Limit default.** Most commands default to `--limit 20`. Pass a higher
  number for more rows.
- **Thousands separators.** Token counts and message counts render as
  `326,297`. Costs render as `$12,345.67`, falling back to `$0.0042` for
  sub-cent values.

## Quick alphabetical index

- [activity](/commands/activity)
- [budget](/commands/budget)
- [completions](/commands/completions)
- [cost](/commands/cost)
- [export](/commands/export)
- [files](/commands/files)
- [index](/commands/index-cmd)
- [models](/commands/models)
- [providers](/commands/providers)
- [prs](/commands/prs)
- [search](/commands/search)
- [session](/commands/session)
- [sessions](/commands/sessions)
- [skills](/commands/skills)
- [summary](/commands/summary)
- [timeline](/commands/timeline)
- [tools](/commands/tools)
- [turns](/commands/turns)
- [update](/commands/update)
- [watch](/commands/watch)

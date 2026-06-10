# claudex-cli

Command-line interface for claudex.

This package installs a binary named `claudex`:

```bash
cargo install claudex-cli
claudex summary
claudex search "migration" --context 1
```

To install a specific release:

```bash
cargo install claudex-cli --version 0.10.1 # x-release-please-version
```

The CLI indexes Claude Code, OpenAI Codex, Pi, and OpenClaw transcripts into
`~/.claudex/index.db` and exposes reports as subcommands with human tables or
`--json` output. For embedding claudex in another Rust application, depend on
the `claudex` library crate instead.

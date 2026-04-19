# `completions`

Generate shell completion scripts.

## Usage

```bash
claudex completions <shell>
```

`<shell>` is one of: `bash`, `zsh`, `fish`, `elvish`, `powershell`.

## Setup

### zsh

```bash
# Add to ~/.zshrc
source <(claudex completions zsh)
```

Uses a custom completer (not clap's default) that:

- Separates subcommand candidates from flag candidates.
- Falls back to zsh's native `_files` for file-path flags
  (`--output`, `-o`, `--follow`) so tilde expansion works.

### bash

```bash
# Add to ~/.bashrc
source <(claudex completions bash)
```

Requires `bash-completion` to be installed and sourced.

### fish

```bash
# Test in the current shell
claudex completions fish | source

# Persist
claudex completions fish > ~/.config/fish/completions/claudex.fish
```

### elvish

```sh
eval (claudex completions elvish | slurp)
```

### PowerShell

```powershell
claudex completions powershell | Out-String | Invoke-Expression
```

## What gets completed

- Subcommand names at position 1.
- Flag names (long and short).
- Values for `--color` (`auto`, `always`, `never`).
- File paths for `--output`, `-o`, `--follow` (zsh only).

## Notes

- **Always current.** The script is generated from `Cli::command()` at
  runtime, so it matches the binary exactly. Re-source after upgrading.
- **See also:** [Shell completions guide](/guide/completions) for cross-shell
  tips and troubleshooting.

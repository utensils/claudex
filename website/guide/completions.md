# Shell completions

`claudex completions <shell>` generates a completion script. Source it from
your shell init file or persist to disk, depending on your shell.

Supported shells: **bash**, **zsh**, **fish**, **elvish**, **powershell**.

## zsh

```bash
# Add to ~/.zshrc
source <(claudex completions zsh)
```

The zsh script is a custom completer, not clap's default. It:

- Shows **subcommands** for `claudex <TAB>` and **flags** for
  `claudex --<TAB>` — no mixing.
- Falls back to zsh's native `_files` for file-path flags (`--output`, `-o`,
  `--follow`) so tilde expansion and directory traversal work naturally.

## bash

```bash
# Add to ~/.bashrc
source <(claudex completions bash)
```

## fish

```bash
# Quick test in the current shell
claudex completions fish | source

# Persist across sessions
claudex completions fish > ~/.config/fish/completions/claudex.fish
```

## elvish

```sh
eval (claudex completions elvish | slurp)
```

## PowerShell

```powershell
# Add to $PROFILE
claudex completions powershell | Out-String | Invoke-Expression
```

## What it completes

- Subcommand names at the first position.
- Flag names after `--` (and short flags after `-`).
- `--color` values: `auto`, `always`, `never`.
- File paths for `--output` / `-o` / `--follow` (zsh only — other shells fall
  back to their default behavior).

## Regenerating

The script is generated on the fly every time you call `claudex completions`,
so it always reflects the binary you're running. Re-source after upgrading if
you added new flags.

## Troubleshooting

**zsh shows "no such argument" on unknown subcommands.**
Compinit caches old completion metadata. Try `rm -f ~/.zcompdump*` and restart
the shell.

**bash shows nothing after `claudex `.**
Check that `bash-completion` is installed and sourced — claudex relies on it
as a carrier.

**fish completes flags but not values.**
The fish script uses clap_complete's dynamic mode; make sure you're running
fish 3.4 or newer.

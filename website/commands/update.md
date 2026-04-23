# `update`

Self-update claudex in place — or print the right upgrade command for your
package manager when in-place replacement would clobber a read-only store.

## Usage

```bash
claudex update [--check] [--force] [--version <tag>]
```

## Flags

| Flag                | Description                                                                             |
| ------------------- | --------------------------------------------------------------------------------------- |
| `--check`           | Only report whether an update is available. Does not touch disk or require write perms. |
| `--force`           | Reinstall or downgrade even when the target version matches the current one.            |
| `--version <tag>`   | Install a specific tag (e.g. `v0.2.0`) instead of the latest release.                   |

## What it does per install source

`claudex update` starts by resolving its own executable path
(`std::env::current_exe()` → canonical) and classifying the install:

| Install source | Detected by                  | Behaviour                                                             |
| -------------- | ---------------------------- | --------------------------------------------------------------------- |
| `install.sh`   | anything not matching below  | Downloads the release tarball, verifies SHA-256, swaps the binary.    |
| Nix            | path contains `/nix/store/`  | Prints `nix profile upgrade claudex` / `nix flake update` and exits.  |
| cargo          | path contains `/.cargo/bin/` | Prints `cargo install --git … --tag vX.Y.Z --force claudex` and exits. |
| Homebrew       | path contains `/Cellar/` or `/homebrew/` | Prints `brew upgrade claudex` and exits.                  |

The non-managed branches exit with a non-zero status so shell wrappers can
tell the difference between "did the upgrade" and "you need to run a different
command".

## How the latest tag is resolved

`claudex update` follows the redirect on
`https://github.com/utensils/claudex/releases/latest` and reads the final URL
(`…/releases/tag/vX.Y.Z`). **No call is made to `api.github.com`**, so the
command is never rate-limited for unauthenticated users — the same approach
`install.sh` takes.

## Examples

```bash
# Check without touching the binary
claudex update --check

# Install the latest release
claudex update

# Pin to a specific tag
claudex update --version v0.2.0

# Force a reinstall of the current version (e.g. recover from a corrupt binary)
claudex update --force
```

## How the swap works

When updating in place claudex:

1. Detects the correct asset for the current target triple
   (`claudex-<triple>.tar.gz`).
2. Downloads the tarball and `SHA256SUMS` for the target tag.
3. Verifies the tarball's SHA-256 against `SHA256SUMS`.
4. Extracts the `claudex` binary from the archive.
5. Writes to a sibling temp file (`.claudex-update-<pid>`), renames the
   current binary to `.old`, then renames the temp file into place. On
   failure the original is restored.
6. On macOS, clears `com.apple.quarantine` on the new binary.

All operations require write access to the directory containing the
binary; if the probe fails, the command exits early with a message about
re-running with `sudo` or reinstalling under `CLAUDEX_INSTALL_DIR`.

## Requirements

- `curl` on `PATH` — used for all network I/O.

## Notes

- **No `--json` output.** This is an action with side effects, not a report.
- **Checksum is mandatory.** `claudex update` always verifies SHA-256 before
  swapping the binary. There is no opt-out flag.
- **Schema unchanged.** This command doesn't touch `~/.claudex/index.db`; the
  next read command will sync the index against the upgraded binary on its
  normal cadence.

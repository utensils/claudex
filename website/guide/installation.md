# Installation

claudex is a single Rust binary. It runs on Linux and macOS, x86_64 and
aarch64. It needs no system dependencies at runtime — rusqlite is bundled.

Three supported install paths:

1. **One-line script** — prebuilt tarball from GitHub Releases (fastest).
2. **Cargo** — build from source off a tag or off `main`.
3. **Nix flake** — reproducible build via crane, with an app and a devshell.

Minimum supported Rust version (for source builds): **1.95**.

## Install script

The quickest path. Downloads a prebuilt, stripped tarball from the latest
release, verifies its SHA256, and drops `claudex` in `~/.local/bin`.

```bash
curl -fsSL https://raw.githubusercontent.com/utensils/claudex/main/install.sh | sh
```

### Pinning a version

```bash
curl -fsSL https://raw.githubusercontent.com/utensils/claudex/main/install.sh \
  | CLAUDEX_VERSION=v0.3.0 sh
```

### Changing the install directory

```bash
curl -fsSL https://raw.githubusercontent.com/utensils/claudex/main/install.sh \
  | CLAUDEX_INSTALL_DIR=/usr/local/bin sh
```

### What the script does

- Detects `uname -s` / `uname -m` and picks the matching asset:
  - `claudex-aarch64-apple-darwin.tar.gz`
  - `claudex-x86_64-apple-darwin.tar.gz`
  - `claudex-x86_64-unknown-linux-gnu.tar.gz`
  - `claudex-aarch64-unknown-linux-gnu.tar.gz`
- Downloads `SHA256SUMS` from the same release and verifies the tarball.
- Extracts, installs `claudex` mode `0755` into `$CLAUDEX_INSTALL_DIR`
  (default `~/.local/bin`).
- On macOS, clears the quarantine attribute so the binary can run.
- Warns if the install directory isn't on `$PATH`.

### Manual download

If you prefer to eyeball the release page and run the steps yourself:

```bash
# Pick the tarball for your platform
curl -LO https://github.com/utensils/claudex/releases/latest/download/claudex-aarch64-apple-darwin.tar.gz
curl -LO https://github.com/utensils/claudex/releases/latest/download/SHA256SUMS

# Verify
shasum -a 256 -c SHA256SUMS --ignore-missing   # macOS
sha256sum  -c SHA256SUMS --ignore-missing      # Linux

# Extract + install
tar xzf claudex-aarch64-apple-darwin.tar.gz
install -m 755 claudex ~/.local/bin/claudex
```

## Cargo

Build from source off the `main` branch:

```bash
cargo install --git https://github.com/utensils/claudex claudex
```

Pin to a specific tag:

```bash
cargo install --git https://github.com/utensils/claudex --tag v0.3.0 claudex
```

The binary lands in `~/.cargo/bin/claudex`. Make sure that directory is on
your `PATH`.

## Nix flake

The repo ships a flake with a reproducible build (crane) and a devshell.
Flakes must be enabled (`experimental-features = nix-command flakes`).

```bash
# Run without installing
nix run github:utensils/claudex -- summary

# Pin to a release tag
nix run github:utensils/claudex/v0.3.0 -- summary

# Install into the user profile
nix profile install github:utensils/claudex

# Build the binary into ./result (then ./result/bin/claudex)
nix build github:utensils/claudex

# Enter a dev shell for development (auto-activates via direnv + use_flake)
nix develop github:utensils/claudex
```

### As a NixOS / nix-darwin module input

In your system flake:

```nix
{
  inputs.claudex.url = "github:utensils/claudex";
  # Optional: dedupe nixpkgs
  inputs.claudex.inputs.nixpkgs.follows = "nixpkgs";

  outputs = { self, nixpkgs, claudex, ... }: {
    # home-manager / NixOS / nix-darwin — reference the package:
    environment.systemPackages = [ claudex.packages.${system}.default ];
  };
}
```

The package's `meta` is populated from `Cargo.toml` (description, homepage,
license, maintainer, `mainProgram = "claudex"`), so `lib.getExe` works out of
the box.

## From a local clone

```bash
git clone https://github.com/utensils/claudex
cd claudex

# Inside the Nix devshell (preferred)
nix develop
ci-local            # fmt-check → check → clippy → test → build

# Or straight cargo
cargo build --release
./target/release/claudex --help
```

The devshell exposes convenience commands (`build`, `build-release`, `check`,
`clippy`, `fmt`, `fmt-check`, `run-tests`, `ci-local`, `coverage`, `claudex`,
plus `docs-dev`, `docs-build`, `docs-preview`, `docs-fmt`, `docs-fmt-check`).
See the project
[CLAUDE.md](https://github.com/utensils/claudex/blob/main/CLAUDE.md) for the
full table.

## Verify the install

```bash
claudex --version
claudex --help
claudex summary        # first run will index ~/.claude/projects/
```

If `~/.claude/projects/` is empty (you've never run Claude Code), you'll see
an empty summary — that's expected.

## Shell completions

Completions are generated on demand by the `completions` subcommand. See the
[Shell completions](/guide/completions) guide for setup in zsh, bash, fish,
elvish, and PowerShell.

## Upgrading

The easiest path for any install source is to let claudex tell you:

```bash
claudex update --check      # report availability without writing anything
claudex update              # install-script: swap binary in place
                            # everything else: print the right upgrade command
```

`claudex update` classifies its own install path and only replaces the
binary when it was installed by `install.sh` (or copied into any writable
directory). For package-manager installs it exits with the right upgrade
recipe and non-zero status, so it's safe in automation.

See the [`update` command page](/commands/update) for flags
(`--check`, `--force`, `--version <tag>`) and the full story on how the
SHA-256 verification and atomic swap work.

By install source:

- **Install script:** `claudex update` — or rerun the one-liner, which also
  fetches the latest tarball and replaces the binary.
- **Cargo:** `cargo install --git https://github.com/utensils/claudex --tag vX.Y.Z --force claudex`.
- **Nix profile:** `nix profile upgrade '.*claudex.*'` (or remove + reinstall).
- **Nix flake input:** `nix flake update claudex` in your system flake.
- **Homebrew:** `brew upgrade claudex`.

The index at `~/.claudex/index.db` carries a `schema_version` — newer
binaries rebuild the index automatically on first run if the schema bumped.

## Uninstall

claudex writes only to `~/.claudex/` (the index and, if you use watch,
`~/.claudex/debug/`). To wipe it:

```bash
# Drop the index and debug logs
rm -rf ~/.claudex

# Remove the binary
rm ~/.local/bin/claudex       # install script
rm ~/.cargo/bin/claudex       # cargo
nix profile remove claudex    # nix profile
```

Nothing lives in `~/.claude/` that claudex owns — it only reads from there.

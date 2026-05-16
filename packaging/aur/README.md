# Arch User Repository (AUR) packaging

claudex is published to the AUR as three packages:

| Package | Source | Update cadence | Audience |
|---|---|---|---|
| [`claudex-bin`](./claudex-bin/PKGBUILD) | Repackages the upstream Linux tarball from GitHub Releases | Every tagged release (automatic via CI) | Most users — fastest install, no compile |
| [`claudex`](./claudex/PKGBUILD) | Builds from the release tarball | Every tagged release (automatic via CI) | Users who want a from-source build matching their toolchain |
| [`claudex-git`](./claudex-git/PKGBUILD) | Builds from `main` HEAD | Pushed manually when the build recipe changes | Bleeding-edge users tracking `main` |

The PKGBUILDs here are the source of truth. The AUR git repos
(`ssh://aur@aur.archlinux.org/<pkgname>.git`) are downstream mirrors
that CI force-publishes to on every release.

## Release flow

On a tag push (`v*`), `.github/workflows/release.yml` builds the
release artifacts as usual, then runs a `publish-aur` matrix job
(one entry per AUR package, currently `claudex-bin` and `claudex`).
The job:

1. Checks out the tagged commit.
2. Runs [`scripts/aur/update-pkgbuild.sh`](../../scripts/aur/update-pkgbuild.sh)
   which downloads the matching release artifact, computes the
   `sha256sum`, and rewrites `pkgver` + `sha256sums` in the PKGBUILD.
3. Uses [`KSXGitHub/github-actions-deploy-aur`](https://github.com/KSXGitHub/github-actions-deploy-aur)
   to regenerate `.SRCINFO` inside an Arch container, commit, and push
   to the AUR git repo over SSH.

`claudex-git` is **not** auto-published. Its `PKGBUILD` only changes
when the build recipe itself changes (deps, feature flags), so it is
pushed manually whenever someone edits it here.

## One-time setup

The CI job needs an SSH key registered with the AUR account that owns
the packages. This repo uses the maintainer's primary key:

1. Make sure `~/.ssh/id_ed25519.pub` is registered with the AUR
   account (https://aur.archlinux.org/account/<user> → "My Account"
   → "SSH Public Key").
2. Add the matching **private** key to this GitHub repository as
   the `AUR_SSH_PRIVATE_KEY` secret:
   ```bash
   gh secret set AUR_SSH_PRIVATE_KEY -R utensils/claudex < ~/.ssh/id_ed25519
   ```
3. Seed each AUR git repo with an initial push from a workstation:
   ```bash
   for pkg in claudex-bin claudex claudex-git; do
     git clone "ssh://aur@aur.archlinux.org/${pkg}.git" "/tmp/${pkg}"
     cp "packaging/aur/${pkg}/PKGBUILD" "packaging/aur/${pkg}/.SRCINFO" "/tmp/${pkg}/"
     (cd "/tmp/${pkg}" && git add . && git commit -m 'Initial upload' && git push)
   done
   ```
   After this first push, CI takes over for `claudex-bin` and
   `claudex`.

`.SRCINFO` regeneration happens inside the Arch container that the
deploy action runs, so contributors editing PKGBUILDs in PRs do not
need `makepkg` installed locally — CI handles it.

## Local PKGBUILD smoke test (macOS / Windows / non-Arch)

`packaging/aur/test/Dockerfile` is a headless Arch Linux container
that builds + installs a PKGBUILD and runs `claudex --version` as a
smoke test. It runs under Rosetta on Apple Silicon and qemu-user
elsewhere — Arch upstream only publishes x86_64 to Docker Hub.

```bash
# Build claudex-bin (the prebuilt-binary package). First run pulls
# ~500 MB (Arch base + Rust toolchain), takes ~1 min; subsequent
# runs reuse the cached image.
scripts/aur/test-in-docker.sh claudex-bin

# Build the from-source package (slow — full cargo release build
# of the workspace, ~10 min under emulation).
scripts/aur/test-in-docker.sh claudex

# Build the VCS package (clones main, then same as above).
scripts/aur/test-in-docker.sh claudex-git

# Force a clean image rebuild (drops the BuildKit layer cache).
scripts/aur/test-in-docker.sh --rebuild

# Drop into a shell after the build (the workdir is preserved).
scripts/aur/test-in-docker.sh --shell claudex-bin
```

Each invocation:

1. (Re)builds the image if missing.
2. Copies the PKGBUILD to a writable tmp dir inside the container
   (the host repo is mounted read-write at `/workspace` but makepkg
   writes its build tree alongside the PKGBUILD, so we work from a
   tmp dir to keep the host clean).
3. Runs `makepkg -si --noconfirm --needed` — builds, packages,
   installs via `pacman -U`.
4. Runs `claudex --version` to confirm the binary launches.

The container is bind-mounted to your repo, so editing PKGBUILDs on
the host and re-running picks up changes immediately — no image
rebuild needed.

## Editing a PKGBUILD locally (Arch host)

If you're on an Arch host with `makepkg` available:

```bash
cd packaging/aur/claudex-bin
# After editing PKGBUILD:
makepkg --printsrcinfo > .SRCINFO
makepkg -sci   # build + install locally to smoke-test
```

If you don't have `makepkg`, use the Docker-based smoke test above
or just edit the PKGBUILD and let CI refresh `.SRCINFO`. The AUR
rejects malformed pushes, so mistakes surface quickly.

## Upgrade behavior

`claudex update` exits cleanly with a hint on AUR installs — pacman
manages updates for AUR users. The `-bin` package follows tagged
releases (CI publishes one AUR commit per `v*` tag); the source
package rebuilds against the same tag; `-git` rebuilds whatever the
user pulls from `main` at install time.

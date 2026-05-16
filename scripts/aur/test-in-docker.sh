#!/usr/bin/env bash
# Build + install + smoke-test an in-tree PKGBUILD inside an
# Arch Linux container. See packaging/aur/test/Dockerfile.
#
# Usage:
#   scripts/aur/test-in-docker.sh                   # claudex-bin (default)
#   scripts/aur/test-in-docker.sh claudex-bin
#   scripts/aur/test-in-docker.sh claudex
#   scripts/aur/test-in-docker.sh claudex-git
#   scripts/aur/test-in-docker.sh --rebuild [pkg]   # force image rebuild
#   scripts/aur/test-in-docker.sh --shell [pkg]     # drop into a shell
#                                                   # after the build
#
# Notes:
#   - On Apple Silicon the image runs under Rosetta (or qemu-user),
#     so the first run takes a few minutes for cold pacman + the
#     Rust toolchain bootstrap. Subsequent runs are cached.
#   - The repo is bind-mounted at /workspace, so edits to PKGBUILDs
#     on the host are picked up on the next invocation without an
#     image rebuild.
set -euo pipefail

IMAGE="claudex-aur-test"
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DOCKERFILE="${REPO_ROOT}/packaging/aur/test/Dockerfile"
CONTEXT="${REPO_ROOT}/packaging/aur/test"

# Pick whichever container runtime is on PATH. Linux contributors
# typically have podman (Arch default); macOS contributors usually
# have docker (Docker Desktop / OrbStack).
runtime=""
for candidate in docker podman; do
  if command -v "$candidate" >/dev/null 2>&1; then
    runtime="$candidate"
    break
  fi
done
if [ -z "$runtime" ]; then
  echo "error: neither docker nor podman is on PATH" >&2
  exit 1
fi

pkgname=""
rebuild=false
shell_after=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --rebuild) rebuild=true ;;
    --shell)   shell_after=true ;;
    -h|--help)
      sed -n '2,18p' "$0" >&2
      exit 0
      ;;
    claudex-bin|claudex|claudex-git)
      pkgname="$1"
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      exit 64
      ;;
  esac
  shift
done

pkgname="${pkgname:-claudex-bin}"

# (Re)build the image if asked or if it's missing. Cached layers
# make subsequent runs effectively instant — the only slow path is
# the initial pacman + Rust toolchain bootstrap.
if "$rebuild" || ! "$runtime" image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "==> building $IMAGE via $runtime"
  "$runtime" build -t "$IMAGE" -f "$DOCKERFILE" "$CONTEXT"
fi

echo "==> running $pkgname build + smoke test inside $IMAGE"

# The makepkg `cp` dance: the container's /workspace is bind-mounted
# from the host, which means makepkg's working directory would write
# into the host repo. Copy the PKGBUILD to a writable tmp dir so the
# host stays clean and the .gitignore stays accurate.
build_cmd=$(cat <<INNER
set -euo pipefail
workdir=\$(mktemp -d)
cp -a /workspace/packaging/aur/${pkgname}/. "\$workdir/"
cd "\$workdir"
echo "==> makepkg -si --noconfirm --needed (pkg: ${pkgname})"
makepkg -si --noconfirm --needed
echo "==> smoke test: \$(which claudex)"
claudex --version
echo "✓ ${pkgname} builds, installs, runs"
INNER
)

if "$shell_after"; then
  build_cmd="${build_cmd}"$'\n''echo "==> dropping into shell (the built workdir is $workdir)"; exec bash'
fi

# --init reaps zombies; --rm removes the container after exit.
exec "$runtime" run --rm -it --init \
  -v "${REPO_ROOT}:/workspace:rw" \
  "$IMAGE" \
  bash -lc "$build_cmd"

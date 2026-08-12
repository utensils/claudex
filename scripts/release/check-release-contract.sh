#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

version=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
if [ -z "$version" ]; then
  echo "error: missing [workspace.package] version" >&2
  exit 1
fi

for manifest in crates/claudex/Cargo.toml crates/claudex-cli/Cargo.toml; do
  grep -qx 'version.workspace = true' "$manifest" || {
    echo "error: $manifest must inherit the workspace version" >&2
    exit 1
  }
done

grep -q 'name = "claudex-cli"' release-plz.toml
grep -q 'pr_name = "chore: release v{{ version }}"' release-plz.toml
grep -q '^## Claudex v{{ release.next_version }}$' release-plz.toml
grep -q 'changelog_path = "./CHANGELOG.md"' release-plz.toml
grep -q 'changelog_include = \["claudex"\]' release-plz.toml
grep -q 'git_tag_name = "v{{ version }}"' release-plz.toml

dependency_version=$(sed -n 's/^claudex = .*version = "\([^"]*\)".*/\1/p' crates/claudex-cli/Cargo.toml)
if [ "$dependency_version" != "$version" ]; then
  echo "error: claudex-cli dependency version $dependency_version does not match $version" >&2
  exit 1
fi

if [ -e release-please-config.json ] || [ -e .release-please-manifest.json ]; then
  echo "error: legacy release-please configuration is still present" >&2
  exit 1
fi

if [ -e crates/claudex/CHANGELOG.md ] || [ -e crates/claudex-cli/CHANGELOG.md ]; then
  echo "error: package-local changelogs must not fragment the product history" >&2
  exit 1
fi

marked_files=(
  README.md
  crates/claudex/README.md
  crates/claudex-cli/README.md
  website/.vitepress/config.ts
  website/reference/library.md
)
marker_count=0
while IFS= read -r line; do
  marker_count=$((marker_count + 1))
  marked_version=$(printf '%s\n' "$line" | grep -Eo 'v?[0-9]+\.[0-9]+\.[0-9]+' | head -1)
  if [ "${marked_version#v}" != "$version" ]; then
    echo "error: marked version does not match $version: $line" >&2
    exit 1
  fi
done < <(grep -H 'x-release-plz-version' "${marked_files[@]}")

if [ "$marker_count" -ne 6 ]; then
  echo "error: expected 6 release version markers, found $marker_count" >&2
  exit 1
fi

if grep -R -q 'x-release-please-version' "${marked_files[@]}"; then
  echo "error: legacy release-please version marker remains" >&2
  exit 1
fi

echo "release contract: $version"

#!/usr/bin/env bash
# Keep user-facing product version surfaces aligned with the workspace version
# that release-plz writes in the release PR.
set -euo pipefail

root="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$root"

version=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
if [ -z "$version" ]; then
  echo "error: could not read [workspace.package] version from Cargo.toml" >&2
  exit 1
fi

update_marked_versions() {
  local file=$1
  local tmp
  tmp=$(mktemp)
  awk -v version="$version" '
    /x-release-plz-version/ {
      gsub(/v[0-9]+\.[0-9]+\.[0-9]+/, "v" version)
      gsub(/[0-9]+\.[0-9]+\.[0-9]+/, version)
    }
    { print }
  ' "$file" > "$tmp"
  if ! cmp -s "$file" "$tmp"; then
    cp "$tmp" "$file"
  fi
  rm -f "$tmp"
}

update_marked_versions README.md
update_marked_versions crates/claudex/README.md
update_marked_versions crates/claudex-cli/README.md
update_marked_versions website/.vitepress/config.ts
update_marked_versions website/reference/library.md

echo "product version surfaces: synced to $version"

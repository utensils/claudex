#!/usr/bin/env bash
# Print the body of one released version from the root Keep-a-Changelog file.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 CHANGELOG VERSION" >&2
  exit 2
fi

changelog=$1
version=$2
notes=$(mktemp)
trap 'rm -f "$notes"' EXIT

awk -v heading="## [$version]" '
  index($0, heading) == 1 { in_release = 1; next }
  in_release && /^## \[/ { exit }
  in_release { print }
' "$changelog" > "$notes"

if ! grep -q '[^[:space:]]' "$notes"; then
  echo "error: $changelog has no notes for [$version]" >&2
  exit 1
fi

cat "$notes"


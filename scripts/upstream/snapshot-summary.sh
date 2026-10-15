#!/usr/bin/env bash
# The Markdown summary of the snapshot changes in the working tree, for the
# body of a ts-rust bump pull request: the changed entries per snapshot file,
# then the diff (cut at about 50 KB). Prints nothing when no snapshot changed.
set -euo pipefail

cd "$(dirname "$0")/../.."
snapshots=(crates/tsgolint-rules/tests/data/snapshots crates/tsgolint-rust/e2e/__snapshots__)
files=$(git diff --name-only -- "${snapshots[@]}")
[ -n "$files" ] || exit 0

echo "| Snapshot file | Changed entries | Lines |"
echo "|---|---:|---:|"
while read -r file; do
  # A rule snapshot entry starts with `[<case> - 1]`; the e2e snapshot is one
  # Vitest `exports[...]` entry per test.
  entries=$(git diff -U100000 -- "$file" | awk '
    /^[ +-]\[.* - 1\]$/ || /^[ +-]exports\[/ { key = substr($0, 2) }
    /^[+-][^+-]/ && key != "" { changed[key] = 1 }
    END { print length(changed) }')
  lines=$(git diff --numstat -- "$file" | awk '{ print "+" $1 " -" $2 }')
  echo "| \`$file\` | $entries | $lines |"
done <<< "$files"

echo
echo "<details><summary>Snapshot diff</summary>"
echo
echo '```diff'
git diff -- "${snapshots[@]}" | head -c 50000
echo
echo '```'
echo
echo "</details>"

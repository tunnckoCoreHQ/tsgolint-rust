#!/usr/bin/env bash
# Opens an issue when oxc-project/tsgolint has a release newer than the one
# in UPSTREAM.json, with the compare link and the changed rule files. One
# issue per release: an issue with the same title, open or closed, stops it.
# With DRY_RUN=1 it prints the issue instead. Needs gh with issues: write.
set -euo pipefail

cd "$(dirname "$0")/../.."
repo=$(jq -r '.tsgolint.repository' UPSTREAM.json)
current=$(jq -r '.tsgolint.tag' UPSTREAM.json)
latest=$(gh api "repos/$repo/releases/latest" --jq .tag_name)

newest=$(printf '%s\n%s\n' "$current" "$latest" | sort -V | tail -1)
if [ "$latest" = "$current" ] || [ "$newest" != "$latest" ]; then
  echo "tsgolint $current is the latest release"
  exit 0
fi

title="Port tsgolint $latest"
if [ "${DRY_RUN:-}" != 1 ] &&
  [ -n "$(gh issue list --state all --search "\"$title\" in:title" --json number --jq '.[].number')" ]; then
  echo "an issue for tsgolint $latest exists already"
  exit 0
fi

compare=$(gh api "repos/$repo/compare/$current...$latest" --paginate --jq '.files[] | [.status, .filename, .additions, .deletions] | @tsv')
rules=$(awk -F'\t' '$2 ~ /^internal\/rules\// && $2 !~ /\/fixtures\// { printf "- `%s` (%s, +%s -%s)\n", $2, $1, $3, $4 }' <<< "$compare")
other=$(awk -F'\t' '$2 ~ /^(typescript-go$|typescript-go\/|patches\/|internal\/(linter|rule|utils|checker)\/|cmd\/)/ { printf "- `%s` (%s)\n", $2, $1 }' <<< "$compare")

body=$(cat <<BODY
tsgolint [$latest](https://github.com/$repo/releases/tag/$latest) is out. tsgolint-rust ports [$current](https://github.com/$repo/tree/$current) (UPSTREAM.json).

Compare: https://github.com/$repo/compare/$current...$latest

### Changed rule files

${rules:-None.}

### Other changes that can affect the port

The typescript-go submodule, its patches, the linter, the rule framework and the command line.

${other:-None.}

Dump the new cases with `scripts/upstream/dump-cases.sh $latest <dir>`, port the changed rules with the Go tsgolint as the reference (see CONTRIBUTING.md), then update \`tsgolint\` in UPSTREAM.json, the \`oxlint-tsgolint\` catalog entry and the README's compatibility table.
BODY
)

if [ "${DRY_RUN:-}" = 1 ]; then
  printf '%s\n\n%s\n' "$title" "$body"
else
  gh issue create --title "$title" --body "$body"
fi

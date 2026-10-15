#!/usr/bin/env bash
# Moves tsgolint to its latest release, when that is newer than the one in
# UPSTREAM.json, with the mechanical part of the port:
#
# - tsgolint's diff of e2e/ and of its rule snapshots, applied to
#   tsgolint-rust/e2e and tsgolint-rules/tests/data/snapshots. A hunk that
#   does not apply (ours differs there, from the TypeScript revision) is
#   listed in the body instead.
# - the new and changed rule test cases (dump-cases.sh, which needs Go).
# - the tag and commit in UPSTREAM.json, the oxlint-tsgolint catalog entry
#   (the Go oracle) and the lockfile.
# - a minor changeset.
#
# The rule changes themselves are left to port: the failing cases and
# snapshots show where.
#
#   scripts/upstream/tsgolint-bump.sh <body-file>
#
# TSGOLINT_TAG=<tag> moves to that release instead of the latest.
#
# Writes the pull request body to <body-file>, and prints `<key>=<value>`
# lines (for $GITHUB_OUTPUT):
#   bump=true|false   whether anything changed
#   from=<tag>        the ported tsgolint release
#   to=<tag>          the new release, when bump=true
set -euo pipefail

body=${1:?usage: tsgolint-bump.sh <body-file>}
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

repo=$(jq -r '.tsgolint.repository' UPSTREAM.json)
from=$(jq -r '.tsgolint.tag' UPSTREAM.json)
to=${TSGOLINT_TAG:-$(gh api "repos/$repo/releases/latest" --jq .tag_name)}
echo "from=$from"

newest=$(printf '%s\n%s\n' "$from" "$to" | sort -V | tail -1)
if [ "$to" = "$from" ] || [ "$newest" != "$to" ]; then
  echo "tsgolint $from is the latest release" >&2
  echo "bump=false"
  exit 0
fi
commit=$(gh api "repos/$repo/commits/$to" --jq .sha)
echo "tsgolint $from..$to ($commit)" >&2

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# tsgolint's e2e/ is tsgolint-rust/e2e, and its internal/rule_tester/__snapshots__
# is tsgolint-rules/tests/data/snapshots.
gh api -H 'Accept: application/vnd.github.diff' "repos/$repo/compare/$from...$to" |
  sed 's#^\(diff --git a/\|--- a/\|+++ b/\)internal/rule_tester/__snapshots__/#\1tsgolint-rules/tests/data/snapshots/#; s#\( b/\)internal/rule_tester/__snapshots__/#\1tsgolint-rules/tests/data/snapshots/#' \
    > "$work/upstream.diff"
git apply --reject --whitespace=nowarn --include='tsgolint-rules/tests/data/snapshots/*' "$work/upstream.diff" 2> "$work/apply-snapshots.log" || true
git apply --reject --whitespace=nowarn --directory=tsgolint-rust --include='tsgolint-rust/e2e/*' "$work/upstream.diff" 2> "$work/apply-e2e.log" || true
rejected=$(find tsgolint-rules/tests/data/snapshots tsgolint-rust/e2e -name '*.rej' -not -path '*/node_modules/*' | sort)
for file in $rejected; do
  cp "$file" "$work/"
  rm "$file"
done

scripts/upstream/dump-cases.sh "$to" | tee "$work/cases.log" >&2
cases=$(grep -E '^(updated|no longer in)' "$work/cases.log" | sed 's/^/- /' || true)

version=${to#v}
jq --arg tag "$to" --arg commit "$commit" '.tsgolint.tag = $tag | .tsgolint.commit = $commit' UPSTREAM.json > UPSTREAM.json.tmp
mv UPSTREAM.json.tmp UPSTREAM.json
sed -i.orig "s/^    oxlint-tsgolint: .*/    oxlint-tsgolint: $version/" pnpm-workspace.yaml
rm -f pnpm-workspace.yaml.orig
pnpm install --no-frozen-lockfile >&2

cat > ".changeset/tsgolint-$to.md" << CHANGESET
---
"tsgolint-rust": minor
---

Port tsgolint [$to](https://github.com/$repo/releases/tag/$to).
CHANGESET

compare=$(gh api "repos/$repo/compare/$from...$to" --paginate --jq '.files[] | [.status, .filename, .additions, .deletions] | @tsv')
rules=$(awk -F'\t' '$2 ~ /^internal\/rules\// && $2 !~ /\/fixtures\// { printf "- `%s` (%s, +%s -%s)\n", $2, $1, $3, $4 }' <<< "$compare")
other=$(awk -F'\t' '$2 ~ /^(typescript-go$|typescript-go\/|patches\/|internal\/(linter|rule|utils|checker)\/|cmd\/)/ { printf "- `%s` (%s)\n", $2, $1 }' <<< "$compare")
rejects=""
for file in $rejected; do
  rejects+=$'\n'"<details><summary><code>${file%.rej}</code></summary>"$'\n\n```diff\n'"$(cat "$work/$(basename "$file")")"$'\n```\n</details>\n'
done

cat > "$body" << BODY
Moves tsgolint from [$from](https://github.com/$repo/tree/$from) to [$to](https://github.com/$repo/releases/tag/$to) ([compare](https://github.com/$repo/compare/$from...$to)), with the mechanical part of the port: tsgolint's changes to its e2e suite and rule snapshots, the new and changed rule test cases, UPSTREAM.json, the \`oxlint-tsgolint\` oracle and a changeset. Port the rule changes below with the Go tsgolint as the reference (see CONTRIBUTING.md) until check and test pass.

### Changed rule files

${rules:-None.}

### Other changes that can affect the port

The typescript-go submodule, its patches, the linter, the rule framework and the command line.

${other:-None.}

### Rule test cases

${cases:-No case changed.}

### Hunks that did not apply

tsgolint's changes to these snapshots and e2e files, where ours differs from tsgolint's (the TypeScript revision).
${rejects:-
None.}
BODY

echo "bump=true"
echo "to=$to"

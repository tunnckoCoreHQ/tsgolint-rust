#!/usr/bin/env bash
# Dumps the rule test cases of a tsgolint release into
# tsgolint-rules/tests/data/cases, and its rule snapshots into a directory
# to compare with (the Go reference when porting).
#
#   scripts/upstream/dump-cases.sh <tag> [<snapshots-out>]
#
# Clones oxc-project/tsgolint at <tag> into a temporary directory, sets it up
# as its `just init` does (typescript-go submodule with tsgolint's patches),
# adds tsgolint-cases/dump_cases.go to its rule tester and runs the Go rule
# tests with TSGOLINT_DUMP_CASES, which write the cases instead of running
# them. Only suites whose content changed are rewritten, so unchanged files
# keep their bytes. Needs git and Go (or nix, for `nix shell nixpkgs#go`).
set -euo pipefail

tag=${1:?usage: dump-cases.sh <tag> [<snapshots-out>]}
snapshots_out=${2:-}
root=$(cd "$(dirname "$0")/../.." && pwd)
cases="$root/tsgolint-rules/tests/data/cases"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

go_cmd=(go)
if ! command -v go > /dev/null; then
  go_cmd=(nix shell nixpkgs#go -c go)
fi

git -c advice.detachedHead=false clone -q --filter=blob:none --branch "$tag" https://github.com/oxc-project/tsgolint.git "$work/tsgolint"
cd "$work/tsgolint"
git submodule update -q --init --depth 1
(cd typescript-go && git -c user.name=dump -c user.email=dump@localhost am -q --3way --no-gpg-sign ../patches/*.patch)
mkdir -p internal/collections
find ./typescript-go/internal/collections -type f ! -name '*_test.go' -exec cp {} internal/collections/ \;

cp "$root/scripts/upstream/tsgolint-cases/dump_cases.go" internal/rule_tester/
tester=internal/rule_tester/rule_tester.go
sed -i.orig 's/^import (/import (\n\t"os"/' "$tester"
sed -i.orig '/^func RunRuleTester(/a\
	if dir := os.Getenv("TSGOLINT_DUMP_CASES"); dir != "" {\
		dumpCases(dir, tsconfigPath, t, r, validTestCases, invalidTestCases)\
		return\
	}' "$tester"
rm -f "$tester.orig"

mkdir "$work/cases"
TSGOLINT_DUMP_CASES="$work/cases" "${go_cmd[@]}" test ./internal/rules/... > /dev/null

for file in "$work/cases"/*.json; do
  name=$(basename "$file")
  if [ ! -f "$cases/$name" ] || ! cmp -s <(jq -S . "$file") <(jq -S . "$cases/$name"); then
    cp "$file" "$cases/$name"
    echo "updated $name"
  fi
done
for file in "$cases"/*.json; do
  [ -f "$work/cases/$(basename "$file")" ] || echo "no longer in $tag: $(basename "$file")"
done

if [ -n "$snapshots_out" ]; then
  mkdir -p "$snapshots_out"
  cp internal/rule_tester/__snapshots__/*.snap "$snapshots_out/"
  echo "Go snapshots of $tag in $snapshots_out"
fi

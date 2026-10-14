#!/usr/bin/env bash
# Publishes tsgolint-rust, its platform packages and its wasm package to npm
# from the builds in <artifacts>, through trusted publishing: pnpm exchanges
# the GitHub OIDC token at npmjs.com, so no npm token is used.
#
#   nightly: a snapshot version of the pending changesets, with nightly.
#   latest: the version of the merged release PR, with latest, then the
#     GitHub release v<version> with the binaries and the changelog entry.
#
# A version that is on npm already is skipped, so a failed run can run again.
# Usage: bash scripts/release/publish.sh <nightly|latest> <artifacts>

set -euo pipefail

case "${1:-}" in
  nightly | latest) channel=$1 ;;
  *)
    echo "Usage: bash scripts/release/publish.sh <nightly|latest> <artifacts>" >&2
    exit 1
    ;;
esac
artifacts=$(realpath "${2:?Usage: bash scripts/release/publish.sh <nightly|latest> <artifacts>}")
repo=${GITHUB_REPOSITORY:-tunnckoCoreHQ/tsgolint-rust}

if [ "$channel" = nightly ]; then
  pnpm exec changeset version --snapshot nightly
fi
version=$(node npm/scripts/packages.mjs version)

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
# The platform packages first, then the wasm package, then tsgolint-rust,
# which depends on the platform packages.
node npm/scripts/packages.mjs generate "$artifacts" > "$out/packages.txt"
while read -r dir; do
  name=$(node -p "require('./$dir/package.json').name")
  if pnpm view "$name@$version" version > /dev/null 2>&1; then
    echo "$name@$version is on npm already"
    continue
  fi
  (cd "$dir" && pnpm publish --provenance --access public --tag "$channel" --no-git-checks)
  echo "Published $name@$version with $channel" | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
done < "$out/packages.txt"

[ "$channel" = latest ] || exit 0

tag=v$version
if gh release view "$tag" --repo "$repo" > /dev/null 2>&1; then
  echo "The GitHub release $tag exists already"
  exit 0
fi
mkdir "$out/release"
for dir in "$artifacts"/tsgolint-*; do
  target=${dir#"$artifacts"/tsgolint-}
  for file in "$dir"/*; do
    case "$file" in
      *.exe) cp "$file" "$out/release/tsgolint-$target.exe" ;;
      *.wasm) cp "$file" "$out/release/tsgolint-$target.wasm" ;;
      *) cp "$file" "$out/release/tsgolint-$target" ;;
    esac
  done
done
(cd "$out/release" && sha256sum -- * > SHA256SUMS)
# The changelog entry is everything between this version's heading and the next one.
[ ! -f npm/tsgolint-rust/CHANGELOG.md ] || awk -v heading="## $version" '$0 == heading { found = 1; next } found && /^## / { exit } found' \
  npm/tsgolint-rust/CHANGELOG.md > "$out/notes.md"
[ -s "$out/notes.md" ] || echo "tsgolint-rust $version" > "$out/notes.md"
# gh creates the tag on the commit that was built.
gh release create "$tag" "$out"/release/* --repo "$repo" --target "$(git rev-parse HEAD)" \
  --title "$tag" --notes-file "$out/notes.md"

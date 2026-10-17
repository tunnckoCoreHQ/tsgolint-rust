#!/usr/bin/env bash
# Publishes tsgolint-rust, its platform packages and its wasm package to npm
# from the builds in <artifacts>, with a Sigstore attestation of every
# tarball and binary.
#
#   nightly: a snapshot version of the pending changesets, with nightly.
#   latest: the version on master, with latest, then the GitHub release
#     v<version> with the binaries, SHA256SUMS, the changelog entry and the
#     attestation.
#
# npm trusted publishing takes the CircleCI OIDC token; when npm refuses it,
# pnpm publishes with NPM_TOKEN. npm adds no provenance to CircleCI
# publishes, so the attestation is our own: an in-toto statement with SLSA
# provenance that names every tarball and binary by digest, signed by cosign
# with the CircleCI OIDC token. It is written to
# <artifacts>/provenance.sigstore.json, and a latest release has it too.
#
# A version that is on npm already is skipped, so a failed run can run again.
# With --dry-run, pnpm checks the publish without doing it, and nothing is
# signed or released.
#
#   bash scripts/release/publish.sh <nightly|latest> <artifacts> [--dry-run]

set -euo pipefail

COSIGN_VERSION=3.1.3

usage="Usage: bash scripts/release/publish.sh <nightly|latest> <artifacts> [--dry-run]"
case "${1:-}" in
  nightly | latest) channel=$1 ;;
  *)
    echo "$usage" >&2
    exit 1
    ;;
esac
artifacts=$(realpath "${2:?$usage}")
dry_run=${3:-}
repo=tunnckoCoreHQ/tsgolint-rust
commit=$(git rev-parse HEAD)

if [ "$channel" = nightly ]; then
  current=$(node npm/scripts/packages.mjs version)
  pnpm exec changeset version --snapshot nightly
  # Empty changesets bump nothing; the nightly is then a prerelease of the
  # current version.
  if [ "$(node npm/scripts/packages.mjs version)" = "$current" ]; then
    snapshot="$current-nightly.$(date -u +%Y%m%d%H%M%S).$(git rev-parse --short=7 HEAD)"
    for pkg in npm/tsgolint-rust npm/wasm32-wasip1; do
      jq --arg v "$snapshot" '.version = $v' "$pkg/package.json" > "$pkg/package.json.tmp"
      mv "$pkg/package.json.tmp" "$pkg/package.json"
    done
  fi
fi
version=$(node npm/scripts/packages.mjs version)

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
mkdir "$out/npm" "$out/release"

# The platform packages first, then the wasm package, then tsgolint-rust,
# which depends on the platform packages.
node npm/scripts/packages.mjs generate "$artifacts" > "$out/packages.txt"
while read -r dir; do
  (cd "$dir" && pnpm pack --pack-destination "$out/npm" > /dev/null)
done < "$out/packages.txt"

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

# One subject per tarball and binary, by file name. cosign verifies a file
# by its sha256.
for file in "$out"/npm/*.tgz "$out"/release/tsgolint-*; do
  jq -n --arg name "$(basename "$file")" --arg sha256 "$(sha256sum "$file" | cut -d' ' -f1)" \
    --arg sha512 "$(sha512sum "$file" | cut -d' ' -f1)" '{name: $name, digest: {sha256: $sha256, sha512: $sha512}}'
done | jq -s --arg repo "$repo" --arg commit "$commit" --arg channel "$channel" --arg version "$version" \
  --arg run "${CIRCLE_BUILD_URL:-local}" '{
    _type: "https://in-toto.io/Statement/v1",
    subject: .,
    predicateType: "https://slsa.dev/provenance/v1",
    predicate: {
      buildDefinition: {
        buildType: "https://github.com/\($repo)/blob/\($commit)/.circleci/config.yml",
        externalParameters: {repository: "https://github.com/\($repo)", ref: "refs/heads/master", channel: $channel, version: $version},
        resolvedDependencies: [{uri: "git+https://github.com/\($repo)@refs/heads/master", digest: {gitCommit: $commit}}]
      },
      runDetails: {builder: {id: "https://circleci.com"}, metadata: {invocationId: $run}}
    }
  }' > "$out/statement.json"

if [ "$dry_run" = --dry-run ]; then
  jq -r '.subject[] | "\(.digest.sha256)  \(.name)"' "$out/statement.json"
  for tarball in "$out"/npm/*.tgz; do
    pnpm publish "$tarball" --access public --tag "$channel" --no-git-checks --dry-run
  done
  exit 0
fi

bin=$HOME/.local/bin
mkdir -p "$bin"
curl -fsSL -o "$bin/cosign" "https://github.com/sigstore/cosign/releases/download/v$COSIGN_VERSION/cosign-linux-amd64"
chmod +x "$bin/cosign"
"$bin/cosign" attest-blob --statement "$out/statement.json" --bundle "$artifacts/provenance.sigstore.json" --yes \
  --identity-token "$(circleci run oidc get --claims '{"aud": "sigstore"}')"

NPM_ID_TOKEN=$(circleci run oidc get --claims '{"aud": "npm:registry.npmjs.org"}')
export NPM_ID_TOKEN
if [ -n "${NPM_TOKEN:-}" ]; then
  # pnpm reads the token when it publishes.
  # shellcheck disable=SC2016
  echo '//registry.npmjs.org/:_authToken=${NPM_TOKEN}' >> "$HOME/.npmrc"
fi
while read -r dir; do
  name=$(node -p "require('./$dir/package.json').name")
  if pnpm view "$name@$version" version > /dev/null 2>&1; then
    echo "$name@$version is on npm already"
    continue
  fi
  tarball=$(echo "$name" | sed 's/^@//; s#/#-#')-$version.tgz
  pnpm publish "$out/npm/$tarball" --access public --tag "$channel" --no-git-checks
  echo "Published $name@$version with $channel"
done < "$out/packages.txt"

[ "$channel" = latest ] || exit 0

tag=v$version
if gh release view "$tag" --repo "$repo" > /dev/null 2>&1; then
  echo "The GitHub release $tag exists already"
  exit 0
fi
cp "$artifacts/provenance.sigstore.json" "$out/release/"
# The changelog entry is everything between this version's heading and the next one.
[ ! -f npm/tsgolint-rust/CHANGELOG.md ] || awk -v heading="## $version" '$0 == heading { found = 1; next } found && /^## / { exit } found' \
  npm/tsgolint-rust/CHANGELOG.md > "$out/notes.md"
[ -s "$out/notes.md" ] || echo "tsgolint-rust $version" > "$out/notes.md"
# gh creates the tag on the commit that was built.
gh release create "$tag" "$out"/release/* --repo "$repo" --target "$commit" \
  --title "$tag" --notes-file "$out/notes.md"

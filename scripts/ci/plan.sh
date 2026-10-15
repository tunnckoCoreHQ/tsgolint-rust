#!/usr/bin/env bash
# What a CI pipeline does, from the commits it runs on. A branch is compared
# with its merge base on master, a master commit with its parent (master
# takes squash merges only).
#
#   scripts/ci/plan.sh changeset
#     Fails when a branch changes the Rust sources of a crate, outside its
#     tests, and adds no changeset.
#   scripts/ci/plan.sh channel
#     Prints the npm dist-tag that the pipeline publishes with, or nothing:
#     latest for a release PR merge or with PUBLISH=true (the `publish`
#     pipeline parameter), nightly for a master commit that adds a changeset
#     with a release.
#   scripts/ci/plan.sh source
#     Prints yes when the pipeline publishes or changes what the macOS and
#     Windows jobs check, test or build, and no otherwise.
set -euo pipefail

cd "$(dirname "$0")/../.."
branch=${CIRCLE_BRANCH:-$(git branch --show-current)}
if [ "$branch" = master ]; then
  base=$(git rev-parse HEAD~1)
else
  git fetch -q origin master
  base=$(git merge-base FETCH_HEAD HEAD)
fi

changed() {
  git diff --name-only "$base" HEAD -- "$@"
}

added_changesets() {
  git diff --name-only --diff-filter=A "$base" HEAD -- '.changeset/*.md'
}

channel() {
  [ "$branch" = master ] || return 0
  if [ "${PUBLISH:-false}" = true ] || git log -1 --format=%s | grep -q '^chore: release tsgolint-rust '; then
    echo latest
    return
  fi
  # An empty changeset (`pnpm changeset --empty`) names no package.
  added=$(added_changesets)
  if [ -n "$added" ] && xargs grep -qE '^"?[@a-z0-9/._-]+"?: (patch|minor|major)$' <<< "$added"; then
    echo nightly
  fi
}

case "${1:-}" in
  changeset)
    [ "$branch" != master ] || exit 0
    rust=$(changed ':(glob)crates/**/*.rs' ':(glob,exclude)crates/*/tests/**')
    if [ -n "$rust" ] && [ -z "$(added_changesets)" ]; then
      printf 'This branch changes Rust sources and adds no changeset:\n%s\n\n' "$rust"
      echo "Add one with \`pnpm changeset\`, or \`pnpm changeset --empty\` to merge without a release."
      exit 1
    fi
    ;;
  channel) channel ;;
  source)
    if [ -n "$(channel)" ] || [ -n "$(changed crates scripts npm .circleci 'Cargo.*' rust-toolchain.toml pnpm-lock.yaml)" ]; then
      echo yes
    else
      echo no
    fi
    ;;
  *)
    echo "usage: scripts/ci/plan.sh changeset|channel|source" >&2
    exit 2
    ;;
esac

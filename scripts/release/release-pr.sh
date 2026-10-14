#!/usr/bin/env bash
# Opens or updates the release PR from the pending changesets, after CI passes
# on master. Merging the PR publishes the release with latest. GITHUB_TOKEN is
# a personal token, so the PR is a user PR that CI runs on.

set -euo pipefail

branch=changeset-release/master
repo=${GITHUB_REPOSITORY:-tunnckoCoreHQ/tsgolint-rust}

if ! compgen -G '.changeset/*.md' > /dev/null; then
  echo "No pending changesets"
  exit 0
fi

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git checkout -B "$branch"
pnpm exec changeset version
# The Cargo workspace version and the README's compatibility row follow the
# npm version.
node npm/scripts/packages.mjs sync
pnpm install --lockfile-only --ignore-scripts --no-frozen-lockfile
version=$(node npm/scripts/packages.mjs version)
title="chore: release tsgolint-rust $version"
git add -A
git commit -s -m "$title"

# Push only when the release itself changed. When the versions and changelogs
# come out the same, the push would only move the branch onto the new master,
# and GitHub's Update branch does that before merging. The release content is
# the diff from master, whatever master it is based on.
pr=$(gh pr list --repo "$repo" --head "$branch" --base master --state open --json number --jq '.[0].number // empty')
if [ -n "$pr" ] && git fetch -q origin "refs/heads/$branch" &&
  [ "$(git diff HEAD~1...FETCH_HEAD | git patch-id --stable)" = "$(git diff HEAD~1 HEAD | git patch-id --stable)" ]; then
  echo "Release PR #$pr is unchanged: the versions and changelogs are the same"
  exit 0
fi
git push --force "https://x-access-token:${GITHUB_TOKEN}@github.com/$repo.git" "HEAD:refs/heads/$branch"

body=$(mktemp)
trap 'rm -f "$body"' EXIT
{
  printf 'Merging this PR publishes tsgolint-rust %s and its platform and wasm packages with latest, and creates the GitHub release v%s.\n\n' "$version" "$version"
  # The changelog entry is everything between this version's heading and the next one.
  awk -v heading="## $version" '$0 == heading { found = 1; next } found && /^## / { exit } found' \
    npm/tsgolint-rust/CHANGELOG.md
} > "$body"

if [ -n "$pr" ]; then
  gh pr edit "$pr" --repo "$repo" --title "$title" --body-file "$body"
else
  gh pr create --repo "$repo" --head "$branch" --base master --title "$title" --body-file "$body"
fi

#!/usr/bin/env bash
# The daily upstream bumps, from CI's scheduled pipelines.
#
#   ts-rust: moves ts-rust to the head of its branch when the head changes its
#     compiler sources (ts-rust-bump.sh), regenerates the snapshots and adds a
#     patch changeset.
#   tsgolint: moves tsgolint to its latest release with the mechanical part of
#     the port (tsgolint-bump.sh), which adds a minor changeset. The rule
#     changes are left to port on the pull request, so an open one is left
#     alone.
#
# Then runs check and test, and opens or updates the pull request
# deps/<upstream>, which merges by itself once its checks pass. GITHUB_TOKEN
# is a personal token, so CI runs on the pull request.
#
#   scripts/upstream/bump.sh ts-rust|tsgolint
#
# The bump script prints bump, from, to, short and typescript, sourced below.
# shellcheck disable=SC2154
set -euo pipefail

cd "$(dirname "$0")/../.."
upstream=${1:-}
repo=tunnckoCoreHQ/tsgolint-rust
branch=deps/$upstream
run_url=${CIRCLE_BUILD_URL:-}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

case "$upstream" in
  ts-rust)
    scripts/upstream/ts-rust-bump.sh | tee "$work/outputs"
    paths=(.changeset Cargo.lock UPSTREAM.json crates/tsgolint-rules/tests/data/snapshots crates/tsgolint-rust/e2e/__snapshots__)
    ;;
  tsgolint)
    if [ -n "$(gh pr list --repo "$repo" --head "$branch" --state open --json number --jq '.[].number')" ]; then
      echo "The pull request of $branch is open: port it first"
      exit 0
    fi
    # dump-cases.sh runs tsgolint's Go rule tests; tsgolint's go.mod picks
    # its toolchain.
    if ! command -v go > /dev/null; then
      go=$(curl -fsSL 'https://go.dev/VERSION?m=text' | head -1)
      curl -fsSL "https://go.dev/dl/$go.linux-amd64.tar.gz" | tar -xz -C "$work"
      export PATH=$work/go/bin:$PATH
    fi
    scripts/upstream/tsgolint-bump.sh "$work/port.md" | tee "$work/outputs"
    paths=(.changeset UPSTREAM.json pnpm-workspace.yaml pnpm-lock.yaml crates/tsgolint-rules/tests/data/cases crates/tsgolint-rules/tests/data/snapshots crates/tsgolint-rust/e2e)
    ;;
  *)
    echo "usage: scripts/upstream/bump.sh ts-rust|tsgolint" >&2
    exit 2
    ;;
esac
# bump, from, to and the other `<key>=<value>` lines of the bump script.
# shellcheck disable=SC1091
. "$work/outputs"
[ "$bump" = true ] || exit 0

if [ "$upstream" = ts-rust ]; then
  # Snapshot differences are written, not failed on. Other failures show in
  # the check and test below.
  TSGOLINT_UPDATE_SNAPSHOTS=1 cargo test -p tsgolint-rules --test rules -- --test-threads 4 || true
  cargo build -p tsgolint-rust
  TSGOLINT_BIN="$PWD/target/debug/tsgolint" pnpm --dir crates/tsgolint-rust/e2e run e2e -- --update || true
  # The changeset puts the move in the next nightly and release.
  cat > ".changeset/ts-rust-${to:0:8}.md" << CHANGESET
---
"tsgolint-rust": patch
---

Move ts-rust to [\`${to:0:8}\`](https://github.com/pingdotgg/ts-rust/commit/$to), which ports TypeScript [\`${typescript:0:12}\`](https://github.com/microsoft/TypeScript/commit/$typescript).
CHANGESET
fi

tests=failure
pnpm exec turbo run check test e2e e2e:wasm && tests=success

{
  if [ "$upstream" = ts-rust ]; then
    title="build(deps): move ts-rust to $short"
    summary=$(scripts/upstream/snapshot-summary.sh)
    echo "Moves ts-rust from [\`${from:0:8}\`](https://github.com/pingdotgg/ts-rust/commit/$from) to [\`${to:0:8}\`](https://github.com/pingdotgg/ts-rust/commit/$to) ([compare](https://github.com/pingdotgg/ts-rust/compare/$from...$to)), which ports TypeScript [\`${typescript:0:12}\`](https://github.com/microsoft/TypeScript/commit/$typescript)."
    echo
    if [ "$tests" = success ]; then
      echo "Check and test pass with the regenerated snapshots."
    else
      echo "**Check or test fails** with the regenerated snapshots, so the change is more than snapshot text: a rule case's expectations, the command line or the wasm build. See the [CI job]($run_url)."
    fi
    echo
    if [ -z "$summary" ]; then
      echo "No snapshot changed."
    else
      echo "### Snapshot changes"
      echo
      echo "Each change needs a cause in ts-rust: the TypeScript revision, or a fix."
      echo
      echo "$summary"
    fi
  else
    title="feat: port tsgolint $to"
    if [ "$tests" = success ]; then
      echo "Check and test pass: the release needs no rule change."
    else
      echo "**Check or test fails** until the rule changes are ported. See the [CI job]($run_url)."
    fi
    echo
    cat "$work/port.md"
  fi
} > "$work/body.md"

user=$(gh api user --jq '"\(.login) \(.id)"')
git config user.name "${user% *}"
git config user.email "${user#* }+${user% *}@users.noreply.github.com"
git checkout -B "$branch"
git add -- "${paths[@]}"
git commit -s -m "$title"
git push --force "https://x-access-token:${GITHUB_TOKEN}@github.com/$repo.git" "HEAD:refs/heads/$branch"

pr=$(gh pr list --repo "$repo" --head "$branch" --base master --state open --json number --jq '.[0].number // empty')
if [ -n "$pr" ]; then
  gh pr edit "$pr" --repo "$repo" --title "$title" --body-file "$work/body.md"
else
  gh pr create --repo "$repo" --head "$branch" --base master --title "$title" --body-file "$work/body.md" --label dependencies
fi
gh pr merge "$branch" --repo "$repo" --auto --squash

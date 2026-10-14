#!/usr/bin/env bash
# Moves ts-rust (goport_core, goport_util) to the head of its branch, when
# the head changes its compiler sources, and records the new commits in
# UPSTREAM.json.
#
# Prints `<key>=<value>` lines (for $GITHUB_OUTPUT):
#   bump=true|false   whether Cargo.lock and UPSTREAM.json changed
#   from=<commit>     the pinned ts-rust commit
#   to=<commit>       the new commit, when bump=true
#   short=<commit>    its first 8 characters
#   typescript=<commit> the TypeScript commit the new ts-rust ports
#
# The compiler sources are what goport_core and goport_util build: ts-rust's
# crates/ts_goport/src without what only ts_goport and goport_lsproto build
# (api, bin, cmd, format, ls, lsp, lsproto, project; see goport_core_root.rs
# and goport_util_root.rs), the two crates'
# manifests and build scripts, and the bundled lib files. Changes outside
# them (docs, tests, tools, the language server) leave Cargo.lock as it was.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

lock_commit() {
  grep -m1 -o 'pingdotgg/ts-rust?branch=main#[0-9a-f]*' Cargo.lock | cut -d'#' -f2
}

from=$(jq -r '."ts-rust".commit' UPSTREAM.json)
if [ "$(lock_commit)" != "$from" ]; then
  echo "UPSTREAM.json names ts-rust $from, Cargo.lock $(lock_commit)" >&2
  exit 1
fi
echo "from=$from"

cargo update -p goport_core -p goport_util >&2
to=$(lock_commit)
if [ "$to" = "$from" ]; then
  echo "ts-rust is at $from already" >&2
  echo "bump=false"
  exit 0
fi

# Cargo's clone of ts-rust has both commits.
db=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/git/db/ts-rust-* | head -1)
changed=$(git -C "$db" diff --name-only "$from" "$to")
relevant=$(printf '%s\n' "$changed" |
  grep -E '^crates/ts_goport/(src/|libs/|parts/goport_(core|util)/)' |
  grep -vE '^crates/ts_goport/src/((api|bin|cmd|format|ls|lsp|lsproto|project)/|lib\.rs$|goport_lsproto_root\.rs$)' || true)
if [ -z "$relevant" ]; then
  echo "ts-rust $from..$to changes no compiler source ($(printf '%s\n' "$changed" | grep -c .) files)" >&2
  git checkout -- Cargo.lock
  echo "bump=false"
  exit 0
fi
echo "ts-rust $from..$to changes $(printf '%s\n' "$relevant" | grep -c .) compiler source files" >&2

# ts-rust's UPSTREAM.md names the TypeScript commit it ports:
# - Commit: [`fed0bf24149f`](https://github.com/microsoft/TypeScript/commit/<full>)
typescript=$(git -C "$db" show "$to:UPSTREAM.md" |
  grep -m1 -oE '^- Commit: .*microsoft/TypeScript/commit/[0-9a-f]{40}' | grep -oE '[0-9a-f]{40}$' || true)
if [ -z "$typescript" ]; then
  echo "no TypeScript commit in ts-rust's UPSTREAM.md at $to; keeping the old one" >&2
  typescript=$(jq -r '.typescript.commit' UPSTREAM.json)
fi

jq --arg to "$to" --arg ts "$typescript" \
  '."ts-rust".commit = $to | .typescript.commit = $ts' UPSTREAM.json > UPSTREAM.json.tmp
mv UPSTREAM.json.tmp UPSTREAM.json
echo "bump=true"
echo "to=$to"
echo "short=${to:0:8}"
echo "typescript=$typescript"

#!/bin/sh
# tsgolint's e2e suite against a debug build: the native binary, or the
# wasm32-wasip1 module under wasmtime (WASMTIME, default wasmtime on PATH).
#
#   scripts/e2e.sh native|wasm
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target}
e2e=$root/crates/tsgolint-rust/e2e

cd "$root"
case "${1:-}" in
  native)
    cargo build -p tsgolint-rust
    TSGOLINT_BIN="$target/debug/tsgolint" pnpm --dir "$e2e" run e2e
    ;;
  wasm)
    cargo build -p tsgolint-rust --target wasm32-wasip1
    TSGOLINT_WASM="$target/wasm32-wasip1/debug/tsgolint.wasm" TSGOLINT_BIN="$e2e/tsgolint-wasm.sh" \
      pnpm --dir "$e2e" run e2e
    ;;
  *)
    echo "usage: scripts/e2e.sh native|wasm" >&2
    exit 2
    ;;
esac

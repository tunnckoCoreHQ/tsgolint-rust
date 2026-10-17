#!/usr/bin/env bash
# The release builds of the npm targets given, into dist/tsgolint-<target>/,
# the layout that `npm/scripts/packages.mjs generate` reads. CI builds
# darwin-* on macOS and everything else on Linux: linux-* on a machine of
# their architecture, win32-* with cargo-xwin and the MSVC SDK.
#
#   scripts/ci/build.sh <npm-target>...
set -euo pipefail

CARGO_XWIN_VERSION=0.23.1

cd "$(dirname "$0")/../.."
sudo=$([ "$(id -u)" = 0 ] || echo sudo)
bin=$HOME/.local/bin
mkdir -p "$bin"
export PATH=$bin:$PATH

for npm in "$@"; do
  case "$npm" in
    darwin-arm64) target=aarch64-apple-darwin ;;
    darwin-x64) target=x86_64-apple-darwin ;;
    linux-arm64) target=aarch64-unknown-linux-musl ;;
    linux-x64) target=x86_64-unknown-linux-musl ;;
    win32-arm64) target=aarch64-pc-windows-msvc ;;
    win32-x64) target=x86_64-pc-windows-msvc ;;
    wasm32-wasip1) target=wasm32-wasip1 ;;
    *)
      echo "unknown npm target: $npm" >&2
      exit 2
      ;;
  esac
  rustup target add "$target"

  build=(cargo build)
  case "$target" in
    *-linux-musl)
      command -v musl-gcc > /dev/null || { $sudo apt-get update && $sudo apt-get install -y --no-install-recommends musl-tools; }
      # jemalloc is C: build it with musl-gcc. The settings that a glibc build
      # applies with an exec at start (src/main.rs) are built in.
      export CC_x86_64_unknown_linux_musl=musl-gcc CC_aarch64_unknown_linux_musl=musl-gcc
      export JEMALLOC_SYS_WITH_MALLOC_CONF=narenas:4,thp:always,metadata_thp:disabled,cache_oblivious:false
      # GCC on arm64 calls outline atomics from libgcc by default, which
      # jemalloc's configure cannot link against musl.
      export CFLAGS_aarch64_unknown_linux_musl=-mno-outline-atomics
      ;;
    *-windows-msvc)
      # CI caches the MSVC CRT and Windows SDK, a download of minutes, and
      # the apt packages.
      export XWIN_CACHE_DIR=$HOME/.cache/cargo-xwin
      if ! command -v cargo-xwin > /dev/null; then
        mkdir -p "$HOME/.cache/apt/partial"
        $sudo apt-get update
        $sudo apt-get install -y --no-install-recommends -o Dir::Cache::Archives="$HOME/.cache/apt" clang lld llvm
        curl -fsSL "https://github.com/rust-cross/cargo-xwin/releases/download/v$CARGO_XWIN_VERSION/cargo-xwin-v$CARGO_XWIN_VERSION.x86_64-unknown-linux-musl.tar.gz" |
          tar -xz -C "$bin" cargo-xwin
      fi
      # clang-cl, lld-link and llvm-lib, unversioned.
      PATH=$(llvm-config --bindir):$PATH
      build=(cargo xwin build)
      ;;
  esac
  "${build[@]}" --release --locked -p tsgolint-rust --target "$target"

  out=dist/tsgolint-$npm
  mkdir -p "$out"
  case "$target" in
    wasm32-*) cp "target/$target/release/tsgolint.wasm" "$out/" ;;
    *-windows-*) cp "target/$target/release/tsgolint.exe" "$out/" ;;
    *) cp "target/$target/release/tsgolint" "$out/" ;;
  esac
  ls -l "$out"

  if [[ "$target" == *-linux-musl ]]; then
    # A static binary has no program interpreter (no ld.so to load).
    if readelf -l "$out/tsgolint" | grep -q INTERP; then
      readelf -l "$out/tsgolint" | grep -A1 INTERP
      exit 1
    fi
    echo '{"version":2}' | "$out/tsgolint" headless
  fi
done

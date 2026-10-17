#!/usr/bin/env bash
# Sets up a CI machine (Linux, macOS or Windows under Git Bash): the Rust
# toolchain of rust-toolchain.toml, the pnpm release of package.json's
# packageManager, the workspace's npm dependencies, and wasmtime with
# --wasmtime. PATH changes go to $BASH_ENV, which every later step sources.
#
#   scripts/ci/setup.sh [--wasmtime]
set -euo pipefail

WASMTIME_VERSION=44.0.1

cd "$(dirname "$0")/../.."
# CircleCI's checkout sends every github.com URL over SSH with a deploy key
# that reads only this repository. Cargo fetches ts-rust from GitHub, and the
# release and bump jobs push over HTTPS with a token.
git config --global --unset-all url.ssh://git@github.com.insteadof || true

bin=$HOME/.local/bin
mkdir -p "$bin"
# The pnpm store at one path on every OS, for the CI cache.
export PATH=$HOME/.cargo/bin:$bin:$PATH pnpm_config_store_dir=$HOME/.pnpm-store
{
  echo "export PATH=\"$HOME/.cargo/bin:$bin:\$PATH\""
  echo "export pnpm_config_store_dir=\"$HOME/.pnpm-store\""
} >> "${BASH_ENV:-/dev/null}"

case "$(uname -m)" in
  x86_64) arch=x86_64 cpu=x64 ;;
  aarch64 | arm64) arch=aarch64 cpu=arm64 ;;
esac
case "$(uname -s)" in
  Linux) os=linux platform=linux ;;
  Darwin) os=macos platform=darwin ;;
  MINGW* | MSYS*) os=windows platform=win32 ;;
esac

if ! command -v rustup > /dev/null; then
  if [ "$os" = windows ]; then
    curl -fsSL -o "$bin/rustup-init.exe" https://win.rustup.rs/x86_64
    "$bin/rustup-init.exe" -y --profile minimal --default-toolchain none
  else
    curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
  fi
fi
rustup toolchain install
rustup show active-toolchain

# The pnpm binary runs from its release folder, next to its dist/.
pnpm=$(sed -n 's/.*"packageManager": "pnpm@\([^"]*\)".*/\1/p' package.json)
url=https://github.com/pnpm/pnpm/releases/download/v$pnpm/pnpm-$platform-$cpu
if [ "$os" = windows ]; then
  curl -fsSL -o "$bin/pnpm.zip" "$url.zip"
  unzip -q -o "$bin/pnpm.zip" -d "$bin"
else
  curl -fsSL "$url.tar.gz" | tar -xz -C "$bin"
fi
pnpm --version
pnpm install --frozen-lockfile

if [ "${1:-}" = --wasmtime ]; then
  name=wasmtime-v$WASMTIME_VERSION-$arch-$os
  url=https://github.com/bytecodealliance/wasmtime/releases/download/v$WASMTIME_VERSION/$name
  if [ "$os" = windows ]; then
    curl -fsSL -o "$bin/$name.zip" "$url.zip"
    unzip -q -o -j "$bin/$name.zip" "$name/wasmtime.exe" -d "$bin"
  else
    curl -fsSL "$url.tar.xz" | tar -xJ -C "$bin" --strip-components 1 "$name/wasmtime"
  fi
  wasmtime --version
fi

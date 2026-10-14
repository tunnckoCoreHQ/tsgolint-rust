# tsgolint-rust

A Rust port of [oxc-project/tsgolint](https://github.com/oxc-project/tsgolint) (v7.0.2004), the type-aware linter that [oxlint](https://oxc.rs) runs for `--type-aware` and `--type-check`. It runs on [ts-rust](https://github.com/pingdotgg/ts-rust), a Rust port of TypeScript 7, instead of typescript-go.

It is a drop-in replacement. oxlint spawns it the same way, sends the same `headless` payload, and gets the same diagnostics, fixes and suggestions back. One ts-rust program and one checker pass give both the lint results and the type errors.

## Status

- **Rules.** All 60 tsgolint rules, with their options, messages, fixes and suggestions.
- **tsgolint's own tests.** The 7,619 rule test cases of tsgolint's Go suite run with their expectations. 7,591 pass; the other 28 are skipped in tsgolint itself. The snapshots hold our own output on ts-rust; they differ from tsgolint's in one message, where TypeScript 7.1 orders a union differently.
- **e2e.** tsgolint's end-to-end suite passes, with the native binary and with the wasm build: 21 tests, plus 3 that run only on Windows or case-insensitive file systems.
- **oxlint.** oxlint 1.86.0 gives the same output on oxc's 25 tsgolint CLI fixtures with either binary, `--fix` included.
- **Real projects.** Compared with the Go binary on 12 projects (hono, TanStack Query, tRPC, h3, Effect, t3code, vercel/commerce). Every remaining difference comes from the TypeScript revision, from tsgolint's own run-to-run variation, or from the Effect diagnostics. [DIFFERENCES.md](DIFFERENCES.md) explains them.
- **wasm32-wasip1.** The WASI build runs on one thread, so it runs where threads are not available. Its output equals the native output.

ts-rust ports TypeScript 7.1.0-dev, a few months newer than the TypeScript 7.0 inside tsgolint. That accounts for most of the differences: some type strings in messages are cut short at another point, and a few diagnostics changed upstream.

## Compatibility

tsgolint-rust has its own version numbers. Each release ports one tsgolint release (its rules and its `headless` protocol) on one ts-rust commit, and is tested with the oxlint versions below. [UPSTREAM.json](UPSTREAM.json) records the pins of the current source.

| tsgolint-rust | tsgolint rules and protocol | TypeScript | ts-rust | Tested with oxlint |
|---|---|---|---|---|
| 0.1.0 | [v7.0.2004](https://github.com/oxc-project/tsgolint/tree/v7.0.2004) | [`fed0bf24149f`](https://github.com/microsoft/TypeScript/commit/fed0bf24149f) (7.1.0-dev) | [`598e6416`](https://github.com/pingdotgg/ts-rust/commit/598e6416) | 1.86.0 |

## Speed and memory

Lint with fixes and suggestions on real projects, all 60 rules, against the Go tsgolint on the same 12-core machine (median of 7 interleaved runs):

| Project | Files | Go | tsgolint-rust | Faster | Peak memory, Go / Rust |
|---|---:|---:|---:|---:|---:|
| TanStack Query core | 68 | 0.85 s | 0.51 s | 1.67x | 489 / 310 MB |
| TanStack Query React, with `--type-check` | 63 | 1.07 s | 0.50 s | 2.14x | 520 / 279 MB |
| h3 | 177 | 0.64 s | 0.49 s | 1.31x | 349 / 243 MB |
| hono | 312 | 1.91 s | 1.29 s | 1.48x | 811 / 498 MB |
| hono, with `--type-check` | 312 | 2.70 s | 1.21 s | 2.23x | 1060 / 507 MB |
| t3code contracts, with `--type-check` | 107 | 2.52 s | 0.94 s | 2.68x | 955 / 471 MB |
| t3code shared | 216 | 1.16 s | 0.72 s | 1.61x | 600 / 355 MB |
| Effect (`packages/effect/src`) | 516 | 3.77 s | 2.54 s | 1.48x | 1411 / 863 MB |
| t3code web chat | 224 | 5.13 s | 3.24 s | 1.58x | 2438 / 1469 MB |

Both run the same 4 checkers, so tsgolint-rust also uses less CPU: 10.3 s against 26.0 s of user time on t3code web chat. The native build uses jemalloc and fat LTO.

The wasm build needs about half the memory of the native build, because it keeps one checker alive at a time: 168 MB for TanStack Query core, 216 MB for hono with `--type-check`.

## Effect diagnostics

ts-rust has the [Effect](https://effect.website) language service diagnostics built in. When a tsconfig enables the `@effect/language-service` plugin, `--type-check` also reports Effect's diagnostics (codes TS377xxx), from the same check.

## Install

oxlint runs the `tsgolint` bin of the `oxlint-tsgolint` package. tsgolint-rust ships the same bin in the [`tsgolint-rust`](https://www.npmjs.com/package/tsgolint-rust) package, so you install it under the name `oxlint-tsgolint`. It picks its binary from one of `@tsgolint-rust/darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64` (static, for glibc and musl), `win32-arm64` and `win32-x64`.

oxlint declares an optional peer dependency on `oxlint-tsgolint@>=7.0.2003`. tsgolint-rust has its own versions, so tell your package manager to accept it, as below. Otherwise pnpm warns about an unmet peer and npm refuses to install.

With pnpm, alias it in `package.json`:

```json
{
  "devDependencies": {
    "oxlint": "^1.86.0",
    "oxlint-tsgolint": "npm:tsgolint-rust@0.1.0"
  }
}
```

and accept the version in `pnpm-workspace.yaml`:

```yaml
peerDependencyRules:
  allowedVersions:
    "oxlint>oxlint-tsgolint": "*"
```

With npm, add the same alias and an override that names it:

```json
{
  "devDependencies": {
    "oxlint": "^1.86.0",
    "oxlint-tsgolint": "npm:tsgolint-rust@0.1.0"
  },
  "overrides": {
    "oxlint-tsgolint": "$oxlint-tsgolint"
  }
}
```

When another package brings `oxlint-tsgolint` in, override it everywhere instead: `overrides: { oxlint-tsgolint: npm:tsgolint-rust@0.1.0 }` in `pnpm-workspace.yaml` (with the peer rule above), or `"overrides": { "oxlint-tsgolint": "npm:tsgolint-rust@0.1.0" }` in npm's `package.json`. Going back to the Go tsgolint is removing the alias or the override.

Where no native build runs, or native binaries are not allowed, use the wasm build on Node's WASI: `"oxlint-tsgolint": "npm:@tsgolint-rust/wasm32-wasip1@0.1.0"`. It gives the same output on one thread, on Linux and macOS.

## Build from source

```sh
cargo build --release -p tsgolint-rust
OXLINT_TSGOLINT_PATH=$PWD/target/release/tsgolint pnpm exec oxlint --type-aware --type-check
```

The wasm build, under [wasmtime](https://wasmtime.dev):

```sh
cargo build --release --target wasm32-wasip1 -p tsgolint-rust
TSGOLINT_WASM=$PWD/target/wasm32-wasip1/release/tsgolint.wasm \
  OXLINT_TSGOLINT_PATH=$PWD/tsgolint-rust/e2e/tsgolint-wasm.sh \
  pnpm exec oxlint --type-aware --type-check
```

## How it works

| Crate | What it has |
|---|---|
| [`tsgolint-core`](tsgolint-core) | Program loading on ts-rust, the rule framework, the AST walk and the lint engine |
| [`tsgolint-rules`](tsgolint-rules) | The 60 rules, their shared helpers, and the test harness with tsgolint's cases and snapshots |
| [`tsgolint-rust`](tsgolint-rust) | The `tsgolint` binary: command line, `headless` protocol, tsconfig discovery, driver |
| [`tsgolint-compare`](tsgolint-compare) | A dev tool that runs the Go and Rust binaries on one payload and compares them |

Each tsconfig becomes one ts-rust program. ts-rust gives every file a checker, and each checker lives on its own thread. The engine sends each checker one job with the TypeScript diagnostics of its files, then one job that runs the rules on them. The binary streams the results to oxlint through tsgolint's 400 KB output buffer while the run goes on. [ARCHITECTURE.md](ARCHITECTURE.md) has the details.

## Development

`pnpm exec turbo run check test build` runs fmt and clippy, the Rust tests and both e2e runs, and the release builds. The toolchain is in `rust-toolchain.toml`. ts-rust comes from [pingdotgg/ts-rust](https://github.com/pingdotgg/ts-rust), branch `main`. `Cargo.lock` pins the commit, and [UPSTREAM.json](UPSTREAM.json) records it with the TypeScript commit it ports and the ported tsgolint release. A daily workflow moves ts-rust when its compiler sources change, regenerates the snapshots and opens a pull request; another opens an issue when tsgolint releases a newer version.

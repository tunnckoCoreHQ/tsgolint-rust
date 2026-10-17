# Contributing to tsgolint-rust

Read [ARCHITECTURE.md](ARCHITECTURE.md) first, then [crates/tsgolint-core/README.md](crates/tsgolint-core/README.md) for how a rule is written.

## Goal

Same behavior as [tsgolint v7.0.2004](https://github.com/oxc-project/tsgolint/tree/v7.0.2004), written as idiomatic Rust. Port the behavior, not the code: no Go structure, goroutines, channels, `defer` or Go naming. When tsgolint does something surprising that the output depends on (an odd sort, a quirk in a range, a skipped node), keep the behavior and write a one-line comment that says what tsgolint does.

The Go sources to compare with are tsgolint at that tag and the typescript-go revision it pins (its `typescript-go` submodule, with its `patches/`). ts-rust comes from [pingdotgg/ts-rust](https://github.com/pingdotgg/ts-rust), branch `main`, at the commit in `Cargo.lock`.

## Code rules

- Rust 2024 on the toolchain in `rust-toolchain.toml`. `unsafe` is forbidden.
- Rules get `&mut Checker` and read types through ts-rust's handles (`TypeId`, `SymbolId`, `SignatureId`, `Node`). Copy ids out of a borrow before the next checker call.
- Rule names and message ids stay as tsgolint has them (kebab-case rule names, camelCase message ids). Messages, help texts and suggestion texts are part of the output: copy them byte for byte from the Go source.
- Options decode with serde from the rule's `schema.json` defaults. A missing or `null` property takes its default. A wrong type panics with `<rule-name>: failed to unmarshal options: ...`, as tsgolint does.
- Shared helpers live in `crates/tsgolint-rules/src/utils` and in `tsgolint-core`. Check them before writing a local one.
- Keep comments short. Explain why, not what.

## Tests

- `pnpm exec turbo run check test e2e e2e:wasm` runs fmt and clippy, the Rust tests, and the e2e suite on the native and the wasm build.
- `cargo test -p tsgolint-rules --test rules -- <rule-name or case key>` runs tsgolint's rule cases for one rule against their expectations and our snapshots.
- `tsgolint-compare` compares the Go binary (`node_modules/.bin/tsgolint`, from the root `oxlint-tsgolint` devDependency) with this one on a real project.

## Reference output

tsgolint-rust's own output, on the ts-rust commit in `Cargo.lock`, is the reference. The rule snapshots (`crates/tsgolint-rules/tests/data/snapshots`) and the e2e snapshot hold it. When a change of ours or a ts-rust bump changes them, regenerate both and review the diff:

```sh
TSGOLINT_UPDATE_SNAPSHOTS=1 cargo test -p tsgolint-rules --test rules
cargo build -p tsgolint-rust
TSGOLINT_BIN=$PWD/target/debug/tsgolint pnpm --dir crates/tsgolint-rust/e2e run e2e -- --update
```

Every changed entry needs a cause: a TypeScript change in ts-rust, or a fix in the port. The case files (`crates/tsgolint-rules/tests/data/cases`) keep the message ids, positions and fix outputs that tsgolint's Go tests expect. When a TypeScript change breaks one of those, edit the case and give the cause in the commit message.

The Go tsgolint is the reference only when porting a new or changed rule from a newer tsgolint release. The daily tsgolint bump opens the pull request of a new release (`scripts/upstream/tsgolint-bump.sh`, `TSGOLINT_TAG=<tag>` for another one): it applies tsgolint's changes to its rule snapshots and e2e suite to ours, writes the new and changed cases to `crates/tsgolint-rules/tests/data/cases` (`scripts/upstream/dump-cases.sh <tag> <dir>`, which also writes the Go snapshots to `<dir>`), and moves UPSTREAM.json and the `oxlint-tsgolint` oracle. Port the rule changes on that pull request until the cases pass against those snapshots, then run `tsgolint-compare` with the Go binary of that release on the cases and on real projects. Each difference that remains must come from the TypeScript revision; [DIFFERENCES.md](DIFFERENCES.md) describes those. Never keep a difference that comes from a bug in the port.

## CI

CI runs on [CircleCI](https://circleci.com), from `.circleci/config.yml`. Every pull request and every push to `master` runs these jobs, and each one reports its own check:

- `changeset` fails when a pull request changes the Rust sources of a crate (`crates/**/*.rs` outside `tests/`) and adds no changeset. To merge such a change without a release, add an empty one: `pnpm changeset --empty`.
- `check (<os>)` runs `turbo run check`, then `test (<os>)` runs `turbo run test e2e e2e:wasm`, on Linux, macOS and Windows. Windows skips `e2e:wasm`: node cannot spawn the wasm build's shell wrapper there. Each OS runs on its own, so Linux never waits on macOS.
- The release builds run after the test job of their OS: `build (linux-x64)`, `build (linux-arm64)` on an arm64 machine, `build (wasm)`, `build (win32-x64)` and `build (win32-arm64)`, cross-compiled on Linux with cargo-xwin, and `build (macos)` with both macOS targets.

Each run keeps turbo's cache in the CircleCI cache, so a task whose inputs did not change is skipped. The macOS and Windows jobs pass at once when the pull request changes nothing in `crates/`, `scripts/`, `npm/`, `.circleci/`, `Cargo.*`, `rust-toolchain.toml`, `package.json`, `pnpm-lock.yaml`, `turbo.json` or `.gitattributes`: the free credits for those machines are few. A new commit cancels the older pipelines of its branch. CircleCI starts no pipeline for a pull request from a fork.

## Releases

Releases use [changesets](https://github.com/changesets/changesets) and run on CircleCI only, after every other job passes on `master`.

1. A pull request with a user-facing change adds a changeset: `pnpm changeset`, for `tsgolint-rust`. `@tsgolint-rust/wasm32-wasip1` and the platform packages always take the same version. The ts-rust and tsgolint bumps add their own.
2. A push to `master` that adds a changeset with a release publishes a snapshot version, `<version>-nightly.<datetime>.<commit>`, with the `nightly` dist-tag. A push with pending changesets also opens or updates the release PR, "chore: release tsgolint-rust <version>". The release PR bumps the npm packages, writes the changelog, sets the Cargo workspace version and adds the version's row to the README's compatibility table (`node npm/scripts/packages.mjs sync`).
3. Merging the release PR publishes the version with `latest` and creates the GitHub release `v<version>` with the binaries, `SHA256SUMS` and the attestation. To publish what a failed release left missing, trigger a pipeline on `master` with the `publish` parameter set to `true`.

The publish job takes the binaries from the build jobs of its pipeline and never builds again (`scripts/release/publish.sh`). It makes the platform packages from `npm/platform`, packs every package, and publishes the tarballs through npm trusted publishing with CircleCI's OIDC token, or with `NPM_TOKEN` when npm refuses that token. A version that is on npm already is skipped. npm makes no provenance for a publish from CircleCI, so the job signs its own: an in-toto statement with SLSA provenance that names every tarball and binary by its digest, signed keyless with Sigstore's cosign and CircleCI's OIDC token. The README's [Provenance](README.md#provenance) section shows how to verify it.

The ts-rust and tsgolint bumps are daily scheduled pipelines with the `bump` parameter (`scripts/upstream/bump.sh`).

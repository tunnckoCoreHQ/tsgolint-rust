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

The Go tsgolint is the reference only when porting a new or changed rule from a newer tsgolint release. The `tsgolint-bump` workflow opens the pull request of a new release (`scripts/upstream/tsgolint-bump.sh`, `TSGOLINT_TAG=<tag>` for another one): it applies tsgolint's changes to its rule snapshots and e2e suite to ours, writes the new and changed cases to `crates/tsgolint-rules/tests/data/cases` (`scripts/upstream/dump-cases.sh <tag> <dir>`, which also writes the Go snapshots to `<dir>`), and moves UPSTREAM.json and the `oxlint-tsgolint` oracle. Port the rule changes on that pull request until the cases pass against those snapshots, then run `tsgolint-compare` with the Go binary of that release on the cases and on real projects. Each difference that remains must come from the TypeScript revision; [DIFFERENCES.md](DIFFERENCES.md) describes those. Never keep a difference that comes from a bug in the port.

## Releases

Releases use [changesets](https://github.com/changesets/changesets) and run on GitHub only.

1. A pull request with a user-facing change adds a changeset: `pnpm changeset`, for `tsgolint-rust`. `@tsgolint-rust/wasm32-wasip1` and the platform packages always take the same version. The ts-rust and tsgolint bumps add their own.
2. When `ci` passes on `master` with pending changesets, `publish-nightly` publishes a snapshot version of them with the `nightly` dist-tag, and `release-pr` opens or updates the release PR, "chore: release tsgolint-rust <version>". The release PR bumps the npm packages, writes the changelog, sets the Cargo workspace version and adds the version's row to the README's compatibility table (`node npm/scripts/packages.mjs sync`).
3. Merging the release PR runs `publish-prod` once `ci` passes: it publishes the version with `latest` and creates the GitHub release `v<version>` with the binaries. Run `publish-prod` by hand on `master` to publish whatever a failed release left missing.

Both publish workflows build every target (`build.yml`), make the platform packages from `npm/platform`, and publish them, `@tsgolint-rust/wasm32-wasip1` and `tsgolint-rust` through npm trusted publishing, with provenance. A version that is on npm already is skipped.

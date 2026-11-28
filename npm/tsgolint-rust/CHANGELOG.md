# tsgolint-rust

## 0.4.0

### Minor Changes

- [#15](https://github.com/tunnckoCoreHQ/tsgolint-rust/pull/15) [`2f4fb51`](https://github.com/tunnckoCoreHQ/tsgolint-rust/commit/2f4fb51dad9b196feca45c40b588f6b2155a6ed3) Thanks [@olstenlarck](https://github.com/olstenlarck)! - Port tsgolint [v7.0.2009](https://github.com/oxc-project/tsgolint/releases/tag/v7.0.2009).

## 0.3.0

### Minor Changes

- [#13](https://github.com/tunnckoCoreHQ/tsgolint-rust/pull/13) [`2a0b1e0`](https://github.com/tunnckoCoreHQ/tsgolint-rust/commit/2a0b1e004701b77341546865d783003ce71625af) Thanks [@olstenlarck](https://github.com/olstenlarck)! - Port tsgolint [v7.0.2008](https://github.com/oxc-project/tsgolint/releases/tag/v7.0.2008).

## 0.2.0

### Minor Changes

- [#10](https://github.com/tunnckoCoreHQ/tsgolint-rust/pull/10) [`2640676`](https://github.com/tunnckoCoreHQ/tsgolint-rust/commit/26406768fce8270564525a0c217a7695ff2c127e) Thanks [@tunnckoCore](https://github.com/tunnckoCore)! - Port tsgolint [v7.0.2007](https://github.com/oxc-project/tsgolint/releases/tag/v7.0.2007).

### Patch Changes

- [#9](https://github.com/tunnckoCoreHQ/tsgolint-rust/pull/9) [`fdc095b`](https://github.com/tunnckoCoreHQ/tsgolint-rust/commit/fdc095b415689c12e5075b5d88e7a0f46eccd324) Thanks [@tunnckoCore](https://github.com/tunnckoCore)! - Move ts-rust to [`91f3d021`](https://github.com/pingdotgg/ts-rust/commit/91f3d021c4f1f102cebee704ac285e85ae0c8ff5), which ports TypeScript [`3e9236361fa9`](https://github.com/microsoft/TypeScript/commit/3e9236361fa97a9e6c685f4833ac457ae2d8c50a). The wasm build now matches differently cased paths on a case-insensitive file system (macOS), as the native build does.

## 0.1.0

### Minor Changes

- [#3](https://github.com/tunnckoCoreHQ/tsgolint-rust/pull/3) [`e93dd79`](https://github.com/tunnckoCoreHQ/tsgolint-rust/commit/e93dd79bce62885fef55f19be77e42b2c738874e) Thanks [@tunnckoCore](https://github.com/tunnckoCore)! - The first release: a Rust port of tsgolint v7.0.2004 on ts-rust, and a drop-in `tsgolint` for oxlint's `--type-aware` and `--type-check`. It has native builds for macOS, Linux and Windows, and a `wasm32-wasip1` build.

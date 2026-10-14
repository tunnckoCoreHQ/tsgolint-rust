# @tsgolint-rust/wasm32-wasip1

[tsgolint-rust](https://github.com/tunnckoCoreHQ/tsgolint-rust) built for `wasm32-wasip1`, with a `tsgolint` bin that runs it on Node's WASI. Use it where no native build of `tsgolint-rust` runs, or where native binaries are not allowed. It gives the same output as the native build, on one thread.

```json
{
  "devDependencies": {
    "oxlint": "^1.86.0",
    "oxlint-tsgolint": "npm:@tsgolint-rust/wasm32-wasip1@0.1.0"
  }
}
```

The bin preopens `/` and passes the working directory as `PWD`, because paths in oxlint's payload are host paths. It needs Node 20 or newer, on Linux or macOS.

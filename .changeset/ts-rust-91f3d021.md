---
"tsgolint-rust": patch
---

Move ts-rust to [`91f3d021`](https://github.com/pingdotgg/ts-rust/commit/91f3d021c4f1f102cebee704ac285e85ae0c8ff5), which ports TypeScript [`3e9236361fa9`](https://github.com/microsoft/TypeScript/commit/3e9236361fa97a9e6c685f4833ac457ae2d8c50a). The wasm build now matches differently cased paths on a case-insensitive file system (macOS), as the native build does.

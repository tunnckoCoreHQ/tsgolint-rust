# How tsgolint-rust differs from tsgolint

tsgolint-rust runs the same rules as the Go tsgolint, with the same options, messages, fixes and suggestions, and speaks the same protocol to oxlint. On most projects you get the same diagnostics. This note lists where the output can differ when you switch, and why.

The main reason is the TypeScript underneath. tsgolint v7.0.2003 is built on typescript-go `2bd066d87f5b`, the TypeScript 7.0 release branch (2026-07-07). tsgolint-rust 0.1.0 is built on ts-rust `598e6416`, a port of TypeScript `fed0bf24149f` (7.1.0-dev, 2026-10-08). A newer checker can print a type differently or reach another result, and tsgolint-rust follows it. Its own output on ts-rust is the reference for its tests; the [compatibility table](README.md#compatibility) gives the revisions of each release.

## Differences from the newer TypeScript

You can reproduce each of these with the TypeScript compiler alone: `typescript@7.0.2` behaves like tsgolint, and `typescript@7.1.0-dev.20261008.1` like tsgolint-rust.

### Long types are shortened at another point

TypeScript 7.1 elides more of a long type when it prints one in a message. The text of a diagnostic or of its labels changes, not the diagnostic itself.

```ts
type FC<P> = (props: P) => unknown;
declare const x: FC<{ a: string }> | FC<{ b: string }> | FC<{ c: string }> | FC<{ d: string }>
  | FC<{ e: string }> | ((props: { f: string }) => unknown) | ((props: { g: string }) => unknown);
export const n: number = x;
```

tsgolint prints `... | FC<...> | ((props: { ...; }) => unknown) | ((props: { ...; }) => unknown)`; tsgolint-rust prints `... | FC<...> | ... | ...`. Object members change the same way: `rerender: (ui: string) => void` becomes `rerender: ...`.

### Union members can print in another order

When a message prints a union, TypeScript 7.1 can order members of the same kind differently. For example, `no-redundant-type-constituents` on `type T = (-1n | 1n) | bigint` says `1n | -1n is overridden by bigint` in tsgolint and `-1n | 1n is overridden by bigint` in tsgolint-rust: TypeScript 7.0 orders bigint literals by internal type id, TypeScript 7.1 by value.

### New type errors with `--type-check`

TypeScript 7.1 reports some errors that 7.0 does not. For example, a destructured parameter with a default, in a function whose type comes from its context:

```ts
type S = (o?: { c?: number }) => number;
export const s: S = ({ c } = {}) => c ?? 0;
```

tsgolint-rust reports `TS7031: Binding element 'c' implicitly has an 'any' type.`; tsgolint reports nothing. On Effect's `packages/effect`, TypeScript 7.1 also reports six TS2322 and TS2345 errors that 7.0 does not.

### Rules that depend on inference or assignability

A rule that asks the checker whether a type is `any` or whether one type is assignable to another follows the newer checker:

- In TanStack Query, `queryCache.build(client, {...}, {...state, data})` infers `TData = any` in TypeScript 7.1 and `unknown` in 7.0, so `no-unsafe-assignment` reports `anyAssignment` there in tsgolint-rust only.
- TypeScript 7.1 measures the variance of some circular generic types differently and accepts an assignment that 7.0 rejects, so `no-unsafe-type-assertion` reports an assertion between two such types in tsgolint only.

### The bundled lib files

The `lib.dom.d.ts` of TypeScript 7.1 changed some signatures. `TransformStreamDefaultController.enqueue` takes `chunk?: O` in 7.0 and `chunk: O` in 7.1, so a message that prints its parameter type says `Head | ChunkData` where tsgolint says `Head | ChunkData | undefined`. With `OXC_LOG=debug`, programs also list more lib files.

## Differences by design

- **Effect diagnostics.** ts-rust has the [Effect](https://effect.website) language service diagnostics built in. When a tsconfig enables the `@effect/language-service` plugin, `--type-check` also reports Effect's diagnostics (codes TS377xxx). tsgolint has no Effect support. Effect's tsconfig warnings do not count as tsconfig errors.
- **The same output on every run.** tsgolint lints each file on whichever checker is free, so results that depend on checker history (union order, some `any` results) can change from run to run. tsgolint-rust lints each file on the checker that ts-rust assigns to it, like tsgolint with `OXLINT_TSGOLINT_CHECKER_SCHEDULING=affinity`, so its output does not change between runs. ts-rust assigns files with TypeScript 7.1's import-graph split, not tsgolint's round-robin.
- **Order of the output.** tsgolint writes programs in random order; tsgolint-rust writes them in payload order, with the inferred program last. oxlint does not depend on that order.
- **Panics.** Where tsgolint panics (for example `no-duplicate-type-constituents` with fixes on a union type that spans lines), tsgolint-rust panics with the same message and exits with code 2. It can write a few more diagnostics before it exits; oxlint ignores the output of a run that fails.
- **When oxlint stops reading.** On Unix both binaries end at their next write. On Windows tsgolint lints to the end, while tsgolint-rust exits with code 2 at once. The WASI build does the same.
- **Profiling flags.** `-trace`, `-cpuprof`, `-heap` and `-allocs` are accepted, but the files they write are empty: there is no Go runtime to profile.
- **Error texts.** The `<rule>: failed to unmarshal options: ...` message keeps tsgolint's prefix, with Rust's wording after it. Payload errors keep Go's wording, except that Go picks "cannot" or "unable to" at random and tsgolint-rust always says "cannot".
- **Regular expressions in options.** Valid patterns behave the same, but the engines differ on edge cases:
  - `switch-exhaustiveness-check` `defaultCaseCommentPattern`: tsgolint uses regexp2 in ECMAScript mode, tsgolint-rust uses `regress`.
  - `dot-notation` `allowPattern`: tsgolint uses Go's RE2, tsgolint-rust uses `regex`.
  - `prefer-regexp-exec` and `prefer-includes` check whether a pattern is valid with ts-rust's own checker, so .NET-only syntax such as `(?i)` is invalid here.

## The wasm build

The `wasm32-wasip1` build runs on one thread and gives the same output as the native build. It reads the current directory from `PWD` and needs `/` preopened at `/`; the `@tsgolint-rust/wasm32-wasip1` package sets both up.

#!/usr/bin/env node
// Runs the single-thread wasm32-wasip1 build of tsgolint on Node's WASI, as a
// drop-in for the native binary. Paths in the payload and the output are host
// paths, and realpath walks every parent directory from the root, so `/` is
// preopened at `/`. WASI has no current directory: the binary enters `PWD`,
// which is the physical working directory here. Arguments, stdio, the
// environment and the exit code pass through.
import { readFile } from 'node:fs/promises';
import process from 'node:process';

// node:wasi warns that it is experimental; oxlint shows tsgolint's stderr.
const emitWarning = process.emitWarning;
process.emitWarning = (warning, ...args) => {
  const type = typeof args[0] === 'string' ? args[0] : args[0]?.type;
  if (type === 'ExperimentalWarning' && String(warning).includes('WASI')) {
    return;
  }
  emitWarning.call(process, warning, ...args);
};
const { WASI } = await import('node:wasi');

const wasi = new WASI({
  version: 'preview1',
  args: ['tsgolint', ...process.argv.slice(2)],
  env: { ...process.env, PWD: process.cwd() },
  preopens: { '/': '/' },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(new URL('../tsgolint.wasm', import.meta.url)));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
process.exitCode = wasi.start(instance);

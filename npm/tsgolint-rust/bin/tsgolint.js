#!/usr/bin/env node
// Runs the tsgolint binary of the platform package that matches this
// machine (an optionalDependency of tsgolint-rust). Arguments, stdio and the
// exit code pass through, and a death by signal is passed on.
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { constants } from 'node:os';
import process from 'node:process';

const require = createRequire(import.meta.url);
const target = `${process.platform}-${process.arch}`;
const exe = process.platform === 'win32' ? 'tsgolint.exe' : 'tsgolint';

let binary;
try {
  binary = require.resolve(`@tsgolint-rust/${target}/${exe}`);
} catch {
  console.error(
    `tsgolint-rust: no binary for ${target}: @tsgolint-rust/${target} is not installed. ` +
      'Install again with optional dependencies, or use @tsgolint-rust/wasm32-wasip1.',
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  throw result.error;
}
if (result.signal) {
  process.kill(process.pid, result.signal);
  // Node ignores some signals (SIGPIPE): exit as a shell reports the death.
  process.exitCode = 128 + (constants.signals[result.signal] ?? 0);
} else {
  process.exitCode = result.status;
}

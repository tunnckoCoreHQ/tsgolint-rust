#!/usr/bin/env node
// The npm packages of tsgolint-rust, at the version of the Cargo workspace.
//
//   node npm/scripts/packages.mjs version
//     Prints the version of the Cargo workspace.
//
//   node npm/scripts/packages.mjs check
//     Checks that npm/tsgolint-rust and npm/wasm32-wasip1 have that version.
//
//   node npm/scripts/packages.mjs generate <artifacts> [tag]
//     For a release: fails unless `tag` is `v<version>`, writes a platform
//     package per target to npm/platforms/<target> from the npm/platform
//     template, and copies the wasm module, LICENSE and README.md into the
//     committed packages. <artifacts> holds the build of each target as
//     tsgolint-<target>/tsgolint[.exe], and tsgolint-wasm32-wasip1/tsgolint.wasm.
//     Prints the package directories in publish order: platforms, wasm, main.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const TARGETS = [
  { os: 'darwin', cpu: 'arm64', rust: 'aarch64-apple-darwin' },
  { os: 'darwin', cpu: 'x64', rust: 'x86_64-apple-darwin' },
  { os: 'linux', cpu: 'arm64', rust: 'aarch64-unknown-linux-musl' },
  { os: 'linux', cpu: 'x64', rust: 'x86_64-unknown-linux-musl' },
  { os: 'win32', cpu: 'arm64', rust: 'aarch64-pc-windows-msvc' },
  { os: 'win32', cpu: 'x64', rust: 'x86_64-pc-windows-msvc' },
];

const root = path.resolve(import.meta.dirname, '../..');
const npm = path.join(root, 'npm');
const mainDir = path.join(npm, 'tsgolint-rust');
const wasmDir = path.join(npm, 'wasm32-wasip1');

function cargoVersion() {
  const toml = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8');
  const section = toml.split(/^\[workspace\.package\]$/m)[1]?.split(/^\[/m)[0] ?? '';
  const version = section.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  assert.ok(version, 'no version in [workspace.package] of Cargo.toml');
  return version;
}

const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));
const writeJson = (file, value) => fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);

function check(version) {
  const errors = [];
  const main = readJson(path.join(mainDir, 'package.json'));
  const wasm = readJson(path.join(wasmDir, 'package.json'));
  for (const pkg of [main, wasm]) {
    if (pkg.version !== version) {
      errors.push(`${pkg.name} has version ${pkg.version}, the Cargo workspace ${version}`);
    }
  }
  const expected = Object.fromEntries(TARGETS.map((t) => [`@tsgolint-rust/${t.os}-${t.cpu}`, version]));
  try {
    assert.deepEqual(main.optionalDependencies, expected);
  } catch {
    errors.push(`tsgolint-rust's optionalDependencies must be ${JSON.stringify(expected)}`);
  }
  return errors;
}

function generate(version, artifacts, tag) {
  assert.equal(tag, `v${version}`, `the tag ${tag} does not match the Cargo workspace version ${version}`);
  const errors = check(version);
  assert.deepEqual(errors, [], errors.join('\n'));

  const template = readJson(path.join(npm, 'platform/package.json'));
  const readme = fs.readFileSync(path.join(npm, 'platform/README.md'), 'utf8');
  const license = path.join(root, 'LICENSE');
  const dirs = [];
  for (const { os, cpu } of TARGETS) {
    const target = `${os}-${cpu}`;
    const exe = os === 'win32' ? 'tsgolint.exe' : 'tsgolint';
    const dir = path.join(npm, 'platforms', target);
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
    writeJson(path.join(dir, 'package.json'), {
      ...template,
      name: `@tsgolint-rust/${target}`,
      version,
      description: template.description.replaceAll('{target}', target),
      os: [os],
      cpu: [cpu],
      files: [exe],
      publishConfig: { ...template.publishConfig, executableFiles: [exe] },
    });
    fs.writeFileSync(path.join(dir, 'README.md'), readme.replaceAll('{target}', target));
    fs.copyFileSync(license, path.join(dir, 'LICENSE'));
    // The artifact holds the binary as built; Windows builds end in .exe.
    const built = [exe, 'tsgolint'].map((name) => path.join(artifacts, `tsgolint-${target}`, name)).find(fs.existsSync);
    assert.ok(built, `no binary in ${path.join(artifacts, `tsgolint-${target}`)}`);
    fs.copyFileSync(built, path.join(dir, exe));
    fs.chmodSync(path.join(dir, exe), 0o755);
    dirs.push(dir);
  }

  fs.copyFileSync(path.join(artifacts, 'tsgolint-wasm32-wasip1', 'tsgolint.wasm'), path.join(wasmDir, 'tsgolint.wasm'));
  fs.copyFileSync(license, path.join(wasmDir, 'LICENSE'));
  fs.copyFileSync(license, path.join(mainDir, 'LICENSE'));
  fs.copyFileSync(path.join(root, 'README.md'), path.join(mainDir, 'README.md'));
  dirs.push(wasmDir, mainDir);
  return dirs;
}

const [command, ...args] = process.argv.slice(2);
const version = cargoVersion();
if (command === 'version') {
  console.log(version);
} else if (command === 'check') {
  const errors = check(version);
  for (const error of errors) {
    console.error(error);
  }
  process.exitCode = errors.length ? 1 : 0;
} else if (command === 'generate') {
  const [artifacts, tag = process.env.GITHUB_REF_NAME] = args;
  assert.ok(artifacts, 'usage: packages.mjs generate <artifacts> [tag]');
  for (const dir of generate(version, path.resolve(artifacts), tag)) {
    console.log(path.relative(root, dir));
  }
} else {
  console.error('usage: packages.mjs version | check | generate <artifacts> [tag]');
  process.exitCode = 2;
}

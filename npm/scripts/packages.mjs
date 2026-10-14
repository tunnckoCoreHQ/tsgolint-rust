#!/usr/bin/env node
// The npm packages of tsgolint-rust. The version is the one of
// npm/tsgolint-rust/package.json, which changesets bumps.
//
//   node npm/scripts/packages.mjs version
//     Prints the version of tsgolint-rust.
//
//   node npm/scripts/packages.mjs check
//     Checks that npm/wasm32-wasip1 and the Cargo workspace have that version,
//     and that the README's compatibility table has a row for it.
//
//   node npm/scripts/packages.mjs sync
//     Sets the Cargo workspace version, in Cargo.toml and Cargo.lock, to that
//     version, and writes its row of the README's compatibility table from
//     UPSTREAM.json and the oxlint of the catalog. The release PR runs it
//     after `changeset version`.
//
//   node npm/scripts/packages.mjs generate <artifacts>
//     For a publish: writes a platform package per target to
//     npm/platforms/<target> from the npm/platform template, adds them to the
//     optionalDependencies of tsgolint-rust, and copies the wasm module,
//     LICENSE and README.md into the committed packages. <artifacts> holds the
//     build of each target as tsgolint-<target>/tsgolint[.exe], and
//     tsgolint-wasm32-wasip1/tsgolint.wasm. Prints the package directories in
//     publish order: platforms, wasm, main.
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
const cargoToml = path.join(root, 'Cargo.toml');
const cargoLock = path.join(root, 'Cargo.lock');
const readmeMd = path.join(root, 'README.md');

const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));
const writeJson = (file, value) => fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);

const WORKSPACE_VERSION = /(^\[workspace\.package\]\n(?:(?!\[).*\n)*?version\s*=\s*")([^"]+)(")/m;

function cargoVersion() {
  const version = fs.readFileSync(cargoToml, 'utf8').match(WORKSPACE_VERSION)?.[2];
  assert.ok(version, 'no version in [workspace.package] of Cargo.toml');
  return version;
}

// The rows of the README's compatibility table start with the version.
const compatibilityRow = (version) => new RegExp(`^\\| ${version.replaceAll('.', '\\.')} \\|.*$`, 'm');

function compatibility(version) {
  const { tsgolint, 'ts-rust': tsRust, typescript } = readJson(path.join(root, 'UPSTREAM.json'));
  const workspace = fs.readFileSync(path.join(root, 'pnpm-workspace.yaml'), 'utf8');
  const oxlint = workspace.match(/^ {4}oxlint: (\S+)$/m)?.[1];
  assert.ok(oxlint, 'no oxlint in the catalog of pnpm-workspace.yaml');
  const ts = typescript.commit.slice(0, 12);
  const rust = tsRust.commit.slice(0, 8);
  const cells = [
    version,
    `[${tsgolint.tag}](https://github.com/${tsgolint.repository}/tree/${tsgolint.tag})`,
    `[\`${ts}\`](https://github.com/${typescript.repository}/commit/${ts}) (${typescript.version})`,
    `[\`${rust}\`](https://github.com/${tsRust.repository}/commit/${rust})`,
    oxlint,
  ];
  return `| ${cells.join(' | ')} |`;
}

function checkWasm(version) {
  const wasm = readJson(path.join(wasmDir, 'package.json'));
  return wasm.version === version ? [] : [`${wasm.name} has version ${wasm.version}, tsgolint-rust ${version}`];
}

function check(version) {
  const errors = checkWasm(version);
  const cargo = cargoVersion();
  if (cargo !== version) {
    errors.push(`the Cargo workspace has version ${cargo}, tsgolint-rust ${version}: run \`node npm/scripts/packages.mjs sync\``);
  }
  if (!compatibilityRow(version).test(fs.readFileSync(readmeMd, 'utf8'))) {
    errors.push(`the compatibility table of README.md has no row for ${version}: run \`node npm/scripts/packages.mjs sync\``);
  }
  // The platform packages of a version exist only once it is published, so
  // the workspace cannot install them: generate adds them.
  if (readJson(path.join(mainDir, 'package.json')).optionalDependencies) {
    errors.push('npm/tsgolint-rust/package.json must have no optionalDependencies: generate adds them');
  }
  return errors;
}

function sync(version) {
  const toml = fs.readFileSync(cargoToml, 'utf8');
  fs.writeFileSync(cargoToml, toml.replace(WORKSPACE_VERSION, (_, start, _old, end) => `${start}${version}${end}`));

  // The workspace crates are the packages of Cargo.lock without a source.
  const separator = '\n[[package]]\n';
  const lock = fs.readFileSync(cargoLock, 'utf8').split(separator);
  const synced = lock.map((entry, i) =>
    i === 0 || /^source = /m.test(entry) ? entry : entry.replace(/^version = "[^"]+"$/m, () => `version = "${version}"`),
  );
  fs.writeFileSync(cargoLock, synced.join(separator));

  // The newest version is the first row, under the header.
  const row = compatibility(version);
  const readme = fs.readFileSync(readmeMd, 'utf8');
  const header = /^\| tsgolint-rust \|.*\n\|[-|]+\|\n/m;
  assert.match(readme, header, 'no compatibility table in README.md');
  fs.writeFileSync(
    readmeMd,
    compatibilityRow(version).test(readme)
      ? readme.replace(compatibilityRow(version), () => row)
      : readme.replace(header, (table) => `${table}${row}\n`),
  );
}

function generate(version, artifacts) {
  const errors = checkWasm(version);
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

  const mainJson = path.join(mainDir, 'package.json');
  const main = readJson(mainJson);
  main.optionalDependencies = Object.fromEntries(TARGETS.map((t) => [`@tsgolint-rust/${t.os}-${t.cpu}`, version]));
  writeJson(mainJson, main);

  fs.copyFileSync(path.join(artifacts, 'tsgolint-wasm32-wasip1', 'tsgolint.wasm'), path.join(wasmDir, 'tsgolint.wasm'));
  fs.copyFileSync(license, path.join(wasmDir, 'LICENSE'));
  fs.copyFileSync(license, path.join(mainDir, 'LICENSE'));
  fs.copyFileSync(readmeMd, path.join(mainDir, 'README.md'));
  dirs.push(wasmDir, mainDir);
  return dirs;
}

const [command, ...args] = process.argv.slice(2);
const { version } = readJson(path.join(mainDir, 'package.json'));
if (command === 'version') {
  console.log(version);
} else if (command === 'check') {
  const errors = check(version);
  for (const error of errors) {
    console.error(error);
  }
  process.exitCode = errors.length ? 1 : 0;
} else if (command === 'sync') {
  sync(version);
} else if (command === 'generate') {
  const [artifacts] = args;
  assert.ok(artifacts, 'usage: packages.mjs generate <artifacts>');
  for (const dir of generate(version, path.resolve(artifacts))) {
    console.log(path.relative(root, dir));
  }
} else {
  console.error('usage: packages.mjs version | check | sync | generate <artifacts>');
  process.exitCode = 2;
}

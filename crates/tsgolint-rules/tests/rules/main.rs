//! tsgolint's rule tests: every case of `tests/data/cases` against the
//! expectations of tsgolint's Go tests and the snapshots in
//! `tests/data/snapshots`.
//!
//! `cargo test -p tsgolint-rules --test rules [-- <filter>]`. A trial is
//! named `<rule>::<GoTest>/valid-<i>` or `.../invalid-<i>`; the filter
//! matches any part of it. Cases that tsgolint skips are ignored.
//!
//! The snapshots hold tsgolint-rust's own output, on the ts-rust revision in
//! `Cargo.lock`. Set `TSGOLINT_UPDATE_SNAPSHOTS=1` to rewrite the entries of
//! the cases that run instead of failing on them; review the diff.

mod cases;
mod pool;
mod runner;
mod snapshot;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use libtest_mimic::{Arguments, Failed, Trial};
use rustc_hash::FxHashMap;

use cases::{Case, load_suites};
use runner::CaseSetup;
use snapshot::{format_snapshot_file, parse_snapshot_file};

struct Shared {
    fixtures: String,
    /// Snapshot entries by rule name, in file order.
    snapshots: FxHashMap<String, Vec<(String, String)>>,
    /// With `TSGOLINT_UPDATE_SNAPSHOTS`: the changed or new entries by rule
    /// name and key.
    updates: Option<Mutex<FxHashMap<String, FxHashMap<String, String>>>>,
    loading: pool::LoadingThreads,
}

impl Shared {
    fn snapshot(&self, rule: &str, key: &str) -> Option<&str> {
        let entries = self.snapshots.get(rule)?;
        let (_, content) = entries.iter().find(|(k, _)| k == key)?;
        Some(content)
    }

    /// Writes the updated entries into their `.snap` files: a changed entry
    /// keeps its place, a new one goes at the end.
    fn write_updates(&self, dir: &Path) {
        let Some(updates) = &self.updates else {
            return;
        };
        for (rule, mut changed) in std::mem::take(&mut *updates.lock().unwrap()) {
            let mut entries = self.snapshots.get(&rule).cloned().unwrap_or_default();
            for (key, content) in &mut entries {
                if let Some(new) = changed.remove(key.as_str()) {
                    *content = new;
                }
            }
            let mut added: Vec<_> = changed.into_iter().collect();
            added.sort();
            entries.extend(added);
            let path = dir.join(format!("{rule}.snap"));
            std::fs::write(&path, format_snapshot_file(&entries))
                .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
            eprintln!("updated {}", path.display());
        }
    }
}

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

/// The `.snap` files of `dir`, parsed, by rule name.
fn load_snapshots(dir: &Path) -> FxHashMap<String, Vec<(String, String)>> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            let name = path
                .file_name()?
                .to_str()?
                .strip_suffix(".snap")?
                .to_owned();
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            Some((name, parse_snapshot_file(&text)))
        })
        .collect()
}

fn main() {
    let args = Arguments::from_args();
    let data = data_dir();
    let update = std::env::var_os("TSGOLINT_UPDATE_SNAPSHOTS").is_some_and(|v| !v.is_empty());
    let shared = Arc::new(Shared {
        fixtures: data.join("fixtures").to_str().unwrap().to_owned(),
        snapshots: load_snapshots(&data.join("snapshots")),
        updates: update.then(Default::default),
        loading: pool::LoadingThreads::new(
            args.test_threads
                .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from)),
        ),
    });

    let mut trials = Vec::new();
    for suite in load_suites(&data.join("cases")) {
        let rule = tsgolint_rules::find_rule(&suite.rule)
            .unwrap_or_else(|| panic!("unknown rule {}", suite.rule));
        let only_mode = suite
            .valid
            .iter()
            .chain(&suite.invalid)
            .any(|case| case.only);
        let suite_tsconfig: Arc<str> = suite.tsconfig.as_str().into();
        let kinds = [(false, suite.valid), (true, suite.invalid)];
        for (invalid, cases) in kinds {
            for case in cases {
                let kind = if invalid { "invalid" } else { "valid" };
                let key = format!("{}/{kind}-{}", suite.test, case.index);
                let ignored = case.skip || (only_mode && !case.only);
                let name = format!("{}::{key}", suite.rule);
                let job = CaseJob {
                    shared: shared.clone(),
                    rule,
                    tsconfig: suite_tsconfig.clone(),
                    key,
                    invalid,
                    case,
                };
                trials.push(Trial::test(name, move || job.run()).with_ignored_flag(ignored));
            }
        }
    }
    let conclusion = libtest_mimic::run(&args, trials);
    shared.write_updates(&data.join("snapshots"));
    conclusion.exit();
}

struct CaseJob {
    shared: Arc<Shared>,
    rule: &'static tsgolint_core::RuleDefinition,
    tsconfig: Arc<str>,
    /// `<GoTest>/<kind>-<index>`.
    key: String,
    invalid: bool,
    case: Case,
}

impl CaseJob {
    /// Runs the case on a loading thread.
    fn run(self) -> Result<(), Failed> {
        let shared = self.shared.clone();
        shared
            .loading
            .run(move || self.check())
            .map_err(|message| Failed::from(format!("panic: {message}")))?
    }

    fn check(&self) -> Result<(), Failed> {
        let setup = CaseSetup {
            fixtures: &self.shared.fixtures,
            default_tsconfig: &self.tsconfig,
            rule: self.rule,
        };
        let failures = if self.invalid {
            let (observed, mut failures) = setup.run_invalid(&self.case)?;
            if let Some(failure) = self.check_snapshot(&observed.snapshot) {
                failures.insert(0, failure);
            }
            failures
        } else {
            setup.run_valid(&self.case)?.1
        };
        if failures.is_empty() {
            return Ok(());
        }
        Err(Failed::from(failures.join("\n\n")))
    }

    /// Compares the snapshot entry of an invalid case with `actual`, or
    /// records `actual` in update mode.
    fn check_snapshot(&self, actual: &str) -> Option<String> {
        let key = format!("[{} - 1]", self.key);
        let expected = self.shared.snapshot(self.rule.name, &key);
        if expected == Some(actual) {
            return None;
        }
        if let Some(updates) = &self.shared.updates {
            updates
                .lock()
                .unwrap()
                .entry(self.rule.name.to_owned())
                .or_default()
                .insert(key, actual.to_owned());
            return None;
        }
        Some(match expected {
            Some(expected) => format!(
                "Snapshot mismatch for {key}.\n\n--- Snapshot ---\n{expected}\n\n--- Actual ---\n{actual}"
            ),
            None => format!("No snapshot entry {key}.\n\n--- Actual ---\n{actual}"),
        })
    }
}

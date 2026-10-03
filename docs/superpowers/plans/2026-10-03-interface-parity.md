# Interface Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A test in pa-testkit that lists every difference between what pinned EVM and Solana repositories publish, separating matches, excused differences, unexcused differences, stale excuses and extraction failures.

**Architecture:** A new workspace member `interface-parity` (library, `publish = false`). It fetches each pinned repository, finds its published packages from their manifests, turns each package and each repository into a *surface* (a map from item key to the sorted values found under it), compares surfaces pair by pair, applies excuses, and renders a report. The real comparison is an `#[ignore]`d test run by `just interface-parity`; CI runs only the tool's own tests.

**Tech Stack:** Rust 2024, `public-api` 0.52.2 + `rustdoc-json` 0.9.10 on toolchain `nightly-2026-02-08`, `cargo metadata` / `cargo package --list`, Node + the package's own TypeScript compiler, `npm ci` / `npm publish --dry-run --json`, `git fetch --depth 1 <url> <sha>`, `git ls-remote --tags`.

**Spec:** `docs/superpowers/specs/2026-10-03-interface-parity-design.md`

## Global Constraints

- Branch: `anthony/interface-parity` in pa-testkit, from `origin/main`. Never push without the user's go-ahead.
- Nightly toolchain: exactly `nightly-2026-02-08`; the tool runs its `rustc` and `rustdoc` (found with `rustup which --toolchain nightly-2026-02-08 <bin>`) through `RUSTC`/`RUSTDOC`.
- No filtering of items: every public item, blanket impls included, every shipped non-source file, every tag prefix.
- A package is published unless `publish = false` (Cargo) or `"private": true` / no `name` (npm).
- Extraction failures become failing report lines; every other package is still compared. No silent skips, no `|| true`, no `2>/dev/null`.
- No mocks. Tool tests use real fixture repositories built with `git` in `CARGO_TARGET_TMPDIR`.
- Run `cargo fmt --all` before every commit. Commit after each task; end commit messages with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## File Structure

| File | Responsibility |
|---|---|
| `Cargo.toml` (root) | adds `[workspace] members = [".", "interface-parity"]` |
| `justfile`, `.github/workflows/rust.yml` | workspace-wide build/lint/test; nightly + Node in CI; `interface-parity` recipe |
| `interface-parity/Cargo.toml` | crate manifest |
| `interface-parity/src/lib.rs` | module list, `run` re-export |
| `interface-parity/src/compare.rs` | `Surface`, `Line`, `Outcome`, `compare` |
| `interface-parity/src/excuses.rs` | `Excuse`, loading `excuses.toml`, `covers` |
| `interface-parity/src/report.rs` | `Report`, `Failure`, classification, markdown |
| `interface-parity/src/tags.rs` | tag prefix and tag surface |
| `interface-parity/src/fetch.rs` | checkout at a commit, list tags |
| `interface-parity/src/packages.rs` | discovery of published Cargo and npm packages, Cargo metadata items |
| `interface-parity/src/rust_api.rs` | rustdoc JSON → surface |
| `interface-parity/src/files.rs` | shipped files → surface, JSON flattening |
| `interface-parity/src/ts_api.rs`, `interface-parity/src/ts_exports.mjs` | TypeScript exports → surface |
| `interface-parity/src/inputs.rs` | `pins.toml`, `pairs.toml` |
| `interface-parity/src/run.rs` | orchestration |
| `interface-parity/{pins,pairs,excuses}.toml` | the real comparison's inputs |
| `interface-parity/tests/evm_solana.rs` | the real comparison (`#[ignore]`) |
| `interface-parity/tests/fixtures/{evm-repo,solana-repo}/` | fixture repositories for the tool's tests |
| `interface-parity/tests/common/mod.rs` | builds fixture git repositories |
| `interface-parity/tests/{rust_api,files,ts_api,run}.rs` | tool tests |

---

### Task 1: Workspace, crate skeleton, surfaces and comparison

**Files:**
- Modify: `Cargo.toml`, `justfile`
- Create: `interface-parity/Cargo.toml`, `interface-parity/src/lib.rs`, `interface-parity/src/compare.rs`

**Interfaces:**
- Produces: `compare::{Surface, Line, Outcome, compare}`; `Surface::insert(key, value)`, `Surface::is_empty()`; `Line { pair, key, evm: Vec<String>, solana: Vec<String> }`, `Line::outcome() -> Outcome`; `compare(pair: &str, evm: &Surface, solana: &Surface) -> Vec<Line>`.

- [ ] **Step 1: Make the root a workspace and add the crate**

Append to root `Cargo.toml`:

```toml
[workspace]
members = [".", "interface-parity"]
```

`interface-parity/Cargo.toml`:

```toml
[package]
name = "interface-parity"
version = "0.1.0"
description = "Lists every difference between what paired EVM and Solana repositories publish."
edition = "2024"
license = "GPL-3.0"
publish = false

[dependencies]
anyhow = "1.0"
hex = "0.4"
public-api = "=0.52.2"
rustdoc-json = "=0.9.10"
semver = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.11.0"
toml = "1"
```

`interface-parity/src/lib.rs`:

```rust
//! Lists every difference between what paired EVM and Solana repositories
//! publish. The design is `docs/superpowers/specs/2026-10-03-interface-parity-design.md`.

pub mod compare;
```

justfile recipes become workspace-wide:

```just
fmt *args:
    cargo fmt --all {{ args }}

fmt-check:
    cargo fmt --all -- --check

build *args:
    cargo build --workspace {{ args }}

test *args:
    cargo test --workspace {{ args }}

lint:
    cargo clippy --workspace --no-deps -- -Dwarnings
    cargo clippy --workspace --no-deps --tests -- -Dwarnings
```

- [ ] **Step 2: Write the failing tests** in `interface-parity/src/compare.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn surface(items: &[(&str, &str)]) -> Surface {
        let mut s = Surface::default();
        for (k, v) in items {
            s.insert(*k, *v);
        }
        s
    }

    #[test]
    fn each_key_of_either_side_gets_one_line_with_its_outcome() {
        let evm = surface(&[("a", "1"), ("b", "1"), ("d", "x")]);
        let solana = surface(&[("a", "1"), ("b", "2"), ("c", "1")]);
        let lines = compare("E ↔ S", &evm, &solana);
        let got: Vec<(&str, Outcome)> = lines.iter().map(|l| (l.key.as_str(), l.outcome())).collect();
        assert_eq!(
            got,
            vec![
                ("a", Outcome::Match),
                ("b", Outcome::Differs),
                ("c", Outcome::OnlySolana),
                ("d", Outcome::OnlyEvm),
            ],
            "lines: {lines:#?}"
        );
        assert_eq!(lines[1].evm, vec!["1"]);
        assert_eq!(lines[1].solana, vec!["2"]);
    }

    #[test]
    fn values_under_one_key_compare_as_a_sorted_multiset() {
        let evm = surface(&[("k", "y"), ("k", "x"), ("k", "x")]);
        let same = surface(&[("k", "x"), ("k", "y"), ("k", "x")]);
        let fewer = surface(&[("k", "x"), ("k", "y")]);
        assert_eq!(compare("p", &evm, &same)[0].outcome(), Outcome::Match);
        let line = &compare("p", &evm, &fewer)[0];
        assert_eq!(line.outcome(), Outcome::Differs, "{line:#?}");
        assert_eq!(line.evm, vec!["x", "x", "y"]);
    }
}
```

- [ ] **Step 3: Run to see it fail**

Run: `cargo test -p interface-parity compare`
Expected: compile errors (`Surface`, `compare` not defined).

- [ ] **Step 4: Implement** (top of `compare.rs`)

```rust
use std::collections::{BTreeMap, BTreeSet};

/// Everything one package or repository publishes: each item's key with the
/// values found under it, kept sorted so two surfaces compare as multisets.
/// A key can carry several values, e.g. a method several impls give a type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface(BTreeMap<String, Vec<String>>);

impl Surface {
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let values = self.0.entry(key.into()).or_default();
        values.push(value.into());
        values.sort();
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Match,
    OnlyEvm,
    OnlySolana,
    Differs,
}

/// One key of a pair: the values each side publishes under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub pair: String,
    pub key: String,
    pub evm: Vec<String>,
    pub solana: Vec<String>,
}

impl Line {
    pub fn outcome(&self) -> Outcome {
        match (self.evm.is_empty(), self.solana.is_empty()) {
            (false, true) => Outcome::OnlyEvm,
            (true, false) => Outcome::OnlySolana,
            _ if self.evm == self.solana => Outcome::Match,
            _ => Outcome::Differs,
        }
    }
}

/// One line per key found on either side.
pub fn compare(pair: &str, evm: &Surface, solana: &Surface) -> Vec<Line> {
    let keys: BTreeSet<&String> = evm.0.keys().chain(solana.0.keys()).collect();
    keys.into_iter()
        .map(|key| Line {
            pair: pair.to_owned(),
            key: key.clone(),
            evm: evm.0.get(key).cloned().unwrap_or_default(),
            solana: solana.0.get(key).cloned().unwrap_or_default(),
        })
        .collect()
}
```

- [ ] **Step 5: Run to see it pass**

Run: `cargo test -p interface-parity compare` → 2 passed. Then `just build && just lint && just fmt-check`.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add Cargo.toml Cargo.lock justfile interface-parity/Cargo.toml interface-parity/src/lib.rs interface-parity/src/compare.rs
git commit -m "feat(interface-parity): workspace member with surfaces and their comparison"
```

---

### Task 2: Excuses and the report

**Files:**
- Create: `interface-parity/src/excuses.rs`, `interface-parity/src/report.rs`
- Modify: `interface-parity/src/lib.rs` (add `pub mod excuses; pub mod report;`)

**Interfaces:**
- Consumes: `compare::{Line, Outcome}`.
- Produces: `excuses::{Excuse, load(path: &Path) -> anyhow::Result<Vec<Excuse>>, parse(text: &str) -> anyhow::Result<Vec<Excuse>>}`, `Excuse::covers(&Line) -> bool`; `report::{Failure { subject: String, error: String }, Report, Report::build(pairs: Vec<String>, failures: Vec<Failure>, lines: Vec<Line>, excuses: Vec<Excuse>) -> Report, Report::passes(), Report::to_markdown(), Report::failing_text()}`.

- [ ] **Step 1: Failing tests** (`excuses.rs` and `report.rs` test modules)

```rust
// excuses.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::Line;

    const TEXT: &str = r#"
        [[excuse]]
        id = "names"
        pair = "E ↔ S"
        key = "package name"
        evm = ["e"]
        solana = ["s"]
        reason = "Package names differ by chain."
    "#;

    fn line(evm: &[&str], solana: &[&str]) -> Line {
        Line {
            pair: "E ↔ S".into(),
            key: "package name".into(),
            evm: evm.iter().map(|s| s.to_string()).collect(),
            solana: solana.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn an_excuse_covers_only_its_exact_values() {
        let excuses = parse(TEXT).unwrap();
        assert!(excuses[0].covers(&line(&["e"], &["s"])));
        assert!(!excuses[0].covers(&line(&["e"], &["s2"])), "a changed Solana value must not stay covered");
        assert!(!excuses[0].covers(&line(&["e2"], &["s"])), "a changed EVM value must not stay covered");
    }

    #[test]
    fn values_are_sorted_on_load_and_duplicate_ids_are_rejected() {
        let unsorted = TEXT.replace(r#"evm = ["e"]"#, r#"evm = ["z", "e"]"#);
        assert_eq!(parse(&unsorted).unwrap()[0].evm, vec!["e", "z"]);
        let twice = format!("{TEXT}{TEXT}");
        let err = parse(&twice).unwrap_err().to_string();
        assert!(err.contains("names"), "{err}");
    }
}

// report.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::Line;
    use crate::excuses::parse;

    fn line(key: &str, evm: &[&str], solana: &[&str]) -> Line {
        Line {
            pair: "E ↔ S".into(),
            key: key.into(),
            evm: evm.iter().map(|s| s.to_string()).collect(),
            solana: solana.iter().map(|s| s.to_string()).collect(),
        }
    }

    const EXCUSES: &str = r#"
        [[excuse]]
        id = "covers-b"
        pair = "E ↔ S"
        key = "b"
        evm = ["1"]
        solana = ["2"]
        reason = "b differs on purpose."

        [[excuse]]
        id = "stale"
        pair = "E ↔ S"
        key = "gone"
        evm = ["1"]
        solana = []
        reason = "No longer applies."
    "#;

    fn report() -> Report {
        Report::build(
            vec!["E ↔ S".into()],
            vec![],
            vec![line("a", &["1"], &["1"]), line("b", &["1"], &["2"]), line("c", &["1"], &[])],
            parse(EXCUSES).unwrap(),
        )
    }

    #[test]
    fn lines_are_split_into_matches_excused_and_unexcused_and_stale_excuses_are_found() {
        let r = report();
        assert_eq!(r.matches.iter().map(|l| l.key.as_str()).collect::<Vec<_>>(), vec!["a"]);
        assert_eq!(r.excused.len(), 1);
        assert_eq!((r.excused[0].0.key.as_str(), r.excused[0].1.id.as_str()), ("b", "covers-b"));
        assert_eq!(r.unexcused.iter().map(|l| l.key.as_str()).collect::<Vec<_>>(), vec!["c"]);
        assert_eq!(r.stale.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["stale"]);
        assert!(!r.passes());
    }

    #[test]
    fn a_report_passes_only_without_unexcused_lines_stale_excuses_or_failures() {
        let clean = Report::build(vec![], vec![], vec![line("a", &["1"], &["1"])], vec![]);
        assert!(clean.passes());
        let failed = Report::build(
            vec![],
            vec![Failure { subject: "evm/cargo:x".into(), error: "boom".into() }],
            vec![],
            vec![],
        );
        assert!(!failed.passes());
        assert!(failed.failing_text().contains("boom"), "{}", failed.failing_text());
    }

    #[test]
    fn markdown_keeps_excused_lines_apart_from_matches() {
        let md = report().to_markdown();
        let excused = md.find("## Excused").expect(&md);
        let matches = md.find("## Matches").expect(&md);
        let unexcused = md.find("## Unexcused differences").expect(&md);
        assert!(unexcused < excused && excused < matches, "{md}");
        assert!(md[excused..matches].contains("covers-b") && md[excused..matches].contains("b differs on purpose."), "{md}");
        assert!(!md[matches..].contains("covers-b"), "{md}");
    }
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p interface-parity excuses report` → compile errors.

- [ ] **Step 3: Implement `excuses.rs`**

```rust
use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::compare::Line;

/// A reviewed difference: the exact values both sides publish under one key
/// of one pair, and why that difference is accepted.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Excuse {
    pub id: String,
    pub pair: String,
    pub key: String,
    pub evm: Vec<String>,
    pub solana: Vec<String>,
    pub reason: String,
}

impl Excuse {
    pub fn covers(&self, line: &Line) -> bool {
        self.pair == line.pair && self.key == line.key && self.evm == line.evm && self.solana == line.solana
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExcusesFile {
    #[serde(default)]
    excuse: Vec<Excuse>,
}

pub fn load(path: &Path) -> anyhow::Result<Vec<Excuse>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn parse(text: &str) -> anyhow::Result<Vec<Excuse>> {
    let mut excuses = toml::from_str::<ExcusesFile>(text)?.excuse;
    let mut ids = BTreeSet::new();
    for excuse in &mut excuses {
        if !ids.insert(excuse.id.clone()) {
            bail!("excuse id {:?} appears more than once", excuse.id);
        }
        excuse.evm.sort();
        excuse.solana.sort();
    }
    Ok(excuses)
}
```

- [ ] **Step 4: Implement `report.rs`**

```rust
use std::fmt::Write;

use crate::compare::{Line, Outcome};
use crate::excuses::Excuse;

/// A package or repository whose surface could not be extracted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub subject: String,
    pub error: String,
}

#[derive(Debug)]
pub struct Report {
    pub pairs: Vec<String>,
    pub failures: Vec<Failure>,
    pub unexcused: Vec<Line>,
    pub stale: Vec<Excuse>,
    pub excused: Vec<(Line, Excuse)>,
    pub matches: Vec<Line>,
}

impl Report {
    pub fn build(pairs: Vec<String>, failures: Vec<Failure>, lines: Vec<Line>, excuses: Vec<Excuse>) -> Report {
        let mut used = vec![false; excuses.len()];
        let (mut unexcused, mut excused, mut matches) = (vec![], vec![], vec![]);
        for line in lines {
            if line.outcome() == Outcome::Match {
                matches.push(line);
            } else if let Some(i) = excuses.iter().position(|e| e.covers(&line)) {
                used[i] = true;
                excused.push((line, excuses[i].clone()));
            } else {
                unexcused.push(line);
            }
        }
        let stale = excuses.into_iter().zip(used).filter(|(_, u)| !u).map(|(e, _)| e).collect();
        Report { pairs, failures, unexcused, stale, excused, matches }
    }

    pub fn passes(&self) -> bool {
        self.failures.is_empty() && self.unexcused.is_empty() && self.stale.is_empty()
    }

    /// The sections that make the test fail.
    pub fn failing_text(&self) -> String {
        let mut out = String::new();
        write_failures(&mut out, &self.failures);
        write_lines(&mut out, "Unexcused differences", self.unexcused.iter().map(|l| (l, None)));
        write_stale(&mut out, &self.stale);
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# Interface parity report\n\n## Pairs\n\n");
        for pair in &self.pairs {
            writeln!(out, "- {pair}").unwrap();
        }
        out.push('\n');
        out.push_str(&self.failing_text());
        write_lines(&mut out, "Excused", self.excused.iter().map(|(l, e)| (l, Some(e))));
        write_lines(&mut out, "Matches", self.matches.iter().map(|l| (l, None)));
        out
    }
}

fn write_failures(out: &mut String, failures: &[Failure]) {
    writeln!(out, "## Extraction failures ({})\n", failures.len()).unwrap();
    for f in failures {
        writeln!(out, "### {}\n\n```\n{}\n```\n", f.subject, f.error.trim_end()).unwrap();
    }
}

fn write_stale(out: &mut String, stale: &[Excuse]) {
    writeln!(out, "## Stale excuses ({})\n", stale.len()).unwrap();
    for e in stale {
        writeln!(out, "- `{}` ({}, key `{}`): {}", e.id, e.pair, e.key, e.reason).unwrap();
    }
    out.push('\n');
}

fn label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Match => "match",
        Outcome::OnlyEvm => "only on EVM",
        Outcome::OnlySolana => "only on Solana",
        Outcome::Differs => "differs",
    }
}

fn write_lines<'a>(out: &mut String, heading: &str, lines: impl Iterator<Item = (&'a Line, Option<&'a Excuse>)>) {
    let lines: Vec<_> = lines.collect();
    writeln!(out, "## {heading} ({})\n\n```", lines.len()).unwrap();
    let mut pair = None;
    for (line, excuse) in lines {
        if pair != Some(&line.pair) {
            writeln!(out, "== {}", line.pair).unwrap();
            pair = Some(&line.pair);
        }
        writeln!(out, "{}  {}", label(line.outcome()), line.key).unwrap();
        for v in &line.evm {
            writeln!(out, "    EVM:    {v}").unwrap();
        }
        for v in &line.solana {
            writeln!(out, "    Solana: {v}").unwrap();
        }
        if let Some(e) = excuse {
            writeln!(out, "    excused by {}: {}", e.id, e.reason).unwrap();
        }
    }
    out.push_str("```\n\n");
}
```

- [ ] **Step 5: Run to see them pass**

Run: `cargo test -p interface-parity` → all pass; `just lint`.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add interface-parity/src
git commit -m "feat(interface-parity): excuses and the report"
```

---

### Task 3: Fetching repositories and tag prefixes

**Files:**
- Create: `interface-parity/src/fetch.rs`, `interface-parity/src/tags.rs`, `interface-parity/tests/common/mod.rs`, `interface-parity/tests/fetch.rs`
- Modify: `interface-parity/src/lib.rs` (`pub mod fetch; pub mod tags;`)

**Interfaces:**
- Produces: `fetch::{checkout(url: &str, commit: &str, dir: &Path) -> anyhow::Result<()>, tags(url: &str) -> anyhow::Result<Vec<String>>, git(dir: Option<&Path>, args: &[&str]) -> anyhow::Result<String>}`; `tags::{prefix(tag: &str) -> &str, surface(tags: &[String]) -> Surface}`.
- Produces for tests: `common::{fixture_repo(name: &str) -> (String /* file:// url */, String /* commit */), scratch(name: &str) -> PathBuf}`.

- [ ] **Step 1: Failing tests**

```rust
// tags.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_is_what_precedes_the_semver_version() {
        assert_eq!(prefix("bindings/v3.0.0"), "bindings/v");
        assert_eq!(prefix("contracts/v2.0.0-rc.7.1"), "contracts/v");
        assert_eq!(prefix("v1.0.0-beta"), "v");
        assert_eq!(prefix("1.2.3"), "");
        assert_eq!(prefix("release-candidate"), "release-candidate", "a tag without a version is its own scheme");
    }

    #[test]
    fn the_surface_holds_each_prefix_once() {
        let s = surface(&["v1.0.0".into(), "v1.1.0".into(), "bindings/v1.0.0".into()]);
        let mut expected = Surface::default();
        expected.insert("tag-prefix bindings/v", "present");
        expected.insert("tag-prefix v", "present");
        assert_eq!(s, expected);
    }
}
```

`interface-parity/tests/common/mod.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;

/// An empty directory under this test binary's temporary directory.
pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Commits `tests/fixtures/<name>` as a git repository with the tags listed
/// in its `TAGS` file, and returns its `file://` URL and commit.
pub fn fixture_repo(name: &str) -> (String, String) {
    let dir = scratch(&format!("repo-{name}"));
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name), &dir);
    run(&dir, &["init", "-q"]);
    run(&dir, &["add", "-A"]);
    run(&dir, &["-c", "user.name=fixture", "-c", "user.email=fixture@example.com", "commit", "-qm", name]);
    let tags = std::fs::read_to_string(dir.join("TAGS")).unwrap();
    for tag in tags.lines().filter(|l| !l.is_empty()) {
        run(&dir, &["tag", tag]);
    }
    (format!("file://{}", dir.display()), run(&dir, &["rev-parse", "HEAD"]))
}
```

`interface-parity/tests/fetch.rs`:

```rust
mod common;

use interface_parity::fetch;

#[test]
fn checkout_fetches_exactly_the_pinned_commit_and_tags_lists_every_tag() {
    let (url, commit) = common::fixture_repo("evm-repo");
    let dir = common::scratch("checkout-evm");
    fetch::checkout(&url, &commit, &dir.join("evm")).unwrap();
    let head = fetch::git(Some(&dir.join("evm")), &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(head.trim(), commit);
    assert!(dir.join("evm/bindings/deployments.json").exists());
    assert_eq!(fetch::tags(&url).unwrap(), vec!["bindings/v1.0.0", "contracts/v1.0.0"]);
}

#[test]
fn checkout_of_an_unknown_commit_fails_loudly() {
    let (url, _) = common::fixture_repo("solana-repo");
    let dir = common::scratch("checkout-unknown");
    let err = fetch::checkout(&url, "0000000000000000000000000000000000000001", &dir.join("x")).unwrap_err();
    assert!(err.to_string().contains("git fetch"), "{err:#}");
}
```

(The fixture repositories are created in Task 4; this task adds a minimal `tests/fixtures/evm-repo/TAGS` with `bindings/v1.0.0` and `contracts/v1.0.0`, `tests/fixtures/evm-repo/bindings/deployments.json` with `{"staging": [], "production": []}`, and `tests/fixtures/solana-repo/TAGS` empty, so these tests run now.)

- [ ] **Step 2: Run to see them fail** — `cargo test -p interface-parity tags --test fetch` → compile errors.

- [ ] **Step 3: Implement**

```rust
// tags.rs
use std::collections::BTreeSet;

use crate::compare::Surface;

/// The part of a tag before its semver version: `bindings/v3.0.0` gives
/// `bindings/v`. A tag with no version suffix is its own scheme.
pub fn prefix(tag: &str) -> &str {
    tag.char_indices()
        .find(|(i, _)| semver::Version::parse(&tag[*i..]).is_ok())
        .map_or(tag, |(i, _)| &tag[..i])
}

pub fn surface(tags: &[String]) -> Surface {
    let mut s = Surface::default();
    for p in tags.iter().map(|t| prefix(t)).collect::<BTreeSet<_>>() {
        s.insert(format!("tag-prefix {p}"), "present");
    }
    s
}
```

```rust
// fetch.rs
use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

/// Runs git and returns its stdout; a non-zero exit is an error carrying stderr.
pub fn git(dir: Option<&Path>, args: &[&str]) -> anyhow::Result<String> {
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let out = cmd.args(args).output().with_context(|| format!("running git {args:?}"))?;
    if !out.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8(out.stdout)?)
}

/// Makes `dir` a checkout of exactly `commit` from `url`.
pub fn checkout(url: &str, commit: &str, dir: &Path) -> anyhow::Result<()> {
    if dir.join(".git").exists() && git(Some(dir), &["rev-parse", "HEAD"])?.trim() == commit {
        return Ok(());
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir)?;
    git(Some(dir), &["init", "-q"])?;
    git(Some(dir), &["fetch", "-q", "--depth", "1", url, commit])?;
    git(Some(dir), &["checkout", "-q", "--detach", "FETCH_HEAD"])?;
    Ok(())
}

/// Every tag of the remote, without the peeled `^{}` entries.
pub fn tags(url: &str) -> anyhow::Result<Vec<String>> {
    let out = git(None, &["ls-remote", "--tags", url])?;
    let mut tags: Vec<String> = out
        .lines()
        .filter_map(|l| l.split('\t').nth(1))
        .filter_map(|r| r.strip_prefix("refs/tags/"))
        .filter(|t| !t.ends_with("^{}"))
        .map(str::to_owned)
        .collect();
    tags.sort();
    Ok(tags)
}
```

- [ ] **Step 4: Run to see them pass** — `cargo test -p interface-parity tags --test fetch`.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add interface-parity
git commit -m "feat(interface-parity): fetch pinned commits and compare tag prefixes"
```

---

### Task 4: Fixture repositories, package discovery and Cargo metadata

**Files:**
- Create: `interface-parity/src/packages.rs`, `interface-parity/tests/packages.rs`, fixture files below
- Modify: `interface-parity/src/lib.rs` (`pub mod packages;`)

**Interfaces:**
- Produces: `packages::{Kind { Cargo, Npm }, Package { id: String, kind: Kind, name: String, dir: PathBuf, manifest: PathBuf, lib_name: Option<String>, cargo: Option<serde_json::Value> }, discover(repo: &str, root: &Path) -> (Vec<Package>, Vec<Failure>), cargo_metadata_surface(pkg: &Package) -> Surface}`. `Package.id` is `<repo>/cargo:<name>` or `<repo>/npm:<name>`.

Fixtures (each repo has `TAGS`):

`tests/fixtures/evm-repo/bindings/Cargo.toml`:
```toml
[package]
name = "evm-bindings"
version = "1.0.0"
edition = "2024"

[workspace]

[features]
extra = []

[dependencies]
```
`tests/fixtures/evm-repo/bindings/src/lib.rs`:
```rust
pub mod addresses {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Environment {
        Staging,
        Production,
    }

    pub fn adapter_address(environment: Environment) -> Option<[u8; 20]> {
        match environment {
            Environment::Staging => Some([1; 20]),
            Environment::Production => None,
        }
    }

    pub fn only_on_evm() {}
}
```
`tests/fixtures/evm-repo/bindings/deployments.json`: `{"staging": [{"chainId": 1, "address": "0x01"}], "production": []}`
`tests/fixtures/evm-repo/integration/Cargo.toml` (must not be extracted):
```toml
[package]
name = "evm-integration"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]
```
with `src/lib.rs`: `pub fn unpublished() {}`.
`tests/fixtures/evm-repo/extra/Cargo.toml` (published, in no pair): package `evm-extra` 0.1.0 with `[workspace]` and `src/lib.rs`: `pub fn unpaired() {}`.

`tests/fixtures/solana-repo/client/Cargo.toml`: package `solana-client` 1.0.0, `[workspace]`, no features.
`tests/fixtures/solana-repo/client/src/lib.rs`:
```rust
pub mod addresses {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Environment {
        Staging,
        Production,
    }

    pub fn adapter_address(environment: Environment) -> Option<[u8; 32]> {
        match environment {
            Environment::Staging => Some([1; 32]),
            Environment::Production => None,
        }
    }
}
```
`tests/fixtures/solana-repo/client/deployments.json`: `{"staging": [{"chainId": "solana:devnet", "address": "So1"}], "production": []}`
`tests/fixtures/solana-repo/ts-private/package.json`: `{"name": "private-thing", "version": "1.0.0", "private": true}` (must not be extracted).
`tests/fixtures/solana-repo/TAGS`: `bindings/v0.1.0`.

- [ ] **Step 1: Failing test** `interface-parity/tests/packages.rs`

```rust
mod common;

use interface_parity::packages::{Kind, cargo_metadata_surface, discover};

#[test]
fn discovery_finds_published_packages_only() {
    let (url, commit) = common::fixture_repo("evm-repo");
    let dir = common::scratch("discover-evm");
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    let (packages, failures) = discover("evm", &dir);
    assert!(failures.is_empty(), "{failures:#?}");
    let ids: Vec<&str> = packages.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["evm/cargo:evm-bindings", "evm/cargo:evm-extra"], "publish = false must be left out");
    assert_eq!(packages[0].kind, Kind::Cargo);
    assert_eq!(packages[0].lib_name.as_deref(), Some("evm_bindings"));
}

#[test]
fn private_npm_packages_are_left_out() {
    let (url, commit) = common::fixture_repo("solana-repo");
    let dir = common::scratch("discover-solana");
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    let (packages, failures) = discover("solana", &dir);
    assert!(failures.is_empty(), "{failures:#?}");
    let ids: Vec<&str> = packages.iter().map(|p| p.id.as_str()).collect();
    assert!(!ids.iter().any(|i| i.contains("private-thing")), "{ids:?}");
}

#[test]
fn cargo_metadata_items_cover_version_features_and_targets() {
    let (url, commit) = common::fixture_repo("evm-repo");
    let dir = common::scratch("metadata-evm");
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    let (packages, _) = discover("evm", &dir);
    let lines = interface_parity::compare::compare("p", &cargo_metadata_surface(&packages[0]), &Default::default());
    let rendered: Vec<String> = lines.iter().map(|l| format!("{} = {:?}", l.key, l.evm)).collect();
    for expected in [
        r#"package name = ["evm-bindings"]"#,
        r#"package version = ["1.0.0"]"#,
        r#"package feature extra = ["[]"]"#,
        r#"package target evm_bindings = ["lib"]"#,
    ] {
        assert!(rendered.iter().any(|r| r == expected), "missing {expected} in {rendered:#?}");
    }
}
```

- [ ] **Step 2: Run to see it fail** — `cargo test -p interface-parity --test packages`.

- [ ] **Step 3: Implement `packages.rs`**

```rust
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::compare::Surface;
use crate::report::Failure;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cargo,
    Npm,
}

#[derive(Clone, Debug)]
pub struct Package {
    /// `<repo>/cargo:<name>` or `<repo>/npm:<name>`.
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub dir: PathBuf,
    pub manifest: PathBuf,
    /// The crate name of the library target, if the package has one.
    pub lib_name: Option<String>,
    /// The package's entry in `cargo metadata`.
    pub cargo: Option<serde_json::Value>,
}

fn manifests(dir: &Path, file: &str, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        if entry.file_type()?.is_dir() {
            if !matches!(name.to_str(), Some("target" | "node_modules" | ".git")) {
                manifests(&entry.path(), file, found)?;
            }
        } else if name == file {
            found.push(entry.path());
        }
    }
    Ok(())
}

fn cargo_metadata(manifest: &Path) -> Result<serde_json::Value, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(manifest)
        .output()
        .map_err(|e| format!("running cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

/// Every published Cargo and npm package under `root`, sorted by id. A
/// manifest that cannot be read becomes a failure.
pub fn discover(repo: &str, root: &Path) -> (Vec<Package>, Vec<Failure>) {
    let (mut packages, mut failures) = (BTreeMap::new(), vec![]);
    let mut cargo_manifests = vec![];
    let mut npm_manifests = vec![];
    if let Err(e) = manifests(root, "Cargo.toml", &mut cargo_manifests)
        .and_then(|_| manifests(root, "package.json", &mut npm_manifests))
    {
        failures.push(Failure { subject: format!("{repo} (walking {})", root.display()), error: e.to_string() });
        return (vec![], failures);
    }
    for manifest in cargo_manifests {
        let metadata = match cargo_metadata(&manifest) {
            Ok(m) => m,
            Err(error) => {
                failures.push(Failure { subject: format!("{repo} (cargo metadata {})", manifest.display()), error });
                continue;
            }
        };
        for pkg in metadata["packages"].as_array().into_iter().flatten() {
            if pkg["publish"].as_array().is_some_and(|registries| registries.is_empty()) {
                continue;
            }
            let name = pkg["name"].as_str().unwrap_or_default().to_owned();
            let manifest = PathBuf::from(pkg["manifest_path"].as_str().unwrap_or_default());
            let lib_name = pkg["targets"].as_array().into_iter().flatten().find_map(|t| {
                let kinds = t["kind"].as_array()?;
                kinds
                    .iter()
                    .any(|k| matches!(k.as_str(), Some("lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro")))
                    .then(|| t["name"].as_str().unwrap_or_default().replace('-', "_"))
            });
            let id = format!("{repo}/cargo:{name}");
            packages.entry(id.clone()).or_insert(Package {
                id,
                kind: Kind::Cargo,
                name,
                dir: manifest.parent().unwrap().to_owned(),
                manifest,
                lib_name,
                cargo: Some(pkg.clone()),
            });
        }
    }
    for manifest in npm_manifests {
        let json: serde_json::Value = match std::fs::read_to_string(&manifest)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        {
            Ok(j) => j,
            Err(error) => {
                failures.push(Failure { subject: format!("{repo} ({})", manifest.display()), error });
                continue;
            }
        };
        let Some(name) = json["name"].as_str() else { continue };
        if json["private"].as_bool() == Some(true) {
            continue;
        }
        let id = format!("{repo}/npm:{name}");
        packages.entry(id.clone()).or_insert(Package {
            id,
            kind: Kind::Npm,
            name: name.to_owned(),
            dir: manifest.parent().unwrap().to_owned(),
            manifest,
            lib_name: None,
            cargo: None,
        });
    }
    (packages.into_values().collect(), failures)
}

/// Name, version, features, targets and dependencies of a Cargo package.
pub fn cargo_metadata_surface(pkg: &Package) -> Surface {
    let mut s = Surface::default();
    let Some(m) = &pkg.cargo else { return s };
    s.insert("package name", m["name"].as_str().unwrap_or_default());
    s.insert("package version", m["version"].as_str().unwrap_or_default());
    for (feature, enables) in m["features"].as_object().into_iter().flatten() {
        s.insert(format!("package feature {feature}"), enables.to_string());
    }
    for target in m["targets"].as_array().into_iter().flatten() {
        s.insert(format!("package target {}", target["name"].as_str().unwrap_or_default()), target["kind"].as_array().map(|k| k.iter().filter_map(|k| k.as_str()).collect::<Vec<_>>().join(",")).unwrap_or_default());
    }
    for dep in m["dependencies"].as_array().into_iter().flatten() {
        let kind = dep["kind"].as_str().unwrap_or("normal");
        s.insert(
            format!("package dependency {} {kind}", dep["name"].as_str().unwrap_or_default()),
            format!(
                "req={} optional={} default-features={} features={} target={}",
                dep["req"], dep["optional"], dep["uses_default_features"], dep["features"], dep["target"]
            ),
        );
    }
    s
}
```

- [ ] **Step 4: Run to see it pass**; adjust only the expected strings if `cargo metadata` renders a field differently, printing the actual output first.

- [ ] **Step 5: Commit** (`git add interface-parity`, message `feat(interface-parity): discover published packages and their Cargo metadata`).

---

### Task 5: Rust public API

**Files:**
- Create: `interface-parity/src/rust_api.rs`, `interface-parity/tests/rust_api.rs`
- Modify: `interface-parity/src/lib.rs` (`pub mod rust_api;`)

**Interfaces:**
- Consumes: `packages::Package`.
- Produces: `rust_api::{NIGHTLY: &str, surface(pkg: &Package) -> Result<Surface, String>}`; keys `rust <path> <kind>` (`member` for fields and variants) and `rust <self type> impl <trait>`; values are renderings with the crate's own name replaced by `crate`.

- [ ] **Step 1: Failing test** `interface-parity/tests/rust_api.rs`

```rust
mod common;

use interface_parity::compare::{Outcome, compare};
use interface_parity::packages::discover;
use interface_parity::rust_api;

fn surface(fixture: &str, repo: &str) -> interface_parity::compare::Surface {
    let (url, commit) = common::fixture_repo(fixture);
    let dir = common::scratch(&format!("rust-api-{repo}"));
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    let (packages, _) = discover(repo, &dir);
    rust_api::surface(&packages[0]).unwrap()
}

#[test]
fn items_pair_by_path_with_the_crate_name_replaced() {
    let evm = surface("evm-repo", "evm");
    let solana = surface("solana-repo", "solana");
    let lines = compare("p", &evm, &solana);
    let find = |key: &str| lines.iter().find(|l| l.key == key).unwrap_or_else(|| panic!("no {key} in {lines:#?}"));

    let env = find("rust crate::addresses::Environment enum");
    assert_eq!(env.outcome(), Outcome::Match, "{env:#?}");
    assert_eq!(env.evm, vec!["pub enum crate::addresses::Environment"]);

    let addr = find("rust crate::addresses::adapter_address fn");
    assert_eq!(addr.outcome(), Outcome::Differs, "{addr:#?}");
    assert_eq!(addr.evm, vec!["pub fn crate::addresses::adapter_address(environment: crate::addresses::Environment) -> core::option::Option<[u8; 20]>"]);

    assert_eq!(find("rust crate::addresses::only_on_evm fn").outcome(), Outcome::OnlyEvm);
    assert_eq!(find("rust crate::addresses::Environment::Staging member").outcome(), Outcome::Match);
    assert_eq!(find("rust crate::addresses::Environment impl core::clone::Clone").outcome(), Outcome::Match);
}

#[test]
fn a_package_that_fails_to_build_is_an_error_carrying_the_compiler_output() {
    let (url, commit) = common::fixture_repo("evm-repo");
    let dir = common::scratch("rust-api-broken");
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    std::fs::write(dir.join("bindings/src/lib.rs"), "pub fn broken() -> u8 { \"no\" }").unwrap();
    let (packages, _) = discover("evm", &dir);
    let err = rust_api::surface(&packages[0]).unwrap_err();
    assert!(err.contains("mismatched types"), "{err}");
}
```

(If `public-api` renders `Option` with a different path, run the test, read the actual value and set the expectation to it after checking it is the correct rendering.)

- [ ] **Step 2: Run to see it fail** — `cargo test -p interface-parity --test rust_api`.

- [ ] **Step 3: Implement `rust_api.rs`**

```rust
use std::path::PathBuf;
use std::process::Command;

use public_api::tokens::Token;

use crate::compare::Surface;
use crate::packages::Package;

/// The toolchain whose rustdoc JSON `public-api` 0.52.2 reads.
pub const NIGHTLY: &str = "nightly-2026-02-08";

fn nightly_binary(name: &str) -> Result<PathBuf, String> {
    let out = Command::new("rustup")
        .args(["which", "--toolchain", NIGHTLY, name])
        .output()
        .map_err(|e| format!("running rustup: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{NIGHTLY} {name} not found ({}); install it with `rustup toolchain install {NIGHTLY} --profile minimal`",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

/// The public API of the package's library target, built with all features.
pub fn surface(pkg: &Package) -> Result<Surface, String> {
    let Some(crate_name) = &pkg.lib_name else { return Ok(Surface::default()) };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let json = rustdoc_json::Builder::default()
        .toolchain(NIGHTLY)
        .env("RUSTC", nightly_binary("rustc")?)
        .env("RUSTDOC", nightly_binary("rustdoc")?)
        .manifest_path(&pkg.manifest)
        .package(&pkg.name)
        .all_features(true)
        .build_with_captured_output(&mut stdout, &mut stderr)
        .map_err(|e| format!("{e}\n{}", String::from_utf8_lossy(&stderr)))?;
    let api = public_api::Builder::from_rustdoc_json(json)
        .include_function_parameter_names(true)
        .build()
        .map_err(|e| e.to_string())?;
    let mut s = Surface::default();
    for item in api.items() {
        let tokens: Vec<&Token> = item.tokens().collect();
        s.insert(key(&tokens, crate_name), render(&tokens, crate_name));
    }
    Ok(s)
}

/// The tokens' text, with the crate's own name replaced by `crate` wherever it starts a path.
fn render(tokens: &[&Token], crate_name: &str) -> String {
    tokens
        .iter()
        .enumerate()
        .map(|(i, t)| match t {
            Token::Identifier(name)
                if name == crate_name && matches!(tokens.get(i + 1), Some(Token::Symbol(s)) if s == "::") =>
            {
                "crate"
            }
            t => t.text(),
        })
        .collect()
}

fn is_path_token(t: &Token) -> bool {
    matches!(t, Token::Identifier(_) | Token::Type(_) | Token::Function(_) | Token::Self_(_))
        || matches!(t, Token::Symbol(s) if s == "::")
}

/// `rust <self type> impl <trait>` for an impl; `rust <path> <kind>` otherwise.
fn key(tokens: &[&Token], crate_name: &str) -> String {
    if matches!(tokens.first(), Some(Token::Keyword(k)) if k == "impl") {
        let (trait_part, self_part) = impl_parts(tokens);
        return format!(
            "rust {} impl {}",
            render(self_part, crate_name).trim(),
            render(trait_part, crate_name).trim()
        )
        .trim_end()
        .to_owned();
    }
    let kind = tokens
        .iter()
        .find_map(|t| match t {
            Token::Kind(k) => Some(k.as_str()),
            _ => None,
        })
        .unwrap_or("member");
    let start = tokens
        .iter()
        .position(|t| !matches!(t, Token::Qualifier(_) | Token::Kind(_) | Token::Keyword(_) | Token::Whitespace))
        .unwrap_or(tokens.len());
    let end = tokens[start..].iter().position(|t| !is_path_token(t)).map_or(tokens.len(), |n| start + n);
    format!("rust {} {kind}", render(&tokens[start..end], crate_name))
}

/// Splits `impl<G> Trait for Self where …` into (`Trait`, `Self`); an inherent
/// impl has an empty trait part. Generic parameters and `where` clauses are dropped.
fn impl_parts<'a>(tokens: &'a [&'a Token]) -> (&'a [&'a Token], &'a [&'a Token]) {
    let mut i = 1;
    if matches!(tokens.get(i), Some(Token::Symbol(s)) if s == "<") {
        let mut depth = 0i32;
        while i < tokens.len() {
            depth += angle_delta(tokens[i]);
            i += 1;
            if depth == 0 {
                break;
            }
        }
    }
    let body = &tokens[i..];
    let mut depth = 0i32;
    let mut for_at = None;
    let mut where_at = body.len();
    for (j, t) in body.iter().enumerate() {
        depth += angle_delta(t);
        match t {
            Token::Keyword(k) if k == "for" && depth == 0 && for_at.is_none() => for_at = Some(j),
            Token::Keyword(k) if k == "where" && depth == 0 => {
                where_at = j;
                break;
            }
            _ => {}
        }
    }
    match for_at {
        Some(f) => (&body[..f], &body[f + 1..where_at]),
        None => (&body[..0], &body[..where_at]),
    }
}

fn angle_delta(t: &Token) -> i32 {
    match t {
        Token::Symbol(s) if s != "->" => {
            s.matches('<').count() as i32 - s.matches('>').count() as i32
        }
        _ => 0,
    }
}
```

Parameter names are included (`include_function_parameter_names(true)`; `public-api` omits them by default), so the report stays complete.

- [ ] **Step 4: Run to see it pass** — `cargo test -p interface-parity --test rust_api`.

- [ ] **Step 5: Commit** (`feat(interface-parity): Rust public API surfaces from rustdoc JSON`).

---

### Task 6: Shipped files

**Files:**
- Create: `interface-parity/src/files.rs`, `interface-parity/tests/files.rs`
- Modify: `interface-parity/src/lib.rs` (`pub mod files;`)

**Interfaces:**
- Consumes: `packages::{Package, Kind}`.
- Produces: `files::{flatten_json(prefix: &str, value: &serde_json::Value, s: &mut Surface), cargo_files(pkg: &Package) -> Result<Surface, String>, npm_files(pkg: &Package, listed: &[String]) -> Result<Surface, String>, is_source(path: &str, kind: Kind) -> bool}`. Keys: `file <path>` (value `sha256 <hex>` or `generated by cargo package`), `file <path>#<json pointer>` (value: the JSON scalar, or `[]` / `{}` for an empty container).

- [ ] **Step 1: Failing tests**

```rust
// files.rs (unit tests)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::compare;

    #[test]
    fn json_flattens_to_one_key_per_scalar_and_empty_container() {
        let mut s = Surface::default();
        flatten_json("file d.json", &serde_json::json!({"staging": [{"chainId": 1}], "production": []}), &mut s);
        let lines = compare("p", &s, &Surface::default());
        let got: Vec<(&str, &str)> = lines.iter().map(|l| (l.key.as_str(), l.evm[0].as_str())).collect();
        assert_eq!(got, vec![("file d.json#/production", "[]"), ("file d.json#/staging/0/chainId", "1")]);
    }

    #[test]
    fn sources_are_left_to_the_api_extractions() {
        assert!(is_source("src/lib.rs", Kind::Cargo));
        assert!(!is_source("deployments.json", Kind::Cargo));
        for p in ["dist/index.d.ts", "dist/index.js", "src/a.ts", "dist/a.js.map", "x.mjs", "x.cjs"] {
            assert!(is_source(p, Kind::Npm), "{p}");
        }
        assert!(!is_source("package.json", Kind::Npm));
    }
}
```

```rust
// tests/files.rs
mod common;

use interface_parity::compare::{Outcome, compare};
use interface_parity::files::cargo_files;
use interface_parity::packages::discover;

#[test]
fn cargo_shipped_files_compare_json_by_key_and_other_files_by_hash() {
    let surfaces: Vec<_> = [("evm-repo", "evm"), ("solana-repo", "solana")]
        .into_iter()
        .map(|(fixture, repo)| {
            let (url, commit) = common::fixture_repo(fixture);
            let dir = common::scratch(&format!("files-{repo}"));
            interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
            let (packages, _) = discover(repo, &dir);
            cargo_files(&packages[0]).unwrap()
        })
        .collect();
    let lines = compare("p", &surfaces[0], &surfaces[1]);
    let outcome = |key: &str| lines.iter().find(|l| l.key == key).unwrap_or_else(|| panic!("no {key} in {lines:#?}")).outcome();
    assert_eq!(outcome("file deployments.json#/production"), Outcome::Match);
    assert_eq!(outcome("file deployments.json#/staging/0/chainId"), Outcome::Differs);
    assert!(lines.iter().all(|l| !l.key.ends_with(".rs")), "{lines:#?}");
    assert!(lines.iter().any(|l| l.key == "file Cargo.toml"), "{lines:#?}");
}
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement `files.rs`**

```rust
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::compare::Surface;
use crate::packages::{Kind, Package};

/// Files whose content the Rust and TypeScript API extractions already cover.
pub fn is_source(path: &str, kind: Kind) -> bool {
    match kind {
        Kind::Cargo => path.ends_with(".rs"),
        Kind::Npm => [".ts", ".js", ".mjs", ".cjs", ".map"].iter().any(|ext| path.ends_with(ext)),
    }
}

pub fn flatten_json(prefix: &str, value: &serde_json::Value, s: &mut Surface) {
    fn walk(prefix: &str, pointer: String, value: &serde_json::Value, s: &mut Surface) {
        match value {
            serde_json::Value::Object(map) if !map.is_empty() => {
                for (k, v) in map {
                    walk(prefix, format!("{pointer}/{}", k.replace('~', "~0").replace('/', "~1")), v, s);
                }
            }
            serde_json::Value::Array(items) if !items.is_empty() => {
                for (i, v) in items.iter().enumerate() {
                    walk(prefix, format!("{pointer}/{i}"), v, s);
                }
            }
            other => s.insert(format!("{prefix}#{pointer}"), other.to_string()),
        }
    }
    walk(prefix, String::new(), value, s);
}

fn file_items(dir: &Path, path: &str, generated_by: &str, s: &mut Surface) -> Result<(), String> {
    let full = dir.join(path);
    if !full.exists() {
        s.insert(format!("file {path}"), format!("generated by {generated_by}"));
        return Ok(());
    }
    let bytes = std::fs::read(&full).map_err(|e| format!("reading {}: {e}", full.display()))?;
    if path.ends_with(".json") {
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("parsing {}: {e}", full.display()))?;
        flatten_json(&format!("file {path}"), &value, s);
    } else {
        s.insert(format!("file {path}"), format!("sha256 {}", hex::encode(Sha256::digest(&bytes))));
    }
    Ok(())
}

/// The non-source files `cargo package --list` ships.
pub fn cargo_files(pkg: &Package) -> Result<Surface, String> {
    let out = Command::new("cargo")
        .args(["package", "--list", "--allow-dirty", "--manifest-path"])
        .arg(&pkg.manifest)
        .output()
        .map_err(|e| format!("running cargo package: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let mut s = Surface::default();
    for path in String::from_utf8_lossy(&out.stdout).lines().filter(|p| !is_source(p, Kind::Cargo)) {
        file_items(&pkg.dir, path, "cargo package", &mut s)?;
    }
    Ok(s)
}

/// The non-source files of an npm package, given the paths `npm publish --dry-run` listed.
pub fn npm_files(pkg: &Package, listed: &[String]) -> Result<Surface, String> {
    let mut s = Surface::default();
    for path in listed.iter().filter(|p| !is_source(p, Kind::Npm)) {
        file_items(&pkg.dir, path, "npm publish", &mut s)?;
    }
    Ok(s)
}
```

- [ ] **Step 4: Run to see them pass.**
- [ ] **Step 5: Commit** (`feat(interface-parity): shipped-file surfaces`).

---

### Task 7: TypeScript exports and npm packages

**Files:**
- Create: `interface-parity/src/ts_exports.mjs`, `interface-parity/src/ts_api.rs`, `interface-parity/tests/ts_api.rs`, fixture `tests/fixtures/solana-repo/ts/` (`package.json`, `package-lock.json`, `tsconfig.json`, `src/index.ts`)
- Modify: `interface-parity/src/lib.rs` (`pub mod ts_api;`)

**Interfaces:**
- Consumes: `packages::Package`, `files::npm_files`.
- Produces: `ts_api::surface(pkg: &Package, work: &Path) -> Result<Surface, String>` covering exports (`ts <name> <declaration kind>`, `ts <Type>.<property> <declaration kind>`) and the package's non-source shipped files.

Fixture `tests/fixtures/solana-repo/ts/package.json`:
```json
{
  "name": "@fixture/solana-client",
  "version": "1.0.0",
  "type": "module",
  "types": "dist/index.d.ts",
  "files": ["dist", "deployments.json"],
  "scripts": { "build": "tsc", "prepublishOnly": "npm run build" },
  "devDependencies": { "typescript": "5.9.3" }
}
```
`tsconfig.json`: `{"compilerOptions": {"target": "ES2022", "module": "NodeNext", "moduleResolution": "NodeNext", "declaration": true, "outDir": "dist", "strict": true}, "include": ["src"]}`
`src/index.ts`:
```ts
export interface Deployment {
  chainId: string;
  address: string;
}
export function adapterAddress(environment: "staging" | "production"): string | undefined {
  return environment === "staging" ? "So1" : undefined;
}
export const VERSION = "1.0.0";
```
`deployments.json`: `{"staging": [], "production": []}`. Generate `package-lock.json` with `npm install --package-lock-only` in that directory and commit it.

- [ ] **Step 1: Failing test** `interface-parity/tests/ts_api.rs`

```rust
mod common;

use interface_parity::compare::compare;
use interface_parity::packages::{Kind, discover};

#[test]
fn exports_and_their_members_are_items_and_built_sources_are_left_out() {
    let (url, commit) = common::fixture_repo("solana-repo");
    let dir = common::scratch("ts-solana");
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    let (packages, failures) = discover("solana", &dir);
    assert!(failures.is_empty(), "{failures:#?}");
    let pkg = packages.iter().find(|p| p.kind == Kind::Npm).expect("npm package");
    let s = interface_parity::ts_api::surface(pkg, &common::scratch("ts-work")).unwrap();
    let lines = compare("p", &s, &Default::default());
    let rendered: Vec<String> = lines.iter().map(|l| format!("{} = {}", l.key, l.evm.join(" | "))).collect();
    for expected in [
        "ts adapterAddress FunctionDeclaration = FunctionDeclaration adapterAddress: (environment: \"staging\" | \"production\") => string | undefined",
        "ts Deployment InterfaceDeclaration = InterfaceDeclaration Deployment",
        "ts Deployment.chainId InterfaceDeclaration = InterfaceDeclaration Deployment.chainId: string",
        "ts VERSION VariableDeclaration = VariableDeclaration VERSION: \"1.0.0\"",
        "file deployments.json#/production = []",
    ] {
        assert!(rendered.iter().any(|r| r == expected), "missing {expected} in {rendered:#?}");
    }
    assert!(!rendered.iter().any(|r| r.starts_with("file dist/")), "{rendered:#?}");
    assert!(rendered.iter().any(|r| r.starts_with("file package.json#/name")), "{rendered:#?}");
}
```

- [ ] **Step 2: Run to see it fail.**

- [ ] **Step 3: Implement**

`interface-parity/src/ts_exports.mjs`:

```js
// Prints one JSON object per exported item of a package's types entry:
// {"key": ..., "value": ...}. Usage: node ts_exports.mjs <package dir> <types entry>
import { createRequire } from "node:module";
import path from "node:path";

const [pkgArg, entry] = process.argv.slice(2);
const pkgDir = path.resolve(pkgArg);
const ts = createRequire(path.join(pkgDir, "package.json"))("typescript");
const entryPath = path.resolve(pkgDir, entry);
const configPath = ts.findConfigFile(pkgDir, ts.sys.fileExists, "tsconfig.json");
const options = configPath
  ? ts.parseJsonConfigFileContent(ts.readConfigFile(configPath, ts.sys.readFile).config, ts.sys, path.dirname(configPath)).options
  : {};
const program = ts.createProgram([entryPath], options);
const checker = program.getTypeChecker();
const source = program.getSourceFile(entryPath);
if (!source) throw new Error(`types entry ${entryPath} not found`);
const flags = ts.TypeFormatFlags.NoTruncation | ts.TypeFormatFlags.UseFullyQualifiedType | ts.TypeFormatFlags.InTypeAlias;
const show = (type) => checker.typeToString(type, undefined, flags).split(pkgDir).join(".");
const emit = (key, value) => console.log(JSON.stringify({ key, value }));

for (const exported of checker.getExportsOfModule(checker.getSymbolAtLocation(source))) {
  const symbol = exported.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(exported) : exported;
  const decl = symbol.declarations[0];
  const kind = ts.SyntaxKind[decl.kind];
  const typeLike = ts.SymbolFlags.Interface | ts.SymbolFlags.TypeAlias | ts.SymbolFlags.Class | ts.SymbolFlags.Enum;
  if (symbol.flags & typeLike) {
    const declared = checker.getDeclaredTypeOfSymbol(symbol);
    const alias = symbol.flags & ts.SymbolFlags.TypeAlias ? ` = ${show(declared)}` : "";
    emit(`ts ${exported.name} ${kind}`, `${kind} ${exported.name}${alias}`);
    for (const prop of checker.getPropertiesOfType(declared)) {
      const pd = prop.declarations?.[0];
      const type = pd ? checker.getTypeOfSymbolAtLocation(prop, pd) : checker.getTypeOfSymbol(prop);
      emit(`ts ${exported.name}.${prop.name} ${kind}`, `${kind} ${exported.name}.${prop.name}: ${show(type)}`);
    }
    if (symbol.flags & ts.SymbolFlags.Class) {
      emit(`ts ${exported.name} constructor`, `${kind} ${exported.name} constructor: ${show(checker.getTypeOfSymbolAtLocation(symbol, decl))}`);
    }
  } else {
    emit(`ts ${exported.name} ${kind}`, `${kind} ${exported.name}: ${show(checker.getTypeOfSymbolAtLocation(symbol, decl))}`);
  }
}
```

`interface-parity/src/ts_api.rs`:

```rust
use std::path::Path;
use std::process::Command;

use crate::compare::Surface;
use crate::files::npm_files;
use crate::packages::Package;

const SCRIPT: &str = include_str!("ts_exports.mjs");

fn run(dir: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("running {program}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{program} {} failed:\n{}", args.join(" "), String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Builds the package as publishing would, then reads its exported items and
/// its non-source shipped files.
pub fn surface(pkg: &Package, work: &Path) -> Result<Surface, String> {
    run(&pkg.dir, "npm", &["ci", "--ignore-scripts", "--no-audit", "--no-fund"])?;
    let published: serde_json::Value = serde_json::from_str(&run(&pkg.dir, "npm", &["publish", "--dry-run", "--json"])?)
        .map_err(|e| format!("parsing npm publish --dry-run --json: {e}"))?;
    let files = published["files"]
        .as_array()
        .ok_or_else(|| format!("npm publish --dry-run --json listed no files: {published}"))?;
    let listed: Vec<String> = files.iter().filter_map(|f| f["path"].as_str().map(str::to_owned)).collect();
    let mut s = npm_files(pkg, &listed)?;

    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg.manifest).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let types = manifest["exports"]["."]["types"]
        .as_str()
        .or(manifest["types"].as_str())
        .or(manifest["typings"].as_str())
        .ok_or("package.json names no types entry (exports[\".\"].types, types or typings)")?;
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let script = work.join("ts_exports.mjs");
    std::fs::write(&script, SCRIPT).map_err(|e| e.to_string())?;
    let dir = pkg.dir.to_string_lossy();
    for line in run(&pkg.dir, "node", &[&script.to_string_lossy(), &dir, types])?.lines() {
        let item: serde_json::Value = serde_json::from_str(line).map_err(|e| format!("{e}: {line}"))?;
        s.insert(item["key"].as_str().unwrap_or_default(), item["value"].as_str().unwrap_or_default());
    }
    Ok(s)
}
```

- [ ] **Step 4: Run to see it pass**; set expectations from observed output only where the observed rendering is correct TypeScript for the fixture.
- [ ] **Step 5: Commit** (`feat(interface-parity): TypeScript export surfaces of npm packages`).

---

### Task 8: Inputs, orchestration, end-to-end test, the real comparison, CI

**Files:**
- Create: `interface-parity/src/inputs.rs`, `interface-parity/src/run.rs`, `interface-parity/tests/run.rs`, `interface-parity/tests/evm_solana.rs`, `interface-parity/{pins,pairs,excuses}.toml`
- Modify: `interface-parity/src/lib.rs` (`pub mod inputs; pub mod run; pub use run::run;`), `justfile`, `.github/workflows/rust.yml`

**Interfaces:**
- Produces: `inputs::{Side { Evm, Solana }, Pin { name, side, url, commit }, Pairs { repository: Vec<PairEntry>, package: Vec<PairEntry> }, PairEntry { evm, solana }, load_pins(path), load_pairs(path)}`; `run(inputs: &Path, work: &Path) -> anyhow::Result<Report>`.

- [ ] **Step 1: Failing end-to-end test** `interface-parity/tests/run.rs`

```rust
mod common;

#[test]
fn the_report_classifies_every_line_of_the_fixture_repositories() {
    let (evm_url, evm_commit) = common::fixture_repo("evm-repo");
    let (sol_url, sol_commit) = common::fixture_repo("solana-repo");
    let inputs = common::scratch("run-inputs");
    std::fs::write(inputs.join("pins.toml"), format!(r#"
        [[repository]]
        name = "evm"
        side = "evm"
        url = "{evm_url}"
        commit = "{evm_commit}"

        [[repository]]
        name = "solana"
        side = "solana"
        url = "{sol_url}"
        commit = "{sol_commit}"
    "#)).unwrap();
    std::fs::write(inputs.join("pairs.toml"), r#"
        [[repository]]
        evm = "evm"
        solana = "solana"

        [[package]]
        evm = "evm/cargo:evm-bindings"
        solana = "solana/cargo:solana-client"
    "#).unwrap();
    std::fs::write(inputs.join("excuses.toml"), r#"
        [[excuse]]
        id = "names"
        pair = "evm/cargo:evm-bindings ↔ solana/cargo:solana-client"
        key = "package name"
        evm = ["evm-bindings"]
        solana = ["solana-client"]
        reason = "Package names differ by chain."

        [[excuse]]
        id = "stale"
        pair = "evm/cargo:evm-bindings ↔ solana/cargo:solana-client"
        key = "rust crate::addresses::gone fn"
        evm = ["pub fn crate::addresses::gone()"]
        solana = []
        reason = "Removed."
    "#).unwrap();

    let report = interface_parity::run(&inputs, &common::scratch("run-work")).unwrap();
    let md = report.to_markdown();
    std::fs::write(inputs.join("report.md"), &md).unwrap();

    assert!(report.failures.is_empty(), "{md}");
    assert_eq!(report.pairs, vec![
        "evm ↔ solana",
        "evm/cargo:evm-bindings ↔ solana/cargo:solana-client",
        "evm/cargo:evm-extra (unpaired)",
        "solana/npm:@fixture/solana-client (unpaired)",
    ], "{md}");
    let has = |lines: &[interface_parity::compare::Line], key: &str| lines.iter().any(|l| l.key == key);
    assert!(has(&report.matches, "rust crate::addresses::Environment enum"), "{md}");
    assert!(report.excused.iter().any(|(l, e)| l.key == "package name" && e.id == "names"), "{md}");
    assert!(has(&report.unexcused, "rust crate::addresses::adapter_address fn"), "{md}");
    assert!(has(&report.unexcused, "rust crate::addresses::only_on_evm fn"), "{md}");
    assert!(has(&report.unexcused, "tag-prefix contracts/v"), "{md}");
    assert!(has(&report.unexcused, "rust crate::unpaired fn"), "unpaired package items are reported: {md}");
    assert!(!md.contains("unpublished"), "publish = false packages are not extracted: {md}");
    assert!(!md.contains("private-thing"), "private npm packages are not extracted: {md}");
    assert_eq!(report.stale.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["stale"]);
    assert!(!report.passes());
}
```

- [ ] **Step 2: Run to see it fail** — `cargo test -p interface-parity --test run`.

- [ ] **Step 3: Implement `inputs.rs`**

```rust
use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Evm,
    Solana,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub name: String,
    pub side: Side,
    pub url: String,
    pub commit: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinsFile {
    repository: Vec<Pin>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairEntry {
    pub evm: String,
    pub solana: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pairs {
    #[serde(default)]
    pub repository: Vec<PairEntry>,
    #[serde(default)]
    pub package: Vec<PairEntry>,
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<T> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn load_pins(path: &Path) -> anyhow::Result<Vec<Pin>> {
    Ok(read::<PinsFile>(path)?.repository)
}

pub fn load_pairs(path: &Path) -> anyhow::Result<Pairs> {
    read(path)
}
```

- [ ] **Step 4: Implement `run.rs`**

```rust
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, bail};

use crate::compare::{Line, Surface, compare};
use crate::inputs::{PairEntry, Side, load_pairs, load_pins};
use crate::packages::{Kind, Package, cargo_metadata_surface, discover};
use crate::report::{Failure, Report};
use crate::{excuses, fetch, files, rust_api, tags, ts_api};

fn package_surface(pkg: &Package, work: &Path) -> Result<Surface, String> {
    match pkg.kind {
        Kind::Cargo => {
            let mut s = cargo_metadata_surface(pkg);
            s.extend(rust_api::surface(pkg)?);
            s.extend(files::cargo_files(pkg)?);
            Ok(s)
        }
        Kind::Npm => ts_api::surface(pkg, &work.join("ts").join(pkg.id.replace(['/', ':', '@'], "_"))),
    }
}

/// Compares the pinned repositories under `inputs` (pins.toml, pairs.toml,
/// excuses.toml), checking them out under `work`.
pub fn run(inputs: &Path, work: &Path) -> anyhow::Result<Report> {
    let pins = load_pins(&inputs.join("pins.toml"))?;
    let pairs = load_pairs(&inputs.join("pairs.toml"))?;
    let excuses = excuses::load(&inputs.join("excuses.toml"))?;

    let mut failures = vec![];
    let mut repo_surfaces: BTreeMap<String, (Side, Surface)> = BTreeMap::new();
    let mut package_surfaces: BTreeMap<String, (Side, Option<Surface>)> = BTreeMap::new();
    for pin in &pins {
        let dir = work.join("repos").join(&pin.name);
        fetch::checkout(&pin.url, &pin.commit, &dir).with_context(|| format!("checking out {}", pin.name))?;
        repo_surfaces.insert(pin.name.clone(), (pin.side, tags::surface(&fetch::tags(&pin.url)?)));
        let (packages, discovery_failures) = discover(&pin.name, &dir);
        failures.extend(discovery_failures);
        for pkg in packages {
            let surface = match package_surface(&pkg, work) {
                Ok(s) => Some(s),
                Err(error) => {
                    failures.push(Failure { subject: pkg.id.clone(), error });
                    None
                }
            };
            package_surfaces.insert(pkg.id.clone(), (pin.side, surface));
        }
    }

    let mut labels = vec![];
    let mut lines: Vec<Line> = vec![];
    for (entries, surfaces) in [
        (&pairs.repository, repo_surfaces.iter().map(|(k, (side, s))| (k.clone(), (*side, Some(s.clone())))).collect::<BTreeMap<_, _>>()),
        (&pairs.package, package_surfaces.clone()),
    ] {
        compare_all(entries, &surfaces, &mut labels, &mut lines)?;
    }
    Ok(Report::build(labels, failures, lines, excuses))
}

fn compare_all(
    entries: &[PairEntry],
    surfaces: &BTreeMap<String, (Side, Option<Surface>)>,
    labels: &mut Vec<String>,
    lines: &mut Vec<Line>,
) -> anyhow::Result<()> {
    let empty = Surface::default();
    let mut paired = BTreeSet::new();
    for entry in entries {
        for (name, side) in [(&entry.evm, Side::Evm), (&entry.solana, Side::Solana)] {
            match surfaces.get(name) {
                Some((s, _)) if *s == side => {}
                Some(_) => bail!("{name} is paired on the {side:?} side but pinned on the other"),
                None => bail!("{name} is paired but is not a pinned repository or a published package"),
            }
            paired.insert(name.clone());
        }
        let label = format!("{} ↔ {}", entry.evm, entry.solana);
        labels.push(label.clone());
        if let (Some(evm), Some(solana)) = (&surfaces[&entry.evm].1, &surfaces[&entry.solana].1) {
            lines.extend(compare(&label, evm, solana));
        }
    }
    for (name, (side, surface)) in surfaces {
        if paired.contains(name) {
            continue;
        }
        let label = format!("{name} (unpaired)");
        labels.push(label.clone());
        if let Some(s) = surface {
            lines.extend(match side {
                Side::Evm => compare(&label, s, &empty),
                Side::Solana => compare(&label, &empty, s),
            });
        }
    }
    Ok(())
}
```

Add to `compare.rs`:

```rust
impl Surface {
    pub fn extend(&mut self, other: Surface) {
        for (key, values) in other.0 {
            for value in values {
                self.insert(key.clone(), value);
            }
        }
    }
}
```

- [ ] **Step 5: Run to see the end-to-end test pass.**

- [ ] **Step 6: The real comparison**

`interface-parity/pins.toml` (the commits are those of the spec's first pin):

```toml
[[repository]]
name = "pa-evm"
side = "evm"
url = "https://github.com/anoma/pa-evm"
commit = "dbac05ae68776199ac904aae4376e886d11f199f"

[[repository]]
name = "anomapay-erc20-forwarder"
side = "evm"
url = "https://github.com/anoma/anomapay-erc20-forwarder"
commit = "8e2d30f246d595bf6b91231f76d4c25b5c0d24bf"

[[repository]]
name = "forwarder-bases"
side = "evm"
url = "https://github.com/anoma/forwarder-bases"
commit = "08f2f2e8b147fc6c9394031063664a6f3cd6a58c"

[[repository]]
name = "anoma-pa-solana-client"
side = "solana"
url = "https://github.com/anoma/anoma-pa-solana-client"
commit = "ba957aa38246cc3906cc1dc1e9c5cacd36ac5f10"

[[repository]]
name = "solana-protocol-adapter"
side = "solana"
url = "https://github.com/anoma/solana-protocol-adapter"
commit = "866d11ed20bfec13d77ac880528206f2eaa5113d"
```

`interface-parity/pairs.toml`:

```toml
[[repository]]
evm = "pa-evm"
solana = "solana-protocol-adapter"

[[repository]]
evm = "anomapay-erc20-forwarder"
solana = "solana-protocol-adapter"

[[package]]
evm = "pa-evm/cargo:anoma-pa-evm-bindings"
solana = "anoma-pa-solana-client/cargo:anoma-pa-solana-client"

[[package]]
evm = "anomapay-erc20-forwarder/cargo:anomapay-erc20-forwarder-bindings"
solana = "anoma-pa-solana-client/cargo:anoma-pa-solana-client"
```

A repository or package may sit in several pairs; `compare_all` must allow it (the `paired` set only decides what is unpaired).

`interface-parity/excuses.toml`: an empty file with the comment `# Each [[excuse]] covers one exact difference; see the spec.`

`interface-parity/tests/evm_solana.rs`:

```rust
use std::path::Path;

#[test]
#[ignore = "compares the pinned EVM and Solana repositories; run with just interface-parity"]
fn the_pinned_evm_and_solana_repositories_publish_the_same_interface() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = crate_dir.join("../target/interface-parity");
    let report = interface_parity::run(crate_dir, &work).expect("comparison ran");
    let path = work.join("report.md");
    std::fs::write(&path, report.to_markdown()).expect("report written");
    assert!(report.passes(), "full report: {}\n\n{}", path.display(), report.failing_text());
}
```

justfile:

```just
# Compare the pinned EVM and Solana repositories; the report is target/interface-parity/report.md
interface-parity:
    rustup toolchain install nightly-2026-02-08 --profile minimal
    cargo test -p interface-parity --test evm_solana -- --ignored
```

CI (`.github/workflows/rust.yml`), before "Build": install `nightly-2026-02-08` (`dtolnay/rust-toolchain@master` with `toolchain: nightly-2026-02-08`, then re-select stable with the existing step order so `cargo` stays stable) and Node (`actions/setup-node@v4`, `node-version: 24`), since the tool's tests run rustdoc JSON and TypeScript.

- [ ] **Step 7: Run everything**

Run: `just fmt-check && just build && just lint && just test` → all pass, with `evm_solana` reported as ignored.
Run: `just interface-parity` → the test FAILS with extraction failures for `protocol-adapter` and `spl-token-forwarder` (`cpi`) and unexcused differences; `target/interface-parity/report.md` exists with every section. Show the section counts.

- [ ] **Step 8: Commit** (`feat(interface-parity): pinned EVM-to-Solana comparison, run with just interface-parity`).

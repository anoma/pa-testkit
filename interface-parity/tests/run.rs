mod common;

use std::fmt::Write;
use std::path::PathBuf;

use interface_parity::compare::Line;
use interface_parity::run::run;

/// An inputs directory pinning each `(name, side, fixture)` repository at its
/// fixture commit, with `pairs` as pairs.toml and `excuses` as excuses.toml.
fn inputs(dir: &str, repositories: &[(&str, &str, &str)], pairs: &str, excuses: &str) -> PathBuf {
    let inputs = common::scratch(dir);
    let mut pins = String::new();
    for (name, side, fixture) in repositories {
        let (url, commit) = common::fixture_repo(fixture);
        writeln!(
            pins,
            "[[repository]]\nname = \"{name}\"\nside = \"{side}\"\nurl = \"{url}\"\ncommit = \"{commit}\"\n"
        )
        .unwrap();
    }
    std::fs::write(inputs.join("pins.toml"), pins).unwrap();
    std::fs::write(inputs.join("pairs.toml"), pairs).unwrap();
    std::fs::write(inputs.join("excuses.toml"), excuses).unwrap();
    inputs
}

#[test]
fn the_report_classifies_every_line_of_the_fixture_repositories() {
    let inputs = inputs(
        "run-inputs",
        &[
            ("evm", "evm", "evm-repo"),
            ("solana", "solana", "solana-repo"),
        ],
        r#"
        [[repository]]
        evm = "evm"
        solana = "solana"

        [[package]]
        evm = "evm/cargo:evm-bindings"
        solana = "solana/cargo:solana-client"
        "#,
        r#"
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
        "#,
    );

    let report = run(&inputs, &common::scratch("run-work")).unwrap();
    let md = report.to_markdown();

    assert!(report.failures.is_empty(), "{md}");
    assert_eq!(
        report.pairs,
        vec![
            "evm ↔ solana",
            "evm/cargo:evm-bindings ↔ solana/cargo:solana-client",
            "evm/cargo:evm-extra (unpaired)",
            "solana/npm:@fixture/solana-client (unpaired)",
        ],
        "{md}"
    );
    let has = |lines: &[Line], key: &str| lines.iter().any(|l| l.key == key);
    assert!(
        has(&report.matches, "rust crate::addresses::Environment enum"),
        "{md}"
    );
    assert!(
        report
            .excused
            .iter()
            .any(|(l, e)| l.key == "package name" && e.id == "names"),
        "{md}"
    );
    for key in [
        "rust crate::addresses::adapter_address fn",
        "rust crate::addresses::only_on_evm fn",
        "tag-prefix contracts/v",
        "rust crate::unpaired fn",
        "ts VERSION VariableDeclaration",
    ] {
        assert!(has(&report.unexcused, key), "{key} must be unexcused: {md}");
    }
    assert!(
        !md.contains("unpublished"),
        "publish = false packages are not extracted: {md}"
    );
    assert!(
        !md.contains("private-thing"),
        "private npm packages are not extracted: {md}"
    );
    assert_eq!(
        report
            .stale
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["stale"]
    );
    assert!(!report.passes());
}

#[test]
fn a_pair_naming_something_unpinned_is_an_error() {
    let inputs = inputs(
        "run-bad-inputs",
        &[("evm", "evm", "evm-repo")],
        "[[package]]\nevm = \"evm/cargo:evm-bindings\"\nsolana = \"solana/cargo:nowhere\"\n",
        "",
    );
    let err = run(&inputs, &common::scratch("run-bad-work")).unwrap_err();
    assert!(err.to_string().contains("solana/cargo:nowhere"), "{err:#}");
}

#[test]
fn a_package_that_fails_to_extract_is_a_failure_and_the_rest_is_still_compared() {
    let inputs = inputs(
        "run-broken-inputs",
        &[
            ("evm", "evm", "evm-repo"),
            ("broken", "solana", "broken-repo"),
        ],
        "[[package]]\nevm = \"evm/cargo:evm-bindings\"\nsolana = \"broken/cargo:broken-client\"\n",
        "",
    );

    let report = run(&inputs, &common::scratch("run-broken-work")).unwrap();
    let md = report.to_markdown();
    assert_eq!(
        report
            .failures
            .iter()
            .map(|f| f.subject.as_str())
            .collect::<Vec<_>>(),
        vec!["broken/cargo:broken-client"],
        "{md}"
    );
    assert!(
        report.failures[0]
            .error
            .contains("cannot find type `NoSuchType`"),
        "{md}"
    );
    assert!(
        !report
            .unexcused
            .iter()
            .any(|l| l.pair == "evm/cargo:evm-bindings ↔ broken/cargo:broken-client"),
        "a pair with a failed side adds no lines: {md}"
    );
    assert!(
        report
            .unexcused
            .iter()
            .any(|l| l.pair == "evm/cargo:evm-extra (unpaired)"),
        "{md}"
    );
    assert!(md.contains("## Extraction failures (1)"), "{md}");
    assert!(!report.passes());
}

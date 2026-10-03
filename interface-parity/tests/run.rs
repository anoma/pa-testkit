mod common;

use interface_parity::compare::Line;

#[test]
fn the_report_classifies_every_line_of_the_fixture_repositories() {
    let (evm_url, evm_commit) = common::fixture_repo("evm-repo");
    let (sol_url, sol_commit) = common::fixture_repo("solana-repo");
    let inputs = common::scratch("run-inputs");
    std::fs::write(
        inputs.join("pins.toml"),
        format!(
            r#"
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
            "#
        ),
    )
    .unwrap();
    std::fs::write(
        inputs.join("pairs.toml"),
        r#"
        [[repository]]
        evm = "evm"
        solana = "solana"

        [[package]]
        evm = "evm/cargo:evm-bindings"
        solana = "solana/cargo:solana-client"
        "#,
    )
    .unwrap();
    std::fs::write(
        inputs.join("excuses.toml"),
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
    )
    .unwrap();

    let report = interface_parity::run(&inputs, &common::scratch("run-work")).unwrap();
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
    let (evm_url, evm_commit) = common::fixture_repo("evm-repo");
    let inputs = common::scratch("run-bad-inputs");
    std::fs::write(
        inputs.join("pins.toml"),
        format!(
            "[[repository]]\nname = \"evm\"\nside = \"evm\"\nurl = \"{evm_url}\"\ncommit = \"{evm_commit}\"\n"
        ),
    )
    .unwrap();
    std::fs::write(
        inputs.join("pairs.toml"),
        "[[package]]\nevm = \"evm/cargo:evm-bindings\"\nsolana = \"solana/cargo:nowhere\"\n",
    )
    .unwrap();
    std::fs::write(inputs.join("excuses.toml"), "").unwrap();
    let err = interface_parity::run(&inputs, &common::scratch("run-bad-work")).unwrap_err();
    assert!(err.to_string().contains("solana/cargo:nowhere"), "{err:#}");
}

#[test]
fn a_package_that_fails_to_extract_is_a_failure_and_the_rest_is_still_compared() {
    let (evm_url, evm_commit) = common::fixture_repo("evm-repo");
    let (broken_url, broken_commit) = common::fixture_repo("broken-repo");
    let inputs = common::scratch("run-broken-inputs");
    std::fs::write(
        inputs.join("pins.toml"),
        format!(
            r#"
            [[repository]]
            name = "evm"
            side = "evm"
            url = "{evm_url}"
            commit = "{evm_commit}"

            [[repository]]
            name = "broken"
            side = "solana"
            url = "{broken_url}"
            commit = "{broken_commit}"
            "#
        ),
    )
    .unwrap();
    std::fs::write(
        inputs.join("pairs.toml"),
        "[[package]]\nevm = \"evm/cargo:evm-bindings\"\nsolana = \"broken/cargo:broken-client\"\n",
    )
    .unwrap();
    std::fs::write(inputs.join("excuses.toml"), "").unwrap();

    let report = interface_parity::run(&inputs, &common::scratch("run-broken-work")).unwrap();
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

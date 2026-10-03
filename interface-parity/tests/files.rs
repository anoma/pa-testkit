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
            let dir = common::scratch(&format!("checkout-{fixture}"));
            interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
            let (packages, _) = discover(repo, &dir);
            cargo_files(&packages[0]).unwrap()
        })
        .collect();
    let lines = compare("p", &surfaces[0], &surfaces[1]);
    let outcome = |key: &str| {
        lines
            .iter()
            .find(|l| l.key == key)
            .unwrap_or_else(|| panic!("no {key} in {lines:#?}"))
            .outcome()
    };
    assert_eq!(outcome("file deployments.json#/production"), Outcome::Match);
    assert_eq!(
        outcome("file deployments.json#/staging/0/chainId"),
        Outcome::Differs
    );
    assert_eq!(outcome("file Cargo.toml"), Outcome::Differs);
    assert!(lines.iter().all(|l| !l.key.ends_with(".rs")), "{lines:#?}");
}

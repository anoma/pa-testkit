mod checkout;
mod common;

use checkout::checkout;
use interface_parity::compare::{Outcome, compare};
use interface_parity::files::cargo_files;
use interface_parity::packages::discover;

#[test]
fn cargo_shipped_files_are_the_files_cargo_packages() {
    let surfaces: Vec<_> = [("evm-repo", "evm"), ("solana-repo", "solana")]
        .into_iter()
        .map(|(fixture, repo)| {
            let (packages, _) = discover(repo, &checkout(fixture));
            cargo_files(&packages[0]).unwrap()
        })
        .collect();
    let lines = compare("p", &surfaces[0], &surfaces[1]);
    let line = |key: &str| {
        lines
            .iter()
            .find(|l| l.key == key)
            .unwrap_or_else(|| panic!("no {key} in {lines:#?}"))
    };
    assert_eq!(
        line("file deployments.json#/production").outcome(),
        Outcome::Match
    );
    // A JSON file is an item, present on both sides, and each of its values
    // belongs to it.
    assert_eq!(line("file deployments.json").outcome(), Outcome::Match);
    assert_eq!(
        line("file deployments.json#/staging/0/chainId")
            .parent
            .as_deref(),
        Some("file deployments.json")
    );
    assert_eq!(
        line("file deployments.json#/staging/0/chainId").outcome(),
        Outcome::Differs
    );
    // The files that describe the package compare by presence (files.rs,
    // `description_key`): the EVM fixture's readme is README.md, the Solana
    // one's readme.txt.
    for key in [
        "file Cargo.toml",
        "file Cargo.toml.orig",
        "file Cargo.lock",
        "file .cargo_vcs_info.json",
        "file README",
    ] {
        let l = line(key);
        assert_eq!(l.outcome(), Outcome::Match, "{l:#?}");
        assert_eq!(l.evm, vec!["present"], "{l:#?}");
    }
    assert!(
        !lines
            .iter()
            .any(|l| l.key.starts_with("file .cargo_vcs_info.json#/")),
        "{lines:#?}"
    );
    assert!(lines.iter().all(|l| !l.key.ends_with(".rs")), "{lines:#?}");
}

/// A crate with a dependency that names no version is one a registry cannot
/// take, so it is consumed from git. Its files are still those cargo counts
/// as the crate's, so they, its metadata and its API are read as for any
/// crate.
#[test]
fn a_crate_a_registry_cannot_take_still_has_its_files_and_api() {
    let (packages, _) = discover("unversioned", &checkout("unversioned-repo"));
    let [pkg] = &packages[..] else {
        panic!("the publish = false core is not published: {packages:#?}");
    };
    let work = common::scratch("files-unversioned");
    let surface = interface_parity::run::package_surface(pkg, &work, &[], None)
        .unwrap()
        .0;
    let lines = compare("p", &surface, &Default::default());
    for key in [
        "file Cargo.toml",
        "file deployments.json",
        "file deployments.json#/staging",
    ] {
        assert!(lines.iter().any(|l| l.key == key), "no {key} in {lines:#?}");
    }
    assert!(
        lines
            .iter()
            .any(|l| l.key.starts_with("rust crate::answer")),
        "{lines:#?}"
    );
}

mod checkout;
mod common;

use checkout::checkout;
use interface_parity::compare::{Outcome, compare};
use interface_parity::files::cargo_files;
use interface_parity::packages::{Kind, discover};

#[test]
fn cargo_shipped_files_are_read_from_the_package_archive() {
    let surfaces: Vec<_> = [("evm-repo", "evm"), ("solana-repo", "solana")]
        .into_iter()
        .map(|(fixture, repo)| {
            let (packages, _) = discover(repo, &checkout(fixture));
            let Kind::Cargo(meta) = &packages[0].kind else {
                panic!("{:?} is not a Cargo package", packages[0].id);
            };
            cargo_files(
                &packages[0],
                meta,
                &common::scratch(&format!("files-{repo}")),
            )
            .unwrap()
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
    // `is_description`): the EVM fixture's readme is README.md, the Solana
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
/// take, so it is consumed from git: it ships no archive, and so no files,
/// and the rest of what it publishes (its metadata and API) is still read.
#[test]
fn a_crate_a_registry_cannot_take_ships_no_files_but_still_has_its_api() {
    let (packages, _) = discover("unversioned", &checkout("unversioned-repo"));
    let [pkg] = &packages[..] else {
        panic!("the publish = false core is not published: {packages:#?}");
    };
    let Kind::Cargo(meta) = &pkg.kind else {
        panic!("{:?} is not a Cargo package", pkg.id);
    };
    let work = common::scratch("files-unversioned");
    let files = cargo_files(pkg, meta, &work).expect("a git-consumed crate has no archive");
    let lines = compare("p", &files, &Default::default());
    assert!(lines.is_empty(), "{lines:#?}");

    let surface = interface_parity::run::package_surface(pkg, &work, &[], None)
        .unwrap()
        .0;
    let lines = compare("p", &surface, &Default::default());
    assert!(
        lines
            .iter()
            .any(|l| l.key.starts_with("rust crate::answer")),
        "{lines:#?}"
    );
}

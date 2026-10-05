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
    assert_eq!(
        line("file deployments.json#/staging/0/chainId").outcome(),
        Outcome::Differs
    );
    // The archive holds the normalized manifest and cargo's own files, with content.
    for key in ["file Cargo.toml", "file Cargo.toml.orig", "file Cargo.lock"] {
        let l = line(key);
        assert!(
            l.evm[0].starts_with("sha256 ") && l.solana[0].starts_with("sha256 "),
            "{l:#?}"
        );
    }
    assert!(
        lines
            .iter()
            .any(|l| l.key.starts_with("file .cargo_vcs_info.json#/")),
        "{lines:#?}"
    );
    assert!(lines.iter().all(|l| !l.key.ends_with(".rs")), "{lines:#?}");
}

mod common;

use interface_parity::compare::compare;
use interface_parity::packages::{Kind, cargo_metadata_surface, discover};

fn checkout(fixture: &str) -> std::path::PathBuf {
    let (url, commit) = common::fixture_repo(fixture);
    let dir = common::scratch(&format!("checkout-{fixture}"));
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    dir
}

#[test]
fn discovery_finds_published_packages_only() {
    let (packages, failures) = discover("evm", &checkout("evm-repo"));
    assert!(failures.is_empty(), "{failures:#?}");
    let ids: Vec<&str> = packages.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["evm/cargo:evm-bindings", "evm/cargo:evm-extra"],
        "publish = false must be left out"
    );
    assert_eq!(packages[0].kind, Kind::Cargo);
    assert_eq!(packages[0].lib_name.as_deref(), Some("evm_bindings"));
}

#[test]
fn private_npm_packages_are_left_out() {
    let (packages, failures) = discover("solana", &checkout("solana-repo"));
    assert!(failures.is_empty(), "{failures:#?}");
    let ids: Vec<&str> = packages.iter().map(|p| p.id.as_str()).collect();
    assert!(!ids.iter().any(|i| i.contains("private-thing")), "{ids:?}");
    assert!(ids.contains(&"solana/cargo:solana-client"), "{ids:?}");
}

#[test]
fn cargo_metadata_items_cover_name_version_features_and_targets() {
    let (packages, _) = discover("evm", &checkout("evm-repo"));
    let lines = compare(
        "p",
        &cargo_metadata_surface(&packages[0]),
        &Default::default(),
    );
    let rendered: Vec<String> = lines
        .iter()
        .map(|l| format!("{} = {:?}", l.key, l.evm))
        .collect();
    for expected in [
        r#"package name = ["evm-bindings"]"#,
        r#"package version = ["1.0.0"]"#,
        r#"package feature extra = ["[]"]"#,
        r#"package target evm_bindings = ["lib"]"#,
    ] {
        assert!(
            rendered.iter().any(|r| r == expected),
            "missing {expected} in {rendered:#?}"
        );
    }
}

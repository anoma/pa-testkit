mod checkout;
mod common;

use checkout::checkout;
use interface_parity::compare::compare;
use interface_parity::packages::{Kind, cargo_metadata_surface, discover};

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
    assert!(matches!(packages[0].kind, Kind::Cargo(_)));
    assert_eq!(packages[0].lib_name().as_deref(), Some("evm_bindings"));
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
    let Kind::Cargo(meta) = &packages[0].kind else {
        panic!("{:?} is not a Cargo package", packages[0].id);
    };
    let lines = compare("p", &cargo_metadata_surface(meta), &Default::default());
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

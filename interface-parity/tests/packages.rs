mod checkout;
mod common;

use checkout::checkout;
use interface_parity::compare::compare;
use interface_parity::packages::{Kind, cargo_metadata_surface, discover, lib_target};

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
    let Kind::Cargo(meta) = &packages[0].kind else {
        panic!("{:?} is not a Cargo package", packages[0].id);
    };
    assert!(lib_target(meta).is_some(), "{meta:#?} has a library target");
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
fn a_workspace_member_is_found_once_under_a_root_path_with_dot_dot() {
    // The comparison's work directory is `interface-parity/../target/...`,
    // while cargo reports manifest paths with `..` resolved.
    let root = checkout("workspace-repo").join("member/..");
    let (packages, failures) = discover("ws", &root);
    assert!(failures.is_empty(), "{failures:#?}");
    let ids: Vec<&str> = packages.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["ws/cargo:member"]);
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

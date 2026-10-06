mod checkout;
mod common;

use checkout::checkout;
use interface_parity::compare::{Outcome, Surface, compare};
use interface_parity::packages::{Kind, discover};
use interface_parity::rust_api;

/// The Rust API of the first package of the fixture repository.
fn surface(fixture: &str, repo: &str) -> anyhow::Result<Surface> {
    let (packages, _) = discover(repo, &checkout(fixture));
    let Kind::Cargo(meta) = &packages[0].kind else {
        panic!("{:?} is not a Cargo package", packages[0].id);
    };
    rust_api::surface(meta, &[])
}

#[test]
fn items_pair_by_path_with_the_crate_name_replaced() {
    let lines = compare(
        "p",
        &surface("evm-repo", "evm").unwrap(),
        &surface("solana-repo", "solana").unwrap(),
    );
    let find = |key: &str| {
        lines
            .iter()
            .find(|l| l.key == key)
            .unwrap_or_else(|| panic!("no {key} in {lines:#?}"))
    };

    let env = find("rust crate::addresses::Environment enum");
    assert_eq!(env.outcome(), Outcome::Match, "{env:#?}");
    assert_eq!(env.evm, vec!["pub enum crate::addresses::Environment"]);

    let addr = find("rust crate::addresses::adapter_address fn");
    assert_eq!(addr.outcome(), Outcome::Differs, "{addr:#?}");
    assert_eq!(
        addr.evm,
        vec![
            "pub fn crate::addresses::adapter_address(environment: crate::addresses::Environment) -> core::option::Option<[u8; 20]>"
        ]
    );

    assert_eq!(
        find("rust crate::addresses::only_on_evm fn").outcome(),
        Outcome::OnlyEvm
    );
    assert_eq!(
        find("rust crate::addresses::Environment::Staging member").outcome(),
        Outcome::Match
    );
    assert_eq!(
        find("rust crate::addresses::Environment impl core::clone::Clone").outcome(),
        Outcome::Match
    );
    let field = find("rust crate::addresses::Deployment::chain_id member");
    assert_eq!(field.outcome(), Outcome::Differs, "{field:#?}");
    assert_eq!(
        (field.evm.clone(), field.solana.clone()),
        (
            vec!["pub crate::addresses::Deployment::chain_id: u64".to_owned()],
            vec!["pub crate::addresses::Deployment::chain_id: alloc::string::String".to_owned()]
        )
    );
    let variant = find("rust crate::addresses::Cluster::Devnet member");
    assert_eq!(variant.outcome(), Outcome::Differs, "{variant:#?}");
    let root = find("rust crate mod");
    assert_eq!(root.outcome(), Outcome::Match, "{root:#?}");
    assert_eq!(root.evm, vec!["pub mod crate"]);
}

#[test]
fn a_package_that_fails_to_build_is_an_error_carrying_the_compiler_output() {
    let err = format!("{:#}", surface("broken-repo", "broken").unwrap_err());
    assert!(err.contains("cannot find type `NoSuchType`"), "{err}");
}

/// A crate that takes a value from the build environment documents only
/// with its repository's build environment set.
#[test]
fn a_crate_reading_the_build_environment_documents_with_it() {
    let dir = checkout("env-repo");
    let (packages, _) = discover("env", &dir);
    let Kind::Cargo(meta) = &packages[0].kind else {
        panic!("{:?} is not a Cargo package", packages[0].id);
    };
    let error = rust_api::surface(meta, &[]).expect_err("FIXTURE_PROGRAM_ID is unset");
    assert!(
        format!("{error:#}").contains("FIXTURE_PROGRAM_ID"),
        "the failure names the variable: {error:#}"
    );

    let env = interface_parity::inputs::load_env(&dir.join("env/localnet.env")).unwrap();
    assert_eq!(
        env,
        vec![(
            "FIXTURE_PROGRAM_ID".to_owned(),
            "Fixture1111111111111111111111111111111111111".to_owned()
        )]
    );
    let s = rust_api::surface(meta, &env).unwrap();
    let lines = compare("p", &s, &Surface::default());
    assert!(
        lines.iter().any(|l| l.key == "rust crate::ID const"),
        "the documented API holds ID: {lines:#?}"
    );
}

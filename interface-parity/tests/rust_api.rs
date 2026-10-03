mod checkout;
mod common;

use checkout::checkout;
use interface_parity::compare::{Outcome, Surface, compare};
use interface_parity::packages::discover;
use interface_parity::rust_api;

fn surface(fixture: &str, repo: &str) -> Surface {
    let (packages, _) = discover(repo, &checkout(fixture));
    rust_api::surface(&packages[0]).unwrap()
}

#[test]
fn items_pair_by_path_with_the_crate_name_replaced() {
    let lines = compare(
        "p",
        &surface("evm-repo", "evm"),
        &surface("solana-repo", "solana"),
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
    let dir = checkout("evm-repo");
    std::fs::write(
        dir.join("bindings/src/lib.rs"),
        "pub fn broken() -> NoSuchType { todo!() }",
    )
    .unwrap();
    let (packages, _) = discover("evm", &dir);
    let err = format!("{:#}", rust_api::surface(&packages[0]).unwrap_err());
    assert!(err.contains("cannot find type `NoSuchType`"), "{err}");
}

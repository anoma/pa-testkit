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

/// A blanket impl is listed where it is declared, and not again on each type
/// it covers: whether a type has it follows from the declaration (the
/// crate's own, or a dependency's) and the type's own impls, which stay.
#[test]
fn a_blanket_impl_is_listed_where_declared_not_on_each_type() {
    let s = surface("blanket-repo", "blanket").unwrap();
    let lines = compare("p", &s, &Surface::default());
    let keys: Vec<&str> = lines.iter().map(|l| l.key.as_str()).collect();
    assert!(
        !keys.iter().any(|k| k.contains("impl core::convert::Into")
            || k.contains("impl alloc::borrow::ToOwned")
            || k.contains("impl core::borrow::Borrow")),
        "an item from another crate's blanket impl: {keys:#?}"
    );
    assert!(
        keys.contains(&"rust T impl crate::Labelled"),
        "the crate's own blanket impl is gone: {keys:#?}"
    );
    assert!(
        keys.contains(&"rust crate::Item impl crate::Marked"),
        "Item's own impl is gone: {keys:#?}"
    );
    // Auto traits follow from field types, private ones included: they stay.
    for auto in ["core::marker::Send", "core::marker::Sync"] {
        let key = format!("rust crate::Item impl {auto}");
        assert!(keys.contains(&key.as_str()), "{key} is gone: {keys:#?}");
    }
}

/// An item's key is its own path, whatever precedes it in the rendering: two
/// enums with `#[repr]` attributes are two keys, and a method of an impl on a
/// primitive type is keyed by the primitive's path.
#[test]
fn an_attribute_or_a_primitive_self_type_keeps_the_items_path_in_its_key() {
    let s = surface("keys-repo", "keys").unwrap();
    let lines = compare("p", &s, &Surface::default());
    let keys: Vec<&str> = lines.iter().map(|l| l.key.as_str()).collect();
    for key in [
        "rust crate::Code enum",
        "rust crate::Small enum",
        "rust u8::from fn",
        "rust u8 impl core::convert::From<crate::Wrapped>",
    ] {
        assert!(keys.contains(&key), "no key {key}: {keys:#?}");
    }
    assert!(
        !keys.iter().any(|k| k.starts_with("rust  ")),
        "an item keyed with an empty path: {keys:#?}"
    );
}

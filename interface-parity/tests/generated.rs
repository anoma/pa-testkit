use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use interface_parity::compare::{Outcome, Surface, compare};
use interface_parity::fetch;
use interface_parity::generated::{self, ForgeBindModule};
use interface_parity::inputs::Foundry;
use interface_parity::rust_api::RustApi;

/// The fixture crate source: `forge bind` v1.8.5's module for a small
/// contract (`generated/token.rs`), a copy of it with one hand-written
/// function (`generated/edited.rs`) and a hand-written root.
fn fixture_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/forge-bind/src")
}

fn module(name: &str) -> ForgeBindModule {
    generated::parse(&fixture_src().join("generated").join(name))
        .unwrap()
        .unwrap_or_else(|| panic!("{name} is a forge bind module"))
}

/// The Foundry release that wrote the fixture modules, fetched once into the
/// tests' temporary directory.
fn forge() -> PathBuf {
    static FORGE: OnceLock<PathBuf> = OnceLock::new();
    FORGE
        .get_or_init(|| {
            let foundry = Foundry {
                version: "1.8.5".into(),
                sha256: [
                    (
                        "linux_amd64",
                        "6c66ffcc55fa4249197721baa3098bc208014ea1d8aa04b2ed50ac6bccffb226",
                    ),
                    (
                        "darwin_arm64",
                        "56049728e25e44a61c7f2e49438023b0890cb45850ffcb7c882f90cd998baf83",
                    ),
                ]
                .into_iter()
                .map(|(p, s)| (p.to_owned(), s.to_owned()))
                .collect(),
            };
            fetch::foundry(
                &foundry,
                &Path::new(env!("CARGO_TARGET_TMPDIR")).join("tools"),
            )
            .unwrap()
        })
        .clone()
}

/// The keys of `surface`, each with its values.
fn items(surface: &Surface) -> Vec<(String, Vec<String>)> {
    compare("x", surface, &Surface::default())
        .into_iter()
        .map(|l| (l.key, l.evm))
        .collect()
}

#[test]
fn a_forge_bind_module_carries_its_contracts_abi_and_bytecode() {
    let token = module("token.rs");
    assert_eq!(token.contract, "Token");
    assert_eq!(
        token.abi.as_array().map(Vec::len),
        Some(3),
        "{:#}",
        token.abi
    );
    assert_eq!(
        token.bytecode.as_deref(),
        Some(&hex::decode("6080604052348015600e575f5ffd5b50").unwrap()[..])
    );
    assert_eq!(
        token.deployed_bytecode.as_deref(),
        Some(&hex::decode("6080604052").unwrap()[..])
    );
    assert!(
        generated::parse(&fixture_src().join("lib.rs"))
            .unwrap()
            .is_none(),
        "a hand-written file is no forge bind module"
    );
}

#[test]
fn a_source_files_module_path_follows_the_crate_layout() {
    let src = fixture_src();
    for (file, path) in [
        ("lib.rs", "crate"),
        ("generated/mod.rs", "crate::generated"),
        ("generated/token.rs", "crate::generated::token"),
    ] {
        assert_eq!(generated::module_path(&src, &src.join(file)).unwrap(), path);
    }
}

#[test]
fn forge_bind_reproduces_the_module_it_wrote_and_not_a_hand_edited_copy() {
    let forge = forge();
    assert!(generated::reproduces(&module("token.rs"), &forge).unwrap());
    assert!(!generated::reproduces(&module("edited.rs"), &forge).unwrap());
}

/// An API of the given items, declared in no file.
fn api(items: &[(&str, &str)]) -> RustApi {
    let mut api = RustApi::default();
    for (key, value) in items {
        api.surface.insert(*key, *value);
    }
    api
}

/// The fixture module `name`'s file.
fn file(name: &str) -> PathBuf {
    fixture_src().join("generated").join(name)
}

#[test]
fn collapsing_replaces_a_modules_rust_items_with_its_abi_and_keeps_every_other_item() {
    let mut api = api(&[
        (
            "rust crate::generated::token::Token::transferCall struct",
            "pub struct crate::generated::token::Token::transferCall",
        ),
        (
            "rust crate::generated::token::Token::BYTECODE static",
            "pub static crate::generated::token::Token::BYTECODE: Bytes",
        ),
        (
            "rust crate::generated::token mod",
            "pub mod crate::generated::token",
        ),
        (
            "rust crate::generated::tokens fn",
            "pub fn crate::generated::tokens()",
        ),
        ("rust crate::deployments fn", "pub fn crate::deployments()"),
    ]);

    generated::collapse(
        &mut api,
        "crate::generated::token",
        &file("token.rs"),
        &module("token.rs"),
    )
    .unwrap();

    let got = items(&api.surface);
    let keys: Vec<&str> = got.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "forge bind crate::generated::token bytecode",
            "forge bind crate::generated::token error InsufficientBalance",
            "forge bind crate::generated::token event Transferred",
            "forge bind crate::generated::token fn transfer",
            "forge bind crate::generated::token module",
            "rust crate::deployments fn",
            "rust crate::generated::tokens fn",
        ],
        "the module's Rust items give way to its ABI; a module whose name only \
         starts with the same letters and the hand-written items stay: {got:#?}"
    );
    let value = |key: &str| got.iter().find(|(k, _)| k == key).unwrap().1.clone();
    assert_eq!(
        value("forge bind crate::generated::token module"),
        vec!["Token"]
    );
    for entry in ["bytecode", "error InsufficientBalance", "fn transfer"] {
        assert_eq!(
            api.surface
                .parent(&format!("forge bind crate::generated::token {entry}")),
            Some("forge bind crate::generated::token module"),
            "{entry} belongs to its module"
        );
    }
    assert_eq!(
        value("forge bind crate::generated::token bytecode"),
        vec!["creation", "runtime"]
    );
    assert_eq!(
        value("forge bind crate::generated::token fn transfer"),
        vec!["(to: address, amount: u256) -> bool"]
    );
}

#[test]
fn collapsing_a_module_the_api_does_not_have_is_an_error() {
    let mut api = api(&[("rust crate::deployments fn", "pub fn crate::deployments()")]);
    let err = generated::collapse(
        &mut api,
        "crate::generated::token",
        &file("token.rs"),
        &module("token.rs"),
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("crate::generated::token"),
        "{err:#}"
    );
}

/// The items a module's file declares go with it wherever their path puts
/// them: an impl of a foreign trait on a primitive, and an impl on a type
/// from another crate. The same items declared in another file stay.
#[test]
fn collapsing_takes_the_items_the_modules_file_declares_and_keeps_those_of_other_files() {
    let generated = [
        (
            "rust u8 impl core::convert::From<crate::generated::token::Token::Kind>",
            "impl core::convert::From<crate::generated::token::Token::Kind> for u8",
        ),
        (
            "rust u8::from fn",
            "pub fn u8::from(value: crate::generated::token::Token::Kind) -> Self",
        ),
        (
            "rust alloy_primitives::log::LogData impl core::convert::From<&crate::generated::token::Token::Transferred>",
            "impl core::convert::From<&crate::generated::token::Token::Transferred> for alloy_primitives::log::LogData",
        ),
    ];
    let hand_written = (
        "rust u8::from fn",
        "pub fn u8::from(value: crate::Code) -> Self",
    );
    let mut api = api(&[
        generated[0],
        generated[1],
        generated[2],
        hand_written,
        (
            "rust crate::generated::token mod",
            "pub mod crate::generated::token",
        ),
    ]);
    let declared = |items: &[(&str, &str)]| {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    api.declared
        .insert("src/generated/token.rs".into(), declared(&generated));
    api.declared
        .insert("src/conv.rs".into(), declared(&[hand_written]));

    generated::collapse(
        &mut api,
        "crate::generated::token",
        &file("token.rs"),
        &module("token.rs"),
    )
    .unwrap();

    let got = items(&api.surface);
    let rust: Vec<_> = got.iter().filter(|(k, _)| k.starts_with("rust ")).collect();
    assert_eq!(
        rust,
        vec![&(
            "rust u8::from fn".to_owned(),
            vec![hand_written.1.to_owned()]
        )],
        "only the item declared in another file stays: {got:#?}"
    );
}

#[test]
fn a_crates_reproduced_modules_collapse_and_the_others_stay_item_by_item() {
    let mut api = RustApi::default();
    for module in ["token", "edited"] {
        api.surface.insert(
            format!("rust crate::generated::{module}::Token::transferCall struct"),
            format!("pub struct crate::generated::{module}::Token::transferCall"),
        );
    }
    let notes = generated::collapse_crate(&mut api, &fixture_src(), Some(&forge())).unwrap();

    let keys: Vec<String> = items(&api.surface).into_iter().map(|(k, _)| k).collect();
    assert!(
        keys.contains(&"rust crate::generated::edited::Token::transferCall struct".into()),
        "the hand-edited module keeps its items: {keys:#?}"
    );
    assert!(
        !keys
            .iter()
            .any(|k| k.starts_with("rust crate::generated::token::")),
        "the reproduced module's items are collapsed: {keys:#?}"
    );
    assert_eq!(notes.len(), 1, "{notes:#?}");
    assert!(
        notes[0].contains("crate::generated::edited") && notes[0].contains("1.8.5"),
        "{notes:#?}"
    );
}

#[test]
fn without_a_pinned_foundry_every_module_stays_item_by_item_and_is_noted() {
    let mut api = api(&[(
        "rust crate::generated::token::Token::transferCall struct",
        "pub struct crate::generated::token::Token::transferCall",
    )]);
    let before = api.surface.clone();
    let notes = generated::collapse_crate(&mut api, &fixture_src(), None).unwrap();
    assert_eq!(api.surface, before);
    assert_eq!(notes.len(), 2, "one note per module: {notes:#?}");
    assert!(notes.iter().all(|n| n.contains("Foundry")), "{notes:#?}");
    let lines = compare("x", &api.surface, &Surface::default());
    assert!(lines.iter().all(|l| l.outcome() == Outcome::OnlyEvm));
}

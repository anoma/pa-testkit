mod common;

use interface_parity::fetch;

#[test]
fn checkout_fetches_exactly_the_pinned_commit_and_tags_lists_every_tag() {
    let (url, commit) = common::fixture_repo("evm-repo");
    let dir = common::scratch("checkout-evm");
    fetch::checkout(&url, &commit, &dir.join("evm")).unwrap();
    let head = fetch::git(Some(&dir.join("evm")), &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(head.trim(), commit);
    assert!(dir.join("evm/bindings/deployments.json").exists());
    assert_eq!(
        fetch::tags(&url).unwrap(),
        vec!["bindings/v1.0.0", "contracts/v1.0.0"]
    );
}

#[test]
fn checkout_of_an_unknown_commit_fails_loudly() {
    let (url, _) = common::fixture_repo("solana-repo");
    let dir = common::scratch("checkout-unknown");
    let err = fetch::checkout(
        &url,
        "0000000000000000000000000000000000000001",
        &dir.join("x"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("git fetch"), "{err:#}");
}

mod common;

use interface_parity::compare::compare;
use interface_parity::packages::{Kind, discover};

#[test]
fn exports_and_their_members_are_items_and_built_sources_are_left_out() {
    let (url, commit) = common::fixture_repo("solana-repo");
    let dir = common::scratch("checkout-solana-repo");
    interface_parity::fetch::checkout(&url, &commit, &dir).unwrap();
    let (packages, failures) = discover("solana", &dir);
    assert!(failures.is_empty(), "{failures:#?}");
    let pkg = packages
        .iter()
        .find(|p| p.kind == Kind::Npm)
        .expect("npm package");
    let s = interface_parity::ts_api::surface(pkg, &common::scratch("ts-work")).unwrap();
    let lines = compare("p", &s, &Default::default());
    let rendered: Vec<String> = lines
        .iter()
        .map(|l| format!("{} = {}", l.key, l.evm.join(" | ")))
        .collect();
    for expected in [
        "ts adapterAddress FunctionDeclaration = FunctionDeclaration adapterAddress: (environment: \"staging\" | \"production\") => string | undefined",
        "ts Deployment InterfaceDeclaration = InterfaceDeclaration Deployment",
        "ts Deployment.chainId InterfaceDeclaration = InterfaceDeclaration Deployment.chainId: string",
        "ts VERSION VariableDeclaration = VariableDeclaration VERSION: \"1.0.0\"",
        "file deployments.json#/production = []",
        "file package.json#/name = \"@fixture/solana-client\"",
    ] {
        assert!(
            rendered.iter().any(|r| r == expected),
            "missing {expected} in {rendered:#?}"
        );
    }
    assert!(
        !rendered.iter().any(|r| r.starts_with("file dist/")),
        "{rendered:#?}"
    );
}

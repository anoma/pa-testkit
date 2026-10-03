mod checkout;
mod common;

use checkout::checkout;
use interface_parity::compare::compare;
use interface_parity::packages::{Kind, discover};

#[test]
fn an_npm_package_is_built_then_its_exports_and_packed_files_are_items() {
    let (packages, failures) = discover("solana", &checkout("solana-repo"));
    assert!(failures.is_empty(), "{failures:#?}");
    let pkg = packages
        .iter()
        .find(|p| matches!(p.kind, Kind::Npm(_)))
        .expect("npm package");
    let s = interface_parity::run::package_surface(pkg, &common::scratch("ts-work")).unwrap();
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
        "built sources are left to the export extraction: {rendered:#?}"
    );
}

use std::path::Path;

#[test]
#[ignore = "compares the pinned EVM and Solana repositories; run with just interface-parity"]
fn the_pinned_evm_and_solana_repositories_publish_the_same_interface() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = crate_dir.join("../target/interface-parity");
    let report = interface_parity::run::run(crate_dir, &work).expect("comparison ran");
    let path = work.join("report.md");
    std::fs::write(&path, report.to_markdown()).expect("report written");
    assert!(
        report.passes(),
        "full report: {}\n\n{}",
        path.display(),
        report.failing_text()
    );
}

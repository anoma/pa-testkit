//! Chain-free smoke tests for the trivial-action fixtures and the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::trivial;
use anoma_pa_testkit::prover::LocalProver;

#[test]
fn build_trivial_action_produces_single_compliance_unit() {
    let built =
        trivial::build(1, trivial::Overrides::default()).expect("valid trivial action must build");
    assert_eq!(built.witnesses.compliance_witnesses.len(), 1);
}

#[test]
fn build_trivial_action_with_overrides_builds_non_ephemeral_consumed() {
    let built = trivial::build(2, trivial::Overrides::invalid_consumed_non_ephemeral())
        .expect("action construction should succeed");
    assert_eq!(built.witnesses.compliance_witnesses.len(), 1);
}

#[test]
fn build_trivial_action_with_overrides_builds_nonzero_quantity() {
    let built = trivial::build(3, trivial::Overrides::invalid_nonzero_quantity())
        .expect("action construction should succeed");
    assert_eq!(built.witnesses.compliance_witnesses.len(), 1);
}

#[tokio::test]
async fn local_prover_constrains_trivial_actions() {
    let actions = trivial::build_many(8, 1).expect("must build trivial action witnesses");
    LocalProver
        .prove(&actions)
        .await
        .expect("local prover must constrain trivial actions");
}

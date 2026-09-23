//! Chain-free smoke tests for the trivial-action fixtures and the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::trivial;
use anoma_pa_testkit::prover::LocalProver;

#[test]
fn build_trivial_action_defaults_to_one_resource_per_side() {
    let built =
        trivial::build(1, trivial::Overrides::default()).expect("valid trivial action must build");
    assert_eq!(built.consumed_ephemerals.len(), 1);
    assert_eq!(built.created_ephemerals.len(), 1);
    assert_eq!(built.witnesses.logic_witnesses.len(), 2);
}

#[test]
fn build_trivial_action_sizes_the_action_via_the_count_overrides() {
    let built = trivial::build(
        4,
        trivial::Overrides {
            consumed_count: Some(2),
            created_count: Some(3),
            ..trivial::Overrides::default()
        },
    )
    .expect("an n:m trivial action must build");
    assert_eq!(built.consumed_ephemerals.len(), 2);
    assert_eq!(built.created_ephemerals.len(), 3);
    assert_eq!(built.witnesses.logic_witnesses.len(), 5);
}

#[test]
fn build_trivial_action_errors_without_consumed_resources() {
    // Created nonces derive from the consumed nullifiers, so a create-only
    // action is unbuildable — mirroring the compliance circuit, which rejects
    // an empty consumed list.
    let result = trivial::build(
        5,
        trivial::Overrides {
            consumed_count: Some(0),
            created_count: Some(1),
            ..trivial::Overrides::default()
        },
    );
    assert!(result.is_err(), "a create-only action must fail to build");
}

#[test]
fn build_trivial_action_with_overrides_builds_non_ephemeral_consumed() {
    let built = trivial::build(2, trivial::Overrides::invalid_consumed_non_ephemeral())
        .expect("action construction should succeed");
    assert_eq!(built.consumed_ephemerals.len(), 1);
}

#[test]
fn build_trivial_action_with_overrides_builds_nonzero_quantity() {
    let built = trivial::build(3, trivial::Overrides::invalid_nonzero_quantity())
        .expect("action construction should succeed");
    assert_eq!(built.consumed_ephemerals.len(), 1);
}

#[tokio::test]
async fn local_prover_mock_aggregates_trivial_actions() {
    let actions = trivial::build_many(8, 1).expect("must build trivial action witnesses");
    let txn = LocalProver
        .prove(&actions)
        .await
        .expect("local prover must constrain trivial actions");

    let arm_txn = txn.as_arm();
    let aggregation = arm_txn
        .aggregation
        .as_ref()
        .expect("the transaction must carry an aggregation");
    assert!(
        arm_txn.actions.is_none(),
        "the base proofs must be erased like after real aggregation"
    );
    assert_eq!(aggregation.instance.actions.len(), 8);
}

#[tokio::test]
async fn local_prover_mock_aggregates_an_n_to_m_action() {
    let built = trivial::build(
        7,
        trivial::Overrides {
            consumed_count: Some(2),
            created_count: Some(3),
            ..trivial::Overrides::default()
        },
    )
    .expect("an n:m trivial action must build");

    let txn = LocalProver
        .prove(&[built.witnesses])
        .await
        .expect("local prover must constrain an n:m trivial action");

    let aggregation = txn
        .as_arm()
        .aggregation
        .as_ref()
        .expect("the transaction must carry an aggregation");
    assert_eq!(aggregation.instance.actions[0].consumed_publics.len(), 2);
    assert_eq!(aggregation.instance.actions[0].created_publics.len(), 3);
}

#[tokio::test]
async fn local_prover_mock_aggregates_a_consume_only_action() {
    let built = trivial::build(
        9,
        trivial::Overrides {
            consumed_count: Some(2),
            created_count: Some(0),
            ..trivial::Overrides::default()
        },
    )
    .expect("a consume-only trivial action must build");

    let txn = LocalProver
        .prove(&[built.witnesses])
        .await
        .expect("local prover must constrain a consume-only trivial action");

    let aggregation = txn
        .as_arm()
        .aggregation
        .as_ref()
        .expect("the transaction must carry an aggregation");
    assert_eq!(aggregation.instance.actions[0].consumed_publics.len(), 2);
    assert!(aggregation.instance.actions[0].created_publics.is_empty());
}

//! Chain-free smoke tests for the trivial-action fixtures and the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::trivial;
use anoma_pa_testkit::prover::LocalProver;
use anoma_pa_testkit::witness::ActionWitnesses;
use anoma_rm_risc0::Digest;
use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::compliance;

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
    let is_consumed: Vec<bool> = aggregation.instance.actions[0]
        .resource_instances
        .iter()
        .map(|resource| resource.is_consumed())
        .collect();
    assert_eq!(is_consumed, [true, true, false, false, false]);
}

#[tokio::test]
async fn local_prover_keeps_the_order_of_the_compliance_unit() {
    let built = trivial::build(
        10,
        trivial::Overrides {
            consumed_count: Some(2),
            created_count: Some(2),
            // The second created resource first, the consumed ones swapped, the first created
            // resource last.
            resource_order: Some(vec![3, 1, 0, 2]),
            ..trivial::Overrides::default()
        },
    )
    .expect("an ordered trivial action must build");
    let tags: Vec<Digest> = compliance::constrain(&built.witnesses.compliance_witness)
        .expect("the ordered compliance witness must constrain")
        .tags()
        .collect();

    let txn = LocalProver
        .prove(&[built.witnesses])
        .await
        .expect("local prover must constrain an ordered trivial action");

    let action = &txn
        .as_arm()
        .aggregation
        .as_ref()
        .expect("the transaction must carry an aggregation")
        .instance
        .actions[0];
    let aggregated_tags: Vec<Digest> = action
        .resource_instances
        .iter()
        .map(|resource| resource.tag())
        .collect();
    assert_eq!(aggregated_tags, tags);
    assert_eq!(aggregated_tags[0], built.created_ephemerals[1].commitment());
    assert_eq!(aggregated_tags[3], built.created_ephemerals[0].commitment());
    let is_consumed: Vec<bool> = action
        .resource_instances
        .iter()
        .map(|resource| resource.is_consumed())
        .collect();
    assert_eq!(is_consumed, [false, true, true, false]);
    assert_eq!(
        action.action_tree_root,
        ActionTree::new(tags).root().expect("the tags have a root")
    );
}

#[tokio::test]
async fn local_prover_rejects_logic_witnesses_for_another_order_than_the_compliance_unit() {
    let overrides = |resource_order: Vec<usize>| trivial::Overrides {
        consumed_count: Some(2),
        created_count: Some(1),
        resource_order: Some(resource_order),
        ..trivial::Overrides::default()
    };
    let in_order = trivial::build(11, overrides(vec![0, 1, 2])).expect("must build");
    // The created resource moves ahead of the consumed ones.
    let reordered = trivial::build(11, overrides(vec![2, 0, 1])).expect("must build");
    assert_eq!(
        in_order.created_ephemerals, reordered.created_ephemerals,
        "both orders give the created resource the same nonce"
    );

    let witnesses = ActionWitnesses {
        compliance_witness: in_order.witnesses.compliance_witness,
        logic_witnesses: reordered.witnesses.logic_witnesses,
    };
    let err = LocalProver
        .prove(&[witnesses])
        .await
        .err()
        .expect("local prover must reject the logic witnesses of the other order");
    assert!(
        err.to_string().contains("another action tree root"),
        "{err:#}"
    );
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
    let resource_instances = &aggregation.instance.actions[0].resource_instances;
    assert_eq!(resource_instances.len(), 2);
    assert!(
        resource_instances
            .iter()
            .all(|resource| resource.is_consumed())
    );
}

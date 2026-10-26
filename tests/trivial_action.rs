//! Chain-free smoke tests for the action builder with trivial resources and for the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::action::{self, ActionResource};
use anoma_pa_testkit::fixtures::trivial;
use anoma_pa_testkit::prover::LocalProver;
use anoma_pa_testkit::witness::ActionWitnesses;
use anoma_rm_risc0::Digest;
use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::compliance;
use anoma_rm_risc0::resource::ResourceWitness;

fn counts(consumed_count: usize, created_count: usize) -> trivial::Overrides {
    trivial::Overrides {
        consumed_count: Some(consumed_count),
        created_count: Some(created_count),
        ..trivial::Overrides::default()
    }
}

/// Whether each resource of the action is consumed, in the order of the resources.
fn is_consumed(built: &ActionWitnesses) -> Vec<bool> {
    built
        .compliance_witness
        .resource_instances
        .iter()
        .map(|witness| matches!(witness, ResourceWitness::Consumed(_)))
        .collect()
}

#[test]
fn trivial_resources_default_to_one_consumed_and_one_created_resource() {
    let built = action::build(trivial::resources(1, trivial::Overrides::default()))
        .expect("a trivial action must build");
    assert_eq!(is_consumed(&built), [true, false]);
    assert_eq!(built.logic_witnesses.len(), 2);
}

#[test]
fn trivial_resources_take_their_counts_from_the_overrides() {
    let built = action::build(trivial::resources(4, counts(2, 3)))
        .expect("an n:m trivial action must build");
    assert_eq!(is_consumed(&built), [true, true, false, false, false]);
    assert_eq!(built.logic_witnesses.len(), 5);
}

#[test]
fn build_errors_on_created_resources_without_consumed_ones() {
    // Created nonces derive from the consumed nullifiers, as in the compliance circuit, which
    // rejects an empty consumed list.
    let err = action::build(trivial::resources(5, counts(0, 1)))
        .err()
        .expect("a create-only action must fail to build");
    assert!(
        format!("{err:#}").contains("failed to derive the nonce of created resource 0"),
        "{err:#}"
    );
}

#[test]
fn build_allows_consumed_resources_without_created_ones() {
    let built = action::build(trivial::resources(6, counts(2, 0)))
        .expect("a consume-only action must build");
    assert_eq!(is_consumed(&built), [true, true]);
    assert_eq!(built.logic_witnesses.len(), 2);
}

#[test]
fn invalid_consumed_non_ephemeral_makes_the_consumed_resource_persistent() {
    let built = action::build(trivial::resources(
        2,
        trivial::Overrides::invalid_consumed_non_ephemeral(),
    ))
    .expect("action construction should succeed");
    assert!(!built.resources()[0].is_ephemeral);
}

#[test]
fn invalid_nonzero_quantity_gives_the_consumed_resource_a_quantity() {
    let built = action::build(trivial::resources(
        3,
        trivial::Overrides::invalid_nonzero_quantity(),
    ))
    .expect("action construction should succeed");
    assert_ne!(built.resources()[0].quantity, 0);
}

#[tokio::test]
async fn local_prover_mock_aggregates_trivial_actions() {
    let actions = (1..=8)
        .map(|seed| action::build(trivial::resources(seed, trivial::Overrides::default())))
        .collect::<anyhow::Result<Vec<ActionWitnesses>>>()
        .expect("must build trivial action witnesses");
    let txn = LocalProver
        .prove(&actions.iter().collect::<Vec<_>>())
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
    assert_eq!(aggregation.instance.actions.len(), actions.len());
}

#[tokio::test]
async fn local_prover_mock_aggregates_an_n_to_m_action() {
    let built = action::build(trivial::resources(7, counts(2, 3)))
        .expect("an n:m trivial action must build");

    let txn = LocalProver
        .prove(&[&built])
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
    let Ok([consumed_0, consumed_1, created_0, created_1]) =
        <[ActionResource; 4]>::try_from(trivial::resources(10, counts(2, 2)))
    else {
        panic!("two consumed and two created resources");
    };
    // Alternating sides and reversed consumed resources: the order of `resources` would hide bugs.
    let built = action::build([consumed_1, created_0, consumed_0, created_1])
        .expect("an ordered trivial action must build");
    let tags: Vec<Digest> = compliance::constrain(&built.compliance_witness)
        .expect("the ordered compliance witness must constrain")
        .tags()
        .collect();

    let txn = LocalProver
        .prove(&[&built])
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
    let resources = built.resources();
    assert_eq!(aggregated_tags[1], resources[1].commitment());
    assert_eq!(aggregated_tags[3], resources[3].commitment());
    let is_consumed: Vec<bool> = action
        .resource_instances
        .iter()
        .map(|resource| resource.is_consumed())
        .collect();
    assert_eq!(is_consumed, [true, false, true, false]);
    assert_eq!(
        action.action_tree_root,
        ActionTree::new(tags).root().expect("the tags have a root")
    );
}

#[tokio::test]
async fn local_prover_rejects_logic_witnesses_for_another_order_than_the_compliance_unit() {
    let in_order = action::build(trivial::resources(11, counts(2, 1))).expect("must build");
    let Ok([consumed_0, consumed_1, created]) =
        <[ActionResource; 3]>::try_from(trivial::resources(11, counts(2, 1)))
    else {
        panic!("two consumed resources and one created resource");
    };
    // Moving the created resource between the consumed ones changes only the action tree root.
    let reordered = action::build([consumed_0, created, consumed_1]).expect("must build");
    assert_eq!(
        in_order.resources()[2],
        reordered.resources()[1],
        "both orders give the created resource the same nonce"
    );

    let witnesses = ActionWitnesses {
        compliance_witness: in_order.compliance_witness,
        logic_witnesses: reordered.logic_witnesses,
    };
    let err = LocalProver
        .prove(&[&witnesses])
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
    let built = action::build(trivial::resources(9, counts(2, 0)))
        .expect("a consume-only trivial action must build");

    let txn = LocalProver
        .prove(&[&built])
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

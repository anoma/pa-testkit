//! Chain-free smoke tests for the trivial-action fixtures and the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::assert::{Needle, expect_integration_panic};
use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::trivial;
use anoma_pa_testkit::prover::LocalProver;
use anoma_pa_testkit::witness::LogicWitness;
use anoma_rm_risc0::Digest;
use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::compliance::KindTreeEntry;
use anoma_rm_risc0::kind_tree::KindTree;
use anoma_rm_risc0::proving_system::JournalEncoding;
use anoma_rm_risc0::resource::{ResourceWitness, kind};
use anoma_rm_risc0::resource_logic::TrivialLogicWitness;
use k256::elliptic_curve::sec1::ToEncodedPoint;

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
    let txn = LocalProver::new(JournalEncoding::Risc0Serde)
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

/// The batch aggregation circuit commits to the kind tree of the actions that
/// use one, and an action whose resources the tree does not list (it commits
/// to the empty tree's root) joins them. The local prover replicates that
/// circuit, so it must aggregate such a pair under the used tree's root.
#[tokio::test]
async fn local_prover_aggregates_an_action_without_a_kind_tree_with_one_using_it() {
    let mut listed = trivial::build(30, trivial::Overrides::default())
        .expect("must build a trivial action")
        .witnesses;
    let unlisted = trivial::build(31, trivial::Overrides::default())
        .expect("must build a trivial action")
        .witnesses;

    // A tree listing the first action's kind, assigned its own kind base, so
    // the action balances as without the tree.
    let resource = *listed.compliance_witness.resource_witnesses[0].resource();
    let kind_base = kind(&resource)
        .expect("the trivial resource must have a kind")
        .to_affine()
        .to_encoded_point(false)
        .as_bytes()
        .to_vec();
    let tree = KindTree::from_entries(vec![KindTreeEntry {
        logic_ref: resource.logic_ref,
        label_ref: resource.label_ref,
        kind_base,
    }])
    .expect("the tree must build");
    let witness = &mut listed.compliance_witness;
    witness.kind_paths = tree.kind_paths_for(&witness.resource_witnesses);
    assert!(
        witness.kind_paths.iter().all(Option::is_some),
        "the tree must list every resource of the first action"
    );
    assert!(
        unlisted
            .compliance_witness
            .kind_paths
            .iter()
            .all(Option::is_none),
        "no kind tree is loaded, so the second action must use none"
    );

    let txn = LocalProver::new(JournalEncoding::Risc0Serde)
        .prove(&[listed, unlisted])
        .await
        .expect("the local prover must aggregate actions with and without a kind tree");

    let instance = txn
        .aggregation_instance()
        .expect("the transaction must carry an aggregation");
    assert_eq!(
        instance.kind_tree_root,
        tree.root(),
        "the aggregation must commit to the kind tree the first action uses"
    );
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

    let txn = LocalProver::new(JournalEncoding::Risc0Serde)
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

/// An action's resources are one ordered list, and created resources may
/// precede consumed ones. The local prover keeps the compliance unit's order
/// in the aggregation, whatever order the logic witnesses come in.
#[tokio::test]
async fn local_prover_keeps_an_interleaved_resource_order() {
    let mut built = trivial::build(
        12,
        trivial::Overrides {
            created_count: Some(2),
            ..trivial::Overrides::default()
        },
    )
    .expect("a 1:2 trivial action must build")
    .witnesses;

    // Created, consumed, created: a created nonce derives from its index
    // among the created resources, so the reordered witness still holds.
    let witness = &mut built.compliance_witness;
    witness.resource_witnesses.swap(0, 1);
    witness.kind_paths.swap(0, 1);
    let ResourceWitness::Consumed(consumed) = &witness.resource_witnesses[1] else {
        panic!("the second resource must be the consumed one");
    };
    let nf_key = consumed.nf_key.clone();
    let tags: Vec<Digest> = witness
        .resource_witnesses
        .iter()
        .map(|resource| match resource {
            ResourceWitness::Consumed(consumed) => consumed
                .resource
                .nullifier(&consumed.nf_key)
                .expect("the consumed resource must have a nullifier"),
            ResourceWitness::Created(created) => created.commitment(),
        })
        .collect();
    let action_tree_root = ActionTree::new(tags.clone())
        .root()
        .expect("the action tree must have a root");
    // The logic witnesses prove against the reordered action tree, listed in
    // reverse to show they pair with the resources by tag.
    built.logic_witnesses = witness
        .resource_witnesses
        .iter()
        .rev()
        .map(|resource| {
            Box::new(TrivialLogicWitness {
                resource: *resource.resource(),
                action_tree_root,
                is_consumed: matches!(resource, ResourceWitness::Consumed(_)),
                nf_key: nf_key.clone(),
            }) as Box<dyn LogicWitness>
        })
        .collect();

    let txn = LocalProver::new(JournalEncoding::Risc0Serde)
        .prove(&[built])
        .await
        .expect("the local prover must constrain an interleaved action");

    let action = &txn
        .aggregation_instance()
        .expect("the transaction must carry an aggregation")
        .actions[0];
    let aggregated: Vec<(Digest, bool)> = action
        .resource_instances
        .iter()
        .map(|resource| (resource.tag(), resource.is_consumed()))
        .collect();
    assert_eq!(
        aggregated,
        [(tags[0], false), (tags[1], true), (tags[2], false)],
        "the aggregation must keep the compliance unit's resource order"
    );
    assert_eq!(action.action_tree_root, action_tree_root);
    assert_eq!(
        txn.created_commitments()
            .expect("the commitments must be readable")
            .collect::<Vec<_>>(),
        [tags[0], tags[2]],
        "the created commitments come in the list's order"
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

    let txn = LocalProver::new(JournalEncoding::Risc0Serde)
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

/// The local prover refuses an action whose padding resource breaks the
/// trivial logic, as every prover does when it constrains the actions first.
async fn local_prover_refuses(seed: u8, overrides: trivial::Overrides) -> anyhow::Result<()> {
    let bad = trivial::build(seed, overrides)?;
    expect_integration_panic(Needle::Static("Invalid padding resource"))(
        LocalProver::new(JournalEncoding::Risc0Serde)
            .prove(&[bad.witnesses])
            .await,
    )
}

#[tokio::test]
async fn local_prover_refuses_a_nonzero_quantity() -> anyhow::Result<()> {
    local_prover_refuses(7, trivial::Overrides::invalid_nonzero_quantity()).await
}

#[tokio::test]
async fn local_prover_refuses_a_non_ephemeral_consumed_resource() -> anyhow::Result<()> {
    local_prover_refuses(8, trivial::Overrides::invalid_consumed_non_ephemeral()).await
}

#[tokio::test]
async fn the_consumed_resources_name_the_overridden_ephemeral_root() -> anyhow::Result<()> {
    let root = anoma_rm_risc0::Digest::from_bytes([7; 32]);
    let built = trivial::build(
        10,
        trivial::Overrides {
            consumed_count: Some(2),
            ephemeral_root: Some(root),
            ..trivial::Overrides::default()
        },
    )?;
    let txn = LocalProver::new(JournalEncoding::Risc0Serde)
        .prove(&[built.witnesses])
        .await?;
    let resource_instances = &txn.aggregation_instance()?.actions[0].resource_instances;
    assert_eq!(resource_instances.len(), 3);
    for consumed in resource_instances
        .iter()
        .filter(|resource| resource.is_consumed())
    {
        assert_eq!(
            consumed.commitment_tree_root(),
            Some(root),
            "a consumed ephemeral resource must name the overridden root"
        );
    }
    Ok(())
}

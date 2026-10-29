//! Checks that the local prover rejects a transaction in which the consumed and the created
//! resources of a kind hold different quantities.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::action::{self, ActionResource, ResourceToCreate};
use anoma_pa_testkit::fixtures::trivial;
use anoma_pa_testkit::prover::LocalProver;
use anoma_pa_testkit::witness::{AppData, LogicWitness};
use anoma_rm_risc0::Digest;
use anoma_rm_risc0::compliance::KindTreeEntry;
use anoma_rm_risc0::constants::init_kind_tree_from_entries;
use anoma_rm_risc0::logic_instance::LogicInstance;
use anoma_rm_risc0::merkle_path::MerklePath;
use anoma_rm_risc0::nullifier_key::NullifierKey;
use anoma_rm_risc0::resource::{Resource, kind};
use k256::elliptic_curve::sec1::ToEncodedPoint;

const LOGIC_REF: Digest = Digest::new([1; 8]);

/// A logic witness that checks nothing, so that its resource can hold any quantity. The trivial
/// logic rejects a nonzero quantity, so trivial resources cannot cause an imbalance.
struct AcceptAll {
    tag: Digest,
    is_consumed: bool,
    action_tree_root: Digest,
}

impl LogicWitness for AcceptAll {
    fn verifying_key(&self) -> Digest {
        LOGIC_REF
    }

    fn constrain(&self) -> anyhow::Result<LogicInstance> {
        Ok(LogicInstance {
            tag: self.tag,
            is_consumed: self.is_consumed,
            action_tree_root: self.action_tree_root,
            app_data: AppData::default(),
        })
    }

    fn witness_to_vec(&self) -> anyhow::Result<Vec<u32>> {
        anyhow::bail!("the logic of this test has no circuit")
    }

    fn proving_key(&self) -> Vec<u8> {
        Vec::new()
    }
}

fn label_ref(label: u32) -> Digest {
    Digest::new([label; 8])
}

fn resource(label_ref: Digest, quantity: u128, seed: u8) -> Resource {
    Resource {
        logic_ref: LOGIC_REF,
        label_ref,
        quantity,
        value_ref: Digest::default(),
        is_ephemeral: true,
        nonce: [seed; 32],
        nk_commitment: NullifierKey::from_bytes([seed; 32]).commit(),
        rand_seed: [seed; 32],
    }
}

fn consumed(label_ref: Digest, quantity: u128, seed: u8) -> ActionResource {
    let nf_key = NullifierKey::from_bytes([seed; 32]);
    ActionResource::consumed(
        resource(label_ref, quantity, seed),
        nf_key.clone(),
        MerklePath::empty(),
        move |resource, action_tree_root| {
            Ok(AcceptAll {
                tag: resource.nullifier(&nf_key)?,
                is_consumed: true,
                action_tree_root,
            })
        },
    )
}

fn created(label_ref: Digest, quantity: u128, seed: u8) -> ActionResource {
    let resource = ResourceToCreate {
        logic_ref: LOGIC_REF,
        label_ref,
        quantity,
        value_ref: Digest::default(),
        is_ephemeral: true,
        nk_commitment: NullifierKey::from_bytes([seed; 32]).commit(),
        rand_seed: [seed; 32],
    };
    ActionResource::created(resource, |resource, action_tree_root| {
        Ok(AcceptAll {
            tag: resource.commitment(),
            is_consumed: false,
            action_tree_root,
        })
    })
}

#[tokio::test]
async fn local_prover_rejects_a_kind_whose_quantities_do_not_balance() {
    let label_ref = label_ref(1);
    let built = action::build([consumed(label_ref, 10, 1), created(label_ref, 9, 2)])
        .expect("an action with an imbalance must build");

    let err = LocalProver
        .prove(&[&built])
        .await
        .err()
        .expect("the local prover must reject the imbalance");

    assert!(
        format!("{err:#}").contains(&format!(
            "the resources of the kind of logic ref {LOGIC_REF} and label ref {label_ref} do \
             not balance: consumed 10, created 9"
        )),
        "{err:#}"
    );
}

#[tokio::test]
async fn local_prover_balances_one_consumed_resource_against_two_created_ones() {
    let label_ref = label_ref(7);
    let consumed_quantity: u128 = 10;
    let first_created_quantity = 6;
    let built = action::build([
        consumed(label_ref, consumed_quantity, 1),
        created(label_ref, first_created_quantity, 2),
        created(label_ref, consumed_quantity - first_created_quantity, 3),
    ])
    .expect("an action with one consumed and two created resources must build");

    LocalProver
        .prove(&[&built])
        .await
        .expect("the two created resources must balance the consumed one");
}

#[tokio::test]
async fn local_prover_rejects_two_consumed_resources_against_one_smaller_created_one() {
    let label_ref = label_ref(8);
    let consumed_quantities: [u128; 2] = [5, 5];
    let consumed_total: u128 = consumed_quantities.iter().sum();
    let created_quantity = consumed_total - 1;
    let built = action::build([
        consumed(label_ref, consumed_quantities[0], 1),
        consumed(label_ref, consumed_quantities[1], 2),
        created(label_ref, created_quantity, 3),
    ])
    .expect("an action with an imbalance must build");

    let err = LocalProver
        .prove(&[&built])
        .await
        .err()
        .expect("the local prover must reject the imbalance");

    assert!(
        format!("{err:#}").contains(&format!(
            "the resources of the kind of logic ref {LOGIC_REF} and label ref {label_ref} do \
             not balance: consumed {consumed_total}, created {created_quantity}"
        )),
        "{err:#}"
    );
}

#[tokio::test]
async fn local_prover_balances_a_kind_over_the_actions_of_a_transaction() {
    let label_ref = label_ref(2);
    let first = action::build([consumed(label_ref, 10, 1), created(label_ref, 6, 2)])
        .expect("the first action must build");
    let second = action::build([consumed(label_ref, 0, 3), created(label_ref, 4, 4)])
        .expect("the second action must build");

    LocalProver
        .prove(&[&first, &second])
        .await
        .expect("the actions together must balance");
}

#[tokio::test]
async fn local_prover_does_not_balance_one_kind_against_another() {
    let (consumed_label_ref, created_label_ref) = (label_ref(3), label_ref(4));
    let built = action::build([
        consumed(consumed_label_ref, 10, 1),
        created(created_label_ref, 10, 2),
    ])
    .expect("an action with two kinds must build");

    let err = LocalProver
        .prove(&[&built])
        .await
        .err()
        .expect("the local prover must reject the imbalance of each kind");

    assert!(
        format!("{err:#}").contains(&format!(
            "label ref {consumed_label_ref} do not balance: consumed 10, created 0"
        )),
        "{err:#}"
    );
}

#[tokio::test]
async fn local_prover_balances_the_aliases_of_a_kind() {
    // The kind tree is global to the process. No other test of this file uses these label refs.
    let (consumed_label_ref, created_label_ref) = (label_ref(5), label_ref(6));
    let kind_base = kind(&resource(consumed_label_ref, 0, 0))
        .expect("the kind of the consumed resource must be a point")
        .to_encoded_point(false)
        .as_bytes()
        .to_vec();
    init_kind_tree_from_entries(
        [consumed_label_ref, created_label_ref]
            .map(|label_ref| KindTreeEntry {
                logic_ref: LOGIC_REF,
                label_ref,
                kind_base: kind_base.clone(),
            })
            .to_vec(),
    )
    .expect("the kind tree must load");

    let built = action::build([
        consumed(consumed_label_ref, 10, 1),
        created(created_label_ref, 10, 2),
    ])
    .expect("an action with two aliases must build");

    LocalProver
        .prove(&[&built])
        .await
        .expect("the aliases of a kind must balance");
}

#[tokio::test]
async fn local_prover_reports_an_invalid_logic_before_an_imbalance() {
    let built = action::build([
        trivial::consumed(1, trivial::Overrides::invalid_nonzero_quantity()),
        trivial::created(),
    ])
    .expect("the action must build");

    let err = LocalProver
        .prove(&[&built])
        .await
        .err()
        .expect("the local prover must reject the trivial resource with a quantity");

    assert!(
        format!("{err:#}").contains("Invalid padding resource"),
        "{err:#}"
    );
}

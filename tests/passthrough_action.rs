//! Chain-free tests for the pass-through fixture and the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::passthrough::{self, PASSTHROUGH_LOGIC_VK};
use anoma_pa_testkit::prover::LocalProver;
use anoma_pa_testkit::witness::{AppData, ExpirableBlob};

/// An external call in some chain's encoding: the fixture carries the bytes
/// as given.
fn call(bytes: &[u32]) -> AppData {
    AppData {
        external_payload: vec![ExpirableBlob {
            blob: bytes.to_vec(),
            deletion_criterion: 0,
        }],
        ..AppData::default()
    }
}

#[test]
fn both_resources_carry_the_pass_through_logic() {
    let built = passthrough::build(1, call(&[7]), passthrough::Overrides::default())
        .expect("a pass-through action must build");
    assert_eq!(built.consumed_ephemeral.logic_ref, PASSTHROUGH_LOGIC_VK);
    assert_eq!(built.created_ephemeral.logic_ref, PASSTHROUGH_LOGIC_VK);
    assert_eq!(built.witnesses.logic_witnesses.len(), 2);
}

#[tokio::test]
async fn the_proven_action_carries_each_resources_app_data_under_its_tag() {
    let consumed = call(&[1, 2, 3]);
    let created = call(&[4, 5]);
    let built = passthrough::build(
        2,
        consumed.clone(),
        passthrough::Overrides {
            created_app_data: Some(created.clone()),
        },
    )
    .expect("a pass-through action must build");
    let nullifier = built
        .consumed_ephemeral
        .nullifier(&anoma_rm_risc0::nullifier_key::NullifierKey::from_bytes(
            [2; 32],
        ))
        .expect("the consumed resource has a nullifier");
    let commitment = built.created_ephemeral.commitment();

    let tx = LocalProver
        .prove(&[built.witnesses])
        .await
        .expect("the local prover must constrain a pass-through action");
    let instance = &tx
        .as_arm()
        .aggregation
        .as_ref()
        .expect("the transaction must carry an aggregation")
        .instance;
    let action = &instance.actions[0];
    assert_eq!(action.consumed_publics.len(), 1);
    assert_eq!(action.created_publics.len(), 1);
    assert_eq!(action.consumed_publics[0].resource_nullifier, nullifier);
    assert_eq!(
        action.consumed_publics[0].resource_logic_ref,
        PASSTHROUGH_LOGIC_VK
    );
    assert_eq!(action.consumed_publics[0].app_data, consumed);
    assert_eq!(action.created_publics[0].resource_commitment, commitment);
    assert_eq!(
        action.created_publics[0].resource_logic_ref,
        PASSTHROUGH_LOGIC_VK
    );
    assert_eq!(action.created_publics[0].app_data, created);
}

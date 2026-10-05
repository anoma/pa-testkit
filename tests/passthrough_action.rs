//! Chain-free tests for the pass-through fixture and the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::passthrough::{self, PASSTHROUGH_LOGIC_VK};
use anoma_pa_testkit::prover::LocalProver;
use anoma_pa_testkit::witness::{AppData, ExpirableBlob};

#[tokio::test]
async fn the_proven_action_carries_the_external_calls_under_the_consumed_tag() {
    let calls = vec![vec![1, 2, 3], vec![4, 5]];
    let built = passthrough::build(2, calls.clone(), passthrough::Overrides::default())
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
    assert_eq!(
        action.consumed_publics[0].app_data,
        AppData {
            external_payload: calls
                .into_iter()
                .map(|blob| ExpirableBlob {
                    blob,
                    deletion_criterion: 0,
                })
                .collect(),
            ..AppData::default()
        }
    );
    assert_eq!(action.created_publics[0].resource_commitment, commitment);
    assert_eq!(
        action.created_publics[0].resource_logic_ref,
        PASSTHROUGH_LOGIC_VK
    );
    assert_eq!(action.created_publics[0].app_data, AppData::default());
}

//! Chain-free tests for the pass-through fixture and the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::passthrough::{self, PASSTHROUGH_LOGIC_VK};
use anoma_pa_testkit::prover::LocalProver;
use anoma_pa_testkit::witness::{AppData, ExpirableBlob};
use anoma_rm_risc0::proving_system::JournalEncoding;

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

    let tx = LocalProver::new(JournalEncoding::Risc0Serde)
        .prove(&[built.witnesses])
        .await
        .expect("the local prover must constrain a pass-through action");
    let instance = &tx
        .as_arm()
        .aggregation
        .as_ref()
        .expect("the transaction must carry an aggregation")
        .instance;
    let [consumed, created] = &instance.actions[0].resource_instances[..] else {
        panic!("the action must hold one consumed and one created resource");
    };
    assert!(consumed.is_consumed(), "the consumed resource comes first");
    assert_eq!(consumed.tag(), nullifier);
    assert_eq!(consumed.logic_ref(), PASSTHROUGH_LOGIC_VK);
    assert_eq!(
        consumed.app_data(),
        &AppData {
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
    assert!(!created.is_consumed(), "the created resource comes second");
    assert_eq!(created.tag(), commitment);
    assert_eq!(created.logic_ref(), PASSTHROUGH_LOGIC_VK);
    assert_eq!(created.app_data(), &AppData::default());
}

#[tokio::test]
async fn the_proven_action_carries_the_application_payload_under_the_consumed_tag() {
    let payload = vec![vec![7, 8, 9], vec![10]];
    let built = passthrough::build(
        3,
        vec![],
        passthrough::Overrides {
            application_payload: payload.clone(),
            ..passthrough::Overrides::default()
        },
    )
    .expect("a pass-through action must build");

    let tx = LocalProver::new(JournalEncoding::Risc0Serde)
        .prove(&[built.witnesses])
        .await
        .expect("the local prover must constrain a pass-through action");
    let [consumed, created] = &tx
        .as_arm()
        .aggregation
        .as_ref()
        .expect("the transaction must carry an aggregation")
        .instance
        .actions[0]
        .resource_instances[..]
    else {
        panic!("the action must hold one consumed and one created resource");
    };
    assert!(consumed.is_consumed() && !created.is_consumed());
    assert_eq!(
        consumed.app_data(),
        &AppData {
            application_payload: payload
                .into_iter()
                .map(|blob| ExpirableBlob {
                    blob,
                    deletion_criterion: 0,
                })
                .collect(),
            ..AppData::default()
        }
    );
    assert_eq!(created.app_data(), &AppData::default());
}

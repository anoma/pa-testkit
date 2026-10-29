//! Chain-free tests for the ordered fixture and the local prover.
//!
//! Gated on `local` + `fixtures`; run with `cargo test --features local,fixtures`.
#![cfg(all(feature = "local", feature = "fixtures"))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::ordered::{self, Logic, ResourceSpec};
use anoma_pa_testkit::fixtures::passthrough::PASSTHROUGH_LOGIC_VK;
use anoma_pa_testkit::prover::LocalProver;
use anoma_pa_testkit::witness::{AppData, ExpirableBlob};
use anoma_rm_risc0::compliance;
use anoma_rm_risc0::constants::PADDING_LOGIC_VK;
use anoma_rm_risc0::nullifier_key::NullifierKey;
use anoma_rm_risc0::proving_system::JournalEncoding;
use risc0_zkvm::Digest;

fn payload(word: u32) -> AppData {
    AppData {
        application_payload: vec![ExpirableBlob {
            blob: vec![word],
            deletion_criterion: 0,
        }],
        ..AppData::default()
    }
}

#[tokio::test]
async fn an_interleaved_action_keeps_its_resources_in_the_listed_order() {
    let specs = vec![
        ResourceSpec {
            consumed: false,
            logic: Logic::Passthrough(payload(1)),
        },
        ResourceSpec {
            consumed: true,
            logic: Logic::Passthrough(payload(2)),
        },
        ResourceSpec {
            consumed: false,
            logic: Logic::Trivial,
        },
    ];
    let built = ordered::build(5, specs, ordered::Overrides::default())
        .expect("an ordered action must build");
    let nf_key = NullifierKey::from_bytes([5; 32]);
    let [created, consumed, trivial] = &built.ephemerals[..] else {
        panic!("the action must hold the three listed resources");
    };
    let tags: Vec<Digest> = vec![
        created.commitment(),
        consumed
            .nullifier(&nf_key)
            .expect("the consumed resource has a nullifier"),
        trivial.commitment(),
    ];

    let compliance_instance = compliance::constrain(&built.witnesses.compliance_witness)
        .expect("the compliance witness must constrain");
    let compliance_order: Vec<(bool, Digest)> = compliance_instance
        .resource_instances
        .iter()
        .map(|resource| (resource.is_consumed(), resource.tag()))
        .collect();
    assert_eq!(
        compliance_order,
        vec![(false, tags[0]), (true, tags[1]), (false, tags[2])],
        "the compliance instance must list created, consumed, created, as built"
    );

    let tx = LocalProver::new(JournalEncoding::Risc0Serde)
        .prove(&[built.witnesses])
        .await
        .expect("the local prover must constrain an ordered action");
    let resources = &tx
        .as_arm()
        .aggregation
        .as_ref()
        .expect("the transaction must carry an aggregation")
        .instance
        .actions[0]
        .resource_instances;
    let aggregated: Vec<(bool, Digest, Digest, &AppData)> = resources
        .iter()
        .map(|r| (r.is_consumed(), r.tag(), r.logic_ref(), r.app_data()))
        .collect();
    assert_eq!(
        aggregated,
        vec![
            (false, tags[0], PASSTHROUGH_LOGIC_VK, &payload(1)),
            (true, tags[1], PASSTHROUGH_LOGIC_VK, &payload(2)),
            (false, tags[2], PADDING_LOGIC_VK, &AppData::default()),
        ],
        "the aggregation instance must keep the listed order, each resource with its logic and \
         app data"
    );
}

#[test]
fn the_consumed_resources_name_the_overridden_root() {
    let root = Digest::from([7; 8]);
    let built = ordered::build(
        6,
        vec![
            ResourceSpec {
                consumed: true,
                logic: Logic::Trivial,
            },
            ResourceSpec {
                consumed: true,
                logic: Logic::Passthrough(AppData::default()),
            },
        ],
        ordered::Overrides {
            ephemeral_root: Some(root),
        },
    )
    .expect("an ordered action must build");
    let instance = compliance::constrain(&built.witnesses.compliance_witness)
        .expect("the compliance witness must constrain");
    let roots: Vec<Option<Digest>> = instance
        .resource_instances
        .iter()
        .map(|resource| resource.commitment_tree_root())
        .collect();
    assert_eq!(roots, vec![Some(root), Some(root)]);
}

use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::compliance;
use anoma_rm_risc0::logic_instance::{AppData, ExpirableBlob, LogicInstance};
use anoma_rm_risc0::nullifier_key::NullifierKey;
use anoma_rm_risc0::resource::{ConsumedResourceWitness, Resource};
use anyhow::Context;
use risc0_zkvm::Digest;

use super::logic::{PASSTHROUGH_LOGIC_VK, PassthroughLogicWitness};
use crate::witness::{ActionWitnesses, LogicWitness};

/// The derived data of a built pass-through action: the action witnesses
/// plus the ephemeral resources it consumes and creates.
pub struct ActionData {
    pub witnesses: ActionWitnesses,
    pub consumed_ephemeral: Resource,
    pub created_ephemeral: Resource,
}

/// Optional deviations from the default pass-through action: none yet.
#[derive(Clone, Debug, Default)]
pub struct Overrides {}

/// Build a pass-through action whose consumed resource makes `external_calls`,
/// each a blob in the encoding the chain's adapter reads. Both resources are
/// ephemeral with zero quantity, so the action balances; `seed` keeps their
/// nonces apart from every other action's.
pub fn build(
    seed: u8,
    external_calls: Vec<Vec<u32>>,
    Overrides {}: Overrides,
) -> anyhow::Result<ActionData> {
    let nf_key = NullifierKey::from_bytes([seed; 32]);
    let nk_commitment = nf_key.commit();

    let consumed = Resource {
        logic_ref: PASSTHROUGH_LOGIC_VK,
        label_ref: Digest::default(),
        quantity: 0,
        value_ref: Digest::default(),
        is_ephemeral: true,
        nonce: [seed; 32],
        nk_commitment,
        rand_seed: [seed.wrapping_add(11); 32],
    };
    let nullifier = consumed
        .nullifier(&nf_key)
        .context("failed to compute the consumed nullifier")?;
    // The created nonce derives from the consumed nullifier; the compliance
    // circuit rejects any other.
    let created = Resource {
        nonce: Resource::derive_nonce_from_nullifiers(0, &[nullifier])
            .context("failed to derive the created nonce")?,
        rand_seed: [seed.wrapping_add(33); 32],
        ..consumed
    };
    let commitment = created.commitment();

    let compliance_witness = compliance::from_resources(
        vec![ConsumedResourceWitness::from_resource(consumed, nf_key)],
        vec![created],
        // The loaded kind table, or the empty one if none is loaded.
        anoma_rm_risc0::constants::kind_table().to_vec(),
    );

    let root = ActionTree::new(vec![nullifier, commitment])
        .root()
        .context("failed to compute the action tree root")?;
    let logic_witnesses: Vec<Box<dyn LogicWitness>> = vec![
        Box::new(PassthroughLogicWitness(LogicInstance {
            tag: nullifier,
            is_consumed: true,
            root,
            app_data: AppData {
                external_payload: external_calls
                    .into_iter()
                    .map(|blob| ExpirableBlob {
                        blob,
                        deletion_criterion: 0,
                    })
                    .collect(),
                ..AppData::default()
            },
        })),
        Box::new(PassthroughLogicWitness(LogicInstance {
            tag: commitment,
            is_consumed: false,
            root,
            app_data: AppData::default(),
        })),
    ];

    Ok(ActionData {
        witnesses: ActionWitnesses {
            compliance_witness: Box::new(compliance_witness),
            logic_witnesses,
        },
        consumed_ephemeral: consumed,
        created_ephemeral: created,
    })
}

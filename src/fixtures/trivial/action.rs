use anoma_rm_risc0::action_tree::MerkleTree as ArmTree;
use anoma_rm_risc0::compliance::ComplianceWitness;
use anoma_rm_risc0::nullifier_key::NullifierKeyExt as _;
use anoma_rm_risc0::resource::Resource;
use anoma_rm_risc0::resource_logic::TrivialLogicWitness;
use anyhow::Context;

use super::resource;
use super::resource::Overrides;
use crate::witness::ActionWitnesses;

/// The derived data of a built trivial action: the action witnesses plus the
/// ephemeral resources it consumes and creates.
pub struct ActionData {
    pub witnesses: ActionWitnesses,
    pub consumed_ephemeral: Resource,
    pub created_ephemeral: Resource,
}

/// Build a trivial action that consumes and creates an ephemeral resource.
pub fn build(seed: u8, overrides: Overrides) -> anyhow::Result<ActionData> {
    let nf_key = resource::nullifier_key(seed);
    let nk_commitment = nf_key.commit();

    let consumed_ephemeral = resource::consumed(seed, nk_commitment, &overrides);
    let consumed_nullifier = consumed_ephemeral
        .nullifier(&nf_key)
        .context("failed to compute consumed nullifier")?;
    let created_ephemeral = resource::created(seed, nk_commitment, consumed_nullifier, &overrides)?;

    let compliance_witness = ComplianceWitness::from_resources(
        consumed_ephemeral,
        anoma_rm_risc0::compliance::initial_root(),
        nf_key.clone(),
        created_ephemeral,
    );

    let action_tree_root = ArmTree::new(vec![consumed_nullifier, created_ephemeral.commitment()])
        .root()
        .context("failed to compute action tree root")?;

    let consumed_logic_witness = TrivialLogicWitness {
        resource: consumed_ephemeral,
        action_tree_root,
        is_consumed: true,
        nf_key: nf_key.clone(),
    };
    let created_logic_witness = TrivialLogicWitness {
        resource: created_ephemeral,
        action_tree_root,
        is_consumed: false,
        nf_key,
    };

    let witnesses = ActionWitnesses {
        compliance_witnesses: vec![Box::new(compliance_witness)],
        logic_witnesses: vec![
            Box::new(consumed_logic_witness),
            Box::new(created_logic_witness),
        ],
    };

    Ok(ActionData {
        witnesses,
        consumed_ephemeral,
        created_ephemeral,
    })
}

/// Build `count` independent trivial actions with consecutive seeds — a batch
/// convenience for tests that only need the action witnesses.
pub fn build_many(count: usize, seed_start: u8) -> anyhow::Result<Vec<ActionWitnesses>> {
    (0..count)
        .map(|idx| {
            let seed = seed_start.wrapping_add(idx as u8);
            Ok(build(seed, Overrides::default())?.witnesses)
        })
        .collect()
}

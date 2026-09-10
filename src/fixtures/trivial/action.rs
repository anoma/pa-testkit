use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::compliance::ComplianceWitness;
use anoma_rm_risc0::resource::{ConsumedResourceWitness, Resource};
use anoma_rm_risc0::resource_logic::TrivialLogicWitness;
use anyhow::Context;
use risc0_zkvm::Digest;

use super::resource;
use super::resource::Overrides;
use crate::witness::{ActionWitnesses, LogicWitness};

/// The derived data of a built trivial action: the action witnesses plus the
/// ephemeral resources it consumes and creates.
pub struct ActionData {
    pub witnesses: ActionWitnesses,
    pub consumed_ephemerals: Vec<Resource>,
    pub created_ephemerals: Vec<Resource>,
}

/// Build a trivial action that consumes and creates ephemeral resources under
/// the trivial/padding resource logic. The counts default to one resource per
/// side (`Overrides::{consumed_count, created_count}`); a created count of
/// zero yields a consume-only action, while a consumed count of zero fails —
/// created nonces derive from the consumed nullifiers, so an action must
/// consume at least one resource.
pub fn build(seed: u8, overrides: Overrides) -> anyhow::Result<ActionData> {
    let consumed_count = overrides.consumed_count.unwrap_or(1);
    let created_count = overrides.created_count.unwrap_or(1);

    let nf_key = resource::nullifier_key(seed);
    let nk_commitment = nf_key.commit();

    let consumed_ephemerals: Vec<Resource> = (0..consumed_count)
        .map(|index| resource::consumed(seed, index, nk_commitment, &overrides))
        .collect();

    let consumed_nullifiers: Vec<Digest> = consumed_ephemerals
        .iter()
        .map(|consumed| consumed.nullifier(&nf_key))
        .collect::<Result<_, _>>()
        .context("failed to compute consumed nullifiers")?;

    let created_ephemerals: Vec<Resource> = (0..created_count)
        .map(|index| {
            // Created nonces derive from the consumed nullifiers; the
            // compliance circuit rejects any other nonce.
            let derived_nonce =
                Resource::derive_nonce_from_nullifiers(index as u32, &consumed_nullifiers)
                    .context("failed to derive the created resource nonce")?;
            Ok(resource::created(
                seed,
                derived_nonce,
                nk_commitment,
                &overrides,
            ))
        })
        .collect::<anyhow::Result<_>>()?;

    let compliance_witness = ComplianceWitness::from_resources(
        consumed_ephemerals
            .iter()
            .map(|consumed| ConsumedResourceWitness::from_resource(*consumed, nf_key.clone()))
            .collect::<Vec<_>>(),
        created_ephemerals.clone(),
        // The trivial fixtures prove with the empty kind table: every kind
        // falls back to hash-to-curve.
        Vec::new(),
    );

    let tags: Vec<Digest> = consumed_nullifiers
        .iter()
        .copied()
        .chain(
            created_ephemerals
                .iter()
                .map(|created| created.commitment()),
        )
        .collect();
    let action_tree_root = ActionTree::new(tags)
        .root()
        .context("failed to compute action tree root")?;

    let logic_witnesses: Vec<Box<dyn LogicWitness>> = consumed_ephemerals
        .iter()
        .map(|consumed| {
            Box::new(TrivialLogicWitness {
                resource: *consumed,
                action_tree_root,
                is_consumed: true,
                nf_key: nf_key.clone(),
            }) as Box<dyn LogicWitness>
        })
        .chain(created_ephemerals.iter().map(|created| {
            Box::new(TrivialLogicWitness {
                resource: *created,
                action_tree_root,
                is_consumed: false,
                nf_key: nf_key.clone(),
            }) as Box<dyn LogicWitness>
        }))
        .collect();

    let witnesses = ActionWitnesses {
        compliance_witness: Box::new(compliance_witness),
        logic_witnesses,
    };

    Ok(ActionData {
        witnesses,
        consumed_ephemerals,
        created_ephemerals,
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

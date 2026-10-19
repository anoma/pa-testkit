use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::compliance;
use anoma_rm_risc0::kind_table::KindTable;
use anoma_rm_risc0::resource::{ConsumedResourceWitness, Resource, ResourceWitness};
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
    let resource_count = consumed_count + created_count;

    let order = overrides
        .resource_order
        .clone()
        .unwrap_or_else(|| (0..resource_count).collect());
    let mut positions = order.clone();
    positions.sort_unstable();
    anyhow::ensure!(
        positions.into_iter().eq(0..resource_count),
        "the resource order {order:?} is not an order of the {resource_count} resources"
    );

    let nf_key = resource::nullifier_key(seed);
    let nk_commitment = nf_key.commit();

    let consumed_ephemerals: Vec<Resource> = (0..consumed_count)
        .map(|index| resource::consumed(seed, index, nk_commitment, &overrides))
        .collect();

    let consumed_nullifiers: Vec<Digest> = order
        .iter()
        .filter_map(|&position| consumed_ephemerals.get(position))
        .map(|consumed| consumed.nullifier(&nf_key))
        .collect::<Result<_, _>>()
        .context("failed to compute consumed nullifiers")?;

    // Created nonces derive from the index among the created resources and from the consumed
    // nullifiers, both in the order of the resources; the compliance circuit rejects any other
    // nonce.
    let mut created_nonces = vec![[0u8; 32]; created_count];
    for (index, created_position) in order
        .iter()
        .filter_map(|&position| position.checked_sub(consumed_count))
        .enumerate()
    {
        created_nonces[created_position] =
            Resource::derive_nonce_from_nullifiers(index as u32, &consumed_nullifiers)
                .context("failed to derive the created resource nonce")?;
    }
    let created_ephemerals: Vec<Resource> = created_nonces
        .into_iter()
        .map(|derived_nonce| resource::created(seed, derived_nonce, nk_commitment, &overrides))
        .collect();

    let ordered: Vec<(Resource, bool)> = order
        .iter()
        .map(|&position| match consumed_ephemerals.get(position) {
            Some(consumed) => (*consumed, true),
            None => (created_ephemerals[position - consumed_count], false),
        })
        .collect();

    let compliance_witness = compliance::from_resources(
        ordered
            .iter()
            .map(|&(resource, is_consumed)| {
                if is_consumed {
                    ResourceWitness::Consumed(ConsumedResourceWitness::from_resource(
                        resource,
                        nf_key.clone(),
                    ))
                } else {
                    ResourceWitness::Created(resource)
                }
            })
            .collect(),
        // The loaded kind table, or the empty one if none is loaded.
        anoma_rm_risc0::constants::global_kind_table().unwrap_or(&KindTable::empty()),
    );

    let tags: Vec<Digest> = ordered
        .iter()
        .map(|(resource, is_consumed)| resource.tag(*is_consumed, &nf_key))
        .collect::<Result<_, _>>()
        .context("failed to compute the resource tags")?;
    let action_tree_root = ActionTree::new(tags)
        .root()
        .context("failed to compute action tree root")?;

    let logic_witnesses: Vec<Box<dyn LogicWitness>> = ordered
        .into_iter()
        .map(|(resource, is_consumed)| {
            Box::new(TrivialLogicWitness {
                resource,
                action_tree_root,
                is_consumed,
                nf_key: nf_key.clone(),
            }) as Box<dyn LogicWitness>
        })
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

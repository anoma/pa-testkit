use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::compliance;
use anoma_rm_risc0::constants::PADDING_LOGIC_VK;
use anoma_rm_risc0::logic_instance::LogicInstance;
use anoma_rm_risc0::nullifier_key::NullifierKey;
use anoma_rm_risc0::resource::{Resource, ResourceWitness, ResourceWitnessConsumed};
use anoma_rm_risc0::resource_logic::TrivialLogicWitness;
use anyhow::Context;
use risc0_zkvm::Digest;

use crate::fixtures::passthrough::PASSTHROUGH_LOGIC_VK;
use crate::fixtures::passthrough::logic::PassthroughLogicWitness;
use crate::witness::{ActionWitnesses, AppData, LogicWitness};

/// The logic of one resource of an ordered action.
#[derive(Clone, Debug)]
pub enum Logic {
    /// The pass-through logic, committing this app data.
    Passthrough(AppData),
    /// The trivial logic, which commits no app data.
    Trivial,
}

/// One resource of an ordered action: whether it is consumed, and its logic.
#[derive(Clone, Debug)]
pub struct ResourceSpec {
    pub consumed: bool,
    pub logic: Logic,
}

/// The derived data of a built ordered action: the action witnesses plus its
/// ephemeral resources, in the order they were listed.
pub struct ActionData {
    pub witnesses: ActionWitnesses,
    pub ephemerals: Vec<Resource>,
}

/// Optional deviations from the default ordered action.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    /// The commitment tree root the consumed resources name, by default the
    /// empty tree's (`INITIAL_ROOT`), which every adapter holds.
    pub ephemeral_root: Option<Digest>,
}

/// Build the action whose resources are `resources`, in that order. They are
/// ephemeral with zero quantity, so the action balances; `seed` keeps their
/// nonces apart from every other action's.
pub fn build(
    seed: u8,
    resources: Vec<ResourceSpec>,
    overrides: Overrides,
) -> anyhow::Result<ActionData> {
    let nf_key = NullifierKey::from_bytes([seed; 32]);
    let resource = |logic: &Logic, nonce: [u8; 32], rand_seed: u8| Resource {
        logic_ref: match logic {
            Logic::Passthrough(_) => PASSTHROUGH_LOGIC_VK,
            Logic::Trivial => PADDING_LOGIC_VK,
        },
        label_ref: Digest::default(),
        quantity: 0,
        value_ref: Digest::default(),
        is_ephemeral: true,
        nonce,
        nk_commitment: nf_key.commit(),
        rand_seed: [rand_seed; 32],
    };

    // The consumed resources first, in their order: the created nonces derive
    // from all of their nullifiers.
    let consumed: Vec<(Resource, Digest)> = resources
        .iter()
        .filter(|spec| spec.consumed)
        .enumerate()
        .map(|(index, spec)| {
            let consumed = resource(
                &spec.logic,
                crate::fixtures::consumed_nonce(seed, index as u8),
                seed.wrapping_add(11),
            );
            let nullifier = consumed
                .nullifier(&nf_key)
                .context("failed to compute a consumed nullifier")?;
            Ok((consumed, nullifier))
        })
        .collect::<anyhow::Result<_>>()?;
    let nullifiers: Vec<Digest> = consumed.iter().map(|(_, nullifier)| *nullifier).collect();

    let mut consumed = consumed.into_iter();
    let mut created_index = 0;
    let mut resource_witnesses = Vec::with_capacity(resources.len());
    let mut tagged = Vec::with_capacity(resources.len());
    for spec in resources {
        if spec.consumed {
            let (resource, nullifier) =
                consumed.next().context("the consumed resources ran out")?;
            resource_witnesses.push(ResourceWitness::Consumed(
                ResourceWitnessConsumed::from_resource(resource, nf_key.clone()),
            ));
            tagged.push((resource, nullifier, spec));
        } else {
            // A created nonce derives from its index among the created
            // resources and the action's nullifiers; the compliance circuit
            // rejects any other.
            let nonce = Resource::derive_nonce_from_nullifiers(created_index, &nullifiers)
                .context("failed to derive a created nonce")?;
            created_index += 1;
            let created = resource(&spec.logic, nonce, seed.wrapping_add(33));
            resource_witnesses.push(ResourceWitness::Created(created));
            tagged.push((created, created.commitment(), spec));
        }
    }

    let compliance_witness = compliance::from_resources_with_ephemeral_root(
        resource_witnesses,
        overrides
            .ephemeral_root
            .unwrap_or(anoma_rm_risc0::compliance::INITIAL_ROOT),
        crate::fixtures::kind_tree(),
    );
    let action_tree_root = ActionTree::new(tagged.iter().map(|(_, tag, _)| *tag).collect())
        .root()
        .context("failed to compute the action tree root")?;

    let ephemerals = tagged.iter().map(|(resource, _, _)| *resource).collect();
    let logic_witnesses = tagged
        .into_iter()
        .map(|(resource, tag, spec)| -> Box<dyn LogicWitness> {
            match spec.logic {
                Logic::Passthrough(app_data) => Box::new(PassthroughLogicWitness(LogicInstance {
                    tag,
                    is_consumed: spec.consumed,
                    action_tree_root,
                    app_data,
                })),
                Logic::Trivial => Box::new(TrivialLogicWitness {
                    resource,
                    action_tree_root,
                    is_consumed: spec.consumed,
                    nf_key: nf_key.clone(),
                }),
            }
        })
        .collect();

    Ok(ActionData {
        witnesses: ActionWitnesses {
            compliance_witness: Box::new(compliance_witness),
            logic_witnesses,
        },
        ephemerals,
    })
}

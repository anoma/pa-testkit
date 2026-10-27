//! One action from the resources of any kinds, in the order of a list. The protocol adapter
//! processes the resources of an action in this order.

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::compliance;
use anoma_rm_risc0::constants::global_kind_tree;
use anoma_rm_risc0::kind_tree::KindTree;
use anoma_rm_risc0::merkle_path::MerklePath;
use anoma_rm_risc0::nullifier_key::{NullifierKey, NullifierKeyCommitment};
use anoma_rm_risc0::resource::{Resource, ResourceWitness, ResourceWitnessConsumed};
use anyhow::Context;

use crate::witness::{ActionWitnesses, LogicWitness};

/// Builds the logic witness of a resource from the resource and the root of its action tree.
type Logic = Box<dyn FnOnce(Resource, Digest) -> anyhow::Result<Box<dyn LogicWitness>>>;

/// A resource to create: the fields of [`Resource`] without the nonce, which [`build`] derives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceToCreate {
    pub logic_ref: Digest,
    pub label_ref: Digest,
    pub quantity: u128,
    pub value_ref: Digest,
    pub is_ephemeral: bool,
    pub nk_commitment: NullifierKeyCommitment,
    pub rand_seed: [u8; 32],
}

impl ResourceToCreate {
    fn with_nonce(self, nonce: [u8; 32]) -> Resource {
        let Self {
            logic_ref,
            label_ref,
            quantity,
            value_ref,
            is_ephemeral,
            nk_commitment,
            rand_seed,
        } = self;
        Resource {
            logic_ref,
            label_ref,
            quantity,
            value_ref,
            is_ephemeral,
            nonce,
            nk_commitment,
            rand_seed,
        }
    }
}

/// One resource of an action, consumed or created, and the function that builds its logic
/// witness.
pub struct ActionResource {
    input: ResourceWitnessInput,
    logic: Logic,
}

/// What [`build`] turns into arm's [`ResourceWitness`].
enum ResourceWitnessInput {
    Consumed(ResourceWitnessConsumed),
    Created(ResourceToCreate),
}

impl ActionResource {
    /// A consumed resource, the key that nullifies it, and the path of its commitment in the
    /// commitment tree. An ephemeral resource ignores the path.
    pub fn consumed<W>(
        resource: Resource,
        nf_key: NullifierKey,
        merkle_path: MerklePath,
        logic: impl FnOnce(Resource, Digest) -> anyhow::Result<W> + 'static,
    ) -> Self
    where
        W: LogicWitness + 'static,
    {
        Self {
            input: ResourceWitnessInput::Consumed(
                ResourceWitnessConsumed::from_resource_with_path(resource, nf_key, merkle_path),
            ),
            logic: boxed(logic),
        }
    }

    /// A created resource. [`build`] derives its nonce.
    pub fn created<W>(
        resource: ResourceToCreate,
        logic: impl FnOnce(Resource, Digest) -> anyhow::Result<W> + 'static,
    ) -> Self
    where
        W: LogicWitness + 'static,
    {
        Self {
            input: ResourceWitnessInput::Created(resource),
            logic: boxed(logic),
        }
    }
}

/// Builds the witnesses of one action from `resources`, in their order. The nonce of a created
/// resource derives from its index among the created resources and from the consumed
/// nullifiers, both in this order. The kind paths come from the loaded kind tree, or from the
/// empty tree if none is loaded.
pub fn build(
    resources: impl IntoIterator<Item = ActionResource>,
) -> anyhow::Result<ActionWitnesses> {
    let (inputs, logics): (Vec<ResourceWitnessInput>, Vec<Logic>) = resources
        .into_iter()
        .map(|resource| (resource.input, resource.logic))
        .unzip();

    let nullifiers = inputs
        .iter()
        .filter_map(|input| match input {
            ResourceWitnessInput::Consumed(witness) => {
                Some(witness.resource.nullifier(&witness.nf_key))
            }
            ResourceWitnessInput::Created(_) => None,
        })
        .collect::<Result<Vec<Digest>, _>>()
        .context("failed to compute the consumed nullifiers")?;

    let mut consumed_index = 0;
    let mut created_index = 0;
    let mut resource_witnesses = Vec::with_capacity(inputs.len());
    let mut tags = Vec::with_capacity(inputs.len());
    for input in inputs {
        match input {
            ResourceWitnessInput::Consumed(witness) => {
                tags.push(nullifiers[consumed_index]);
                consumed_index += 1;
                resource_witnesses.push(ResourceWitness::Consumed(witness));
            }
            ResourceWitnessInput::Created(resource) => {
                let nonce = Resource::derive_nonce_from_nullifiers(created_index, &nullifiers)
                    .with_context(|| {
                        format!("failed to derive the nonce of created resource {created_index}")
                    })?;
                created_index += 1;
                let resource = resource.with_nonce(nonce);
                tags.push(resource.commitment());
                resource_witnesses.push(ResourceWitness::Created(resource));
            }
        }
    }
    let action_tree_root = ActionTree::new(tags)
        .root()
        .context("failed to compute the action tree root")?;

    let logic_witnesses = logics
        .into_iter()
        .zip(&resource_witnesses)
        .enumerate()
        .map(|(position, (logic, witness))| {
            logic(*witness.resource(), action_tree_root).with_context(|| {
                format!("failed to build the logic witness of resource {position}")
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(ActionWitnesses {
        compliance_witness: compliance::from_resources(
            resource_witnesses,
            global_kind_tree().unwrap_or(&KindTree::empty()),
        ),
        logic_witnesses,
    })
}

fn boxed<W>(logic: impl FnOnce(Resource, Digest) -> anyhow::Result<W> + 'static) -> Logic
where
    W: LogicWitness + 'static,
{
    Box::new(move |resource, action_tree_root| {
        Ok(Box::new(logic(resource, action_tree_root)?) as Box<dyn LogicWitness>)
    })
}

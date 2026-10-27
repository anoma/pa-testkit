//! The trivial resource: an ephemeral resource with zero quantity under the trivial (padding)
//! resource logic. The logic reads only its own resource. It rejects the resource if the
//! quantity is not zero or the resource is persistent, so a trivial resource cannot cause an
//! imbalance. [`consumed`] and [`created`] each give one action resource for
//! [`crate::fixtures::action::build`]. The named `invalid_*` constructors of [`Overrides`] give
//! variants of the consumed resource for negative tests.

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::constants::PADDING_LOGIC_VK;
use anoma_rm_risc0::logic_proof::PaddingResourceLogic;
use anoma_rm_risc0::merkle_path::MerklePath;
use anoma_rm_risc0::nullifier_key::NullifierKey;
use anoma_rm_risc0::resource::Resource;

use crate::fixtures::action::{ActionResource, ResourceToCreate};

/// Overrides for a consumed trivial resource: invalid variants for negative tests, and a nonce
/// other than the default.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub quantity: Option<u128>,
    pub is_ephemeral: Option<bool>,
    pub nonce: Option<[u8; 32]>,
}

impl Overrides {
    pub fn invalid_nonzero_quantity() -> Self {
        Self {
            quantity: Some(1),
            ..Self::default()
        }
    }

    pub fn invalid_non_ephemeral() -> Self {
        Self {
            is_ephemeral: Some(false),
            ..Self::default()
        }
    }
}

/// A consumed trivial resource. `seed` sets its nullifier key, its nonce and its random seed, so
/// two consumed resources of one transaction need different seeds.
pub fn consumed(seed: u8, overrides: Overrides) -> ActionResource {
    let nf_key = nullifier_key(seed);
    let resource = Resource {
        logic_ref: PADDING_LOGIC_VK,
        label_ref: Digest::default(),
        quantity: overrides.quantity.unwrap_or(0),
        value_ref: Digest::default(),
        is_ephemeral: overrides.is_ephemeral.unwrap_or(true),
        nonce: overrides.nonce.unwrap_or([seed; 32]),
        nk_commitment: nf_key.commit(),
        rand_seed: [seed.wrapping_add(11); 32],
    };
    ActionResource::consumed(
        resource,
        nf_key.clone(),
        MerklePath::empty(),
        logic(nf_key, true),
    )
}

/// A created trivial resource. `seed` sets its nullifier key commitment and its random seed; the
/// action builder derives its nonce.
pub fn created(seed: u8) -> ActionResource {
    let nf_key = nullifier_key(seed);
    let resource = ResourceToCreate {
        logic_ref: PADDING_LOGIC_VK,
        label_ref: Digest::default(),
        quantity: 0,
        value_ref: Digest::default(),
        is_ephemeral: true,
        nk_commitment: nf_key.commit(),
        rand_seed: [seed.wrapping_add(33); 32],
    };
    ActionResource::created(resource, logic(nf_key, false))
}

fn nullifier_key(seed: u8) -> NullifierKey {
    NullifierKey::from_bytes([seed; 32])
}

fn logic(
    nf_key: NullifierKey,
    is_consumed: bool,
) -> impl FnOnce(Resource, Digest) -> anyhow::Result<PaddingResourceLogic> {
    move |resource, action_tree_root| {
        Ok(PaddingResourceLogic::new(
            resource,
            action_tree_root,
            nf_key,
            is_consumed,
        ))
    }
}

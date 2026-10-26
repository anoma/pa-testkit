use anoma_rm_risc0::Digest;
use anoma_rm_risc0::logic_proof::PaddingResourceLogic;
use anoma_rm_risc0::merkle_path::MerklePath;
use anoma_rm_risc0::nullifier_key::NullifierKey;
use anoma_rm_risc0::resource::Resource;

use super::resource::{self, Overrides};
use crate::fixtures::action::ActionResource;

/// The resources of a trivial action: the consumed ones, then the created ones. Each side has one
/// resource, unless `overrides` sets its count. A created resource needs a consumed one, because
/// its nonce derives from the consumed nullifiers.
pub fn resources(seed: u8, overrides: Overrides) -> Vec<ActionResource> {
    let nf_key = resource::nullifier_key(seed);
    let nk_commitment = nf_key.commit();

    let consumed = (0..overrides.consumed_count.unwrap_or(1)).map(|index| {
        ActionResource::consumed(
            resource::consumed(seed, index, nk_commitment, &overrides),
            nf_key.clone(),
            MerklePath::empty(),
            logic(nf_key.clone(), true),
        )
    });
    let created = (0..overrides.created_count.unwrap_or(1)).map(|_| {
        ActionResource::created(
            resource::created(seed, nk_commitment, &overrides),
            logic(nf_key.clone(), false),
        )
    });
    consumed.chain(created).collect()
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

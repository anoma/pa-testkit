//! Checks that a transaction balances: over all its actions, the consumed and the created
//! resources of each kind hold the same quantity, as the delta proof requires.

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::resource::{self, ResourceWitness};
use anyhow::Context;
use k256::elliptic_curve::sec1::ToEncodedPoint;

use crate::witness::ActionWitnesses;

/// The resources of one kind in a transaction.
struct Kind {
    /// The uncompressed SEC1 encoding of the kind base, which identifies the kind.
    kind_base: Vec<u8>,
    /// The logic ref and the label ref of the first resource of the kind.
    logic_ref: Digest,
    label_ref: Digest,
    consumed: u128,
    created: u128,
}

/// Fails on the first kind whose consumed and created quantities differ. A resource takes its
/// kind base from its kind path, so the aliases of a kind balance against each other, as in
/// the compliance circuit.
pub(super) fn check(actions: &[&ActionWitnesses]) -> anyhow::Result<()> {
    let mut kinds: Vec<Kind> = Vec::new();
    for (action_index, witnesses) in actions.iter().enumerate() {
        let witness = &witnesses.compliance_witness;
        for (position, (resource_witness, kind_path)) in witness
            .resource_instances
            .iter()
            .zip(&witness.kind_paths)
            .enumerate()
        {
            let resource = resource_witness.resource();
            let kind_base = match kind_path {
                Some(kind_path) => kind_path.kind_base.clone(),
                None => resource::kind(resource)
                    .with_context(|| {
                        format!(
                            "failed to compute the kind of resource {position} of action \
                             {action_index}"
                        )
                    })?
                    .to_encoded_point(false)
                    .as_bytes()
                    .to_vec(),
            };
            let index = match kinds.iter().position(|kind| kind.kind_base == kind_base) {
                Some(index) => index,
                None => {
                    kinds.push(Kind {
                        kind_base,
                        logic_ref: resource.logic_ref,
                        label_ref: resource.label_ref,
                        consumed: 0,
                        created: 0,
                    });
                    kinds.len() - 1
                }
            };
            let kind = &mut kinds[index];
            let sum = match resource_witness {
                ResourceWitness::Consumed(_) => &mut kind.consumed,
                ResourceWitness::Created(_) => &mut kind.created,
            };
            *sum = sum.checked_add(resource.quantity).with_context(|| {
                format!(
                    "the quantities of the kind of resource {position} of action {action_index} \
                     exceed u128"
                )
            })?;
        }
    }

    for kind in kinds {
        anyhow::ensure!(
            kind.consumed == kind.created,
            "the resources of the kind of logic ref {} and label ref {} do not balance: consumed \
             {}, created {}",
            kind.logic_ref,
            kind.label_ref,
            kind.consumed,
            kind.created,
        );
    }
    Ok(())
}

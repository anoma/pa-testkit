//! Shared front-half of proving: constrain an action's witnesses into validated
//! instances. Both provers run this first — the local prover mints a mock
//! aggregation seal over the resulting instances, the queue prover assembles
//! the transaction while submitting the witnesses for real proving.

use std::collections::HashMap;

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::compliance::ComplianceInstance;
use anoma_rm_risc0::logic_instance::LogicInstance;
use anyhow::Context;

use crate::witness::{ActionWitnesses, LogicWitness};

/// A logic witness constrained to its instance and verifying key.
pub(super) struct ConstrainedLogic {
    /// The constrained logic instance (carries tag, is_consumed, and app_data).
    pub instance: LogicInstance,
    /// The logic's verifying key.
    pub verifying_key: Digest,
    /// Position of the originating witness in
    /// [`ActionWitnesses::logic_witnesses`], correlating canonical-order
    /// logics with their per-witness proving jobs.
    #[cfg_attr(not(feature = "e2e"), allow(dead_code))]
    pub witness_index: usize,
}

/// An action's witnesses, constrained and validated against the canonical
/// tag-correlation rules.
pub(super) struct ConstrainedAction {
    /// The action's compliance instance.
    pub compliance_instance: ComplianceInstance,
    /// Logic inputs for the consumed resources, in `consumed_publics` order.
    pub consumed_logics: Vec<ConstrainedLogic>,
    /// Logic inputs for the created resources, in `created_publics` order.
    pub created_logics: Vec<ConstrainedLogic>,
}

/// Constrains every witness of an action and checks that each of the compliance
/// unit's consumed and created resources has a matching logic witness — by tag,
/// role, and verifying key — mirroring [`anoma_rm_risc0::action::Action`]. The
/// returned logics are reordered into the canonical tag order (consumed
/// nullifiers, then created commitments).
pub(super) fn action(
    witnesses: &ActionWitnesses,
    action_idx: usize,
) -> anyhow::Result<ConstrainedAction> {
    let compliance_instance = witnesses.compliance_witness.constrain().with_context(|| {
        format!("failed to constrain the compliance unit of action {action_idx}")
    })?;

    let resource_count =
        compliance_instance.consumed_publics.len() + compliance_instance.created_publics.len();
    anyhow::ensure!(
        witnesses.logic_witnesses.len() == resource_count,
        "action {action_idx} must have exactly one logic witness per resource: \
         logic={} resources={resource_count}",
        witnesses.logic_witnesses.len(),
    );

    let mut logics = witnesses
        .logic_witnesses
        .iter()
        .enumerate()
        .map(|(logic_idx, logic_witness)| {
            let instance = logic_witness.constrain().with_context(|| {
                format!("failed to constrain logic witness {logic_idx} of action {action_idx}")
            })?;
            Ok(Some(ConstrainedLogic {
                verifying_key: logic_witness.verifying_key(),
                instance,
                witness_index: logic_idx,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut logic_index_by_tag = HashMap::with_capacity(logics.len());
    for (logic_idx, logic) in logics.iter().enumerate() {
        let tag = logic.as_ref().expect("not yet taken").instance.tag;
        anyhow::ensure!(
            logic_index_by_tag.insert(tag, logic_idx).is_none(),
            "action {action_idx} has two logic witnesses sharing tag {tag}",
        );
    }

    let consumed_logics = compliance_instance
        .consumed_publics
        .iter()
        .map(|consumed| {
            take_validated_logic(
                &mut logics,
                &logic_index_by_tag,
                consumed.resource_nullifier,
                true,
                consumed.resource_logic_ref,
                action_idx,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let created_logics = compliance_instance
        .created_publics
        .iter()
        .map(|created| {
            take_validated_logic(
                &mut logics,
                &logic_index_by_tag,
                created.resource_commitment,
                false,
                created.resource_logic_ref,
                action_idx,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(ConstrainedAction {
        compliance_instance,
        consumed_logics,
        created_logics,
    })
}

/// Takes the logic witness matching the compliance unit's resource (identified
/// by `tag`) after checking its consumed/created role and verifying key.
fn take_validated_logic(
    logics: &mut [Option<ConstrainedLogic>],
    logic_index_by_tag: &HashMap<Digest, usize>,
    tag: Digest,
    expected_consumed: bool,
    expected_logic_ref: Digest,
    action_idx: usize,
) -> anyhow::Result<ConstrainedLogic> {
    let role = if expected_consumed {
        "consumed"
    } else {
        "created"
    };
    let logic = logic_index_by_tag
        .get(&tag)
        .and_then(|&logic_idx| logics[logic_idx].take())
        .with_context(|| {
            format!("action {action_idx} has no {role} logic witness for tag {tag}")
        })?;

    anyhow::ensure!(
        logic.instance.is_consumed == expected_consumed,
        "action {action_idx} {role} logic witness for tag {tag} has the wrong is_consumed flag"
    );
    anyhow::ensure!(
        logic.verifying_key == expected_logic_ref,
        "action {action_idx} {role} logic verifying key mismatch for tag {tag}"
    );
    Ok(logic)
}

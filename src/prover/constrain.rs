//! Shared front-half of proving: constrain an action's witnesses into validated
//! instances. Both provers run this first — the local prover mints mock seals
//! from the resulting instances, the queue prover assembles the transaction
//! while submitting the witnesses for real proving.

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
}

/// An action's witnesses, constrained and validated against the canonical
/// tag-correlation rules.
pub(super) struct ConstrainedAction {
    /// Compliance instances in compliance-unit order.
    pub compliance_instances: Vec<ComplianceInstance>,
    /// Logic inputs in the order their witnesses were supplied.
    pub logics: Vec<ConstrainedLogic>,
}

/// Constrains every witness of an action and checks that each compliance unit's
/// consumed and created resources have a matching logic witness — by tag, role,
/// and verifying key — mirroring [`anoma_rm_risc0::action::Action`].
pub(super) fn action(
    witnesses: &ActionWitnesses,
    action_idx: usize,
) -> anyhow::Result<ConstrainedAction> {
    anyhow::ensure!(
        !witnesses.compliance_witnesses.is_empty(),
        "action {action_idx} has no compliance units"
    );
    anyhow::ensure!(
        witnesses.logic_witnesses.len() == witnesses.compliance_witnesses.len() * 2,
        "action {action_idx} must have exactly two logic witnesses per compliance unit: \
         logic={} compliance={}",
        witnesses.logic_witnesses.len(),
        witnesses.compliance_witnesses.len()
    );

    let logics = witnesses
        .logic_witnesses
        .iter()
        .enumerate()
        .map(|(logic_idx, logic_witness)| {
            let instance = logic_witness.constrain().with_context(|| {
                format!("failed to constrain logic witness {logic_idx} of action {action_idx}")
            })?;
            Ok(ConstrainedLogic {
                verifying_key: logic_witness.verifying_key(),
                instance,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut logic_index_by_tag = HashMap::with_capacity(logics.len());
    for (logic_idx, logic) in logics.iter().enumerate() {
        anyhow::ensure!(
            logic_index_by_tag
                .insert(logic.instance.tag, logic_idx)
                .is_none(),
            "action {action_idx} has two logic witnesses sharing tag {:?}",
            logic.instance.tag
        );
    }

    let compliance_instances = witnesses
        .compliance_witnesses
        .iter()
        .enumerate()
        .map(|(unit_idx, compliance_witness)| {
            let instance = compliance_witness.constrain().with_context(|| {
                format!("failed to constrain compliance unit {unit_idx} of action {action_idx}")
            })?;

            validate_unit_logic(
                &logics,
                &logic_index_by_tag,
                instance.consumed_nullifier,
                true,
                instance.consumed_logic_ref,
                action_idx,
                unit_idx,
            )?;
            validate_unit_logic(
                &logics,
                &logic_index_by_tag,
                instance.created_commitment,
                false,
                instance.created_logic_ref,
                action_idx,
                unit_idx,
            )?;

            Ok(instance)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(ConstrainedAction {
        compliance_instances,
        logics,
    })
}

/// Checks that the compliance unit's resource (identified by `tag`) has a logic
/// witness with the expected consumed/created role and verifying key.
fn validate_unit_logic(
    logics: &[ConstrainedLogic],
    logic_index_by_tag: &HashMap<Digest, usize>,
    tag: Digest,
    expected_consumed: bool,
    expected_logic_ref: Digest,
    action_idx: usize,
    unit_idx: usize,
) -> anyhow::Result<()> {
    let role = if expected_consumed {
        "consumed"
    } else {
        "created"
    };
    let logic = logic_index_by_tag
        .get(&tag)
        .map(|&logic_idx| &logics[logic_idx])
        .with_context(|| {
            format!("action {action_idx} unit {unit_idx} has no {role} logic witness for its tag")
        })?;

    anyhow::ensure!(
        logic.instance.is_consumed == expected_consumed,
        "action {action_idx} unit {unit_idx} {role} logic witness has the wrong is_consumed flag"
    );
    anyhow::ensure!(
        logic.verifying_key == expected_logic_ref,
        "action {action_idx} unit {unit_idx} {role} logic verifying key mismatch"
    );
    Ok(())
}

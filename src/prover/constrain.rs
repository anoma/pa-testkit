//! Shared front-half of proving: constrain an action's witnesses into validated
//! instances. Both provers run this first — the local prover mints a mock
//! aggregation seal over the resulting instances, the queue prover assembles
//! the transaction while submitting the witnesses for real proving.

use anoma_rm_risc0::compliance::{self, ComplianceInstance};
use anoma_rm_risc0::logic_instance::LogicInstance;
use anyhow::Context;

use crate::witness::{ActionWitnesses, LogicWitness};

/// An action's witnesses, constrained and validated.
pub(super) struct ConstrainedAction {
    /// The action's compliance instance.
    pub compliance_instance: ComplianceInstance,
    /// One logic instance per resource, in the order of the resources.
    pub logic_instances: Vec<LogicInstance>,
}

/// Constrains every witness of an action and checks, as [`anoma_rm_risc0::action::Action`]
/// does, that logic witness `i` belongs to resource `i`: the same tag, the same side, and the
/// verifying key that the resource names as its logic.
pub(super) fn action(
    witnesses: &ActionWitnesses,
    action_index: usize,
) -> anyhow::Result<ConstrainedAction> {
    let compliance_instance =
        compliance::constrain(&witnesses.compliance_witness).with_context(|| {
            format!("failed to constrain the compliance unit of action {action_index}")
        })?;

    let resource_count = compliance_instance.resource_instances.len();
    anyhow::ensure!(
        witnesses.logic_witnesses.len() == resource_count,
        "action {action_index} has {} logic witnesses for {resource_count} resources",
        witnesses.logic_witnesses.len(),
    );

    let logic_instances = compliance_instance
        .resource_instances
        .iter()
        .zip(&witnesses.logic_witnesses)
        .enumerate()
        .map(|(position, (resource, logic_witness))| {
            let instance = logic_witness.constrain().with_context(|| {
                format!("failed to constrain logic witness {position} of action {action_index}")
            })?;
            anyhow::ensure!(
                instance.tag == resource.tag(),
                "logic witness {position} of action {action_index} has tag {}, but resource \
                 {position} has tag {}",
                instance.tag,
                resource.tag(),
            );
            anyhow::ensure!(
                instance.is_consumed == resource.is_consumed(),
                "logic witness {position} of action {action_index} has the wrong is_consumed flag"
            );
            let verifying_key = logic_witness.verifying_key();
            anyhow::ensure!(
                verifying_key == resource.logic_ref(),
                "logic witness {position} of action {action_index} has verifying key \
                 {verifying_key}, but resource {position} has logic ref {}",
                resource.logic_ref(),
            );
            Ok(instance)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(ConstrainedAction {
        compliance_instance,
        logic_instances,
    })
}

//! Shared back-half of real proving: assembles constrained actions and their
//! base proofs into the transaction to aggregate, and checks the aggregated
//! result. The queue prover and the risc0 prover both run it around their own
//! base proving and aggregation.

use std::collections::HashMap;

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::action::Action;
use anoma_rm_risc0::compliance_unit::{self, ComplianceUnit};
use anoma_rm_risc0::delta_proof;
use anoma_rm_risc0::logic_proof::LogicVerifierInput;
use anoma_rm_risc0::transaction::{self, Delta, Transaction as ArmTxn};
use anyhow::Context;

use super::JOURNAL_ENCODING;
use super::constrain::ConstrainedAction;
use crate::transaction::Transaction;
use crate::witness::ActionWitnesses;

/// Identifies one base proof within an action: either the logic proof of the
/// witness at a given index in the action's logic list, or the compliance
/// proof of the action's single compliance unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum BaseProofSlot {
    Logic(usize),
    Compliance,
}

/// Identifies one base proof of a transaction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct BaseProofKey {
    pub action_idx: usize,
    pub slot: BaseProofSlot,
}

/// A succinct base proof: the serialized inner receipt and the journal it
/// commits to.
pub(super) struct BaseProof {
    pub receipt: Vec<u8>,
    pub instance: Vec<u8>,
}

/// The transaction to aggregate: every action with its base proofs and the
/// delta proof, plus the kind table commitment its aggregation is checked
/// against.
pub(super) struct Assembled {
    pub transaction: ArmTxn,
    pub kind_table_commitment: Digest,
}

/// Assembles each constrained action with its base proofs into an
/// [`Action`], its logic verifier inputs in the canonical tag order (consumed
/// nullifiers, then created commitments), and proves the transaction's delta
/// from the actions' rcvs. Every base proof must be used exactly once.
pub(super) fn assemble(
    action_witnesses: &[ActionWitnesses],
    constrained: Vec<ConstrainedAction>,
    mut base_proofs: HashMap<BaseProofKey, BaseProof>,
) -> anyhow::Result<Assembled> {
    let mut actions = Vec::with_capacity(constrained.len());
    for (action_idx, action) in constrained.into_iter().enumerate() {
        let compliance_proof =
            take_base_proof(&mut base_proofs, action_idx, BaseProofSlot::Compliance)?;
        let compliance_unit = ComplianceUnit {
            proof: compliance_proof.receipt,
            instance: compliance_proof.instance,
        };

        // `constrain` returns the logics in the canonical tag order, and
        // `witness_index` correlates each with its proof.
        let logic_count = action.consumed_logics.len() + action.created_logics.len();
        let mut logic_verifier_inputs = Vec::with_capacity(logic_count);
        for logic in action
            .consumed_logics
            .into_iter()
            .chain(action.created_logics)
        {
            let proof = take_base_proof(
                &mut base_proofs,
                action_idx,
                BaseProofSlot::Logic(logic.witness_index),
            )?;
            logic_verifier_inputs.push(LogicVerifierInput {
                tag: logic.instance.tag,
                verifying_key: logic.verifying_key,
                app_data: logic.instance.app_data,
                proof: proof.receipt,
            });
        }

        actions.push(Action {
            compliance_unit,
            logic_verifier_inputs,
        });
    }

    anyhow::ensure!(
        base_proofs.is_empty(),
        "unused base proofs remaining after assembly: {}",
        base_proofs.len()
    );

    let rcvs: Vec<Vec<u8>> = action_witnesses
        .iter()
        .map(|witnesses| witnesses.compliance_witness.rcv.clone())
        .collect();
    let delta = Delta::Witness(
        delta_proof::from_bytes_vec(&rcvs)
            .context("failed to construct delta witness from rcv values")?,
    );
    // `verify` takes the commitment the transaction is checked against. No global kind table is installed here,
    // so it comes from the compliance instances, which makes the check compare the aggregation against them.
    let kind_table_commitment = compliance_unit::get_instance(
        &actions
            .first()
            .context("the transaction carries no action")?
            .compliance_unit,
    )
    .map_err(|error| anyhow::anyhow!("failed to read the compliance instance: {error:?}"))?
    .kind_table_commitment;

    let transaction = transaction::generate_delta_proof(ArmTxn::create(actions, delta))
        .context("failed to generate delta proof")?;

    Ok(Assembled {
        transaction,
        kind_table_commitment,
    })
}

/// Verifies the aggregated transaction against the kind table commitment of
/// its [`Assembled`] form, in the provers' journal encoding.
pub(super) fn verify_aggregated(
    aggregated: ArmTxn,
    kind_table_commitment: Digest,
) -> anyhow::Result<Transaction> {
    transaction::verify(&aggregated, kind_table_commitment, JOURNAL_ENCODING)
        .context("aggregated transaction failed local verification")?;

    Ok(Transaction::from_arm(aggregated))
}

fn take_base_proof(
    base_proofs: &mut HashMap<BaseProofKey, BaseProof>,
    action_idx: usize,
    slot: BaseProofSlot,
) -> anyhow::Result<BaseProof> {
    base_proofs
        .remove(&BaseProofKey { action_idx, slot })
        .with_context(|| format!("missing base proof for action {action_idx} {slot:?}"))
}

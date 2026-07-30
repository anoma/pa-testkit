//! Local prover: constrains circuits natively and emits mock Groth16 seals.

use std::panic::AssertUnwindSafe;

use anoma_rm_risc0::action::Action;
use anoma_rm_risc0::compliance::ComplianceInstance;
use anoma_rm_risc0::compliance_unit::ComplianceUnit;
use anoma_rm_risc0::delta_proof::DeltaWitness;
use anoma_rm_risc0::logic_instance::LogicInstance;
use anoma_rm_risc0::logic_proof::LogicVerifierInputs;
use anoma_rm_risc0::transaction::{Delta, Transaction as ArmTxn};
use anyhow::Context;
use risc0_zkvm::sha::Digestible;
use risc0_zkvm::{Digest, Groth16Receipt, InnerReceipt, MaybePruned, ReceiptClaim};
use sha2::{Digest as _, Sha256};

use super::constrain;
use crate::environment::Prover;
use crate::transaction::Transaction;
use crate::witness::ActionWitnesses;

/// Prover for the local environment: runs resource logic and compliance via
/// `constrain` and emits mock Groth16 seals. No real proving — fast and offline.
#[derive(Default)]
pub struct LocalProver;

impl Prover for LocalProver {
    type Transaction = Transaction;

    async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<Self::Transaction> {
        // NOTE: this may not actually be unwind safe, but we don't care, because
        // we will hardly ever run into unwind safety issues during these tests.
        std::panic::catch_unwind(AssertUnwindSafe(|| constrain_txn(actions))).unwrap_or_else(
            |cause| {
                if let Some(panic_msg) = cause.downcast_ref::<String>() {
                    anyhow::bail!("proving failed: {panic_msg}");
                }
                if let Some(panic_msg) = cause.downcast_ref::<&'static str>() {
                    anyhow::bail!("proving failed: {panic_msg}");
                }
                std::panic::resume_unwind(cause)
            },
        )
    }
}

fn encode_seal(verifying_key: Digest, journal: Digest) -> Vec<u8> {
    // risc0's RiscZeroMockVerifier accepts a seal of the form
    // `SELECTOR ++ claim_digest`, where the claim is the canonical "ok"
    // `ReceiptClaim` for this image id and journal digest. The bindings'
    // `encode_seal` prepends the selector taken from `verifier_parameters[..4]`,
    // so here we set the seal body to the claim digest and the verifier params to
    // the mock selector (`0xFFFFFFFF`).
    let claim_digest =
        ReceiptClaim::ok(verifying_key, MaybePruned::<Vec<u8>>::Pruned(journal)).digest();

    bincode::serialize(&InnerReceipt::Groth16(Groth16Receipt::new(
        claim_digest.as_bytes().to_vec(),
        MaybePruned::Pruned(Digest::default()),
        Digest::new([u32::MAX; 8]),
    )))
    .unwrap()
}

#[inline]
fn logic_instance_to_journal(instance: &LogicInstance) -> anyhow::Result<Digest> {
    let words = risc0_zkvm::serde::to_vec(instance)
        .context("failed to convert logic instance to risc0-zkvm words")?;

    Ok(journal_digest_from_words(&words))
}

#[inline]
fn compliance_instance_to_journal(instance: &ComplianceInstance) -> anyhow::Result<Digest> {
    let words = risc0_zkvm::serde::to_vec(instance)
        .context("failed to convert compliance instance to risc0-zkvm words")?;

    Ok(journal_digest_from_words(&words))
}

#[inline]
fn journal_digest_from_words(words: &[u32]) -> Digest {
    let raw: [u8; 32] = Sha256::digest(anoma_rm_risc0::utils::words_to_bytes(words)).into();

    raw.into()
}

fn constrain_txn(action_witnesses: &[ActionWitnesses]) -> anyhow::Result<Transaction> {
    let mut actions = Vec::with_capacity(action_witnesses.len());
    let mut rcvs = Vec::new();

    for (action_idx, witnesses) in action_witnesses.iter().enumerate() {
        let constrained = constrain::action(witnesses, action_idx)?;

        let compliance_units = constrained
            .compliance_instances
            .iter()
            .map(|instance| {
                Ok(ComplianceUnit {
                    proof: Some(encode_seal(
                        *anoma_rm_risc0::constants::COMPLIANCE_VK,
                        compliance_instance_to_journal(instance)?,
                    )),
                    instance: anoma_rm_risc0::utils::words_to_bytes(
                        &risc0_zkvm::serde::to_vec(instance)
                            .context("failed to serialize compliance instance words")?,
                    )
                    .to_vec(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let logic_verifier_inputs = constrained
            .logics
            .into_iter()
            .map(|logic| {
                Ok(LogicVerifierInputs {
                    tag: logic.instance.tag,
                    verifying_key: logic.verifying_key,
                    proof: Some(encode_seal(
                        logic.verifying_key,
                        logic_instance_to_journal(&logic.instance)?,
                    )),
                    app_data: logic.instance.app_data,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        for compliance_witness in &witnesses.compliance_witnesses {
            rcvs.push(compliance_witness.rcv.clone());
        }

        actions.push(Action {
            compliance_units,
            logic_verifier_inputs,
        });
    }

    let delta = Delta::Witness(
        DeltaWitness::from_bytes_vec(&rcvs)
            .context("failed to construct delta witness from rcv values")?,
    );
    let arm_txn = ArmTxn::create(actions, delta)
        .generate_delta_proof()
        .context("failed to generate delta proof")?;

    Ok(Transaction::from_arm(arm_txn))
}

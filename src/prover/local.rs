//! Local prover: constrains circuits natively and emits a mock Groth16
//! aggregation seal.

use std::panic::AssertUnwindSafe;

use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::aggregation_instance::{
    ActionAggregated, AggregationInstance, ConsumedResourceAggregated, CreatedResourceAggregated,
};
use anoma_rm_risc0::delta_proof::DeltaWitness;
use anoma_rm_risc0::transaction::{Aggregation, Delta, Transaction as ArmTxn};
use anyhow::Context;
use risc0_zkvm::sha::Digestible;
use risc0_zkvm::{Digest, Groth16Receipt, InnerReceipt, MaybePruned, ReceiptClaim};
use sha2::{Digest as _, Sha256};

use super::constrain;
use crate::environment::Prover;
use crate::transaction::Transaction;
use crate::witness::ActionWitnesses;

/// Prover for the local environment: runs resource logic and compliance via
/// `constrain` and replicates the aggregation guest host-side, emitting one
/// mock Groth16 seal over the aggregation instance — mirroring real
/// aggregation, the base proofs are erased. No real proving — fast and offline.
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

fn journal_digest(journal: &[u8]) -> Digest {
    let raw: [u8; 32] = Sha256::digest(journal).into();

    raw.into()
}

/// Constrains every action and builds the aggregation instance the way the
/// batch aggregation guest does: the action tree roots are recomputed from the
/// compliance tags, the per-resource app data is merged in canonical tag order,
/// and all actions must share one kind table commitment.
fn constrain_txn(action_witnesses: &[ActionWitnesses]) -> anyhow::Result<Transaction> {
    let mut actions = Vec::with_capacity(action_witnesses.len());
    let mut rcvs = Vec::new();
    let mut kind_table_commitment: Option<Digest> = None;

    for (action_idx, witnesses) in action_witnesses.iter().enumerate() {
        let constrained = constrain::action(witnesses, action_idx)?;
        let instance = &constrained.compliance_instance;

        let shared = *kind_table_commitment.get_or_insert(instance.kind_table_commitment);
        anyhow::ensure!(
            instance.kind_table_commitment == shared,
            "action {action_idx} commits to a different kind table than the transaction"
        );

        let tags: Vec<Digest> = instance.tags().collect();
        let action_tree_root = ActionTree::new(tags).root().with_context(|| {
            format!("failed to compute the action tree root of action {action_idx}")
        })?;

        let consumed_publics = instance
            .consumed_publics
            .iter()
            .zip(constrained.consumed_logics)
            .map(|(consumed, logic)| ConsumedResourceAggregated {
                resource_nullifier: consumed.resource_nullifier,
                resource_logic_ref: consumed.resource_logic_ref,
                commitment_tree_root: consumed.commitment_tree_root,
                app_data: logic.instance.app_data,
            })
            .collect();

        let created_publics = instance
            .created_publics
            .iter()
            .zip(constrained.created_logics)
            .map(|(created, logic)| CreatedResourceAggregated {
                resource_commitment: created.resource_commitment,
                resource_logic_ref: created.resource_logic_ref,
                app_data: logic.instance.app_data,
            })
            .collect();

        actions.push(ActionAggregated {
            consumed_publics,
            created_publics,
            delta_x: instance.delta_x,
            delta_y: instance.delta_y,
            action_tree_root,
        });

        rcvs.push(witnesses.compliance_witness.rcv.clone());
    }

    let instance = AggregationInstance {
        compliance_key: *anoma_rm_risc0::constants::COMPLIANCE_VK,
        kind_table_commitment: kind_table_commitment
            .context("cannot aggregate a transaction without actions")?,
        actions,
    };

    let journal_words = risc0_zkvm::serde::to_vec(&instance)
        .context("failed to serialize the aggregation instance")?;
    let journal = anoma_rm_risc0::utils::words_to_bytes(&journal_words);

    let proof = encode_seal(
        *anoma_rm_risc0::constants::BATCH_AGGREGATION_VK,
        journal_digest(journal),
    );

    let arm_txn = ArmTxn {
        actions: None,
        delta_proof: Delta::Witness(
            DeltaWitness::from_bytes_vec(&rcvs)
                .context("failed to construct delta witness from rcv values")?,
        ),
        expected_balance: None,
        aggregation: Some(Aggregation { proof, instance }),
    }
    .generate_delta_proof()
    .context("failed to generate delta proof")?;

    Ok(Transaction::from_arm(arm_txn))
}

//! Risc0 prover: makes every base proof and the aggregation in-process with
//! risc0, one proof at a time, and returns the aggregated ARM transaction.

use std::collections::HashMap;

use anoma_rm_risc0::compliance::ComplianceWitness;
use anoma_rm_risc0::compliance_unit;
use anoma_rm_risc0::error::ArmError;
use anoma_rm_risc0::proving_system::{self, JournalEncoding, ProofType};
use anoma_rm_risc0::transaction;
use anyhow::Context;
use serde::ser::{Serialize, SerializeTuple, Serializer};

use super::assemble::{self, BaseProof, BaseProofKey, BaseProofSlot};
use super::constrain;
use crate::environment::Prover;
use crate::transaction::Transaction;
use crate::witness::{ActionWitnesses, LogicWitness};

/// Prover that makes real proofs on this machine, for an e2e environment
/// without the remote proving queue. It does what the queue prover does with
/// risc0 in-process: it constrains the actions, makes a succinct proof of each
/// compliance and logic witness, aggregates them into one Groth16 proof, and
/// verifies the result.
///
/// Proving is CPU- and memory-heavy, so the proofs are made one at a time,
/// each on tokio's blocking thread pool: it needs a tokio runtime and takes
/// minutes per transaction. The Groth16 step runs risc0's Groth16 prover in a
/// container, so it needs a container runtime on the `PATH` as `docker`
/// (Docker, or podman behind a `docker` wrapper).
pub struct Risc0Prover {
    encoding: JournalEncoding,
}

impl Risc0Prover {
    /// A risc0 prover that aggregates with the batch aggregation circuit for
    /// `encoding`.
    pub fn new(encoding: JournalEncoding) -> Self {
        Self { encoding }
    }
}

impl Prover for Risc0Prover {
    async fn prove(&self, action_witnesses: &[ActionWitnesses]) -> anyhow::Result<Transaction> {
        let constrained = constrain::actions(action_witnesses)?;

        let mut base_proofs = HashMap::new();
        for (action_idx, witnesses) in action_witnesses.iter().enumerate() {
            for (logic_idx, logic_witness) in witnesses.logic_witnesses.iter().enumerate() {
                let proof = prove_logic(logic_witness.as_ref()).await.with_context(|| {
                    format!("failed to prove logic witness {logic_idx} of action {action_idx}")
                })?;
                base_proofs.insert(
                    BaseProofKey {
                        action_idx,
                        slot: BaseProofSlot::Logic(logic_idx),
                    },
                    proof,
                );
            }

            let proof = prove_compliance(&witnesses.compliance_witness)
                .await
                .with_context(|| {
                    format!("failed to prove the compliance unit of action {action_idx}")
                })?;
            base_proofs.insert(
                BaseProofKey {
                    action_idx,
                    slot: BaseProofSlot::Compliance,
                },
                proof,
            );
        }

        let assembled = assemble::assemble(action_witnesses, constrained, base_proofs)?;
        let mut transaction = assembled.transaction;
        let encoding = self.encoding;
        let aggregated = on_blocking_thread(move || {
            transaction::aggregate(&mut transaction, ProofType::Groth16, encoding)?;
            Ok(transaction)
        })
        .await
        .context("failed to aggregate the transaction")?;
        assemble::verify_aggregated(aggregated, assembled.kind_table_commitment, encoding)
    }
}

async fn prove_logic(logic_witness: &dyn LogicWitness) -> anyhow::Result<BaseProof> {
    let witness = logic_witness
        .witness_to_vec()
        .context("failed to serialize logic witness to risc0 words")?;
    let proving_key = logic_witness.proving_key();
    let (receipt, instance) = on_blocking_thread(move || {
        proving_system::prove(&proving_key, &Words(&witness), ProofType::Succinct)
    })
    .await?;
    Ok(BaseProof { receipt, instance })
}

async fn prove_compliance(witness: &ComplianceWitness) -> anyhow::Result<BaseProof> {
    let witness = witness.clone();
    let unit =
        on_blocking_thread(move || compliance_unit::create(&witness, ProofType::Succinct)).await?;
    Ok(BaseProof {
        receipt: unit.proof,
        instance: unit.instance,
    })
}

/// Runs one proving job on tokio's blocking thread pool: risc0 proving is
/// synchronous, CPU-bound work.
async fn on_blocking_thread<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, ArmError> + Send + 'static,
) -> anyhow::Result<T> {
    Ok(tokio::task::spawn_blocking(job)
        .await
        .context("the proving task panicked")??)
}

/// A witness already serialized to risc0 words, as the guest reads it. It
/// serializes as a tuple, which risc0 serde writes as its elements with no
/// length prefix, so proving it feeds the guest exactly these words.
struct Words<'a>(&'a [u32]);

impl Serialize for Words<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_tuple(self.0.len())?;
        for word in self.0 {
            tuple.serialize_element(word)?;
        }
        tuple.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_serialize_to_themselves_without_a_length_prefix() {
        let words = [7, 0, u32::MAX, 42];
        let serialized = risc0_zkvm::serde::to_vec(&Words(&words)).expect("words must serialize");
        assert_eq!(
            serialized, words,
            "a logic witness's risc0 words must reach the guest unchanged"
        );
    }

    #[cfg(feature = "fixtures")]
    #[tokio::test]
    #[ignore = "real proving: minutes of CPU and a container runtime for Groth16"]
    async fn risc0_prover_proves_a_trivial_transaction_that_verifies() {
        use crate::fixtures::trivial;

        let action =
            trivial::build(1, trivial::Overrides::default()).expect("a trivial action must build");

        let started = std::time::Instant::now();
        let encoding = JournalEncoding::Risc0Serde;
        let txn = Risc0Prover::new(encoding)
            .prove(&[action.witnesses])
            .await
            .expect("the risc0 prover must prove a trivial transaction");
        eprintln!("proved a trivial transaction in {:?}", started.elapsed());

        let arm_txn = txn.as_arm();
        assert!(
            arm_txn.actions.is_none(),
            "aggregation must erase the base proofs"
        );
        let aggregation = arm_txn
            .aggregation
            .as_ref()
            .expect("the transaction must carry an aggregation");
        assert_eq!(aggregation.instance.actions.len(), 1);
        assert!(
            matches!(
                bincode::deserialize::<risc0_zkvm::InnerReceipt>(&aggregation.proof),
                Ok(risc0_zkvm::InnerReceipt::Groth16(_))
            ),
            "the aggregation proof must be a Groth16 receipt"
        );

        // The fixtures commit to the loaded kind table, the empty one when
        // none is loaded.
        let kind_table_commitment = anoma_rm_risc0::compliance::hash_kind_table_entries(
            anoma_rm_risc0::constants::kind_table(),
        );
        transaction::verify(arm_txn, kind_table_commitment, encoding)
            .expect("the aggregated transaction must verify");
    }
}

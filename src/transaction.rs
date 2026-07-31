//! The proven ARM transaction produced by a [`crate::prover`].

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::transaction::Transaction as ArmTxn;

use crate::environment::Transaction as CoreTransaction;

/// Transaction produced by a prover and consumed by a protocol adapter.
///
/// A thin newtype over the proving backend's ARM transaction, existing so the
/// testkit can implement [`CoreTransaction`] on a type it owns (the inner
/// `ArmTxn` is foreign). Each target chain's protocol adapter converts the inner
/// transaction into chain-specific calldata at execution time.
pub struct Transaction {
    pub(crate) arm_txn: ArmTxn,
}

impl Transaction {
    /// Wrap an ARM transaction.
    #[inline]
    pub fn from_arm(arm_txn: ArmTxn) -> Self {
        Self { arm_txn }
    }

    /// Borrow the inner ARM transaction.
    #[inline]
    pub fn as_arm(&self) -> &ArmTxn {
        &self.arm_txn
    }

    /// Mutably borrow the inner ARM transaction (used by tamper helpers in tests).
    #[inline]
    pub fn as_arm_mut(&mut self) -> &mut ArmTxn {
        &mut self.arm_txn
    }

    /// Consume the newtype, yielding the inner ARM transaction.
    #[inline]
    pub fn into_arm(self) -> ArmTxn {
        self.arm_txn
    }

    /// Flip all bits of one byte of the aggregation proof's inner Groth16
    /// seal — used by negative tests to check that a protocol adapter rejects
    /// tampered proofs.
    #[cfg(any(feature = "local", feature = "e2e"))]
    pub fn tamper_aggregation_seal(&mut self) -> anyhow::Result<()> {
        use anyhow::Context;

        let proof = &mut self
            .arm_txn
            .aggregation
            .as_mut()
            .context("tamper requires an aggregation proof")?
            .proof;

        let mut inner: risc0_zkvm::InnerReceipt = bincode::deserialize(proof)
            .context("tamper requires bincode-encoded inner receipt proof")?;

        let receipt = match &mut inner {
            risc0_zkvm::InnerReceipt::Groth16(receipt) => receipt,
            _ => anyhow::bail!("tamper requires Groth16 inner receipt proof"),
        };

        let byte = receipt
            .seal
            .first_mut()
            .context("tamper requires non-empty inner seal")?;
        *byte ^= 0xff;

        *proof = bincode::serialize(&inner)
            .context("tamper must re-serialize modified inner receipt")?;

        Ok(())
    }
}

impl CoreTransaction for Transaction {
    fn created_commitments(&self) -> anyhow::Result<impl Iterator<Item = Digest> + '_> {
        // When the transaction is aggregated, the proof-backed aggregation
        // instance is authoritative; before aggregation the commitments come
        // from the compliance instances.
        let commitments: Vec<Digest> = if let Some(aggregation) = &self.arm_txn.aggregation {
            aggregation
                .instance
                .actions
                .iter()
                .flat_map(|action| {
                    action
                        .created_publics
                        .iter()
                        .map(|created| created.resource_commitment)
                })
                .collect()
        } else {
            self.arm_txn
                .actions
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|action| {
                    action.compliance_unit.get_instance().map(|instance| {
                        instance
                            .created_publics
                            .iter()
                            .map(|created| created.resource_commitment)
                            .collect::<Vec<_>>()
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect()
        };

        Ok(commitments.into_iter())
    }
}

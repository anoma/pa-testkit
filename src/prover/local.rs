//! Local prover: constrains circuits natively and emits a mock Groth16
//! aggregation seal.

use anoma_rm_risc0::action_tree::ActionTree;
use anoma_rm_risc0::aggregation_instance::{
    ActionAggregated, AggregationInstance, AggregationResourceInstance,
};
use anoma_rm_risc0::delta_proof;
use anoma_rm_risc0::proving_system::JournalEncoding;
use anoma_rm_risc0::resource::ComplianceResourceInstance;
use anoma_rm_risc0::transaction::{self, Aggregation, Delta, Transaction as ArmTxn};
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
pub struct LocalProver {
    encoding: JournalEncoding,
}

impl LocalProver {
    /// A local prover whose seals claim the aggregation journal in `encoding`.
    pub fn new(encoding: JournalEncoding) -> Self {
        Self { encoding }
    }
}

impl Prover for LocalProver {
    async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<Transaction> {
        constrain::catching_panics(|| constrain_txn(actions, self.encoding))
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

/// The mock aggregation seal over `instance`: the claim of the batch
/// aggregation circuit for `encoding` over the instance's journal in that
/// encoding.
pub(crate) fn mock_aggregation_seal(
    instance: &AggregationInstance,
    encoding: JournalEncoding,
) -> Vec<u8> {
    let (journal, verifying_key) = match encoding {
        JournalEncoding::Abi => (
            anoma_rm_risc0::aggregation_instance::abi_encode_instance(instance.clone()),
            anoma_rm_risc0::constants::BATCH_AGGREGATION_EVM_VK,
        ),
        JournalEncoding::Risc0Serde => (
            instance.to_journal(),
            anoma_rm_risc0::constants::BATCH_AGGREGATION_VK,
        ),
    };
    encode_seal(verifying_key, journal_digest(&journal))
}

fn journal_digest(journal: &[u8]) -> Digest {
    let raw: [u8; 32] = Sha256::digest(journal).into();

    raw.into()
}

/// Constrains every action and builds the aggregation instance the way the
/// batch aggregation guest does: the action tree roots are recomputed from the
/// compliance tags and every logic instance must be bound to its action's, the
/// per-resource app data is merged in the compliance
/// unit's resource order, and the actions using a kind tree must share it.
fn constrain_txn(
    action_witnesses: &[ActionWitnesses],
    encoding: JournalEncoding,
) -> anyhow::Result<Transaction> {
    let mut actions = Vec::with_capacity(action_witnesses.len());
    let mut rcvs = Vec::new();
    let mut kind_tree_roots = Vec::with_capacity(action_witnesses.len());

    for (action_idx, witnesses) in action_witnesses.iter().enumerate() {
        let constrained = constrain::action(witnesses, action_idx)?;
        let instance = &constrained.compliance_instance;

        kind_tree_roots.push(instance.kind_tree_root);

        let tags: Vec<Digest> = instance.tags().collect();
        let action_tree_root = ActionTree::new(tags).root().with_context(|| {
            format!("failed to compute the action tree root of action {action_idx}")
        })?;

        let resource_instances = instance
            .resource_instances
            .iter()
            .zip(constrained.resource_logics)
            .map(|(resource, logic)| {
                anyhow::ensure!(
                    logic.instance.action_tree_root == action_tree_root,
                    "logic witness {} of action {action_idx} proves against another action tree \
                     root",
                    logic.witness_index
                );
                let app_data = logic.instance.app_data;
                Ok(match *resource {
                    ComplianceResourceInstance::Consumed {
                        nullifier,
                        logic_ref,
                        commitment_tree_root,
                    } => AggregationResourceInstance::Consumed {
                        nullifier,
                        logic_ref,
                        commitment_tree_root,
                        app_data,
                    },
                    ComplianceResourceInstance::Created {
                        commitment,
                        logic_ref,
                    } => AggregationResourceInstance::Created {
                        commitment,
                        logic_ref,
                        app_data,
                    },
                })
            })
            .collect::<anyhow::Result<_>>()?;

        actions.push(ActionAggregated {
            resource_instances,
            delta_x: instance.delta_x,
            delta_y: instance.delta_y,
            action_tree_root,
        });

        rcvs.push(witnesses.compliance_witness.rcv.clone());
    }

    let instance = AggregationInstance {
        compliance_vk: anoma_rm_risc0::constants::COMPLIANCE_VK,
        kind_tree_root: constrain::aggregated_kind_tree_root(kind_tree_roots)?,
        actions,
    };

    let proof = mock_aggregation_seal(&instance, encoding);

    let arm_txn = transaction::generate_delta_proof(ArmTxn {
        actions: None,
        delta_proof: Delta::Witness(
            delta_proof::from_bytes_vec(&rcvs)
                .context("failed to construct delta witness from rcv values")?,
        ),
        expected_balance: None,
        aggregation: Some(Aggregation { proof, instance }),
    })
    .context("failed to generate delta proof")?;

    Ok(Transaction::from_arm(arm_txn))
}

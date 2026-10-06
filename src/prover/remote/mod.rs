//! Queue prover: submits base proofs to the remote proving queue, assembles an
//! aggregation payload, and returns the aggregated ARM transaction.

use std::collections::HashMap;

use anoma_rm_risc0::constants::{
    BATCH_AGGREGATION_EVM_PK, BATCH_AGGREGATION_PK, COMPLIANCE_PK, COMPLIANCE_VK,
};
use anoma_rm_risc0::proving_system::JournalEncoding;
use anoma_rm_risc0::transaction::Transaction as ArmTxn;
use anyhow::Context;
use futures::future::try_join_all;
use heliax_ap_orchestrator_sdk::QueueClient;
use heliax_ap_orchestrator_sdk::{
    AggregateProofResult, BaseProofResult, GpuAggregationProofPayload, GpuComplianceProofPayload,
    GpuLogicProofPayload, ProofPayload, ProofType,
};

mod queue;

use super::assemble::{self, BaseProof, BaseProofKey, BaseProofSlot};
use super::constrain;
use crate::environment::Prover;
use crate::transaction::Transaction;
use crate::witness::{ActionWitnesses, LogicWitness};

/// Prover for the e2e environment: submits proofs to the real remote proving
/// queue. Built from typed connection params; reads no environment variables.
pub struct QueueProver {
    queue: QueueClient,
    encoding: JournalEncoding,
}

impl QueueProver {
    /// Build a queue prover from a base URL and auth token, aggregating with
    /// the batch aggregation circuit for `encoding`.
    pub fn new(
        base_url: &str,
        auth_token: &str,
        encoding: JournalEncoding,
    ) -> anyhow::Result<Self> {
        let queue = QueueClient::builder(base_url)
            .auth_token(auth_token)
            .build()
            .context("failed to build queue client")?;

        Ok(Self::from_client(queue, encoding))
    }

    /// Build a queue prover from an already-constructed client.
    pub fn from_client(queue: QueueClient, encoding: JournalEncoding) -> Self {
        Self { queue, encoding }
    }
}

impl Prover for QueueProver {
    async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<Transaction> {
        prove_via_queue(&self.queue, actions, self.encoding).await
    }
}

#[derive(Clone, Debug)]
enum BaseJobPayload {
    Logic(ProofPayload),
    Compliance(ProofPayload),
}

#[derive(Clone, Debug)]
struct BaseJobSpec {
    key: BaseProofKey,
    payload: BaseJobPayload,
}

#[derive(Debug)]
struct SubmittedBaseJob {
    key: BaseProofKey,
    job_id: String,
}

#[derive(Debug)]
struct FetchedBaseJob {
    key: BaseProofKey,
    result: BaseProofResult,
}

async fn prove_via_queue(
    queue: &QueueClient,
    action_witnesses: &[ActionWitnesses],
    encoding: JournalEncoding,
) -> anyhow::Result<Transaction> {
    let constrained = constrain::actions(action_witnesses)?;

    let base_job_specs = build_base_job_specs(action_witnesses)?;

    let submitted_jobs = try_join_all(
        base_job_specs
            .into_iter()
            .map(|spec| submit_base_job(queue, spec)),
    )
    .await?;

    let base_results = try_join_all(
        submitted_jobs
            .into_iter()
            .map(|submitted| fetch_base_job(queue, submitted)),
    )
    .await?;

    let mut base_proofs: HashMap<BaseProofKey, BaseProof> =
        HashMap::with_capacity(base_results.len());
    for fetched in base_results {
        let replaced = base_proofs.insert(
            fetched.key,
            BaseProof {
                receipt: fetched.result.receipt,
                instance: fetched.result.instance,
            },
        );
        anyhow::ensure!(
            replaced.is_none(),
            "duplicate base proof result for action {} {:?}",
            fetched.key.action_idx,
            fetched.key.slot
        );
    }

    let assembled = assemble::assemble(action_witnesses, constrained, base_proofs)?;

    let serialized = bincode::serialize(&assembled.transaction)
        .context("failed to serialize transaction for aggregation")?;

    let batch_aggregation_pk = match encoding {
        JournalEncoding::Abi => BATCH_AGGREGATION_EVM_PK,
        JournalEncoding::Risc0Serde => BATCH_AGGREGATION_PK,
    };

    // Without these keys the worker aggregates with its own compiled-in circuits.
    let agg_payload = GpuAggregationProofPayload {
        transaction: serialized,
        batch_aggregation_pk: Some(batch_aggregation_pk.to_vec()),
        compliance_vk: Some(COMPLIANCE_VK.as_bytes().to_vec()),
    };
    let agg_job_id = queue
        .submit(agg_payload)
        .send()
        .await
        .context("failed to submit aggregation proof job")?;

    let agg_result: AggregateProofResult = queue::fetch_job_result(queue, &agg_job_id)
        .await
        .context("failed to fetch aggregation proof result")?;

    let aggregated: ArmTxn = bincode::deserialize(&agg_result.transaction)
        .context("failed to decode aggregated transaction")?;

    assemble::verify_aggregated(aggregated, assembled.kind_table_commitment, encoding)
}

fn build_base_job_specs(action_witnesses: &[ActionWitnesses]) -> anyhow::Result<Vec<BaseJobSpec>> {
    let mut specs = Vec::new();

    for (action_idx, witnesses) in action_witnesses.iter().enumerate() {
        for (logic_idx, logic_witness) in witnesses.logic_witnesses.iter().enumerate() {
            specs.push(BaseJobSpec {
                key: BaseProofKey {
                    action_idx,
                    slot: BaseProofSlot::Logic(logic_idx),
                },
                payload: BaseJobPayload::Logic(build_logic_proof_payload(logic_witness)?),
            });
        }

        specs.push(BaseJobSpec {
            key: BaseProofKey {
                action_idx,
                slot: BaseProofSlot::Compliance,
            },
            payload: BaseJobPayload::Compliance(build_compliance_proof_payload(
                &witnesses.compliance_witness,
            )?),
        });
    }

    Ok(specs)
}

async fn submit_base_job(
    queue: &QueueClient,
    spec: BaseJobSpec,
) -> anyhow::Result<SubmittedBaseJob> {
    let BaseJobSpec { key, payload } = spec;
    let job_id = match payload {
        BaseJobPayload::Logic(payload) => queue
            .submit(GpuLogicProofPayload(payload))
            .send()
            .await
            .with_context(|| {
                format!(
                    "failed to submit base proof for action {} {:?}",
                    key.action_idx, key.slot
                )
            })?,
        BaseJobPayload::Compliance(payload) => queue
            .submit(GpuComplianceProofPayload(payload))
            .send()
            .await
            .with_context(|| {
                format!(
                    "failed to submit base proof for action {} {:?}",
                    key.action_idx, key.slot
                )
            })?,
    };

    Ok(SubmittedBaseJob { key, job_id })
}

async fn fetch_base_job(
    queue: &QueueClient,
    submitted: SubmittedBaseJob,
) -> anyhow::Result<FetchedBaseJob> {
    let key = submitted.key;
    let result = queue::fetch_job_result::<BaseProofResult>(queue, &submitted.job_id)
        .await
        .with_context(|| {
            format!(
                "failed to fetch base proof result for action {} {:?}",
                key.action_idx, key.slot
            )
        })?;

    Ok(FetchedBaseJob { key, result })
}

fn build_logic_proof_payload(logic_witness: &impl LogicWitness) -> anyhow::Result<ProofPayload> {
    Ok(ProofPayload {
        witness: logic_witness
            .witness_to_vec()
            .context("failed to serialize logic witness to risc0 words")?,
        proving_key: logic_witness.proving_key(),
        proof_type: ProofType::Succinct,
        verifying_key: logic_witness.verifying_key().as_bytes().to_vec(),
    })
}

fn build_compliance_proof_payload(
    compliance_witness: &anoma_rm_risc0::compliance::ComplianceWitness,
) -> anyhow::Result<ProofPayload> {
    let witness = risc0_zkvm::serde::to_vec(compliance_witness)
        .context("failed to serialize compliance witness to risc0 words")?;

    Ok(ProofPayload {
        witness,
        proving_key: COMPLIANCE_PK.to_vec(),
        proof_type: ProofType::Succinct,
        verifying_key: COMPLIANCE_VK.as_bytes().to_vec(),
    })
}

//! Queue prover: submits base proofs to the remote proving queue, assembles an
//! aggregation payload, and returns the aggregated ARM transaction.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;

use anoma_rm_risc0::action::Action;
use anoma_rm_risc0::compliance_unit::ComplianceUnit;
use anoma_rm_risc0::constants::{COMPLIANCE_PK, COMPLIANCE_VK};
use anoma_rm_risc0::delta_proof::DeltaWitness;
use anoma_rm_risc0::logic_proof::LogicVerifierInputs;
use anoma_rm_risc0::transaction::{Delta, Transaction as ArmTxn};
use anyhow::Context;
use futures::future::try_join_all;
use heliax_ap_orchestrator_sdk::QueueClient;
use heliax_ap_orchestrator_sdk::{
    AggregateProofResult, BaseProofResult, GpuAggregationProofPayload, GpuComplianceProofPayload,
    GpuLogicProofPayload, ProofPayload, ProofType,
};

mod queue;

use super::constrain::{self, ConstrainedAction, ConstrainedLogic};
use crate::environment::Prover;
use crate::transaction::Transaction;
use crate::witness::{ActionWitnesses, LogicWitness};

/// Prover for the e2e environment: submits proofs to the real remote proving
/// queue. Built from typed connection params; reads no environment variables.
pub struct QueueProver {
    queue: QueueClient,
}

impl QueueProver {
    /// Build a queue prover from a base URL and auth token.
    pub fn new(base_url: &str, auth_token: &str) -> anyhow::Result<Self> {
        let queue = QueueClient::builder(base_url)
            .auth_token(auth_token)
            .build()
            .context("failed to build queue client")?;

        Ok(Self { queue })
    }

    /// Build a queue prover from an already-constructed client.
    pub fn from_client(queue: QueueClient) -> Self {
        Self { queue }
    }
}

impl Prover for QueueProver {
    type Transaction = Transaction;

    async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<Self::Transaction> {
        prove_via_queue(&self.queue, actions).await
    }
}

/// Identifies one base proof within an action: either the logic proof of the
/// witness at a given index in the action's logic list, or the compliance
/// proof of the action's single compliance unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum BaseJobSlot {
    Logic(usize),
    Compliance,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BaseJobKey {
    action_idx: usize,
    slot: BaseJobSlot,
}

#[derive(Clone, Debug)]
enum BaseJobPayload {
    Logic(ProofPayload),
    Compliance(ProofPayload),
}

#[derive(Clone, Debug)]
struct BaseJobSpec {
    key: BaseJobKey,
    payload: BaseJobPayload,
}

#[derive(Debug)]
struct SubmittedBaseJob {
    key: BaseJobKey,
    job_id: String,
}

#[derive(Debug)]
struct FetchedBaseJob {
    key: BaseJobKey,
    result: BaseProofResult,
}

async fn prove_via_queue(
    queue: &QueueClient,
    action_witnesses: &[ActionWitnesses],
) -> anyhow::Result<Transaction> {
    let constrained =
        std::panic::catch_unwind(AssertUnwindSafe(|| constrain_actions(action_witnesses)))
            .unwrap_or_else(|cause| {
                if let Some(panic_msg) = cause.downcast_ref::<String>() {
                    anyhow::bail!("proving failed: {panic_msg}");
                }
                if let Some(panic_msg) = cause.downcast_ref::<&'static str>() {
                    anyhow::bail!("proving failed: {panic_msg}");
                }
                std::panic::resume_unwind(cause)
            })?;

    let (base_job_specs, rcvs) = build_base_job_specs(action_witnesses)?;

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

    let mut base_results_by_key: HashMap<BaseJobKey, BaseProofResult> =
        HashMap::with_capacity(base_results.len());
    for fetched in base_results {
        let replaced = base_results_by_key.insert(fetched.key, fetched.result);
        anyhow::ensure!(
            replaced.is_none(),
            "duplicate base proof result for action {} {:?}",
            fetched.key.action_idx,
            fetched.key.slot
        );
    }

    let mut actions = Vec::with_capacity(constrained.len());
    for (action_idx, action) in constrained.into_iter().enumerate() {
        let compliance_result = take_base_result(
            &mut base_results_by_key,
            action_idx,
            BaseJobSlot::Compliance,
        )?;
        let compliance_unit = ComplianceUnit {
            proof: compliance_result.receipt,
            instance: compliance_result.instance,
        };

        // The logic verifier inputs must be in the canonical tag order
        // (consumed nullifiers, then created commitments) — `constrain`
        // returns them reordered, and `witness_index` correlates each with
        // its proving job.
        let logic_count = action.consumed_logics.len() + action.created_logics.len();
        let mut logic_verifier_inputs = Vec::with_capacity(logic_count);
        for logic in action
            .consumed_logics
            .into_iter()
            .chain(action.created_logics)
        {
            let result = take_base_result(
                &mut base_results_by_key,
                action_idx,
                BaseJobSlot::Logic(logic.witness_index),
            )?;
            logic_verifier_inputs.push(logic_verifier_inputs_from(logic, result));
        }

        actions.push(Action {
            compliance_unit,
            logic_verifier_inputs,
        });
    }

    anyhow::ensure!(
        base_results_by_key.is_empty(),
        "unused base proof results remaining after assembly: {}",
        base_results_by_key.len()
    );

    let delta = Delta::Witness(
        DeltaWitness::from_bytes_vec(&rcvs)
            .context("failed to construct delta witness from rcv values")?,
    );
    let arm_txn = ArmTxn::create(actions, delta)
        .generate_delta_proof()
        .context("failed to generate delta proof")?;

    let serialized =
        bincode::serialize(&arm_txn).context("failed to serialize transaction for aggregation")?;

    let agg_payload = GpuAggregationProofPayload {
        transaction: serialized,
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

    aggregated
        .clone()
        .verify()
        .context("aggregated transaction failed local verification")?;

    Ok(Transaction::from_arm(aggregated))
}

/// Constrains and validates every action, panicking on invalid witnesses (the
/// caller catches the panic).
fn constrain_actions(
    action_witnesses: &[ActionWitnesses],
) -> anyhow::Result<Vec<ConstrainedAction>> {
    action_witnesses
        .iter()
        .enumerate()
        .map(|(action_idx, witnesses)| constrain::action(witnesses, action_idx))
        .collect()
}

fn logic_verifier_inputs_from(
    logic: ConstrainedLogic,
    result: BaseProofResult,
) -> LogicVerifierInputs {
    LogicVerifierInputs {
        tag: logic.instance.tag,
        verifying_key: logic.verifying_key,
        app_data: logic.instance.app_data,
        proof: result.receipt,
    }
}

fn take_base_result(
    results: &mut HashMap<BaseJobKey, BaseProofResult>,
    action_idx: usize,
    slot: BaseJobSlot,
) -> anyhow::Result<BaseProofResult> {
    results
        .remove(&BaseJobKey { action_idx, slot })
        .with_context(|| format!("missing base proof result for action {action_idx} {slot:?}"))
}

fn build_base_job_specs(
    action_witnesses: &[ActionWitnesses],
) -> anyhow::Result<(Vec<BaseJobSpec>, Vec<Vec<u8>>)> {
    let mut specs = Vec::new();
    let mut rcvs = Vec::new();

    for (action_idx, witnesses) in action_witnesses.iter().enumerate() {
        for (logic_idx, logic_witness) in witnesses.logic_witnesses.iter().enumerate() {
            specs.push(BaseJobSpec {
                key: BaseJobKey {
                    action_idx,
                    slot: BaseJobSlot::Logic(logic_idx),
                },
                payload: BaseJobPayload::Logic(build_logic_proof_payload(logic_witness)?),
            });
        }

        specs.push(BaseJobSpec {
            key: BaseJobKey {
                action_idx,
                slot: BaseJobSlot::Compliance,
            },
            payload: BaseJobPayload::Compliance(build_compliance_proof_payload(
                &witnesses.compliance_witness,
            )?),
        });
        rcvs.push(witnesses.compliance_witness.rcv.clone());
    }

    Ok((specs, rcvs))
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

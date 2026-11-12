//! The queue's aggregation job with the journal encoding it aggregates for,
//! which the published SDK (0.18.0) lacks: the worker decodes the journal of
//! a job that names none as ABI, so a risc0-serde aggregation it proves fails
//! to decode and the job dead-letters. Built on the SDK's public `Job` trait
//! under the SDK's kind, as the queue's `arm-v2` defines it (its
//! `AggregationProofRequest`). Replace with the SDK's type once a release
//! carries the field.

use anoma_rm_risc0::proving_system::JournalEncoding;
use heliax_ap_orchestrator_sdk::{JOB_KIND_GPU_AGGREGATION_PROOF, Job};
use serde::Serialize;

/// The queue's name of a journal encoding: `"Abi"` or `"Risc0Serde"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) enum QueueJournalEncoding {
    Abi,
    Risc0Serde,
}

impl From<JournalEncoding> for QueueJournalEncoding {
    fn from(encoding: JournalEncoding) -> Self {
        match encoding {
            JournalEncoding::Abi => Self::Abi,
            JournalEncoding::Risc0Serde => Self::Risc0Serde,
        }
    }
}

/// Aggregate every base proof in a bincode transaction with the batch
/// aggregation circuit `batch_aggregation_pk`, whose journal is in
/// `journal_encoding`.
#[derive(Debug, Clone, Serialize)]
pub(super) struct GpuAggregationProofPayload {
    pub transaction: Vec<u8>,
    pub batch_aggregation_pk: Option<Vec<u8>>,
    pub compliance_vk: Option<Vec<u8>>,
    pub journal_encoding: QueueJournalEncoding,
}

impl Job for GpuAggregationProofPayload {
    const KIND: &'static str = JOB_KIND_GPU_AGGREGATION_PROOF;
    fn timeout_seconds(&self) -> u64 {
        600
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solana_aggregation_job_names_risc0_serde() {
        let payload = GpuAggregationProofPayload {
            transaction: vec![1],
            batch_aggregation_pk: None,
            compliance_vk: None,
            journal_encoding: JournalEncoding::Risc0Serde.into(),
        };
        let json = serde_json::to_value(&payload).expect("the payload serializes");
        println!("the job the queue receives: {json}");
        assert_eq!(json["journal_encoding"], "Risc0Serde");
        assert_eq!(
            serde_json::to_value(QueueJournalEncoding::from(JournalEncoding::Abi))
                .expect("an encoding serializes"),
            "Abi"
        );
    }
}

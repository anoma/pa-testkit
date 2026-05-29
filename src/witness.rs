use anoma_rm_risc0::Digest;
use anoma_rm_risc0::compliance::ComplianceWitness;
use anoma_rm_risc0::logic_instance::LogicInstance;
use anoma_rm_risc0::logic_proof::LogicProver;
use anoma_rm_risc0::resource_logic::LogicCircuit;
use anyhow::Context;

/// Application data attached to a resource logic, re-exported from the proving
/// stack so witnesses, the prover, and tests all share one definition.
pub use anoma_rm_risc0::logic_instance::{AppData, ExpirableBlob};

/// Witness data of an individual action, mirroring [`anoma_rm_risc0::action::Action`].
///
/// The compliance witnesses and the per-resource logic witnesses are two
/// independent lists, correlated by tag rather than by position: each compliance
/// unit's consumed nullifier and created commitment identify the two logic
/// witnesses that belong to it.
pub struct ActionWitnesses {
    /// One compliance witness per compliance unit; each unit pairs one consumed
    /// and one created resource.
    pub compliance_witnesses: Vec<Box<ComplianceWitness>>,
    /// One logic witness per resource in the action, matched to a compliance
    /// unit by the tag it produces when constrained.
    pub logic_witnesses: Vec<Box<dyn LogicWitness>>,
}

/// Witness of a logic proof.
pub trait LogicWitness {
    /// Verifying key of the circuit.
    fn verifying_key(&self) -> Digest;

    /// Constrain the circuit, yielding a logic instance.
    fn constrain(&self) -> anyhow::Result<LogicInstance>;

    /// Serialize the witness to RISC-V words for remote proving.
    fn witness_to_vec(&self) -> anyhow::Result<Vec<u32>>;

    /// Proving key of the circuit.
    fn proving_key(&self) -> Vec<u8>;
}

impl LogicWitness for Box<dyn LogicWitness> {
    #[inline]
    fn verifying_key(&self) -> Digest {
        (**self).verifying_key()
    }

    #[inline]
    fn constrain(&self) -> anyhow::Result<LogicInstance> {
        (**self).constrain()
    }

    #[inline]
    fn witness_to_vec(&self) -> anyhow::Result<Vec<u32>> {
        (**self).witness_to_vec()
    }

    #[inline]
    fn proving_key(&self) -> Vec<u8> {
        (**self).proving_key()
    }
}

impl<W> LogicWitness for W
where
    W: LogicProver + LogicCircuit,
{
    #[inline]
    fn verifying_key(&self) -> Digest {
        <W as LogicProver>::verifying_key()
    }

    #[inline]
    fn constrain(&self) -> anyhow::Result<LogicInstance> {
        <W as LogicCircuit>::constrain(self)
            .with_context(|| format!("invalid proof of {} witness", std::any::type_name::<W>()))
    }

    #[inline]
    fn witness_to_vec(&self) -> anyhow::Result<Vec<u32>> {
        risc0_zkvm::serde::to_vec(<W as LogicProver>::witness(self)).with_context(|| {
            format!(
                "failed to serialize {} witness to risc0 words",
                std::any::type_name::<W>()
            )
        })
    }

    #[inline]
    fn proving_key(&self) -> Vec<u8> {
        <W as LogicProver>::proving_key().to_vec()
    }
}

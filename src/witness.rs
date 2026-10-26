use anoma_rm_risc0::Digest;
use anoma_rm_risc0::compliance::ComplianceWitness;
use anoma_rm_risc0::logic_instance::LogicInstance;
use anoma_rm_risc0::logic_proof::LogicProver;
use anoma_rm_risc0::resource::Resource;
use anoma_rm_risc0::resource_logic::LogicCircuit;
use anyhow::Context;

/// Application data attached to a resource logic, re-exported from the proving
/// stack so witnesses, the prover, and tests all share one definition.
pub use anoma_rm_risc0::logic_instance::{AppData, ExpirableBlob};

/// The witnesses of one action, mirroring [`anoma_rm_risc0::action::Action`]: the compliance
/// witness and one logic witness per resource. Logic witness `i` belongs to resource `i` of the
/// compliance witness.
pub struct ActionWitnesses {
    /// The compliance witness. The order of its resources is the order in which the protocol
    /// adapter processes them.
    pub compliance_witness: ComplianceWitness,
    /// One logic witness per resource, in the order of the resources.
    pub logic_witnesses: Vec<Box<dyn LogicWitness>>,
}

impl ActionWitnesses {
    /// The resources in their order, with the derived nonces of the created resources.
    pub fn resources(&self) -> Vec<Resource> {
        self.compliance_witness
            .resource_instances
            .iter()
            .map(|witness| *witness.resource())
            .collect()
    }
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

impl<L> LogicWitness for L
where
    L: LogicProver,
    L::Witness: LogicCircuit,
{
    #[inline]
    fn verifying_key(&self) -> Digest {
        <L as LogicProver>::verifying_key()
    }

    #[inline]
    fn constrain(&self) -> anyhow::Result<LogicInstance> {
        LogicCircuit::constrain(<L as LogicProver>::witness(self))
            .with_context(|| format!("invalid proof of {} witness", std::any::type_name::<L>()))
    }

    #[inline]
    fn witness_to_vec(&self) -> anyhow::Result<Vec<u32>> {
        risc0_zkvm::serde::to_vec(<L as LogicProver>::witness(self)).with_context(|| {
            format!(
                "failed to serialize {} witness to risc0 words",
                std::any::type_name::<L>()
            )
        })
    }

    #[inline]
    fn proving_key(&self) -> Vec<u8> {
        <L as LogicProver>::proving_key().to_vec()
    }
}

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::logic_instance::LogicInstance;
use anyhow::Context;

use crate::witness::LogicWitness;

/// The pass-through logic guest (`circuits/passthrough-logic`), as
/// `scripts/update_elfs.sh` builds it reproducibly.
pub const PASSTHROUGH_LOGIC_PK: &[u8] = include_bytes!("../../../elfs/passthrough-logic-guest.bin");

/// The image id of [`PASSTHROUGH_LOGIC_PK`].
pub const PASSTHROUGH_LOGIC_VK: Digest = Digest::new([
    0x777b3887, 0x8fc62951, 0x7a31cbf1, 0xe85c61ed, 0xfd532e28, 0x41089b30, 0x1720dd4e, 0xcd2315ae,
]);

/// The pass-through logic's witness: the logic instance itself, which the
/// guest reads and commits unchanged.
pub struct PassthroughLogicWitness(pub LogicInstance);

impl LogicWitness for PassthroughLogicWitness {
    fn verifying_key(&self) -> Digest {
        PASSTHROUGH_LOGIC_VK
    }

    fn constrain(&self) -> anyhow::Result<LogicInstance> {
        Ok(self.0.clone())
    }

    fn witness_to_vec(&self) -> anyhow::Result<Vec<u32>> {
        risc0_zkvm::serde::to_vec(&self.0).context("failed to serialize the pass-through witness")
    }

    fn proving_key(&self) -> Vec<u8> {
        PASSTHROUGH_LOGIC_PK.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verifying_key_is_the_committed_guests_image_id() {
        let image_id = risc0_zkvm::compute_image_id(PASSTHROUGH_LOGIC_PK)
            .expect("the committed guest is a risc0 program");
        assert_eq!(
            image_id, PASSTHROUGH_LOGIC_VK,
            "elfs/passthrough-logic-guest.bin is not the guest PASSTHROUGH_LOGIC_VK names; \
             rebuild it with scripts/update_elfs.sh and update the constant"
        );
    }
}

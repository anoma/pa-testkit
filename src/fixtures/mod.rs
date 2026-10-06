//! Fixtures for building actions to exercise a protocol adapter — app- and
//! chain-agnostic, needing only the risc0 proving stack, so every
//! integration-test crate can reuse them.

pub mod identities;
pub mod passthrough;
pub mod trivial;

use anoma_rm_risc0::Digest;

/// The nonce of the consumed resource at `index` of a fixture action built
/// from `seed`: the testkit's tag, then the seed and the index, so no fixture
/// shares a nullifier with a resource another tool created on a live chain a
/// test forks.
pub(crate) fn consumed_nonce(seed: u8, index: u8) -> [u8; 32] {
    let mut nonce = *b"anoma pa-testkit fixture nonce\0\0";
    nonce[30] = seed;
    nonce[31] = index;
    nonce
}

/// The commitment to the kind table the fixtures prove against: the loaded
/// table, or the empty one when none is loaded.
pub fn kind_table_commitment() -> Digest {
    anoma_rm_risc0::compliance::hash_kind_table_entries(anoma_rm_risc0::constants::kind_table())
}

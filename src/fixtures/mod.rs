//! Fixtures for building actions to exercise a protocol adapter — app- and
//! chain-agnostic, needing only the risc0 proving stack, so every
//! integration-test crate can reuse them.

pub mod identities;
pub mod passthrough;
pub mod trivial;

use anoma_rm_risc0::Digest;

/// The commitment to the kind table the fixtures prove against: the loaded
/// table, or the empty one when none is loaded.
pub fn kind_table_commitment() -> Digest {
    anoma_rm_risc0::compliance::hash_kind_table_entries(anoma_rm_risc0::constants::kind_table())
}

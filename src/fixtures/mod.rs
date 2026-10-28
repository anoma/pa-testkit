//! Fixtures for building actions to exercise a protocol adapter — app- and
//! chain-agnostic, needing only the risc0 proving stack, so every
//! integration-test crate can reuse them.

pub mod identities;
pub mod passthrough;
pub mod trivial;

use std::sync::LazyLock;

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::kind_tree::KindTree;
use anoma_rm_risc0::merkle_path::PADDING_LEAF;

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

/// The kind tree the fixtures prove against: the loaded tree, or the
/// empty one when none is loaded.
pub fn kind_tree() -> &'static KindTree {
    static EMPTY: LazyLock<KindTree> = LazyLock::new(KindTree::empty);
    anoma_rm_risc0::constants::global_kind_tree().unwrap_or(&EMPTY)
}

/// The root of the kind tree the fixtures prove against.
pub fn kind_tree_root() -> Digest {
    kind_tree().root()
}

/// The root of the empty kind tree, its padding leaf, which every protocol
/// adapter accepts whatever tree it stores.
pub fn empty_kind_tree_root() -> Digest {
    PADDING_LEAF
}

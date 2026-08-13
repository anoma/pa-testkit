use anoma_rm_risc0::nullifier_key::NullifierKey;
use anoma_rm_risc0::nullifier_key::NullifierKeyCommitment;
use anoma_rm_risc0::resource::Resource;
use risc0_zkvm::Digest;

/// Overrides for the consumed/created trivial resources, used to size the
/// action and to build deliberately-invalid variants for negative tests.
///
/// The per-side overrides apply uniformly to every resource on that side; the
/// nonce overrides only make sense with the default counts of one resource per
/// side (identical nonces on one side would collide).
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub consumed_count: Option<usize>,
    pub created_count: Option<usize>,
    pub consumed_quantity: Option<u128>,
    pub created_quantity: Option<u128>,
    pub consumed_is_ephemeral: Option<bool>,
    pub created_is_ephemeral: Option<bool>,
    pub consumed_nonce: Option<[u8; 32]>,
    pub created_nonce: Option<[u8; 32]>,
}

impl Overrides {
    pub fn invalid_nonzero_quantity() -> Self {
        Self {
            consumed_quantity: Some(1),
            ..Self::default()
        }
    }

    pub fn invalid_consumed_non_ephemeral() -> Self {
        Self {
            consumed_is_ephemeral: Some(false),
            ..Self::default()
        }
    }

    pub fn invalid_created_non_ephemeral() -> Self {
        Self {
            created_is_ephemeral: Some(false),
            ..Self::default()
        }
    }
}

pub(super) fn consumed(
    seed: u8,
    index: usize,
    nk_commitment: NullifierKeyCommitment,
    overrides: &Overrides,
) -> Resource {
    // The default nonce is unique per (seed, index): actions use distinct
    // seeds, and the last byte separates the resources within an action.
    let mut default_nonce = [seed; 32];
    default_nonce[31] = index as u8;

    Resource {
        logic_ref: *anoma_rm_risc0::constants::PADDING_LOGIC_VK,
        label_ref: Digest::default(),
        quantity: overrides.consumed_quantity.unwrap_or(0),
        value_ref: Digest::default(),
        is_ephemeral: overrides.consumed_is_ephemeral.unwrap_or(true),
        nonce: overrides.consumed_nonce.unwrap_or(default_nonce),
        nk_commitment,
        rand_seed: [seed.wrapping_add(11); 32],
    }
}

pub(super) fn created(
    seed: u8,
    derived_nonce: [u8; 32],
    nk_commitment: NullifierKeyCommitment,
    overrides: &Overrides,
) -> Resource {
    Resource {
        logic_ref: *anoma_rm_risc0::constants::PADDING_LOGIC_VK,
        label_ref: Digest::default(),
        quantity: overrides.created_quantity.unwrap_or(0),
        value_ref: Digest::default(),
        is_ephemeral: overrides.created_is_ephemeral.unwrap_or(true),
        nonce: overrides.created_nonce.unwrap_or(derived_nonce),
        nk_commitment,
        rand_seed: [seed.wrapping_add(33); 32],
    }
}

pub(super) fn nullifier_key(seed: u8) -> NullifierKey {
    NullifierKey::from_bytes([seed; 32])
}

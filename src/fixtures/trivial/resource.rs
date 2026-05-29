use anoma_rm_risc0::nullifier_key::NullifierKey;
use anoma_rm_risc0::nullifier_key::NullifierKeyCommitment;
use anoma_rm_risc0::resource::Resource;
use anyhow::Context;
use risc0_zkvm::Digest;

/// Overrides for the consumed/created trivial resources, used to build
/// deliberately-invalid variants for negative tests.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
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
    nk_commitment: NullifierKeyCommitment,
    overrides: &Overrides,
) -> Resource {
    Resource {
        logic_ref: *anoma_rm_risc0::constants::PADDING_LOGIC_VK,
        label_ref: Digest::default(),
        quantity: overrides.consumed_quantity.unwrap_or(0),
        value_ref: Digest::default(),
        is_ephemeral: overrides.consumed_is_ephemeral.unwrap_or(true),
        nonce: overrides.consumed_nonce.unwrap_or([seed; 32]),
        nk_commitment,
        rand_seed: [seed.wrapping_add(11); 32],
    }
}

pub(super) fn created(
    seed: u8,
    nk_commitment: NullifierKeyCommitment,
    consumed_nullifier: Digest,
    overrides: &Overrides,
) -> anyhow::Result<Resource> {
    let default_nonce: [u8; 32] = consumed_nullifier
        .as_bytes()
        .try_into()
        .context("nullifier must be 32 bytes")?;

    Ok(Resource {
        logic_ref: *anoma_rm_risc0::constants::PADDING_LOGIC_VK,
        label_ref: Digest::default(),
        quantity: overrides.created_quantity.unwrap_or(0),
        value_ref: Digest::default(),
        is_ephemeral: overrides.created_is_ephemeral.unwrap_or(true),
        nonce: overrides.created_nonce.unwrap_or(default_nonce),
        nk_commitment,
        rand_seed: [seed.wrapping_add(33); 32],
    })
}

pub(super) fn nullifier_key(seed: u8) -> NullifierKey {
    NullifierKey::from_bytes([seed; 32])
}

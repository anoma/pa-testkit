//! A program that takes its address from the build environment, as the
//! Solana programs do with `declare_id!(env!(..))`.

/// The program's address.
pub const ID: &str = env!("FIXTURE_PROGRAM_ID");

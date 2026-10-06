//! Risc0 provers: turn action witnesses into a proven ARM
//! [`crate::transaction::Transaction`].
//!
//! Every prover is built with the aggregation journal encoding of the chain
//! that verifies its transactions (`JournalEncoding::Abi` for the EVM,
//! `Risc0Serde` for Solana), which also selects the batch aggregation circuit;
//! nothing else about the chain reaches it. The [`LocalProver`] runs
//! circuits via `constrain` and emits a mock aggregation seal (no real
//! proving); the [`QueueProver`] submits to the real remote proving queue; the
//! [`Risc0Prover`] makes the same real proofs in-process. The shared
//! constraining step they all run first lives in [`constrain`], and the
//! assembly the two real provers share in [`assemble`].

#[cfg(any(feature = "e2e", feature = "prove"))]
mod assemble;
#[cfg(any(feature = "local", feature = "e2e", feature = "prove"))]
mod constrain;
#[cfg(feature = "local")]
mod local;
#[cfg(feature = "e2e")]
mod remote;
#[cfg(feature = "prove")]
mod risc0;

#[cfg(feature = "local")]
pub use local::LocalProver;
#[cfg(all(test, feature = "local"))]
pub(crate) use local::mock_aggregation_seal;
#[cfg(feature = "e2e")]
pub use remote::QueueProver;
#[cfg(feature = "prove")]
pub use risc0::Risc0Prover;

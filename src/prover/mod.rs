//! Risc0 provers: turn action witnesses into a proven ARM
//! [`crate::transaction::Transaction`].
//!
//! Both provers are agnostic to the target chain. The [`LocalProver`] runs
//! circuits via `constrain` and emits a mock aggregation seal (no real
//! proving); the [`QueueProver`] submits to the real remote proving queue. The
//! shared constraining step they both run first lives in [`constrain`].

#[cfg(any(feature = "local", feature = "e2e"))]
mod constrain;
#[cfg(feature = "local")]
mod local;
#[cfg(feature = "e2e")]
mod remote;

#[cfg(feature = "local")]
pub use local::LocalProver;
#[cfg(feature = "e2e")]
pub use remote::QueueProver;

/// How both provers encode the aggregation journal, which also selects the
/// batch aggregation circuit.
#[cfg(any(feature = "local", feature = "e2e"))]
const JOURNAL_ENCODING: anoma_rm_risc0::proving_system::JournalEncoding = {
    #[cfg(feature = "abi_encoding")]
    {
        anoma_rm_risc0::proving_system::JournalEncoding::Abi
    }
    #[cfg(not(feature = "abi_encoding"))]
    {
        anoma_rm_risc0::proving_system::JournalEncoding::Risc0Serde
    }
};

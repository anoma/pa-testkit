//! The interface a chain's harness implements, and the vocabulary the suite
//! speaks through it. Nothing here names a chain: whatever differs between
//! chains (how a call is encoded, how an adapter reports a refusal) is the
//! harness's to translate into these terms.

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::merkle_path::MerklePath;

use crate::transaction::Transaction;
use crate::witness::ActionWitnesses;

/// What a protocol adapter did with a transaction it was asked to settle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Settled,
    Refused(Refusal),
}

/// Why a protocol adapter refused to settle a transaction: the protocol's
/// reasons, which every chain's adapter checks. A harness decodes its
/// adapter's error into one; an error it cannot decode is not a refusal but a
/// failure of the harness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The aggregation seal does not prove the transaction's aggregation
    /// instance.
    InvalidAggregationSeal,
    /// An external call returned other than the output its proof expects.
    ExternalCallOutputMismatch,
}

/// An external call the suite makes, to one of the example programs every
/// chain's harness provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalCall {
    /// Asks the block-time forwarder how `time`, in seconds since the Unix
    /// epoch, compares with the block's, expecting `expected`.
    BlockTime { time: u32, expected: TimeComparison },
}

/// How a block-time forwarder finds the time it is asked about compared with
/// the block's: the byte both chains' example forwarders return (`LT`, `EQ`
/// and `GT` in pa-evm's `BlockTimeForwarder.TimeComparison` and the Solana
/// adapter's `block_time_forwarder`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TimeComparison {
    Before = 0,
    At = 1,
    After = 2,
}

/// A protocol adapter on one chain, with a prover for it.
pub trait Environment {
    /// Protocol adapter.
    type ProtocolAdapter: ProtocolAdapter;

    /// Transaction prover.
    type Prover: Prover;

    /// Get a reference to the tx prover.
    fn prover(&self) -> &Self::Prover;

    /// Get a reference to the protocol adapter.
    fn protocol_adapter(&self) -> &Self::ProtocolAdapter;

    /// Get a mut reference to the protocol adapter.
    fn protocol_adapter_mut(&mut self) -> &mut Self::ProtocolAdapter;

    /// The external payload blob of `call`, in this chain's encoding. The
    /// program it calls is ready for the call once this returns.
    #[allow(async_fn_in_trait)]
    async fn external_call(&mut self, call: ExternalCall) -> anyhow::Result<Vec<u32>>;
}

/// Protocol adapter abstraction.
pub trait ProtocolAdapter {
    /// Commitment tree.
    type CommitmentTree: CommitmentTree;

    /// Asks the adapter to settle `transaction`, adding its commitments and
    /// nullifiers to the commitment tree and nullifier set. A refusal is an
    /// [`Outcome`]; an error means the harness could not ask, or could not
    /// decode the adapter's answer.
    #[allow(async_fn_in_trait)]
    async fn settle(&mut self, transaction: Transaction) -> anyhow::Result<Outcome>;

    /// Get a reference to the commitment tree root.
    fn commitment_tree(&self) -> &Self::CommitmentTree;
}

/// Commitment tree associated with the protocol adapter.
pub trait CommitmentTree {
    /// Compute the current root of the tree.
    fn root(&self) -> anyhow::Result<Digest>;

    /// Compute the path to a leaf in the tree.
    fn path_to(&self, leaf: Digest) -> anyhow::Result<MerklePath>;
}

/// Transaction prover.
pub trait Prover {
    /// Prove an ARM transaction.
    ///
    /// Invalid witnesses will result in an error.
    #[allow(async_fn_in_trait)]
    async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<Transaction>;
}

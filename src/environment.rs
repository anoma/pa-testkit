//! The interface a chain's harness implements, and the vocabulary the suite
//! speaks through it. Nothing here names a chain: whatever differs between
//! chains (how a call is encoded, how an adapter reports a refusal) is the
//! harness's to translate into these terms.

use anoma_rm_risc0::Digest;

use crate::commitment_tree::FrontierCommitmentTree;
use crate::transaction::Transaction;
use crate::witness::ActionWitnesses;

/// What a protocol adapter did with a transaction it was asked to settle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It settled it, emitting these events in this order.
    Settled(Vec<Event>),
    Refused(Refusal),
}

/// The deletion criterion of a blob a protocol adapter keeps: it emits the
/// blob as a payload event when it settles the resource carrying it
/// (pa-evm's `DeletionCriterion.Never`, the Solana adapter's
/// `DELETION_CRITERION_NEVER`).
pub const DELETION_CRITERION_NEVER: u32 = 1;

/// An event a protocol adapter emits when it settles a transaction: pa-evm's
/// settlement events, which the Solana adapter mirrors, with only what every
/// chain gives the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A forwarder returned the output its call expected. What it was called
    /// with and returned are in its chain's encoding, so they are not here.
    ForwarderCallExecuted,
    /// A blob of a settled resource's app data the adapter keeps
    /// ([`DELETION_CRITERION_NEVER`]): the resource's tag, the blob's index
    /// in its payload, and the blob's bytes.
    Payload {
        kind: PayloadKind,
        tag: Digest,
        index: u32,
        blob: Vec<u8>,
    },
    /// An action settled.
    ActionExecuted {
        action_tree_root: Digest,
        nullifiers: Vec<Digest>,
        consumed_logic_refs: Vec<Digest>,
        commitments: Vec<Digest>,
        created_logic_refs: Vec<Digest>,
    },
    /// The transaction's commitments made this the latest root.
    CommitmentTreeRootAdded { root: Digest },
    /// The transaction settled; its id is the keccak hash of its action tree
    /// roots, in order.
    TransactionExecuted { transaction_id: Digest },
}

/// Which payload of a resource's app data a [`Event::Payload`] blob is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadKind {
    Resource,
    Discovery,
    External,
    Application,
}

/// Why a protocol adapter refused to settle a transaction: the protocol's
/// reasons, which every chain's adapter checks. A harness decodes its
/// adapter's error into one; an error it cannot decode is not a refusal but a
/// failure of the harness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The adapter is paused.
    Paused,
    /// A resource's logic ref is on the adapter's denylist for its side:
    /// the one for consumed resources or the one for created resources.
    DeniedLogicRef,
    /// A consumed resource names a commitment tree root the adapter never
    /// stored.
    UnknownRoot,
    /// A consumed resource's nullifier is already spent.
    NullifierSpent,
    /// The transaction's kind-table commitment is neither the one the adapter
    /// stores nor the empty table's.
    UnacceptedKindTableCommitment,
    /// The aggregation proof does not prove the transaction's actions under
    /// the adapter's compliance key and the transaction's kind-table
    /// commitment.
    InvalidAggregationProof,
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

/// A logic ref to add to one of a protocol adapter's denylists: the one for
/// consumed resources when `consumed`, else the one for created resources,
/// as both chains' adapters take it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DeniedLogicRef {
    pub logic_ref: Digest,
    pub consumed: bool,
}

/// A protocol adapter on one chain, with a prover for it.
#[allow(async_fn_in_trait)]
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
    async fn external_call(&mut self, call: ExternalCall) -> anyhow::Result<Vec<u32>>;
}

/// Protocol adapter abstraction.
#[allow(async_fn_in_trait)]
pub trait ProtocolAdapter {
    /// Asks the adapter to settle `transaction`, adding its commitments and
    /// nullifiers to the commitment tree and nullifier set. A refusal is an
    /// [`Outcome`]; an error means the harness could not ask, or could not
    /// decode the adapter's answer.
    async fn settle(&mut self, transaction: Transaction) -> anyhow::Result<Outcome>;

    /// The commitment tree as the adapter stores it now, read from the
    /// chain: its commitment count and sides.
    async fn commitment_tree(&self) -> anyhow::Result<FrontierCommitmentTree>;

    /// The latest commitment tree root the adapter stores, read from the
    /// chain.
    async fn latest_root(&self) -> anyhow::Result<Digest>;

    /// As the adapter's owner, makes `commitment` the kind-table commitment
    /// it stores: from then on a transaction settles when proven against that
    /// table or against the empty one.
    async fn set_kind_table_commitment(&mut self, commitment: Digest) -> anyhow::Result<()>;

    /// As the adapter's owner, pauses settlement.
    async fn pause(&mut self) -> anyhow::Result<()>;

    /// As the adapter's owner, resumes settlement.
    async fn unpause(&mut self) -> anyhow::Result<()>;

    /// As the adapter's owner, adds each of `logic_refs` to its denylist:
    /// the adapter refuses any transaction consuming a resource whose logic
    /// ref is on the one for consumed resources, or creating one whose logic
    /// ref is on the one for created resources. The adapter refuses the zero
    /// logic ref and one already on its denylist.
    async fn deny_logic_refs(&mut self, logic_refs: &[DeniedLogicRef]) -> anyhow::Result<()>;
}

/// Transaction prover.
#[allow(async_fn_in_trait)]
pub trait Prover {
    /// Prove an ARM transaction.
    ///
    /// Invalid witnesses will result in an error.
    async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<Transaction>;
}

use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::merkle_path::MerklePath;
use anyhow::Context;

use crate::witness::ActionWitnesses;

/// Transaction produced by the harness prover and consumed by the harness
/// protocol adapter.
pub trait Transaction {
    /// Commitments created by successful execution of this transaction.
    fn created_commitments(&self) -> anyhow::Result<impl Iterator<Item = Digest> + '_>;
}

/// Environment used to test the protocol adapter.
pub trait Environment {
    /// ARM transaction.
    type Transaction: Transaction;

    /// Protocol adapter.
    type ProtocolAdapter: ProtocolAdapter<Transaction = Self::Transaction>;

    /// Transaction prover.
    type Prover: Prover<Transaction = Self::Transaction>;

    /// Get a reference to the tx prover.
    fn prover(&self) -> &Self::Prover;

    /// Get a reference to the state.
    fn state(&self) -> &State;

    /// Get a mut reference to the state.
    fn state_mut(&mut self) -> &mut State;

    /// Get a reference to the protocol adapter.
    fn protocol_adapter(&self) -> &Self::ProtocolAdapter;

    /// Get a mut reference to the protocol adapter.
    fn protocol_adapter_mut(&mut self) -> &mut Self::ProtocolAdapter;
}

/// Protocol adapter abstraction.
pub trait ProtocolAdapter {
    /// ARM transaction.
    type Transaction: Transaction;

    /// Commitment tree.
    type CommitmentTree: CommitmentTree;

    /// Executes a transaction by adding the commitments and nullifiers
    /// to the commitment tree and nullifier set, respectively.
    #[allow(async_fn_in_trait)]
    async fn execute(&mut self, transaction: Self::Transaction) -> anyhow::Result<()>;

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
    /// ARM transaction.
    type Transaction: Transaction;

    /// Prove an ARM transaction.
    ///
    /// Invalid witnesses will result in an error.
    #[allow(async_fn_in_trait)]
    async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<Self::Transaction>;
}

/// Builder of a [`State`] instance.
#[derive(Default, Debug)]
pub struct StateBuilder {
    inner: HashMap<Cow<'static, str>, Box<dyn Any>>,
}

impl StateBuilder {
    /// Create a new [`State`].
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores the given data at the specified key.
    #[inline]
    pub fn insert<K, V>(&mut self, key: K, data: V)
    where
        K: Into<Cow<'static, str>>,
        V: Any,
    {
        self.inner.insert(key.into(), Box::new(data));
    }

    /// Get a reference to a [`State`].
    #[inline]
    pub fn as_state(&self) -> &State {
        // SAFETY: `State` and `StateBuilder` are essentially the same type
        // (same layout and elems)
        unsafe { &*(self as *const _ as *const _) }
    }

    /// Get a mut reference to a [`State`].
    #[inline]
    pub fn as_state_mut(&mut self) -> &mut State {
        // SAFETY: `State` and `StateBuilder` are essentially the same type
        // (same layout and elems)
        unsafe { &mut *(self as *mut _ as *mut _) }
    }

    /// Finalize the [`State`].
    #[inline]
    pub fn finalize(self) -> State {
        State { inner: self.inner }
    }
}

/// Storage of arbitrary state.
///
/// The primary use case is to store data during the set-up phase of a
/// test. Different environments will have different strategies to init
/// that data, but ultimately, the test logic will expect it to be present.
///
/// For instance, USDC is already deployed on Sepolia, but it will not be
/// deployed on a local Anvil node. In a Sepolia environment, we just store
/// the canonical USDC address. In a local Anvil environment, we need to
/// deploy USDC, then store the address we got.
#[derive(Debug)]
pub struct State {
    inner: HashMap<Cow<'static, str>, Box<dyn Any>>,
}

impl State {
    /// Get a ref to the data stored at the specified key.
    #[inline]
    pub fn get<V>(&self, key: &str) -> anyhow::Result<&V>
    where
        V: Any,
    {
        let value = self
            .inner
            .get(key)
            .with_context(|| format!("the key {key} is not present in the state"))?;

        value.downcast_ref().with_context(|| {
            format!(
                "the key {key} is not of type {}",
                std::any::type_name::<V>()
            )
        })
    }

    /// Get a mut ref to the data stored at the specified key.
    #[inline]
    pub fn get_mut<V>(&mut self, key: &str) -> anyhow::Result<&mut V>
    where
        V: Any,
    {
        let value = self
            .inner
            .get_mut(key)
            .with_context(|| format!("the key {key} is not present in the state"))?;

        value.downcast_mut().with_context(|| {
            format!(
                "the key {key} is not of type {}",
                std::any::type_name::<V>()
            )
        })
    }

    /// Remove and return the data stored at the specified key.
    #[inline]
    pub fn remove<V>(&mut self, key: &str) -> anyhow::Result<Box<V>>
    where
        V: Any,
    {
        let value = self
            .inner
            .remove(key)
            .with_context(|| format!("the key {key} is not present in the state"))?;

        value.downcast().map_err(|_| {
            anyhow::anyhow!(
                "the key {key} is not of type {}",
                std::any::type_name::<V>()
            )
        })
    }
}

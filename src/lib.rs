pub mod assert;
pub mod commitment_tree;
pub mod environment;
#[cfg(feature = "fixtures")]
pub mod fixtures;
#[cfg(any(feature = "local", feature = "e2e", feature = "prove"))]
pub mod prover;
#[cfg(feature = "fixtures")]
pub mod suite;
pub mod transaction;
pub mod witness;

use anoma_rm_risc0::Digest;
use anyhow::Context;

use self::environment::{Environment, Outcome, ProtocolAdapter, Prover};
use self::transaction::Transaction;
use self::witness::ActionWitnesses;

pub async fn prove_actions<Env: Environment>(
    env: &Env,
    actions: &[ActionWitnesses],
) -> anyhow::Result<Transaction> {
    env.prover()
        .prove(actions)
        .await
        .context("failed to prove action witnesses")
}

/// Asks the protocol adapter to settle `tx`, and returns what it did.
pub async fn settle_tx<Env: Environment>(
    env: &mut Env,
    tx: Transaction,
) -> anyhow::Result<Outcome> {
    env.protocol_adapter_mut()
        .settle(tx)
        .await
        .context("failed to ask the protocol adapter to settle the transaction")
}

/// Settles `tx`, which the protocol adapter must not refuse.
pub async fn execute_tx<Env: Environment>(env: &mut Env, tx: Transaction) -> anyhow::Result<()> {
    match settle_tx(env, tx).await? {
        Outcome::Settled => Ok(()),
        Outcome::Refused(refusal) => {
            anyhow::bail!("the protocol adapter refused the transaction: {refusal:?}")
        }
    }
}

/// The latest commitment tree root the protocol adapter stores.
pub async fn latest_root<Env: Environment>(env: &Env) -> anyhow::Result<Digest> {
    env.protocol_adapter()
        .latest_root()
        .await
        .context("failed to read the protocol adapter's latest root")
}

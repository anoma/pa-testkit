pub mod assert;
pub mod environment;
#[cfg(feature = "fixtures")]
pub mod fixtures;
#[cfg(feature = "mocks")]
pub mod mocks;
#[cfg(any(feature = "local", feature = "e2e"))]
pub mod prover;
pub mod transaction;
pub mod witness;

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::merkle_path::MerklePath;
use anyhow::Context;

use self::environment::{CommitmentTree, Environment, ProtocolAdapter, Prover};
use self::witness::ActionWitnesses;

pub async fn prove_actions<'a, Env: Environment>(
    env: &Env,
    actions: impl IntoIterator<Item = &'a ActionWitnesses>,
) -> anyhow::Result<Env::Transaction> {
    let actions: Vec<&ActionWitnesses> = actions.into_iter().collect();
    env.prover()
        .prove(&actions)
        .await
        .context("failed to prove action witnesses")
}

pub async fn execute_tx<Env: Environment>(
    env: &mut Env,
    tx: Env::Transaction,
) -> anyhow::Result<()> {
    env.protocol_adapter_mut()
        .execute(tx)
        .await
        .context("failed to execute transaction on protocol adapter")
}

pub fn commitment_root<Env: Environment>(env: &Env) -> anyhow::Result<Digest> {
    env.protocol_adapter()
        .commitment_tree()
        .root()
        .context("failed to compute commitment tree root")
}

/// The path of `commitment` in the commitment tree of the protocol adapter.
pub fn commitment_path<Env: Environment>(
    env: &Env,
    commitment: Digest,
) -> anyhow::Result<MerklePath> {
    env.protocol_adapter()
        .commitment_tree()
        .path_to(commitment)
        .with_context(|| format!("failed to compute the path of commitment {commitment}"))
}

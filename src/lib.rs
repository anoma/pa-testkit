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
use anyhow::Context;

use self::environment::{CommitmentTree, Environment, ProtocolAdapter, Prover};
use self::witness::ActionWitnesses;

pub async fn prove_actions<Env: Environment>(
    env: &Env,
    actions: &[ActionWitnesses],
) -> anyhow::Result<Env::Transaction> {
    env.prover()
        .prove(actions)
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

//! Chain-agnostic integration tests. Each one proves and settles, or refuses,
//! one kind of transaction through any [`Environment`]; a chain's harness runs
//! them against each of its environments. Only the error a chain reports for a
//! refused settlement is chain-specific, so the test that expects one takes it
//! as a [`Needle`].

use anyhow::Context;

use crate::assert::{Needle, expect_integration_panic};
use crate::environment::Environment;
use crate::fixtures::trivial;
use crate::transaction::Transaction;
use crate::{commitment_root, execute_tx, prove_actions};

/// The nonce of the trivial transaction's consumed resource, outside the
/// `[seed; 32]` default that arm-risc0's transaction generator also produces,
/// so a fork of a live chain does not start with its nullifier spent.
const TRIVIAL_NONCE: [u8; 32] = *b"anoma pa-testkit suite trivial 1";

/// A transaction of one trivial action settles and moves the root.
pub async fn settles_a_trivial_transaction<Env: Environment>(env: &mut Env) -> anyhow::Result<()> {
    let action = trivial::build(
        1,
        trivial::Overrides {
            consumed_nonce: Some(TRIVIAL_NONCE),
            ..trivial::Overrides::default()
        },
    )
    .context("failed to build trivial action")?
    .witnesses;
    settles_and_moves_the_root(env, vec![action]).await
}

/// An action consuming two resources and creating three settles and moves the
/// root.
pub async fn settles_an_n_to_m_transaction<Env: Environment>(env: &mut Env) -> anyhow::Result<()> {
    let action = trivial::build(
        21,
        trivial::Overrides {
            consumed_count: Some(2),
            created_count: Some(3),
            ..trivial::Overrides::default()
        },
    )
    .context("failed to build an n:m trivial action")?
    .witnesses;
    settles_and_moves_the_root(env, vec![action]).await
}

/// A transaction of three actions settles and moves the root.
pub async fn settles_a_multi_action_transaction<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let actions = trivial::build_many(3, 31).context("failed to build trivial actions")?;
    settles_and_moves_the_root(env, actions).await
}

/// Two consume-only transactions settle one after the other and leave the
/// root where it was: they create nothing, so they add no root.
pub async fn settles_consume_only_transactions_without_a_root_change<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let before = commitment_root(env)?;
    for seed in [41, 42] {
        let action = trivial::build(
            seed,
            trivial::Overrides {
                consumed_count: Some(2),
                created_count: Some(0),
                ..trivial::Overrides::default()
            },
        )
        .context("failed to build a consume-only trivial action")?
        .witnesses;
        let tx = prove_actions(env, &[action]).await?;
        execute_tx(env, tx).await?;
        anyhow::ensure!(
            commitment_root(env)? == before,
            "consume-only transaction {seed} must leave the commitment tree root unchanged"
        );
    }
    Ok(())
}

/// The prover refuses a padding resource of nonzero quantity.
pub async fn proving_refuses_a_nonzero_quantity<Env: Environment>(env: &Env) -> anyhow::Result<()> {
    let bad = trivial::build(7, trivial::Overrides::invalid_nonzero_quantity())
        .context("failed to build invalid trivial action")?;
    expect_integration_panic(Needle::Static("Invalid padding resource"))(
        prove_actions(env, &[bad.witnesses]).await,
    )
}

/// The prover refuses a consumed padding resource that is not ephemeral.
pub async fn proving_refuses_a_non_ephemeral_consumed_resource<Env: Environment>(
    env: &Env,
) -> anyhow::Result<()> {
    let bad = trivial::build(8, trivial::Overrides::invalid_consumed_non_ephemeral())
        .context("failed to build invalid trivial action")?;
    expect_integration_panic(Needle::Static("Invalid padding resource"))(
        prove_actions(env, &[bad.witnesses]).await,
    )
}

/// The protocol adapter refuses a transaction whose aggregation seal was
/// tampered with, with the error `refusal` finds.
pub async fn settlement_refuses_a_tampered_aggregation_seal<Env>(
    env: &mut Env,
    refusal: Needle,
) -> anyhow::Result<()>
where
    Env: Environment<Transaction = Transaction>,
{
    let actions = trivial::build_many(1, 11).context("failed to build trivial actions")?;
    let mut tx = prove_actions(env, &actions)
        .await
        .context("valid witnesses should prove before tampering")?;
    tx.tamper_aggregation_seal()
        .context("failed to tamper the aggregation seal")?;
    expect_integration_panic(refusal)(execute_tx(env, tx).await)
}

#[cfg(all(test, feature = "local"))]
mod tests {
    use std::collections::HashSet;

    use anoma_rm_risc0::Digest;
    use anoma_rm_risc0::merkle_path::PADDING_LEAF;

    use super::*;
    use crate::commitment_tree::FrontierCommitmentTree;
    use crate::environment::{CommitmentTree, ProtocolAdapter, State, StateBuilder};
    use crate::prover::{LocalProver, mock_aggregation_seal};

    /// A protocol adapter in memory, checking what both chains' adapters
    /// check: the local prover's seal, the consumed roots and the nullifiers.
    struct InMemoryAdapter {
        tree: FrontierCommitmentTree,
        roots: HashSet<Digest>,
        nullifiers: HashSet<Digest>,
    }

    impl ProtocolAdapter for InMemoryAdapter {
        type Transaction = Transaction;
        type CommitmentTree = FrontierCommitmentTree;

        async fn execute(&mut self, transaction: Transaction) -> anyhow::Result<()> {
            let aggregation = transaction
                .into_arm()
                .aggregation
                .context("the transaction carries no aggregation")?;
            anyhow::ensure!(
                aggregation.proof == mock_aggregation_seal(&aggregation.instance)?,
                "the aggregation seal does not verify"
            );
            let consumed = aggregation
                .instance
                .actions
                .iter()
                .flat_map(|a| &a.consumed_publics);
            let mut spent = HashSet::new();
            for c in consumed {
                anyhow::ensure!(
                    c.commitment_tree_root == PADDING_LEAF
                        || self.roots.contains(&c.commitment_tree_root),
                    "unknown root {}",
                    c.commitment_tree_root
                );
                anyhow::ensure!(
                    !self.nullifiers.contains(&c.resource_nullifier)
                        && spent.insert(c.resource_nullifier),
                    "nullifier {} is spent",
                    c.resource_nullifier
                );
            }
            self.nullifiers.extend(spent);
            let created: Vec<Digest> = aggregation
                .instance
                .actions
                .iter()
                .flat_map(|a| a.created_publics.iter().map(|c| c.resource_commitment))
                .collect();
            if !created.is_empty() {
                self.tree.add(created);
                self.roots.insert(self.tree.root()?);
            }
            Ok(())
        }

        fn commitment_tree(&self) -> &FrontierCommitmentTree {
            &self.tree
        }
    }

    struct InMemoryEnvironment {
        state: State,
        prover: LocalProver,
        adapter: InMemoryAdapter,
    }

    impl InMemoryEnvironment {
        fn new() -> Self {
            Self {
                state: StateBuilder::new().finalize(),
                prover: LocalProver,
                adapter: InMemoryAdapter {
                    tree: FrontierCommitmentTree::new(0, Vec::new()).unwrap(),
                    roots: HashSet::new(),
                    nullifiers: HashSet::new(),
                },
            }
        }
    }

    impl Environment for InMemoryEnvironment {
        type Transaction = Transaction;
        type ProtocolAdapter = InMemoryAdapter;
        type Prover = LocalProver;

        fn prover(&self) -> &LocalProver {
            &self.prover
        }
        fn state(&self) -> &State {
            &self.state
        }
        fn state_mut(&mut self) -> &mut State {
            &mut self.state
        }
        fn protocol_adapter(&self) -> &InMemoryAdapter {
            &self.adapter
        }
        fn protocol_adapter_mut(&mut self) -> &mut InMemoryAdapter {
            &mut self.adapter
        }
    }

    #[tokio::test]
    async fn the_suite_passes_on_an_adapter_in_memory() {
        settles_a_trivial_transaction(&mut InMemoryEnvironment::new())
            .await
            .unwrap();
        settles_an_n_to_m_transaction(&mut InMemoryEnvironment::new())
            .await
            .unwrap();
        settles_a_multi_action_transaction(&mut InMemoryEnvironment::new())
            .await
            .unwrap();
        settles_consume_only_transactions_without_a_root_change(&mut InMemoryEnvironment::new())
            .await
            .unwrap();
        proving_refuses_a_nonzero_quantity(&InMemoryEnvironment::new())
            .await
            .unwrap();
        proving_refuses_a_non_ephemeral_consumed_resource(&InMemoryEnvironment::new())
            .await
            .unwrap();
        settlement_refuses_a_tampered_aggregation_seal(
            &mut InMemoryEnvironment::new(),
            Needle::Static("the aggregation seal does not verify"),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_settled_transaction_cannot_settle_again() {
        let mut env = InMemoryEnvironment::new();
        let actions = trivial::build_many(1, 51).unwrap();
        let tx = prove_actions(&env, &actions).await.unwrap();
        execute_tx(&mut env, Transaction::from_arm(tx.as_arm().clone()))
            .await
            .unwrap();
        expect_integration_panic(Needle::Regexp(
            regex::Regex::new("nullifier .* is spent").unwrap(),
        ))(execute_tx(&mut env, tx).await)
        .unwrap();
    }
}

async fn settles_and_moves_the_root<Env: Environment>(
    env: &mut Env,
    actions: Vec<crate::witness::ActionWitnesses>,
) -> anyhow::Result<()> {
    let before = commitment_root(env)?;
    let tx = prove_actions(env, &actions).await?;
    execute_tx(env, tx).await?;
    anyhow::ensure!(
        commitment_root(env)? != before,
        "the commitment tree root must change"
    );
    Ok(())
}

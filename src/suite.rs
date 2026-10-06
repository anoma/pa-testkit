//! Chain-agnostic integration tests. Each one proves and settles, or refuses,
//! one kind of transaction through any [`Environment`]; a chain's harness runs
//! them against each of its environments. The error a chain reports for a
//! refused settlement is chain-specific, so a test that expects one takes it
//! as a [`Needle`]; so is the encoding of an external call, which an
//! environment supplies as a [`BlockTimeForwarder`].

use anyhow::Context;

use crate::assert::{Needle, expect_integration_panic};
use crate::environment::Environment;
use crate::fixtures::{passthrough, trivial};
use crate::transaction::Transaction;
use crate::{commitment_root, execute_tx, prove_actions};

/// The nonce of the trivial transaction's consumed resource, outside the
/// `[seed; 32]` default that arm-risc0's transaction generator also produces,
/// so a fork of a live chain does not start with its nullifier spent.
const TRIVIAL_NONCE: [u8; 32] = *b"anoma pa-testkit suite trivial 1";

/// A transaction of one trivial action settles and moves the root.
pub async fn settles_a_trivial_transaction<Env: Environment>(env: &mut Env) -> anyhow::Result<()> {
    settles_one_action(
        env,
        1,
        trivial::Overrides {
            consumed_nonce: Some(TRIVIAL_NONCE),
            ..trivial::Overrides::default()
        },
    )
    .await
}

/// An action consuming two resources and creating three settles and moves the
/// root.
pub async fn settles_an_n_to_m_transaction<Env: Environment>(env: &mut Env) -> anyhow::Result<()> {
    settles_one_action(
        env,
        21,
        trivial::Overrides {
            consumed_count: Some(2),
            created_count: Some(3),
            ..trivial::Overrides::default()
        },
    )
    .await
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
    proving_refuses_an_invalid_padding_resource(
        env,
        7,
        trivial::Overrides::invalid_nonzero_quantity(),
    )
    .await
}

/// The prover refuses a consumed padding resource that is not ephemeral.
pub async fn proving_refuses_a_non_ephemeral_consumed_resource<Env: Environment>(
    env: &Env,
) -> anyhow::Result<()> {
    proving_refuses_an_invalid_padding_resource(
        env,
        8,
        trivial::Overrides::invalid_consumed_non_ephemeral(),
    )
    .await
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

/// An environment whose protocol adapter can call its chain's example
/// block-time forwarder.
pub trait BlockTimeForwarder: Environment {
    /// The external payload blob of a call to the block-time forwarder asking
    /// how `time`, in seconds since the Unix epoch, compares with the block's,
    /// expecting `expected`. The forwarder is ready for the call once this
    /// returns.
    #[allow(async_fn_in_trait)]
    async fn block_time_call(
        &mut self,
        time: u32,
        expected: TimeComparison,
    ) -> anyhow::Result<Vec<u32>>;
}

/// A transaction whose external call expects what the forwarder returns
/// settles: time 0 is before every block.
pub async fn settles_an_external_call_whose_output_matches<Env: BlockTimeForwarder>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let call = env.block_time_call(0, TimeComparison::Before).await?;
    let tx = prove_external_call(env, 61, call).await?;
    execute_tx(env, tx).await
}

/// The protocol adapter refuses a transaction whose external call expects
/// other than what the forwarder returns, with the error `refusal` finds.
pub async fn settlement_refuses_an_external_call_whose_output_differs<Env: BlockTimeForwarder>(
    env: &mut Env,
    refusal: Needle,
) -> anyhow::Result<()> {
    let call = env.block_time_call(0, TimeComparison::After).await?;
    let tx = prove_external_call(env, 62, call).await?;
    expect_integration_panic(refusal)(execute_tx(env, tx).await)
}

#[cfg(all(test, feature = "local"))]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use anoma_rm_risc0::Digest;
    use anoma_rm_risc0::proving_system::JournalEncoding;

    use super::*;
    use crate::commitment_tree::FrontierCommitmentTree;
    use crate::environment::{ProtocolAdapter, State, StateBuilder, Transaction as _};
    use crate::prover::{LocalProver, mock_aggregation_seal};

    /// A protocol adapter in memory that checks what the suite expects it to
    /// refuse: the local prover's seal, over the aggregation journal in
    /// `encoding`, and each external call's output. Its one forwarder is a
    /// block-time forwarder whose calls are `[time, expected output]`.
    struct InMemoryAdapter {
        tree: FrontierCommitmentTree,
        encoding: JournalEncoding,
    }

    impl ProtocolAdapter for InMemoryAdapter {
        type Transaction = Transaction;
        type CommitmentTree = FrontierCommitmentTree;

        async fn execute(&mut self, transaction: Transaction) -> anyhow::Result<()> {
            let created: Vec<Digest> = transaction.created_commitments()?.collect();
            let aggregation = transaction
                .into_arm()
                .aggregation
                .context("the transaction carries no aggregation")?;
            anyhow::ensure!(
                aggregation.proof == mock_aggregation_seal(&aggregation.instance, self.encoding),
                "the aggregation seal does not verify"
            );
            for action in &aggregation.instance.actions {
                let consumed = action.consumed_publics.iter().map(|c| &c.app_data);
                let created = action.created_publics.iter().map(|c| &c.app_data);
                for call in consumed.chain(created).flat_map(|a| &a.external_payload) {
                    call_block_time_forwarder(&call.blob)?;
                }
            }
            self.tree.add(created);
            Ok(())
        }

        fn commitment_tree(&self) -> &FrontierCommitmentTree {
            &self.tree
        }
    }

    fn call_block_time_forwarder(call: &[u32]) -> anyhow::Result<()> {
        let [time, expected] = call else {
            anyhow::bail!("an external call is [time, expected output], not {call:?}");
        };
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let output = match u64::from(*time).cmp(&now) {
            std::cmp::Ordering::Less => TimeComparison::Before,
            std::cmp::Ordering::Equal => TimeComparison::At,
            std::cmp::Ordering::Greater => TimeComparison::After,
        };
        anyhow::ensure!(
            output as u32 == *expected,
            "the external call returned {output:?}, not {expected}"
        );
        Ok(())
    }

    struct InMemoryEnvironment {
        state: State,
        prover: LocalProver,
        adapter: InMemoryAdapter,
    }

    impl InMemoryEnvironment {
        /// An adapter verifying seals over the aggregation journal in
        /// `encoding`, with a local prover that seals it in `encoding`.
        fn new(encoding: JournalEncoding) -> Self {
            Self {
                state: StateBuilder::new().finalize(),
                prover: LocalProver::new(encoding),
                adapter: InMemoryAdapter {
                    tree: FrontierCommitmentTree::new(0, Vec::new()).unwrap(),
                    encoding,
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

    impl BlockTimeForwarder for InMemoryEnvironment {
        async fn block_time_call(
            &mut self,
            time: u32,
            expected: TimeComparison,
        ) -> anyhow::Result<Vec<u32>> {
            Ok(vec![time, expected as u32])
        }
    }

    /// The whole suite against the adapter in memory, for each journal
    /// encoding.
    macro_rules! in_memory_suite {
        ($module:ident, $encoding:expr) => {
            mod $module {
                use super::*;

                crate::suite_tests!(
                    async { anyhow::Ok(InMemoryEnvironment::new($encoding)) },
                    refusal = Needle::Static("the aggregation seal does not verify"),
                    output_mismatch = Needle::Static("the external call returned Before, not 2"),
                );
            }
        };
    }

    in_memory_suite!(in_memory_risc0_serde, JournalEncoding::Risc0Serde);
    in_memory_suite!(in_memory_abi, JournalEncoding::Abi);

    /// A seal over the journal in one encoding claims a different circuit and
    /// journal than the other's, so an adapter verifying the other refuses it.
    #[tokio::test]
    async fn settlement_refuses_a_seal_in_the_other_encoding() -> anyhow::Result<()> {
        for (proved, verified) in [
            (JournalEncoding::Abi, JournalEncoding::Risc0Serde),
            (JournalEncoding::Risc0Serde, JournalEncoding::Abi),
        ] {
            let mut env = InMemoryEnvironment::new(proved);
            env.adapter.encoding = verified;
            let actions = trivial::build_many(1, 71).context("failed to build trivial actions")?;
            let tx = prove_actions(&env, &actions).await?;
            expect_integration_panic(Needle::Static("the aggregation seal does not verify"))(
                execute_tx(&mut env, tx).await,
            )
            .with_context(|| format!("proved in {proved:?}, verified in {verified:?}"))?;
        }
        Ok(())
    }
}

async fn settles_one_action<Env: Environment>(
    env: &mut Env,
    seed: u8,
    overrides: trivial::Overrides,
) -> anyhow::Result<()> {
    let action = trivial::build(seed, overrides)
        .with_context(|| format!("failed to build trivial action {seed}"))?
        .witnesses;
    settles_and_moves_the_root(env, vec![action]).await
}

async fn proving_refuses_an_invalid_padding_resource<Env: Environment>(
    env: &Env,
    seed: u8,
    overrides: trivial::Overrides,
) -> anyhow::Result<()> {
    let bad = trivial::build(seed, overrides).context("failed to build invalid trivial action")?;
    expect_integration_panic(Needle::Static("Invalid padding resource"))(
        prove_actions(env, &[bad.witnesses]).await,
    )
}

/// Proves a pass-through action, with nonces from `seed`, whose consumed
/// resource makes the external call `call`.
async fn prove_external_call<Env: Environment>(
    env: &Env,
    seed: u8,
    call: Vec<u32>,
) -> anyhow::Result<Env::Transaction> {
    let action = passthrough::build(seed, vec![call], passthrough::Overrides::default())
        .context("failed to build a pass-through action")?
        .witnesses;
    prove_actions(env, &[action]).await
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

/// Emits one test per suite function against the environment `$setup`
/// builds (an expression evaluating to `anyhow::Result<Env>`), each test
/// with an environment of its own. Invoke it inside a module per
/// environment; the tests run on tokio's multi-thread runtime, so the crate
/// depends on `tokio` with `macros` and `rt-multi-thread`.
///
/// With a `refusal` (the [`Needle`] the chain's error for a tampered
/// aggregation seal matches) and an `output_mismatch` (the one its error for
/// an external call's differing output matches), every suite test, for an
/// environment that is a [`BlockTimeForwarder`]; without them, the three that
/// settle a transaction, which pa-evm also runs against a fork of a live
/// chain, where each proof is a proving-queue job.
///
/// ```ignore
/// mod local {
///     anoma_pa_testkit::suite_tests!(
///         Env::setup_bare(),
///         refusal = Needle::Static("..."),
///         output_mismatch = Needle::Static("..."),
///     );
/// }
/// mod e2e_test {
///     anoma_pa_testkit::suite_tests!(E2eEnv::setup_bare());
/// }
/// ```
#[macro_export]
macro_rules! suite_tests {
    ($setup:expr, refusal = $refusal:expr, output_mismatch = $output_mismatch:expr $(,)?) => {
        $crate::suite_tests!($setup);

        #[tokio::test(flavor = "multi_thread")]
        async fn settles_consume_only_transactions_without_a_root_change() -> ::anyhow::Result<()> {
            $crate::suite::settles_consume_only_transactions_without_a_root_change(
                &mut $setup.await?,
            )
            .await
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn proving_refuses_a_nonzero_quantity() -> ::anyhow::Result<()> {
            $crate::suite::proving_refuses_a_nonzero_quantity(&$setup.await?).await
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn proving_refuses_a_non_ephemeral_consumed_resource() -> ::anyhow::Result<()> {
            $crate::suite::proving_refuses_a_non_ephemeral_consumed_resource(&$setup.await?).await
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn settlement_refuses_a_tampered_aggregation_seal() -> ::anyhow::Result<()> {
            $crate::suite::settlement_refuses_a_tampered_aggregation_seal(
                &mut $setup.await?,
                $refusal,
            )
            .await
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn settles_an_external_call_whose_output_matches() -> ::anyhow::Result<()> {
            $crate::suite::settles_an_external_call_whose_output_matches(&mut $setup.await?).await
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn settlement_refuses_an_external_call_whose_output_differs() -> ::anyhow::Result<()>
        {
            $crate::suite::settlement_refuses_an_external_call_whose_output_differs(
                &mut $setup.await?,
                $output_mismatch,
            )
            .await
        }
    };
    ($setup:expr $(,)?) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn settles_a_trivial_transaction() -> ::anyhow::Result<()> {
            $crate::suite::settles_a_trivial_transaction(&mut $setup.await?).await
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn settles_an_n_to_m_transaction() -> ::anyhow::Result<()> {
            $crate::suite::settles_an_n_to_m_transaction(&mut $setup.await?).await
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn settles_a_multi_action_transaction() -> ::anyhow::Result<()> {
            $crate::suite::settles_a_multi_action_transaction(&mut $setup.await?).await
        }
    };
}

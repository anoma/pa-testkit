//! Chain-agnostic integration tests. Each one proves and settles, or refuses,
//! one kind of transaction through any [`Environment`]; a chain's harness runs
//! them against each of its environments. A test takes nothing but the
//! environment: it names refusals and external calls in the
//! [`crate::environment`] vocabulary, which the harness translates for its
//! chain, so every chain runs the same test.

use anyhow::Context;

use crate::assert::{Needle, expect_integration_panic};
use crate::environment::{Environment, ExternalCall, Outcome, Refusal, TimeComparison};
use crate::fixtures::{passthrough, trivial};
use crate::transaction::Transaction;
use crate::{commitment_root, execute_tx, prove_actions, settle_tx};

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
/// tampered with.
pub async fn settlement_refuses_a_tampered_aggregation_seal<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let actions = trivial::build_many(1, 11).context("failed to build trivial actions")?;
    let mut tx = prove_actions(env, &actions)
        .await
        .context("valid witnesses should prove before tampering")?;
    tx.tamper_aggregation_seal()
        .context("failed to tamper the aggregation seal")?;
    refuses(env, tx, Refusal::InvalidAggregationSeal).await
}

/// A transaction whose external call expects what the forwarder returns
/// settles: time 0 is before every block.
pub async fn settles_an_external_call_whose_output_matches<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let tx = prove_external_call(env, 61, block_time_call(TimeComparison::Before)).await?;
    execute_tx(env, tx).await
}

/// The protocol adapter refuses a transaction whose external call expects
/// other than what the forwarder returns.
pub async fn settlement_refuses_an_external_call_whose_output_differs<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let tx = prove_external_call(env, 62, block_time_call(TimeComparison::After)).await?;
    refuses(env, tx, Refusal::ExternalCallOutputMismatch).await
}

#[cfg(all(test, feature = "local"))]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use anoma_rm_risc0::Digest;
    use anoma_rm_risc0::proving_system::JournalEncoding;

    use super::*;
    use crate::commitment_tree::FrontierCommitmentTree;
    use crate::environment::ProtocolAdapter;
    use crate::prover::{LocalProver, mock_aggregation_seal};

    /// A protocol adapter in memory that checks what the suite expects it to
    /// refuse: the local prover's seal, over the aggregation journal in
    /// `encoding`, and each external call's output. Its one forwarder is a
    /// block-time forwarder whose calls are `[time, expected output]`; it
    /// adds commitments only when it settles.
    struct InMemoryAdapter {
        tree: FrontierCommitmentTree,
        encoding: JournalEncoding,
    }

    impl ProtocolAdapter for InMemoryAdapter {
        type CommitmentTree = FrontierCommitmentTree;

        async fn settle(&mut self, transaction: Transaction) -> anyhow::Result<Outcome> {
            let created: Vec<Digest> = transaction.created_commitments()?.collect();
            let aggregation = transaction
                .into_arm()
                .aggregation
                .context("the transaction carries no aggregation")?;
            if aggregation.proof != mock_aggregation_seal(&aggregation.instance, self.encoding) {
                return Ok(Outcome::Refused(Refusal::InvalidAggregationSeal));
            }
            for action in &aggregation.instance.actions {
                let consumed = action.consumed_publics.iter().map(|c| &c.app_data);
                let created = action.created_publics.iter().map(|c| &c.app_data);
                for call in consumed.chain(created).flat_map(|a| &a.external_payload) {
                    if !call_block_time_forwarder(&call.blob)? {
                        return Ok(Outcome::Refused(Refusal::ExternalCallOutputMismatch));
                    }
                }
            }
            self.tree.add(created);
            Ok(Outcome::Settled)
        }

        fn commitment_tree(&self) -> &FrontierCommitmentTree {
            &self.tree
        }
    }

    /// Whether the block-time forwarder returns the output `call` expects.
    fn call_block_time_forwarder(call: &[u32]) -> anyhow::Result<bool> {
        let [time, expected] = call else {
            anyhow::bail!("an external call is [time, expected output], not {call:?}");
        };
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let output = match u64::from(*time).cmp(&now) {
            std::cmp::Ordering::Less => TimeComparison::Before,
            std::cmp::Ordering::Equal => TimeComparison::At,
            std::cmp::Ordering::Greater => TimeComparison::After,
        };
        Ok(output as u32 == *expected)
    }

    struct InMemoryEnvironment {
        prover: LocalProver,
        adapter: InMemoryAdapter,
    }

    impl InMemoryEnvironment {
        /// An adapter verifying seals over the aggregation journal in
        /// `encoding`, with a local prover that seals it in `encoding`.
        fn new(encoding: JournalEncoding) -> Self {
            Self {
                prover: LocalProver::new(encoding),
                adapter: InMemoryAdapter {
                    tree: FrontierCommitmentTree::new(0, Vec::new()).unwrap(),
                    encoding,
                },
            }
        }
    }

    impl Environment for InMemoryEnvironment {
        type ProtocolAdapter = InMemoryAdapter;
        type Prover = LocalProver;

        fn prover(&self) -> &LocalProver {
            &self.prover
        }
        fn protocol_adapter(&self) -> &InMemoryAdapter {
            &self.adapter
        }
        fn protocol_adapter_mut(&mut self) -> &mut InMemoryAdapter {
            &mut self.adapter
        }
        async fn external_call(&mut self, call: ExternalCall) -> anyhow::Result<Vec<u32>> {
            let ExternalCall::BlockTime { time, expected } = call;
            Ok(vec![time, expected as u32])
        }
    }

    /// The whole suite against the adapter in memory, for each journal
    /// encoding.
    macro_rules! in_memory_suite {
        ($module:ident, $encoding:expr) => {
            mod $module {
                use super::*;

                crate::suite_tests!(async { anyhow::Ok(InMemoryEnvironment::new($encoding)) });
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
            refuses(&mut env, tx, Refusal::InvalidAggregationSeal)
                .await
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

/// A call asking the block-time forwarder about time 0, expecting `expected`.
fn block_time_call(expected: TimeComparison) -> ExternalCall {
    ExternalCall::BlockTime { time: 0, expected }
}

/// Proves a pass-through action, with nonces from `seed`, whose consumed
/// resource makes the external call `call`.
async fn prove_external_call<Env: Environment>(
    env: &mut Env,
    seed: u8,
    call: ExternalCall,
) -> anyhow::Result<Transaction> {
    let call = env.external_call(call).await?;
    let action = passthrough::build(seed, vec![call], passthrough::Overrides::default())
        .context("failed to build a pass-through action")?
        .witnesses;
    prove_actions(env, &[action]).await
}

/// Checks that the protocol adapter refuses `tx` for `refusal`, leaving the
/// root where it was.
async fn refuses<Env: Environment>(
    env: &mut Env,
    tx: Transaction,
    refusal: Refusal,
) -> anyhow::Result<()> {
    let before = commitment_root(env)?;
    let outcome = settle_tx(env, tx).await?;
    anyhow::ensure!(
        outcome == Outcome::Refused(refusal),
        "the protocol adapter must refuse the transaction ({refusal:?}), but returned {outcome:?}"
    );
    anyhow::ensure!(
        commitment_root(env)? == before,
        "a refused transaction must leave the commitment tree root unchanged"
    );
    Ok(())
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
/// `suite_tests!(setup)` emits every suite test. `suite_tests!(setup,
/// settling_only)` emits the three that settle a transaction, for an
/// environment where each proof is a proving-queue job, such as a fork of a
/// live chain.
///
/// ```ignore
/// mod local {
///     anoma_pa_testkit::suite_tests!(Env::setup_bare());
/// }
/// mod e2e_test {
///     anoma_pa_testkit::suite_tests!(E2eEnv::setup_bare(), settling_only);
/// }
/// ```
#[macro_export]
macro_rules! suite_tests {
    ($setup:expr, settling_only $(,)?) => {
        $crate::suite_tests!(@tests $setup;
            settles_a_trivial_transaction,
            settles_an_n_to_m_transaction,
            settles_a_multi_action_transaction,
        );
    };
    ($setup:expr $(,)?) => {
        $crate::suite_tests!($setup, settling_only);
        $crate::suite_tests!(@tests $setup;
            settles_consume_only_transactions_without_a_root_change,
            proving_refuses_a_nonzero_quantity,
            proving_refuses_a_non_ephemeral_consumed_resource,
            settlement_refuses_a_tampered_aggregation_seal,
            settles_an_external_call_whose_output_matches,
            settlement_refuses_an_external_call_whose_output_differs,
        );
    };
    (@tests $setup:expr; $($test:ident),* $(,)?) => {
        $(
            #[tokio::test(flavor = "multi_thread")]
            async fn $test() -> ::anyhow::Result<()> {
                $crate::suite::$test(&mut $setup.await?).await
            }
        )*
    };
}

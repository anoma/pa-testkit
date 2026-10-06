//! Chain-agnostic integration tests. Each one proves and settles, or refuses,
//! one kind of transaction through any [`Environment`]; a chain's harness runs
//! them against each of its environments. A test takes nothing but the
//! environment: it names refusals and external calls in the
//! [`crate::environment`] vocabulary, which the harness translates for its
//! chain, and checks the roots the chain stores, so every chain runs the same
//! test.

use anoma_rm_risc0::Digest;
use anyhow::Context;
use sha3::{Digest as _, Keccak256};

use crate::commitment_tree::FrontierCommitmentTree;
use crate::environment::{
    DELETION_CRITERION_NEVER, Environment, Event, ExternalCall, Outcome, PayloadKind,
    ProtocolAdapter, Refusal, TimeComparison,
};
use crate::fixtures::{passthrough, trivial};
use crate::transaction::Transaction;
use crate::witness::{ActionWitnesses, AppData};
use crate::{latest_root, prove_actions, settle_tx};

/// A transaction of one trivial action settles and adds its commitment.
pub async fn settles_a_trivial_transaction<Env: Environment>(env: &mut Env) -> anyhow::Result<()> {
    proves_and_settles(env, &[trivial_action(1, trivial::Overrides::default())?]).await
}

/// An action consuming two resources and creating three settles and adds its
/// commitments.
pub async fn settles_an_n_to_m_transaction<Env: Environment>(env: &mut Env) -> anyhow::Result<()> {
    let action = trivial_action(
        21,
        trivial::Overrides {
            consumed_count: Some(2),
            created_count: Some(3),
            ..trivial::Overrides::default()
        },
    )?;
    proves_and_settles(env, &[action]).await
}

/// A transaction of three actions settles and adds their commitments.
pub async fn settles_a_multi_action_transaction<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let actions = trivial::build_many(3, 31).context("failed to build trivial actions")?;
    proves_and_settles(env, &actions).await
}

/// Two consume-only transactions settle one after the other and leave the
/// root where it was: they create nothing, so they add no root.
pub async fn settles_consume_only_transactions_without_a_root_change<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    for seed in [41, 42] {
        let action = trivial_action(
            seed,
            trivial::Overrides {
                consumed_count: Some(2),
                created_count: Some(0),
                ..trivial::Overrides::default()
            },
        )?;
        proves_and_settles(env, &[action]).await?;
    }
    Ok(())
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
    refuses(env, tx, Refusal::InvalidAggregationProof).await
}

/// The protocol adapter refuses a transaction spending a nullifier it already
/// settled: the same transaction a second time.
pub async fn settlement_refuses_a_spent_nullifier<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let tx = prove_actions(env, &[trivial_action(51, trivial::Overrides::default())?]).await?;
    settles(env, tx.clone()).await?;
    refuses(env, tx, Refusal::NullifierSpent).await
}

/// The protocol adapter refuses a transaction consuming a resource under a
/// root it never stored.
pub async fn settlement_refuses_an_unknown_root<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let action = trivial_action(
        52,
        trivial::Overrides {
            ephemeral_root: Some(Digest::from_bytes(*b"anoma pa-testkit suite: no root!")),
            ..trivial::Overrides::default()
        },
    )?;
    let tx = prove_actions(env, &[action]).await?;
    refuses(env, tx, Refusal::UnknownRoot).await
}

/// The protocol adapter refuses a transaction proven against another kind
/// table than the one it holds, and settles it once its owner installs that
/// table.
pub async fn settles_only_under_the_kind_table_it_was_proven_against<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let tx = prove_actions(env, &[trivial_action(53, trivial::Overrides::default())?]).await?;
    env.protocol_adapter_mut()
        .set_kind_table_commitment(Digest::from_bytes(*b"anoma pa-testkit suite: kindtabl"))
        .await
        .context("failed to install another kind table")?;
    refuses(env, tx.clone(), Refusal::InvalidAggregationProof).await?;

    env.protocol_adapter_mut()
        .set_kind_table_commitment(crate::fixtures::kind_table_commitment())
        .await
        .context("failed to install the kind table the transaction was proven against")?;
    settles(env, tx).await
}

/// The protocol adapter refuses every transaction while its owner has paused
/// it, and settles it once unpaused.
pub async fn settles_only_while_unpaused<Env: Environment>(env: &mut Env) -> anyhow::Result<()> {
    let tx = prove_actions(env, &[trivial_action(54, trivial::Overrides::default())?]).await?;
    env.protocol_adapter_mut()
        .pause()
        .await
        .context("failed to pause the protocol adapter")?;
    refuses(env, tx.clone(), Refusal::Paused).await?;

    env.protocol_adapter_mut()
        .unpause()
        .await
        .context("failed to unpause the protocol adapter")?;
    settles(env, tx).await
}

/// The protocol adapter refuses a transaction with a resource whose logic ref
/// its owner denied.
pub async fn settlement_refuses_a_denied_logic_ref<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    env.protocol_adapter_mut()
        .deny_logic_ref(passthrough::PASSTHROUGH_LOGIC_VK)
        .await
        .context("failed to deny the pass-through logic")?;
    let action = passthrough::build(55, Vec::new(), passthrough::Overrides::default())
        .context("failed to build a pass-through action")?
        .witnesses;
    let tx = prove_actions(env, &[action]).await?;
    refuses(env, tx, Refusal::DeniedLogicRef).await
}

/// A transaction whose external call expects what the forwarder returns
/// settles: time 0 is before every block.
pub async fn settles_an_external_call_whose_output_matches<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let tx = prove_external_call(env, 61, block_time_call(TimeComparison::Before)).await?;
    settles(env, tx).await
}

/// A transaction whose external call's blob the adapter keeps settles, and
/// the adapter emits the blob after the call.
pub async fn settles_an_external_call_kept_as_a_payload<Env: Environment>(
    env: &mut Env,
) -> anyhow::Result<()> {
    let call = env
        .external_call(block_time_call(TimeComparison::Before))
        .await?;
    let action = passthrough::build(63, vec![call], passthrough::Overrides { keep_calls: true })
        .context("failed to build a pass-through action keeping its call")?
        .witnesses;
    proves_and_settles(env, &[action]).await
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
    use std::collections::HashSet;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anoma_rm_risc0::aggregation_instance::AggregationInstance;
    use anoma_rm_risc0::proving_system::JournalEncoding;

    use super::*;
    use crate::assert::{Needle, expect_integration_panic};
    use crate::prover::{LocalProver, mock_aggregation_seal};

    /// A protocol adapter in memory that makes the checks the suite expects an
    /// adapter to make: the pause, first, as on every chain; each resource's
    /// logic ref, and each consumed resource's root and nullifier; the
    /// external calls (whose one forwarder is a block-time forwarder taking
    /// `[time, expected output]`); and the local prover's seal over the
    /// aggregation journal in `encoding`, with the compliance key and the
    /// kind-table commitment the adapter holds. The chains' adapters make
    /// the checks after the pause in different orders, so a transaction
    /// failing more than one of them would be refused for a different reason
    /// on each: it fails the test instead. It emits its events as it settles,
    /// apart from the suite's `expected_events`, so the suite's check of
    /// them is a check here too.
    struct InMemoryAdapter {
        tree: FrontierCommitmentTree,
        roots: HashSet<Digest>,
        nullifiers: HashSet<Digest>,
        denied_logic_refs: HashSet<Digest>,
        kind_table_commitment: Digest,
        paused: bool,
        encoding: JournalEncoding,
    }

    impl InMemoryAdapter {
        /// An adapter with an empty tree, holding the kind table the fixtures
        /// prove against.
        fn new(encoding: JournalEncoding) -> Self {
            let tree = FrontierCommitmentTree::new(0, Vec::new()).unwrap();
            Self {
                roots: HashSet::from([tree.root()]),
                tree,
                nullifiers: HashSet::new(),
                denied_logic_refs: HashSet::new(),
                kind_table_commitment: crate::fixtures::kind_table_commitment(),
                paused: false,
                encoding,
            }
        }

        /// Why the adapter refuses `transaction`.
        fn refusal(&self, transaction: &Transaction) -> anyhow::Result<Option<Refusal>> {
            if self.paused {
                return Ok(Some(Refusal::Paused));
            }
            let mut refusals = Vec::new();
            let mut refuse = |refusal| {
                if !refusals.contains(&refusal) {
                    refusals.push(refusal);
                }
            };
            let instance = transaction.aggregation_instance()?;
            let mut spent = HashSet::new();
            for action in &instance.actions {
                for consumed in &action.consumed_publics {
                    if !self.roots.contains(&consumed.commitment_tree_root) {
                        refuse(Refusal::UnknownRoot);
                    }
                    if self.nullifiers.contains(&consumed.resource_nullifier)
                        || !spent.insert(consumed.resource_nullifier)
                    {
                        refuse(Refusal::NullifierSpent);
                    }
                }
                let resources = action
                    .consumed_publics
                    .iter()
                    .map(|c| (c.resource_logic_ref, &c.app_data))
                    .chain(
                        action
                            .created_publics
                            .iter()
                            .map(|c| (c.resource_logic_ref, &c.app_data)),
                    );
                for (logic_ref, app_data) in resources {
                    if self.denied_logic_refs.contains(&logic_ref) {
                        refuse(Refusal::DeniedLogicRef);
                    }
                    for call in &app_data.external_payload {
                        if !call_block_time_forwarder(&call.blob)? {
                            refuse(Refusal::ExternalCallOutputMismatch);
                        }
                    }
                }
            }
            let verified = AggregationInstance {
                compliance_key: anoma_rm_risc0::constants::COMPLIANCE_VK,
                kind_table_commitment: self.kind_table_commitment,
                ..instance.clone()
            };
            let proof = &transaction
                .as_arm()
                .aggregation
                .as_ref()
                .context("the transaction carries no aggregation")?
                .proof;
            if *proof != mock_aggregation_seal(&verified, self.encoding) {
                refuse(Refusal::InvalidAggregationProof);
            }
            match refusals[..] {
                [] => Ok(None),
                [refusal] => Ok(Some(refusal)),
                _ => anyhow::bail!(
                    "the transaction is refused for {refusals:?}: the chains' adapters check \
                     these in different orders, so a suite transaction must fail one check"
                ),
            }
        }

        /// Settles `transaction`, which it does not refuse, and returns the
        /// events it emits.
        fn apply(&mut self, transaction: &Transaction) -> anyhow::Result<Vec<Event>> {
            let instance = transaction.aggregation_instance()?;
            let before = self.tree.root();
            let mut events = Vec::new();
            for action in &instance.actions {
                for consumed in &action.consumed_publics {
                    self.nullifiers.insert(consumed.resource_nullifier);
                    emit_resource(&mut events, consumed.resource_nullifier, &consumed.app_data);
                }
                for created in &action.created_publics {
                    self.tree.add([created.resource_commitment]);
                    emit_resource(&mut events, created.resource_commitment, &created.app_data);
                }
                events.push(Event::ActionExecuted {
                    action_tree_root: action.action_tree_root,
                    nullifiers: action
                        .consumed_publics
                        .iter()
                        .map(|c| c.resource_nullifier)
                        .collect(),
                    consumed_logic_refs: action
                        .consumed_publics
                        .iter()
                        .map(|c| c.resource_logic_ref)
                        .collect(),
                    commitments: action
                        .created_publics
                        .iter()
                        .map(|c| c.resource_commitment)
                        .collect(),
                    created_logic_refs: action
                        .created_publics
                        .iter()
                        .map(|c| c.resource_logic_ref)
                        .collect(),
                });
            }
            let root = self.tree.root();
            if root != before {
                self.roots.insert(root);
                events.push(Event::CommitmentTreeRootAdded { root });
            }
            let mut transaction_id = Keccak256::new();
            for action in &instance.actions {
                transaction_id.update(action.action_tree_root.as_bytes());
            }
            events.push(Event::TransactionExecuted {
                transaction_id: Digest::from_bytes(transaction_id.finalize().into()),
            });
            Ok(events)
        }
    }

    /// Emits a settled resource's events: one per external call it makes,
    /// then one per blob it keeps.
    fn emit_resource(events: &mut Vec<Event>, tag: Digest, app_data: &AppData) {
        for _ in &app_data.external_payload {
            events.push(Event::ForwarderCallExecuted);
        }
        let payloads = [
            (PayloadKind::Resource, &app_data.resource_payload),
            (PayloadKind::Discovery, &app_data.discovery_payload),
            (PayloadKind::External, &app_data.external_payload),
            (PayloadKind::Application, &app_data.application_payload),
        ];
        for (kind, payload) in payloads {
            for (index, blob) in payload.iter().enumerate() {
                if blob.deletion_criterion == DELETION_CRITERION_NEVER {
                    events.push(Event::Payload {
                        kind,
                        tag,
                        index: index as u32,
                        blob: anoma_rm_risc0::utils::words_to_bytes(&blob.blob).to_vec(),
                    });
                }
            }
        }
    }

    impl ProtocolAdapter for InMemoryAdapter {
        async fn settle(&mut self, transaction: Transaction) -> anyhow::Result<Outcome> {
            Ok(match self.refusal(&transaction)? {
                Some(refusal) => Outcome::Refused(refusal),
                None => Outcome::Settled(self.apply(&transaction)?),
            })
        }

        async fn commitment_tree(&self) -> anyhow::Result<FrontierCommitmentTree> {
            Ok(self.tree.clone())
        }

        async fn latest_root(&self) -> anyhow::Result<Digest> {
            Ok(self.tree.root())
        }

        // The owner's calls refuse what both chains' adapters refuse.

        async fn set_kind_table_commitment(&mut self, commitment: Digest) -> anyhow::Result<()> {
            anyhow::ensure!(
                commitment != Digest::ZERO,
                "the zero kind-table commitment is not allowed"
            );
            self.kind_table_commitment = commitment;
            Ok(())
        }

        async fn pause(&mut self) -> anyhow::Result<()> {
            anyhow::ensure!(!self.paused, "the adapter is already paused");
            self.paused = true;
            Ok(())
        }

        async fn unpause(&mut self) -> anyhow::Result<()> {
            anyhow::ensure!(self.paused, "the adapter is not paused");
            self.paused = false;
            Ok(())
        }

        async fn deny_logic_ref(&mut self, logic_ref: Digest) -> anyhow::Result<()> {
            anyhow::ensure!(
                logic_ref != Digest::ZERO,
                "the zero logic ref is not allowed"
            );
            anyhow::ensure!(
                self.denied_logic_refs.insert(logic_ref),
                "the logic ref {logic_ref} is already denied"
            );
            Ok(())
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
                adapter: InMemoryAdapter::new(encoding),
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

    /// A transaction failing two checks can be refused for a different
    /// reason on each chain (proven against another kind table and under an
    /// unknown root: the Solana adapter checks the kind table first, pa-evm
    /// the root), so the adapter in memory fails the test instead.
    #[tokio::test]
    async fn a_transaction_failing_two_checks_fails_the_test() -> anyhow::Result<()> {
        let mut env = InMemoryEnvironment::new(JournalEncoding::Risc0Serde);
        let action = trivial_action(
            81,
            trivial::Overrides {
                ephemeral_root: Some(Digest::from_bytes([7; 32])),
                ..trivial::Overrides::default()
            },
        )?;
        let tx = prove_actions(&env, &[action]).await?;
        env.adapter
            .set_kind_table_commitment(Digest::from_bytes([8; 32]))
            .await?;
        expect_integration_panic(Needle::Static(
            "the transaction is refused for [UnknownRoot, InvalidAggregationProof]",
        ))(settle_tx(&mut env, tx).await)
    }

    /// The adapter in memory emits a kept blob as a payload event of its
    /// resource, after the resource's external call.
    #[tokio::test]
    async fn a_kept_call_is_emitted_after_the_call() -> anyhow::Result<()> {
        let mut env = InMemoryEnvironment::new(JournalEncoding::Risc0Serde);
        let call = vec![0, TimeComparison::Before as u32];
        let built = passthrough::build(
            82,
            vec![call.clone()],
            passthrough::Overrides { keep_calls: true },
        )?;
        let tx = prove_actions(&env, &[built.witnesses]).await?;
        let Outcome::Settled(events) = settle_tx(&mut env, tx).await? else {
            anyhow::bail!("the adapter refused the transaction");
        };
        let nullifier = built.consumed_ephemeral.nullifier(
            &anoma_rm_risc0::nullifier_key::NullifierKey::from_bytes([82; 32]),
        )?;
        anyhow::ensure!(
            events[..2]
                == [
                    Event::ForwarderCallExecuted,
                    Event::Payload {
                        kind: PayloadKind::External,
                        tag: nullifier,
                        index: 0,
                        blob: anoma_rm_risc0::utils::words_to_bytes(&call).to_vec(),
                    },
                ],
            "the settlement's events start {:#?}",
            &events[..2]
        );
        Ok(())
    }

    /// The owner's calls refuse what both chains' adapters refuse.
    #[tokio::test]
    async fn the_owners_calls_refuse_what_both_chains_refuse() -> anyhow::Result<()> {
        let mut adapter = InMemoryAdapter::new(JournalEncoding::Risc0Serde);
        anyhow::ensure!(
            adapter.unpause().await.is_err(),
            "unpaused while not paused"
        );
        adapter.pause().await?;
        anyhow::ensure!(adapter.pause().await.is_err(), "paused while paused");
        anyhow::ensure!(
            adapter
                .set_kind_table_commitment(Digest::ZERO)
                .await
                .is_err(),
            "installed the zero kind-table commitment"
        );
        anyhow::ensure!(
            adapter.deny_logic_ref(Digest::ZERO).await.is_err(),
            "denied the zero logic ref"
        );
        let logic_ref = passthrough::PASSTHROUGH_LOGIC_VK;
        adapter.deny_logic_ref(logic_ref).await?;
        anyhow::ensure!(
            adapter.deny_logic_ref(logic_ref).await.is_err(),
            "denied a logic ref twice"
        );
        Ok(())
    }

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
            refuses(&mut env, tx, Refusal::InvalidAggregationProof)
                .await
                .with_context(|| format!("proved in {proved:?}, verified in {verified:?}"))?;
        }
        Ok(())
    }
}

/// A trivial action with nonces from `seed`.
fn trivial_action(seed: u8, overrides: trivial::Overrides) -> anyhow::Result<ActionWitnesses> {
    Ok(trivial::build(seed, overrides)
        .with_context(|| format!("failed to build trivial action {seed}"))?
        .witnesses)
}

async fn proves_and_settles<Env: Environment>(
    env: &mut Env,
    actions: &[ActionWitnesses],
) -> anyhow::Result<()> {
    let tx = prove_actions(env, actions).await?;
    settles(env, tx).await
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

/// Checks that the protocol adapter settles `tx`, emitting the events
/// `expected_events` gives in that order, and then stores the root of its
/// tree with the transaction's commitments added (the root it stored before,
/// if it creates none).
async fn settles<Env: Environment>(env: &mut Env, tx: Transaction) -> anyhow::Result<()> {
    let mut tree = stored_tree(env).await?;
    tree.add(tx.created_commitments()?);
    let root = tree.root();
    let expected = expected_events(&tx, root)?;
    let outcome = settle_tx(env, tx).await?;
    anyhow::ensure!(
        matches!(&outcome, Outcome::Settled(events) if *events == expected),
        "the protocol adapter must settle the transaction emitting {expected:#?}, but returned \
         {outcome:#?}"
    );
    let after = stored_tree(env).await?.root();
    anyhow::ensure!(
        after == root,
        "after the settlement, the adapter stores the root {after}, not {root}"
    );
    Ok(())
}

/// The events settling `tx` emits, `root` being the tree's root after it, in
/// pa-evm's order, which the Solana adapter mirrors: per action, for each
/// resource (consumed ones first) one per external call it makes and then one
/// per blob it keeps, then the action's; then, if the transaction creates a
/// commitment, the new root; then the transaction's.
fn expected_events(tx: &Transaction, root: Digest) -> anyhow::Result<Vec<Event>> {
    let instance = tx.aggregation_instance()?;
    let mut events = Vec::new();
    for action in &instance.actions {
        let consumed = &action.consumed_publics;
        let created = &action.created_publics;
        let resources = consumed
            .iter()
            .map(|c| (c.resource_nullifier, &c.app_data))
            .chain(created.iter().map(|c| (c.resource_commitment, &c.app_data)));
        for (tag, app_data) in resources {
            let calls = app_data.external_payload.len();
            events.extend(std::iter::repeat_n(Event::ForwarderCallExecuted, calls));
            events.extend(kept_blobs(tag, app_data));
        }
        events.push(Event::ActionExecuted {
            action_tree_root: action.action_tree_root,
            nullifiers: consumed.iter().map(|c| c.resource_nullifier).collect(),
            consumed_logic_refs: consumed.iter().map(|c| c.resource_logic_ref).collect(),
            commitments: created.iter().map(|c| c.resource_commitment).collect(),
            created_logic_refs: created.iter().map(|c| c.resource_logic_ref).collect(),
        });
    }
    if tx.created_commitments()?.next().is_some() {
        events.push(Event::CommitmentTreeRootAdded { root });
    }
    events.push(Event::TransactionExecuted {
        transaction_id: Digest::from_bytes(Keccak256::digest(instance.delta_msg()).into()),
    });
    Ok(events)
}

/// The payload events of the blobs in `app_data` the adapter keeps, for the
/// resource `tag`: its resource, discovery, external and application
/// payloads, in that order.
fn kept_blobs(tag: Digest, app_data: &AppData) -> impl Iterator<Item = Event> + '_ {
    [
        (PayloadKind::Resource, &app_data.resource_payload),
        (PayloadKind::Discovery, &app_data.discovery_payload),
        (PayloadKind::External, &app_data.external_payload),
        (PayloadKind::Application, &app_data.application_payload),
    ]
    .into_iter()
    .flat_map(move |(kind, payload)| {
        (0u32..)
            .zip(payload)
            .filter(|(_, blob)| blob.deletion_criterion == DELETION_CRITERION_NEVER)
            .map(move |(index, blob)| Event::Payload {
                kind,
                tag,
                index,
                blob: anoma_rm_risc0::utils::words_to_bytes(&blob.blob).to_vec(),
            })
    })
}

/// Checks that the protocol adapter refuses `tx` for `refusal`, leaving the
/// root it stores where it was.
async fn refuses<Env: Environment>(
    env: &mut Env,
    tx: Transaction,
    refusal: Refusal,
) -> anyhow::Result<()> {
    let before = stored_tree(env).await?.root();
    let outcome = settle_tx(env, tx).await?;
    anyhow::ensure!(
        outcome == Outcome::Refused(refusal),
        "the protocol adapter must refuse the transaction ({refusal:?}), but returned {outcome:?}"
    );
    let after = stored_tree(env).await?.root();
    anyhow::ensure!(
        after == before,
        "a refused transaction moved the stored root from {before} to {after}"
    );
    Ok(())
}

/// The commitment tree the protocol adapter stores, after checking that its
/// root is the latest root the adapter stores.
async fn stored_tree<Env: Environment>(env: &Env) -> anyhow::Result<FrontierCommitmentTree> {
    let latest = latest_root(env).await?;
    let tree = env.protocol_adapter().commitment_tree().await?;
    anyhow::ensure!(
        tree.root() == latest,
        "the protocol adapter stores the latest root {latest}, but its commitment tree's root is \
         {}",
        tree.root()
    );
    Ok(tree)
}

/// Emits one test per suite function against the environment `$setup`
/// builds (an expression evaluating to `anyhow::Result<Env>`), each test
/// with an environment of its own. Invoke it inside a module per
/// environment; the tests run on tokio's multi-thread runtime, so the crate
/// depends on `tokio` with `macros` and `rt-multi-thread`.
///
/// Every environment runs every suite test.
///
/// ```ignore
/// mod local {
///     anoma_pa_testkit::suite_tests!(Env::setup_bare());
/// }
/// ```
#[macro_export]
macro_rules! suite_tests {
    ($setup:expr $(,)?) => {
        $crate::suite_tests!(@tests $setup;
            settles_a_trivial_transaction,
            settles_an_n_to_m_transaction,
            settles_a_multi_action_transaction,
            settles_consume_only_transactions_without_a_root_change,
            settlement_refuses_a_tampered_aggregation_seal,
            settlement_refuses_a_spent_nullifier,
            settlement_refuses_an_unknown_root,
            settles_only_under_the_kind_table_it_was_proven_against,
            settles_only_while_unpaused,
            settlement_refuses_a_denied_logic_ref,
            settles_an_external_call_whose_output_matches,
            settles_an_external_call_kept_as_a_payload,
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

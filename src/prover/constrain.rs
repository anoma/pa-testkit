//! Shared front-half of proving: constrain an action's witnesses into validated
//! instances. Every prover runs this first — the local prover mints a mock
//! aggregation seal over the resulting instances, the queue and risc0 provers
//! assemble the transaction from real proofs of the witnesses.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::compliance::{self, ComplianceInstance};
use anoma_rm_risc0::logic_instance::LogicInstance;
use anoma_rm_risc0::merkle_path::PADDING_LEAF;
use anyhow::Context;

use crate::witness::{ActionWitnesses, LogicWitness};

/// A logic witness constrained to its instance and verifying key.
pub(super) struct ConstrainedLogic {
    /// The constrained logic instance (carries tag, is_consumed, and app_data).
    pub instance: LogicInstance,
    /// The logic's verifying key.
    pub verifying_key: Digest,
    /// Position of the originating witness in
    /// [`ActionWitnesses::logic_witnesses`], correlating the logics, in the
    /// compliance unit's resource order, with their per-witness proving jobs.
    #[cfg_attr(not(any(feature = "e2e", feature = "prove")), allow(dead_code))]
    pub witness_index: usize,
}

/// An action's witnesses, constrained and validated against the canonical
/// tag-correlation rules.
pub(super) struct ConstrainedAction {
    /// The action's compliance instance.
    #[cfg_attr(not(feature = "local"), allow(dead_code))]
    pub compliance_instance: ComplianceInstance,
    /// Logic inputs for the resources, in `resource_instances` order.
    pub resource_logics: Vec<ConstrainedLogic>,
}

/// Runs `f`, turning a panic with a message into an error: the circuits panic
/// on invalid witnesses.
pub(super) fn catching_panics<T>(f: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
    // NOTE: this may not actually be unwind safe, but we don't care, because
    // we will hardly ever run into unwind safety issues during these tests.
    std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|cause| {
        if let Some(panic_msg) = cause.downcast_ref::<String>() {
            anyhow::bail!("proving failed: {panic_msg}");
        }
        if let Some(panic_msg) = cause.downcast_ref::<&'static str>() {
            anyhow::bail!("proving failed: {panic_msg}");
        }
        std::panic::resume_unwind(cause)
    })
}

/// The kind tree root the batch aggregation circuit commits to for actions
/// whose compliance instances commit to `roots`: the one root every action
/// using a kind tree commits to, or the empty tree's root (the padding leaf)
/// when none uses one. An action whose resources no tree lists commits to
/// the empty tree's root and joins any. A transaction without actions does
/// not aggregate.
pub(super) fn aggregated_kind_tree_root(
    roots: impl IntoIterator<Item = Digest>,
) -> anyhow::Result<Digest> {
    let mut roots = roots.into_iter().peekable();
    anyhow::ensure!(
        roots.peek().is_some(),
        "cannot aggregate a transaction without actions"
    );
    let mut used: Option<Digest> = None;
    for (action_idx, root) in roots.enumerate() {
        if root == PADDING_LEAF {
            continue;
        }
        let shared = *used.get_or_insert(root);
        anyhow::ensure!(
            root == shared,
            "action {action_idx} uses a different kind tree than the transaction"
        );
    }
    Ok(used.unwrap_or(PADDING_LEAF))
}

/// Constrains every action of a transaction (see [`action`]).
#[cfg(any(feature = "e2e", feature = "prove"))]
pub(super) fn actions(
    action_witnesses: &[ActionWitnesses],
) -> anyhow::Result<Vec<ConstrainedAction>> {
    catching_panics(|| {
        action_witnesses
            .iter()
            .enumerate()
            .map(|(action_idx, witnesses)| action(witnesses, action_idx))
            .collect()
    })
}

/// Constrains every witness of an action and checks that each of the compliance
/// unit's resources has a matching logic witness — by tag, role, and verifying
/// key — mirroring [`anoma_rm_risc0::action::Action`]. The returned logics are
/// reordered into the compliance unit's resource order, the order of its tags.
pub(super) fn action(
    witnesses: &ActionWitnesses,
    action_idx: usize,
) -> anyhow::Result<ConstrainedAction> {
    let compliance_instance =
        compliance::constrain(&witnesses.compliance_witness).with_context(|| {
            format!("failed to constrain the compliance unit of action {action_idx}")
        })?;

    let resource_count = compliance_instance.resource_instances.len();
    anyhow::ensure!(
        witnesses.logic_witnesses.len() == resource_count,
        "action {action_idx} must have exactly one logic witness per resource: \
         logic={} resources={resource_count}",
        witnesses.logic_witnesses.len(),
    );

    let mut logics = witnesses
        .logic_witnesses
        .iter()
        .enumerate()
        .map(|(logic_idx, logic_witness)| {
            let instance = logic_witness.constrain().with_context(|| {
                format!("failed to constrain logic witness {logic_idx} of action {action_idx}")
            })?;
            Ok(Some(ConstrainedLogic {
                verifying_key: logic_witness.verifying_key(),
                instance,
                witness_index: logic_idx,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut logic_index_by_tag = HashMap::with_capacity(logics.len());
    for (logic_idx, logic) in logics.iter().enumerate() {
        let tag = logic.as_ref().expect("not yet taken").instance.tag;
        anyhow::ensure!(
            logic_index_by_tag.insert(tag, logic_idx).is_none(),
            "action {action_idx} has two logic witnesses sharing tag {tag}",
        );
    }

    let resource_logics = compliance_instance
        .resource_instances
        .iter()
        .map(|resource| {
            take_validated_logic(
                &mut logics,
                &logic_index_by_tag,
                resource.tag(),
                resource.is_consumed(),
                resource.logic_ref(),
                action_idx,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(ConstrainedAction {
        compliance_instance,
        resource_logics,
    })
}

/// Takes the logic witness matching the compliance unit's resource (identified
/// by `tag`) after checking its consumed/created role and verifying key.
fn take_validated_logic(
    logics: &mut [Option<ConstrainedLogic>],
    logic_index_by_tag: &HashMap<Digest, usize>,
    tag: Digest,
    expected_consumed: bool,
    expected_logic_ref: Digest,
    action_idx: usize,
) -> anyhow::Result<ConstrainedLogic> {
    let role = if expected_consumed {
        "consumed"
    } else {
        "created"
    };
    let logic = logic_index_by_tag
        .get(&tag)
        .and_then(|&logic_idx| logics[logic_idx].take())
        .with_context(|| {
            format!("action {action_idx} has no {role} logic witness for tag {tag}")
        })?;

    anyhow::ensure!(
        logic.instance.is_consumed == expected_consumed,
        "action {action_idx} {role} logic witness for tag {tag} has the wrong is_consumed flag"
    );
    anyhow::ensure!(
        logic.verifying_key == expected_logic_ref,
        "action {action_idx} {role} logic verifying key mismatch for tag {tag}"
    );
    Ok(logic)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: Digest = Digest::new([7; 8]);
    const OTHER_TREE: Digest = Digest::new([9; 8]);

    #[test]
    fn actions_without_a_kind_tree_aggregate_under_the_empty_trees_root() {
        assert_eq!(
            aggregated_kind_tree_root([PADDING_LEAF, PADDING_LEAF]).unwrap(),
            PADDING_LEAF
        );
    }

    #[test]
    fn an_action_without_a_kind_tree_joins_the_tree_the_others_use() {
        assert_eq!(
            aggregated_kind_tree_root([PADDING_LEAF, TREE, PADDING_LEAF, TREE]).unwrap(),
            TREE
        );
    }

    #[test]
    fn a_transaction_without_actions_does_not_aggregate() {
        let error = aggregated_kind_tree_root([])
            .expect_err("a transaction without actions must not aggregate");
        assert!(
            error.to_string().contains("without actions"),
            "the error must say why: {error}"
        );
    }

    #[test]
    fn actions_using_two_kind_trees_do_not_aggregate() {
        let error = aggregated_kind_tree_root([TREE, PADDING_LEAF, OTHER_TREE])
            .expect_err("two kind trees must not aggregate");
        assert!(
            error
                .to_string()
                .contains("action 2 uses a different kind tree"),
            "the error must name the action: {error}"
        );
    }
}

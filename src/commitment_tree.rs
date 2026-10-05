//! The commitment tree of a protocol adapter that stores only its frontier: its
//! commitment count and, for each level, the last left node (its sides). The
//! tree starts from these and adds the leaves the tests create, so a harness
//! can answer roots and paths for an adapter whose earlier leaves it never saw.

use anoma_rm_risc0::Digest;
use anoma_rm_risc0::merkle_path::{MerklePath, PADDING_LEAF};
use anoma_rm_risc0::utils::hash_two;
use anyhow::Context;

use crate::environment::CommitmentTree;

pub struct FrontierCommitmentTree {
    /// The adapter's commitment count when the tree was read.
    commitment_count: usize,
    /// The adapter's sides when the tree was read, one per level of the tree
    /// at that count.
    sides: Vec<Digest>,
    /// The leaves added since, in order.
    leaves: Vec<Digest>,
}

impl FrontierCommitmentTree {
    /// The tree an adapter holds after `commitment_count` commitments, from
    /// its sides: for each level below the tree's depth, the last left node.
    pub fn new(commitment_count: usize, sides: Vec<Digest>) -> anyhow::Result<Self> {
        anyhow::ensure!(
            sides.len() == depth_at(commitment_count),
            "a tree of {commitment_count} leaves has {} sides, not {}",
            depth_at(commitment_count),
            sides.len()
        );
        Ok(Self {
            commitment_count,
            sides,
            leaves: Vec::new(),
        })
    }

    /// Adds the commitments a transaction created as the next leaves, in order.
    pub fn add(&mut self, commitments: impl IntoIterator<Item = Digest>) {
        self.leaves.extend(commitments);
    }

    /// The number of leaves: those read and those added since.
    fn count(&self) -> usize {
        self.commitment_count + self.leaves.len()
    }

    /// The node at `level` and `index`, from the added leaves, the sides, or
    /// the empty subtree.
    fn node(&self, level: usize, index: usize) -> Digest {
        if index << level >= self.count() {
            return empty(level);
        }
        if (index + 1) << level <= self.commitment_count {
            // A path reaches a node over read commitments only as its level's
            // last left node, which the sides hold.
            return self.sides[level];
        }
        if level == 0 {
            return self.leaves[index - self.commitment_count];
        }
        hash_two(
            &self.node(level - 1, 2 * index),
            &self.node(level - 1, 2 * index + 1),
        )
    }
}

impl CommitmentTree for FrontierCommitmentTree {
    fn root(&self) -> anyhow::Result<Digest> {
        Ok(self.node(depth_at(self.count()), 0))
    }

    fn path_to(&self, leaf: Digest) -> anyhow::Result<MerklePath> {
        let position = self
            .leaves
            .iter()
            .position(|added| *added == leaf)
            .context("the tests did not add this leaf")?;
        let index = self.commitment_count + position;
        let path: Vec<_> = (0..depth_at(self.count()))
            .map(|level| {
                let node = index >> level;
                (self.node(level, node ^ 1), node % 2 == 1)
            })
            .collect();
        Ok(MerklePath::from_path(&path))
    }
}

/// The depth of an adapter's tree at `count` leaves. It grows by one level
/// each time it fills up.
pub fn depth_at(count: usize) -> usize {
    (usize::BITS - count.leading_zeros()) as usize
}

/// The root of an empty subtree of height `level`.
fn empty(level: usize) -> Digest {
    (0..level).fold(PADDING_LEAF, |node, _| hash_two(&node, &node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anoma_rm_risc0::action_tree::ActionTree;

    fn leaves(count: usize) -> Vec<Digest> {
        (1..=count)
            .map(|seed| Digest::from([seed as u32; 8]))
            .collect()
    }

    /// The tree over all leaves, one level deeper when full, as an adapter
    /// grows it.
    fn reference(leaves: &[Digest]) -> ActionTree {
        let mut padded = leaves.to_vec();
        if padded.is_empty() || padded.len().is_power_of_two() {
            padded.push(PADDING_LEAF);
        }
        ActionTree::new(padded)
    }

    /// The tree read after the first `commitment_count` leaves, with the rest
    /// added since. A side no path reads stays zero.
    fn read_after(leaves: &[Digest], commitment_count: usize) -> FrontierCommitmentTree {
        let sides = (0..depth_at(commitment_count))
            .map(|level| {
                let node = commitment_count >> level;
                if node.is_multiple_of(2) {
                    return Digest::default();
                }
                let start = (node - 1) << level;
                ActionTree::new(leaves[start..start + (1 << level)].to_vec())
                    .root()
                    .unwrap()
            })
            .collect();
        let mut tree = FrontierCommitmentTree::new(commitment_count, sides).unwrap();
        tree.add(leaves[commitment_count..].iter().copied());
        tree
    }

    #[test]
    fn root_matches_the_tree_over_all_leaves() {
        for count in 0..=33 {
            let all = leaves(count);
            let expected = reference(&all).root().unwrap();
            for commitment_count in 0..=count {
                assert_eq!(
                    read_after(&all, commitment_count).root().unwrap(),
                    expected,
                    "{count} leaves, {commitment_count} read"
                );
            }
        }
    }

    #[test]
    fn path_to_matches_the_tree_over_all_leaves() {
        for count in 1..=33 {
            let all = leaves(count);
            let expected = reference(&all);
            for commitment_count in 0..count {
                let tree = read_after(&all, commitment_count);
                for leaf in &all[commitment_count..] {
                    assert_eq!(
                        tree.path_to(*leaf).unwrap(),
                        expected.generate_path(leaf).unwrap(),
                        "{count} leaves, {commitment_count} read"
                    );
                }
            }
        }
    }

    #[test]
    fn sides_must_match_the_depth_at_the_count() {
        let error = FrontierCommitmentTree::new(5, vec![Digest::default(); 2])
            .err()
            .expect("5 leaves need 3 sides");
        assert_eq!(error.to_string(), "a tree of 5 leaves has 3 sides, not 2");
    }
}

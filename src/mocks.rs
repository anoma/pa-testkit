use anoma_rm_risc0::Digest;
use anoma_rm_risc0::merkle_path::MerklePath;

use crate::environment::CommitmentTree;
use crate::environment::Environment;
use crate::environment::ProtocolAdapter;
use crate::environment::Prover;
use crate::environment::State;
use crate::environment::Transaction;
use crate::witness::ActionWitnesses;

impl Transaction for () {
    fn created_commitments(&self) -> anyhow::Result<impl Iterator<Item = Digest> + '_> {
        Ok(std::iter::empty())
    }
}

mockall::mock! {
    pub CommitmentTree {}

    impl CommitmentTree for CommitmentTree {
        fn root(&self) -> anyhow::Result<Digest>;
        fn path_to(&self, leaf: Digest) -> anyhow::Result<MerklePath>;
    }
}

mockall::mock! {
    pub ProtocolAdapter {}

    impl ProtocolAdapter for ProtocolAdapter {
        type Transaction = ();
        type CommitmentTree = MockCommitmentTree;

        async fn execute(&mut self, transaction: ()) -> anyhow::Result<()>;
        fn commitment_tree(&self) -> &MockCommitmentTree;
    }
}

mockall::mock! {
    pub Prover {}

    impl Prover for Prover {
        type Transaction = ();

        async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<()>;
    }
}

mockall::mock! {
    pub Environment {}

    impl Environment for Environment {
        type Transaction = ();
        type ProtocolAdapter = MockProtocolAdapter;
        type Prover = MockProver;

        fn prover(&self) -> &MockProver;
        fn state(&self) -> &State;
        fn state_mut(&mut self) -> &mut State;
        fn protocol_adapter(&self) -> &MockProtocolAdapter;
        fn protocol_adapter_mut(&mut self) -> &mut MockProtocolAdapter;
    }
}

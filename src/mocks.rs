use anoma_rm_risc0::Digest;
use anoma_rm_risc0::merkle_path::MerklePath;

use crate::environment::CommitmentTree;
use crate::environment::Environment;
use crate::environment::ExternalCall;
use crate::environment::Outcome;
use crate::environment::ProtocolAdapter;
use crate::environment::Prover;
use crate::transaction::Transaction;
use crate::witness::ActionWitnesses;

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
        type CommitmentTree = MockCommitmentTree;

        async fn settle(&mut self, transaction: Transaction) -> anyhow::Result<Outcome>;
        fn commitment_tree(&self) -> &MockCommitmentTree;
    }
}

mockall::mock! {
    pub Prover {}

    impl Prover for Prover {
        async fn prove(&self, actions: &[ActionWitnesses]) -> anyhow::Result<Transaction>;
    }
}

mockall::mock! {
    pub Environment {}

    impl Environment for Environment {
        type ProtocolAdapter = MockProtocolAdapter;
        type Prover = MockProver;

        fn prover(&self) -> &MockProver;
        fn protocol_adapter(&self) -> &MockProtocolAdapter;
        fn protocol_adapter_mut(&mut self) -> &mut MockProtocolAdapter;
        async fn external_call(&mut self, call: ExternalCall) -> anyhow::Result<Vec<u32>>;
    }
}

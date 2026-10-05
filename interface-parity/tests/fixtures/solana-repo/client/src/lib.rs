pub mod addresses {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Environment {
        Staging,
        Production,
    }

    pub fn adapter_address(environment: Environment) -> Option<[u8; 32]> {
        match environment {
            Environment::Staging => Some([1; 32]),
            Environment::Production => None,
        }
    }

    pub struct Deployment {
        pub chain_id: String,
    }

    pub enum Cluster {
        Devnet(String),
    }
}

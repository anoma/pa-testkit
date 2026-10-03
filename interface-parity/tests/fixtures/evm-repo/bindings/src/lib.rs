pub mod addresses {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Environment {
        Staging,
        Production,
    }

    pub fn adapter_address(environment: Environment) -> Option<[u8; 20]> {
        match environment {
            Environment::Staging => Some([1; 20]),
            Environment::Production => None,
        }
    }

    pub fn only_on_evm() {}
}

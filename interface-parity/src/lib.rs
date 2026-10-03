//! Lists every difference between what paired EVM and Solana repositories
//! publish. The design is `docs/superpowers/specs/2026-10-03-interface-parity-design.md`.

pub mod compare;
pub mod excuses;
pub mod fetch;
pub mod files;
pub mod inputs;
pub mod packages;
pub mod report;
pub mod run;
pub mod rust_api;
pub mod tags;
pub mod ts_api;

pub use run::run;

//! Fixtures for building actions to exercise a protocol adapter — app- and
//! chain-agnostic, needing only the risc0 proving stack, so every
//! integration-test crate can reuse them. [`action::build`] builds the witnesses
//! of one action from a list of action resources, in any order; resource
//! constructors such as [`trivial::consumed`] give them.

pub mod action;
pub mod identities;
pub mod trivial;

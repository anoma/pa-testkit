//! Trivial action kind: ephemeral resources with zero quantity under the trivial (padding)
//! resource logic. [`resources`] gives the resources of an action for
//! [`crate::fixtures::action::build`], one per side unless [`Overrides`] sets the counts. The named
//! `invalid_*` constructors of [`Overrides`] give variants for negative tests.

mod action;
mod resource;

pub use action::resources;
pub use resource::Overrides;

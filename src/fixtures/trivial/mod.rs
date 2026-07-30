//! Trivial action kind: consumes and creates an ephemeral resource under the
//! trivial/padding resource logic.
//!
//! Public surface per the single-builder rule (ADR-0003): [`build`] (plus the
//! batch convenience [`build_many`]), the derived-data bundle [`ActionData`],
//! and [`Overrides`] with its named `invalid_*` variants for negative tests.

mod action;
mod resource;

pub use action::{ActionData, build, build_many};
pub use resource::Overrides;

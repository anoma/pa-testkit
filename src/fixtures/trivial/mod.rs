//! Trivial action kind: consumes and creates ephemeral resources under the
//! trivial/padding resource logic. The counts default to one resource per
//! side and are sized via [`Overrides`].
//!
//! Public surface per the single-builder rule (ADR-0003): [`build`] (plus the
//! batch convenience [`build_many`]), the derived-data bundle [`ActionData`],
//! and [`Overrides`] with its named `invalid_*` variants for negative tests.

mod action;
mod resource;

pub use action::{ActionData, build, build_many};
pub use resource::Overrides;

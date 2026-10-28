//! Ordered action kind: an action whose consumed and created ephemeral
//! resources come in the order a test lists them, interleaved as it likes,
//! each under the pass-through logic with the app data the test gives it, or
//! under the trivial logic. The `trivial` and `passthrough` kinds put an
//! action's consumed resources first.
//!
//! Public surface per the single-builder rule (ADR-0003): [`build`], the
//! derived-data bundle [`ActionData`], and [`Overrides`], with the list's
//! element [`ResourceSpec`] and its [`Logic`].

mod action;

pub use action::{ActionData, Logic, Overrides, ResourceSpec, build};

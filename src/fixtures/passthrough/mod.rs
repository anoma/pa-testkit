//! Pass-through action kind: consumes and creates one ephemeral resource
//! each under the pass-through logic, whose guest commits the logic instance
//! it is given. The consumed resource makes the external calls a test gives
//! it, so an action can call any program a chain's adapter forwards to, in
//! the payload encoding that chain reads.
//!
//! Public surface per the single-builder rule (ADR-0003): [`build`], the
//! derived-data bundle [`ActionData`], and [`Overrides`].

mod action;
mod logic;

pub use action::{ActionData, Overrides, build};
pub use logic::{PASSTHROUGH_LOGIC_PK, PASSTHROUGH_LOGIC_VK};

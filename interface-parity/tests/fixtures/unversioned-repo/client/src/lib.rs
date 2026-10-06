//! A client that only git consumers can depend on.

/// The core's answer.
pub fn answer() -> u32 {
    unversioned_core::ANSWER
}

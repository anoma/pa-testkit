//! Conversions out of the crate's types.

use crate::Wrapped;

impl From<Wrapped> for u8 {
    fn from(w: Wrapped) -> u8 {
        w.0
    }
}

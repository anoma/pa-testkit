//! Items whose rendering starts with an attribute, and an impl on a
//! primitive type with its method.

/// An enum with an explicit representation.
#[repr(u32)]
pub enum Code {
    /// The only code.
    Only = 1,
}

/// Another enum with an explicit representation.
#[repr(u8)]
pub enum Small {
    /// The only value.
    One = 1,
}

/// A wrapped byte.
pub struct Wrapped(pub u8);

impl From<Wrapped> for u8 {
    fn from(w: Wrapped) -> u8 {
        w.0
    }
}

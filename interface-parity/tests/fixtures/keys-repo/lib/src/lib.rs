//! Items whose rendering starts with an attribute, and an impl on a
//! primitive type with its method, declared in another file.

mod conv;

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

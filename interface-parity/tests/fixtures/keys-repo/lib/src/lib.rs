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

/// A generic holder, whose methods render after its generic arguments.
pub struct Holder<'a, T>(pub &'a T);

impl<'a, T> Holder<'a, T> {
    /// The held value.
    pub fn get(&self) -> &T {
        self.0
    }

    /// A holder of `value`.
    pub fn new(value: &'a T) -> Self {
        Self(value)
    }
}

/// A module holding a type with a method.
pub mod inner {
    /// A type in the module.
    pub struct Inside;

    impl Inside {
        /// A method of the type.
        pub fn method(&self) {}
    }
}

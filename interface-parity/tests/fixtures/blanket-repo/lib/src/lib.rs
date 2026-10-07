//! A crate declaring a blanket impl of its own, and a type that gets it and
//! the standard library's blanket impls.

/// Marks a type.
pub trait Marked {}

/// Every marked type is labelled.
pub trait Labelled {
    /// The label.
    fn label(&self) -> &'static str {
        "marked"
    }
}

impl<T: Marked> Labelled for T {}

/// A marked type.
pub struct Item;

impl Marked for Item {}

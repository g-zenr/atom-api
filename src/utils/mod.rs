//! Generic, domain-independent utilities.
//!
//! A utility must not depend on the business rules of notes, projects or
//! tasks, and must not know about the error type of a particular feature. If a
//! helper needs either, it belongs in [`crate::helpers`] instead.

pub mod text;

pub use text::{char_len, is_blank};

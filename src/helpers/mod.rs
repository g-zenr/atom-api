//! Small, reusable operations that support a particular behaviour.
//!
//! A helper belongs here when it is cohesive, small, and reused by more than
//! one module. It must not become a general-purpose container: if a helper
//! needs a table, a service or a configuration, it belongs in the module that
//! owns that responsibility.

pub mod validation;

pub use validation::{
    MAX_SEARCH_LENGTH, bounded_text, field_error, required_text, validate_search,
};

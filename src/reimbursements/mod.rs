//! The `reimbursements` module: expense claims and their line items.
//!
//! A claim is the aggregate root. It owns its currency and its stored total,
//! and its items are denominated in that currency. Both tables carry the
//! owner, so the ownership predicate of every statement is a column of the
//! table it reads (ADR-0005).
//!
//! The module follows the Phase 1 layering. This chain creates the persistence
//! shapes only:
//!
//! * `models` — row structs and the `ClaimStatus` enum.
//!
//! `handlers`, `service`, `repository` and `dto` land with the routes, so the
//! public HTTP surface of the crate is unchanged until then.

pub mod models;

pub use models::{ClaimStatus, ExpenseClaim, ExpenseItem};

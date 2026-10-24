//! The `reimbursements` module: expense claims and their line items.
//!
//! A claim is the aggregate root. It owns its currency and its stored total,
//! and its items are denominated in that currency. Both tables carry the
//! owner, so the ownership predicate of every statement is a column of the
//! table it reads (ADR-0005).
//!
//! The module follows the Phase 1 layering:
//!
//! * `handlers` — HTTP extraction, status codes, response serialization.
//! * `service` — validation, the project rule and the ownership mapping.
//! * `repository` — the SQL; the only file in the module that holds it.
//! * `models` — row structs and the `ClaimStatus` enum.
//! * `dto` — the published request and response contracts.
//!
//! This chain builds the claim half: list, create, read, update and delete.
//! Items are read (the detail response contains them) and never written; the
//! lifecycle actions are a later chain.

pub mod dto;
pub mod handlers;
pub mod models;
pub mod repository;
pub mod service;

pub use dto::{
    ClaimDetailResponse, ClaimPage, ClaimQuery, ClaimResponse, ClaimsListQuery, CreateClaimRequest,
    ItemResponse, UpdateClaimRequest,
};
pub use models::{ClaimStatus, ExpenseClaim, ExpenseItem};
pub use repository::ClaimRepository;
pub use service::ClaimService;

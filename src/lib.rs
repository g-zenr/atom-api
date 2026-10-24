//! ATOM API — Phase 1 backend.
//!
//! The crate is organised as a modular monolith:
//!
//! * `api`     — HTTP routing, middleware and shared HTTP concerns.
//! * `auth`    — authentication use cases, token lifecycle and password hashing.
//! * `users`   — user persistence and the public user representation.
//! * `notes`   — personal notes.
//! * `projects`— projects.
//! * `tasks`   — tasks belonging to a project.
//! * `reimbursements` — expense claims and their line items.
//! * `job_applications` — companies, applications and their timeline.
//! * `config`  — configuration loading, validation and the message catalogue.
//! * `helpers` — cohesive operations that support a behaviour.
//! * `utils`   — generic, domain-independent utilities.
//!
//! Each feature module keeps the same internal layering: `handlers` (HTTP),
//! `service` (business rules), `repository` (persistence), `models`
//! (persistence shapes) and `dto` (public contracts).
//!
//! Module boundaries and the request lifecycle are documented in
//! `docs/architecture/`.

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod health;
pub mod helpers;
pub mod job_applications;
pub mod notes;
pub mod pagination;
pub mod projects;
pub mod ratelimit;
pub mod reimbursements;
pub mod state;
pub mod tasks;
pub mod telemetry;
pub mod users;
pub mod utils;

pub use config::{Config, ConfigError, Environment};
pub use error::{AppError, AppResult, ErrorBody, ErrorDetail, FieldViolation};
pub use state::AppState;

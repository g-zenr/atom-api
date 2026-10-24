//! The `job_applications` module: companies, applications and their timeline.
//!
//! A company is an employer. An application is one role the owner is pursuing
//! at that company. An event is one dated entry in the application's history.
//! All three tables carry the owner, so the ownership predicate of every
//! statement is a column of the table it reads (`ADR-0005`), and composite
//! foreign keys make a cross-owner child unrepresentable rather than merely
//! rejected by the service.
//!
//! The module follows the Phase 1 layering:
//!
//! * `handlers` — HTTP extraction, status codes, response serialization.
//! * `service` — validation, the stage-transition table and the ownership map.
//! * `repository` — the SQL; the only file in the module that holds it.
//! * `models` — row structs and the two enums.
//! * `dto` — the published request and response contracts.
//!
//! This chain builds the schema half only: the row shapes and the two enum
//! types. The other four layers arrive with the routes, and the module does not
//! declare them before their files exist — a `pub mod` with no file behind it
//! would not compile, and an empty placeholder would suggest behaviour that is
//! not there yet.

pub mod models;

pub use models::{
    ApplicationEvent, ApplicationEventKind, ApplicationStatus, Company, JobApplication,
};

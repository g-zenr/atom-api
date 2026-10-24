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
//! # Where each rule lives
//!
//! | Rule | Owner |
//! | ---- | ----- |
//! | A row belongs to one account; a child's owner equals its parent's | The `user_id` column and the two composite keys |
//! | A company name is unique per owner, ignoring case | The functional index; the service maps `23505` to `409` |
//! | The seven stages exist | The `job_application_status` enum |
//! | The twelve edges, and which stages are terminal | `service::ALLOWED_MOVES`, read by both the refusal and the guarded `UPDATE` |
//! | A company with applications cannot be deleted | `repository::DELETE_COMPANY_IF_UNUSED` |
//! | The next-action task belongs to the caller | The write statements, with `tasks → projects → user_id` |
//! | `role_title`, `summary` and `source` are trimmed | The service: the schema documents "trimmed" but carries no `CHECK` for those three |
//!
//! # One deliberate difference from the claim listing
//!
//! The claim list is split into a filtered and an unfiltered statement so that
//! `expense_claims_user_id_status_created_at_idx` stays readable by the planner.
//! The application list keeps **one** shape: it filters on stage, company, role
//! and two dates, so no single listing index could serve the filtered form
//! anyway, and a second copy of the statement would duplicate four filters to
//! help one index. The reason is repeated next to the statement in
//! [`repository`].

pub mod dto;
pub mod handlers;
pub mod models;
pub mod repository;
pub mod service;

pub use dto::{
    ApplicationPage, ApplicationQuery, ApplicationResponse, ApplicationsListQuery,
    CompaniesListQuery, CompanyPage, CompanyQuery, CompanyResponse, CreateApplicationRequest,
    CreateCompanyRequest, CreateEventRequest, EventPage, EventResponse, UpdateApplicationRequest,
    UpdateCompanyRequest, UpdateEventRequest,
};
pub use models::{
    ApplicationEvent, ApplicationEventKind, ApplicationStatus, Company, JobApplication,
};
pub use repository::JobRepository;
pub use service::{JobService, Transition, ensure_transition};

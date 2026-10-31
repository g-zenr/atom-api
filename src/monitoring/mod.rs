//! The `monitoring` module: laptop price monitoring, the first automation
//! use case.
//!
//! The orchestration — scheduling, outbound collection, notifications — is
//! **external** by decision `ADR-0012`: an n8n workflow reaches ATOM only
//! through these authenticated endpoints, and ATOM owns the data and the
//! rules. The module is therefore an ordinary Phase 2 feature module, with
//! no HTTP client and no scheduler of its own.
//!
//! A tracked product is the aggregate root. Its listings carry the places a
//! price is collected from; observations are the collected prices, idempotent
//! on the workflow-supplied fingerprint; the rule is the one target-price
//! rule a product may carry; automation runs are the workflow's audit trail;
//! alerts are what the rule evaluation raised. Every table carries the
//! owner, so the ownership predicate of every statement is a column of the
//! table it reads (`ADR-0005`).
//!
//! The module follows the Phase 1 layering:
//!
//! * `handlers` — HTTP extraction, status codes, response serialization.
//! * `service` — validation, the identity rule, the idempotency mapping.
//! * `repository` — the SQL; the only file in the module that holds it, and
//!   the place the rule evaluation and the alert insert live, inside the
//!   observation transaction.
//! * `models` — row structs and the `CHECK`ed `TEXT` vocabularies.
//! * `dto` — the published request and response contracts.

pub mod dto;
pub mod handlers;
pub mod models;
pub mod repository;
pub mod service;

pub use dto::{
    AlertPage, AlertQuery, AlertResponse, CreateListingRequest, CreateObservationRequest,
    CreateProductRequest, CreateRuleRequest, FinishRunRequest, ListingPage, ListingResponse,
    ObservationPage, ObservationResponse, ProductQuery, ProductsListQuery, RulePage, RuleResponse,
    RunPage, RunResponse, TrackedProductPage, TrackedProductResponse, UpdateAlertRequest,
    UpdateListingRequest, UpdateProductRequest, UpdateRuleRequest,
};
pub use models::{
    ListingAvailability, ListingValidationState, NotificationStatus, NotifyVia, PriceAlert,
    PriceObservation, ProductListing, ProductStatus, RunOutcome, TrackedProduct,
};
pub use repository::MonitoringRepository;
pub use service::{
    MonitoringService, RecordedObservation, derive_identity, ensure_identity, validate_brand,
    validate_currency_code, validate_fingerprint, validate_listing_url, validate_model,
    validate_percent_drop, validate_retailer, validate_specs,
};

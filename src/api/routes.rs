//! Route table, OpenAPI document and API explorer.

use axum::Router;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
#[cfg(feature = "docs")]
use utoipa_swagger_ui::SwaggerUi;

use crate::auth::handlers;
use crate::config::Config;
use crate::error::AppError;
use crate::job_applications::handlers as job_handlers;
use crate::ml::handlers as ml_handlers;
use crate::notes::handlers as note_handlers;
use crate::projects::handlers as project_handlers;
use crate::reimbursements::handlers as claim_handlers;
use crate::state::AppState;
use crate::tasks::handlers as task_handlers;

/// The OpenAPI document for the ATOM API.
///
/// The derive takes no `info(...)` block on purpose: the title and the
/// version are set at run time from the configuration and from `Cargo.toml`,
/// so the derive must not carry a second, stale copy of either. What the
/// derive does derive - the title, the version and the description - comes
/// from the crate metadata.
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::health::live,
        crate::health::ready,
        handlers::register,
        handlers::login,
        handlers::refresh,
        handlers::logout,
        handlers::me,
        note_handlers::list_notes,
        note_handlers::create_note,
        note_handlers::get_note,
        note_handlers::update_note,
        note_handlers::delete_note,
        ml_handlers::search_notes,
        project_handlers::list_projects,
        project_handlers::create_project,
        project_handlers::get_project,
        project_handlers::update_project,
        project_handlers::delete_project,
        task_handlers::list_tasks,
        task_handlers::create_task,
        task_handlers::get_task,
        task_handlers::update_task,
        task_handlers::delete_task,
        claim_handlers::list_claims,
        claim_handlers::create_claim,
        claim_handlers::get_claim,
        claim_handlers::update_claim,
        claim_handlers::delete_claim,
        claim_handlers::create_item,
        claim_handlers::update_item,
        claim_handlers::delete_item,
        claim_handlers::submit_claim,
        claim_handlers::approve_claim,
        claim_handlers::reject_claim,
        claim_handlers::pay_claim,
        job_handlers::list_companies,
        job_handlers::create_company,
        job_handlers::get_company,
        job_handlers::update_company,
        job_handlers::delete_company,
        job_handlers::list_applications,
        job_handlers::create_application,
        job_handlers::get_application,
        job_handlers::update_application,
        job_handlers::delete_application,
        job_handlers::list_events,
        job_handlers::create_event,
        job_handlers::update_event,
        job_handlers::delete_event,
        ml_handlers::match_application,
        ml_handlers::reindex,
    ),
    components(
        schemas(
            crate::error::ErrorBody,
            crate::error::ErrorDetail,
            crate::error::FieldViolation,
            crate::health::LiveResponse,
            crate::health::ReadyResponse,
            crate::health::ReadyChecks,
            crate::users::dto::UserResponse,
            crate::users::dto::RegisterRequest,
            crate::auth::dto::AuthResponse,
            crate::auth::dto::TokenPair,
            crate::auth::dto::LoginRequest,
            crate::auth::dto::RefreshRequest,
            crate::auth::dto::LogoutRequest,
            crate::notes::dto::NoteResponse,
            crate::notes::dto::NotePage,
            crate::notes::dto::CreateNoteRequest,
            crate::notes::dto::UpdateNoteRequest,
            crate::projects::dto::ProjectResponse,
            crate::projects::dto::ProjectPage,
            crate::projects::dto::CreateProjectRequest,
            crate::projects::dto::UpdateProjectRequest,
            crate::tasks::dto::TaskResponse,
            crate::tasks::dto::TaskPage,
            crate::tasks::dto::CreateTaskRequest,
            crate::tasks::dto::UpdateTaskRequest,
            crate::tasks::models::TaskStatus,
            crate::tasks::models::TaskPriority,
            crate::reimbursements::dto::ClaimResponse,
            crate::reimbursements::dto::ClaimDetailResponse,
            crate::reimbursements::dto::ClaimPage,
            crate::reimbursements::dto::ItemResponse,
            crate::reimbursements::dto::CreateClaimRequest,
            crate::reimbursements::dto::UpdateClaimRequest,
            crate::reimbursements::dto::CreateItemRequest,
            crate::reimbursements::dto::UpdateItemRequest,
            crate::reimbursements::models::ClaimStatus,
            crate::job_applications::dto::CompanyResponse,
            crate::job_applications::dto::CompanyPage,
            crate::job_applications::dto::CreateCompanyRequest,
            crate::job_applications::dto::UpdateCompanyRequest,
            crate::job_applications::dto::ApplicationResponse,
            crate::job_applications::dto::ApplicationPage,
            crate::job_applications::dto::CreateApplicationRequest,
            crate::job_applications::dto::UpdateApplicationRequest,
            crate::job_applications::dto::EventResponse,
            crate::job_applications::dto::EventPage,
            crate::job_applications::dto::CreateEventRequest,
            crate::job_applications::dto::UpdateEventRequest,
            crate::job_applications::models::ApplicationStatus,
            crate::job_applications::models::ApplicationEventKind,
            crate::ml::dto::SearchMode,
            crate::ml::dto::MatchMode,
            crate::ml::dto::DegradedReason,
            crate::ml::dto::SearchRequest,
            crate::ml::dto::SearchHit,
            crate::ml::dto::SearchResponse,
            crate::ml::dto::MatchRequest,
            crate::ml::dto::MatchHit,
            crate::ml::dto::MatchResponse,
            crate::ml::dto::ReindexResponse,
            crate::pagination::Pagination,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Liveness and readiness probes"),
        (name = "auth", description = "Registration, login and token lifecycle"),
        (name = "notes", description = "Personal notes"),
        (name = "projects", description = "Project tracking"),
        (name = "tasks", description = "Tasks belonging to a project"),
        (name = "claims", description = "Expense claims and their line items"),
        (name = "companies", description = "Employers a job application points at"),
        (name = "job-applications", description = "Job applications and their event timeline"),
        (
            name = "ml",
            description = "Semantic search, note matching and the embedding reindex"
        ),
    )
)]
pub struct ApiDoc;

/// Registers the bearer security scheme in the generated OpenAPI document.
///
/// `utoipa` 5 expects security schemes to be added to the component registry
/// through a modifier rather than through the `components` attribute.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut OpenApiDocument) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            utoipa::openapi::security::SecurityScheme::Http(utoipa::openapi::security::Http::new(
                utoipa::openapi::security::HttpAuthScheme::Bearer,
            )),
        );
    }
}

/// Builds the OpenAPI document, with the configured title and the crate
/// version.
///
/// The title comes from the configuration and the version from `Cargo.toml`,
/// so there is one source of truth for each. The derive fills the title and
/// the version from the crate metadata; they are overwritten here so that the
/// title is the configured one and the version is the crate's own.
pub fn openapi_document(config: &Config) -> OpenApiDocument {
    let mut document = ApiDoc::openapi();
    document.info.title = config.api.title.clone();
    document.info.version = env!("CARGO_PKG_VERSION").to_string();
    document
}

/// The version prefix every versioned route is mounted under.
///
/// It is declared here, once, and everything else derives from it: the router
/// nests this prefix, and the explorer builds its two paths from it. Typing the
/// prefix a second time is what lets the router and the generated document
/// drift apart.
pub const API_PREFIX: &str = "/api/v1";

/// Every versioned operation, as `(METHOD, path)`.
///
/// The method is upper case, and the path is **relative** to [`API_PREFIX`] and
/// written in axum's syntax, so the path of the operation in the generated
/// document is [`API_PREFIX`] followed by the path here.
///
/// The order is the order in which [`api_routes`] declares the routes, so the
/// table can be read as the router.
///
/// It is maintained by hand next to the router: `utoipa` derives the document
/// at compile time and cannot read a constant, so neither can be generated from
/// the other. `tests/openapi_contract.rs` compares the table and the document in
/// both directions - an entry the table declares but the document does not
/// describe, and an operation the document describes but the table does not
/// declare - and separately asserts that every entry is mounted. That pair of
/// assertions is the guarantee that replaces the generation.
pub const ROUTE_TABLE: &[(&str, &str)] = &[
    // --- authentication ---
    ("POST", "/auth/register"),
    ("POST", "/auth/login"),
    ("POST", "/auth/refresh"),
    ("POST", "/auth/logout"),
    ("GET", "/auth/me"),
    // --- notes ---
    ("GET", "/notes"),
    ("POST", "/notes"),
    // `POST /notes/search` is ownership of the `ml` module even though it lives
    // under `/notes`: it is a search over the same rows, and its shape is the
    // scored one. `GET /notes/search` is answered by `GET /notes/{note_id}`
    // and is a `400`, because `search` is not a UUID.
    ("POST", "/notes/search"),
    ("GET", "/notes/{note_id}"),
    ("PATCH", "/notes/{note_id}"),
    ("DELETE", "/notes/{note_id}"),
    // --- projects ---
    ("GET", "/projects"),
    ("POST", "/projects"),
    ("GET", "/projects/{project_id}"),
    ("PATCH", "/projects/{project_id}"),
    ("DELETE", "/projects/{project_id}"),
    // --- tasks ---
    ("GET", "/projects/{project_id}/tasks"),
    ("POST", "/projects/{project_id}/tasks"),
    ("GET", "/tasks/{task_id}"),
    ("PATCH", "/tasks/{task_id}"),
    ("DELETE", "/tasks/{task_id}"),
    // --- expense claims ---
    ("GET", "/claims"),
    ("POST", "/claims"),
    ("GET", "/claims/{claim_id}"),
    ("PATCH", "/claims/{claim_id}"),
    ("DELETE", "/claims/{claim_id}"),
    // --- expense claim items ---
    ("POST", "/claims/{claim_id}/items"),
    ("PATCH", "/claims/{claim_id}/items/{item_id}"),
    ("DELETE", "/claims/{claim_id}/items/{item_id}"),
    // --- expense claim lifecycle ---
    ("POST", "/claims/{claim_id}/submit"),
    ("POST", "/claims/{claim_id}/approve"),
    ("POST", "/claims/{claim_id}/reject"),
    ("POST", "/claims/{claim_id}/pay"),
    // --- companies ---
    ("GET", "/companies"),
    ("POST", "/companies"),
    ("GET", "/companies/{company_id}"),
    ("PATCH", "/companies/{company_id}"),
    ("DELETE", "/companies/{company_id}"),
    // --- job applications ---
    ("GET", "/job-applications"),
    ("POST", "/job-applications"),
    ("GET", "/job-applications/{application_id}"),
    ("PATCH", "/job-applications/{application_id}"),
    ("DELETE", "/job-applications/{application_id}"),
    // --- application events ---
    ("GET", "/job-applications/{application_id}/events"),
    ("POST", "/job-applications/{application_id}/events"),
    (
        "PATCH",
        "/job-applications/{application_id}/events/{event_id}",
    ),
    (
        "DELETE",
        "/job-applications/{application_id}/events/{event_id}",
    ),
    // --- machine learning ---
    ("POST", "/job-applications/{application_id}/match"),
    ("POST", "/ml/reindex"),
];

/// Every operation outside [`API_PREFIX`], as `(METHOD, path)`.
///
/// The probes live outside the version prefix on purpose: an orchestrator must
/// be able to call them without knowing which API version is deployed. They are
/// part of the generated document all the same.
pub const UNVERSIONED_ROUTE_TABLE: &[(&str, &str)] =
    &[("GET", "/health/live"), ("GET", "/health/ready")];

/// Builds the versioned API router.
///
/// The routes declared here are mirrored by [`ROUTE_TABLE`], in the same order.
pub fn api_routes() -> Router<AppState> {
    Router::new()
        // --- authentication ---
        .route("/auth/register", post(handlers::register))
        .route("/auth/login", post(handlers::login))
        .route("/auth/refresh", post(handlers::refresh))
        .route("/auth/logout", post(handlers::logout))
        .route("/auth/me", get(handlers::me))
        // --- notes ---
        .route(
            "/notes",
            get(note_handlers::list_notes).post(note_handlers::create_note),
        )
        .route("/notes/search", post(ml_handlers::search_notes))
        .route(
            "/notes/{note_id}",
            get(note_handlers::get_note)
                .patch(note_handlers::update_note)
                .delete(note_handlers::delete_note),
        )
        // --- projects ---
        .route(
            "/projects",
            get(project_handlers::list_projects).post(project_handlers::create_project),
        )
        .route(
            "/projects/{project_id}",
            get(project_handlers::get_project)
                .patch(project_handlers::update_project)
                .delete(project_handlers::delete_project),
        )
        // --- tasks ---
        .route(
            "/projects/{project_id}/tasks",
            get(task_handlers::list_tasks).post(task_handlers::create_task),
        )
        .route(
            "/tasks/{task_id}",
            get(task_handlers::get_task)
                .patch(task_handlers::update_task)
                .delete(task_handlers::delete_task),
        )
        // --- expense claims ---
        .route(
            "/claims",
            get(claim_handlers::list_claims).post(claim_handlers::create_claim),
        )
        .route(
            "/claims/{claim_id}",
            get(claim_handlers::get_claim)
                .patch(claim_handlers::update_claim)
                .delete(claim_handlers::delete_claim),
        )
        .route(
            "/claims/{claim_id}/items",
            post(claim_handlers::create_item),
        )
        .route(
            "/claims/{claim_id}/items/{item_id}",
            patch(claim_handlers::update_item).delete(claim_handlers::delete_item),
        )
        .route(
            "/claims/{claim_id}/submit",
            post(claim_handlers::submit_claim),
        )
        .route(
            "/claims/{claim_id}/approve",
            post(claim_handlers::approve_claim),
        )
        .route(
            "/claims/{claim_id}/reject",
            post(claim_handlers::reject_claim),
        )
        .route("/claims/{claim_id}/pay", post(claim_handlers::pay_claim))
        // --- companies ---
        .route(
            "/companies",
            get(job_handlers::list_companies).post(job_handlers::create_company),
        )
        .route(
            "/companies/{company_id}",
            get(job_handlers::get_company)
                .patch(job_handlers::update_company)
                .delete(job_handlers::delete_company),
        )
        // --- job applications ---
        .route(
            "/job-applications",
            get(job_handlers::list_applications).post(job_handlers::create_application),
        )
        .route(
            "/job-applications/{application_id}",
            get(job_handlers::get_application)
                .patch(job_handlers::update_application)
                .delete(job_handlers::delete_application),
        )
        // --- application events ---
        .route(
            "/job-applications/{application_id}/events",
            get(job_handlers::list_events).post(job_handlers::create_event),
        )
        .route(
            "/job-applications/{application_id}/events/{event_id}",
            patch(job_handlers::update_event).delete(job_handlers::delete_event),
        )
        // --- machine learning ---
        .route(
            "/job-applications/{application_id}/match",
            post(ml_handlers::match_application),
        )
        .route("/ml/reindex", post(ml_handlers::reindex))
}

/// The API explorer, as an Axum router.
///
/// `utoipa-swagger-ui` integrates with Axum through `From<SwaggerUi> for
/// Router<S>`, so the explorer is merged into the application router rather
/// than used as a handler. The router serves both the UI and the OpenAPI
/// document, under [`API_PREFIX`]: the two paths are built from it, so the
/// prefix is not typed again here.
///
/// This function exists only when the `docs` feature is enabled. The feature
/// carries the Swagger UI asset bundle — 10.73 MiB embedded in the binary — and
/// a build that serves no documentation should not carry it. See `api::router`
/// for the behaviour when the configuration asks for the explorer but the
/// binary was built without it.
#[cfg(feature = "docs")]
pub fn swagger_ui(config: &Config) -> Router<AppState> {
    SwaggerUi::new(format!("{API_PREFIX}/docs"))
        .url(
            format!("{API_PREFIX}/openapi.json"),
            openapi_document(config),
        )
        .into()
}

/// Converts a panic into a generic internal error response.
pub fn handle_panic(_panic: Box<dyn std::any::Any + Send + 'static>) -> Response {
    tracing::error!("request handler panicked; the panic payload is intentionally not logged");
    AppError::Internal("panic".to_string()).into_response()
}

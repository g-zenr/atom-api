//! Route table, OpenAPI document and API explorer.

use axum::Router;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use utoipa_swagger_ui::SwaggerUi;

use crate::auth::handlers;
use crate::config::Config;
use crate::error::AppError;
use crate::notes::handlers as note_handlers;
use crate::projects::handlers as project_handlers;
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

/// Builds the versioned API router.
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
}

/// The API explorer, as an Axum router.
///
/// `utoipa-swagger-ui` integrates with Axum through `From<SwaggerUi> for
/// Router<S>`, so the explorer is merged into the application router rather
/// than used as a handler. The router serves both the UI at `/api/v1/docs` and
/// the OpenAPI document at `/api/v1/openapi.json`.
pub fn swagger_ui(config: &Config) -> Router<AppState> {
    SwaggerUi::new("/api/v1/docs")
        .url("/api/v1/openapi.json", openapi_document(config))
        .into()
}

/// Converts a panic into a generic internal error response.
pub fn handle_panic(_panic: Box<dyn std::any::Any + Send + 'static>) -> Response {
    tracing::error!("request handler panicked; the panic payload is intentionally not logged");
    AppError::Internal("panic".to_string()).into_response()
}

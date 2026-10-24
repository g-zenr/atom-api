//! HTTP handlers for the company, job-application and event endpoints.
//!
//! Each handler extracts, calls one service method and picks the status. No
//! handler holds SQL, compares ownership or builds a response by hand: the error
//! envelope and the status code both come from [`crate::error::AppError`].
//!
//! The `utoipa` paths are absolute because the derive runs at compile time and
//! cannot read [`crate::api::routes::API_PREFIX`]; the agreement test compares
//! the generated document with the route table in both directions, which is what
//! keeps the two from drifting apart (amendment `A-C-D0-2`).
//!
//! Two tags, two audiences: `companies` is the employer list an application
//! points at, `job-applications` is the application itself and its timeline.
//! They are separate because a client can manage companies without ever
//! creating an application.

use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::api::extract::{Json, Path, Query};
use crate::api::middleware::AuthUser;
use crate::error::AppResult;
use crate::job_applications::dto::{
    ApplicationPage, ApplicationQuery, ApplicationResponse, ApplicationsListQuery,
    CompaniesListQuery, CompanyPage, CompanyQuery, CompanyResponse, CreateApplicationRequest,
    CreateCompanyRequest, CreateEventRequest, EventPage, EventResponse, UpdateApplicationRequest,
    UpdateCompanyRequest, UpdateEventRequest,
};
use crate::pagination::PaginationQuery;
use crate::state::AppState;

/// `GET /api/v1/companies` — list the caller's companies.
#[utoipa::path(
    get,
    path = "/api/v1/companies",
    tag = "companies",
    security(("bearer" = [])),
    params(
        CompanyQuery,
        PaginationQuery
    ),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's companies, alphabetically.", body = crate::job_applications::dto::CompanyPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn list_companies(
    State(state): State<AppState>,
    user: AuthUser,
    Query(list): Query<CompaniesListQuery>,
) -> AppResult<Json<CompanyPage>> {
    let (query, pagination) = list.into_parts();
    let page = state
        .job_applications
        .list_companies(user.user_id, query, pagination)
        .await?;

    Ok(Json(CompanyPage::from(page)))
}

/// `POST /api/v1/companies` — create a company.
#[utoipa::path(
    post,
    path = "/api/v1/companies",
    tag = "companies",
    security(("bearer" = [])),
    request_body = CreateCompanyRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The company was created.", body = crate::job_applications::dto::CompanyResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 409, description = "The caller already has a company with this name, ignoring case.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn create_company(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateCompanyRequest>,
) -> AppResult<(StatusCode, Json<CompanyResponse>)> {
    let company = state
        .job_applications
        .create_company(user.user_id, payload)
        .await?;

    Ok((StatusCode::CREATED, Json(company)))
}

/// `GET /api/v1/companies/{company_id}` — read one company.
#[utoipa::path(
    get,
    path = "/api/v1/companies/{company_id}",
    tag = "companies",
    security(("bearer" = [])),
    params(("company_id" = Uuid, Path, description = "Company identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The company.", body = crate::job_applications::dto::CompanyResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The company does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn get_company(
    State(state): State<AppState>,
    user: AuthUser,
    Path(company_id): Path<Uuid>,
) -> AppResult<Json<CompanyResponse>> {
    let company = state
        .job_applications
        .get_company(user.user_id, company_id)
        .await?;

    Ok(Json(company))
}

/// `PATCH /api/v1/companies/{company_id}` — partially update a company.
#[utoipa::path(
    patch,
    path = "/api/v1/companies/{company_id}",
    tag = "companies",
    security(("bearer" = [])),
    params(("company_id" = Uuid, Path, description = "Company identifier")),
    request_body = UpdateCompanyRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated company.", body = crate::job_applications::dto::CompanyResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The company does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The new name collides with another company of the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn update_company(
    State(state): State<AppState>,
    user: AuthUser,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<UpdateCompanyRequest>,
) -> AppResult<Json<CompanyResponse>> {
    let company = state
        .job_applications
        .update_company(user.user_id, company_id, payload)
        .await?;

    Ok(Json(company))
}

/// `DELETE /api/v1/companies/{company_id}` — delete a company that has no
/// applications.
#[utoipa::path(
    delete,
    path = "/api/v1/companies/{company_id}",
    tag = "companies",
    security(("bearer" = [])),
    params(("company_id" = Uuid, Path, description = "Company identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The company was deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The company does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "At least one application still references the company. Delete or move them first.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn delete_company(
    State(state): State<AppState>,
    user: AuthUser,
    Path(company_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    state
        .job_applications
        .delete_company(user.user_id, company_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/job-applications` — list the caller's applications.
#[utoipa::path(
    get,
    path = "/api/v1/job-applications",
    tag = "job-applications",
    security(("bearer" = [])),
    params(
        ApplicationQuery,
        PaginationQuery
    ),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's applications, newest first.", body = crate::job_applications::dto::ApplicationPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn list_applications(
    State(state): State<AppState>,
    user: AuthUser,
    Query(list): Query<ApplicationsListQuery>,
) -> AppResult<Json<ApplicationPage>> {
    let (query, pagination) = list.into_parts();
    let page = state
        .job_applications
        .list_applications(user.user_id, query, pagination)
        .await?;

    Ok(Json(ApplicationPage::from(page)))
}

/// `POST /api/v1/job-applications` — create an application in `wishlist`.
#[utoipa::path(
    post,
    path = "/api/v1/job-applications",
    tag = "job-applications",
    security(("bearer" = [])),
    request_body = CreateApplicationRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The application was created.", body = crate::job_applications::dto::ApplicationResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The company or the task does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation, including a stage other than wishlist.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn create_application(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateApplicationRequest>,
) -> AppResult<(StatusCode, Json<ApplicationResponse>)> {
    let application = state
        .job_applications
        .create_application(user.user_id, payload)
        .await?;

    Ok((StatusCode::CREATED, Json(application)))
}

/// `GET /api/v1/job-applications/{application_id}` — read one application.
#[utoipa::path(
    get,
    path = "/api/v1/job-applications/{application_id}",
    tag = "job-applications",
    security(("bearer" = [])),
    params(("application_id" = Uuid, Path, description = "Application identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The application.", body = crate::job_applications::dto::ApplicationResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The application does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn get_application(
    State(state): State<AppState>,
    user: AuthUser,
    Path(application_id): Path<Uuid>,
) -> AppResult<Json<ApplicationResponse>> {
    let application = state
        .job_applications
        .get_application(user.user_id, application_id)
        .await?;

    Ok(Json(application))
}

/// `PATCH /api/v1/job-applications/{application_id}` — partially update an
/// application and move it through the stage machine.
#[utoipa::path(
    patch,
    path = "/api/v1/job-applications/{application_id}",
    tag = "job-applications",
    security(("bearer" = [])),
    params(("application_id" = Uuid, Path, description = "Application identifier")),
    request_body = UpdateApplicationRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated application.", body = crate::job_applications::dto::ApplicationResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The application, the company or the task does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The requested stage is not a permitted move from the current one.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation, including an unknown stage value.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn update_application(
    State(state): State<AppState>,
    user: AuthUser,
    Path(application_id): Path<Uuid>,
    Json(payload): Json<UpdateApplicationRequest>,
) -> AppResult<Json<ApplicationResponse>> {
    let application = state
        .job_applications
        .update_application(user.user_id, application_id, payload)
        .await?;

    Ok(Json(application))
}

/// `DELETE /api/v1/job-applications/{application_id}` — delete an application
/// and its events.
#[utoipa::path(
    delete,
    path = "/api/v1/job-applications/{application_id}",
    tag = "job-applications",
    security(("bearer" = [])),
    params(("application_id" = Uuid, Path, description = "Application identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The application and its events were deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The application does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn delete_application(
    State(state): State<AppState>,
    user: AuthUser,
    Path(application_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    state
        .job_applications
        .delete_application(user.user_id, application_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/job-applications/{application_id}/events` — list the events of
/// one application, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/job-applications/{application_id}/events",
    tag = "job-applications",
    security(("bearer" = [])),
    params(
        ("application_id" = Uuid, Path, description = "Application identifier"),
        PaginationQuery
    ),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The application's events, occurred_at descending then id descending.", body = crate::job_applications::dto::EventPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The application does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn list_events(
    State(state): State<AppState>,
    user: AuthUser,
    Path(application_id): Path<Uuid>,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<Json<EventPage>> {
    let page = state
        .job_applications
        .list_events(user.user_id, application_id, pagination)
        .await?;

    Ok(Json(EventPage::from(page)))
}

/// `POST /api/v1/job-applications/{application_id}/events` — add an event.
#[utoipa::path(
    post,
    path = "/api/v1/job-applications/{application_id}/events",
    tag = "job-applications",
    security(("bearer" = [])),
    params(("application_id" = Uuid, Path, description = "Application identifier")),
    request_body = CreateEventRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The event was added.", body = crate::job_applications::dto::EventResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The application does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation, including an unknown kind.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn create_event(
    State(state): State<AppState>,
    user: AuthUser,
    Path(application_id): Path<Uuid>,
    Json(payload): Json<CreateEventRequest>,
) -> AppResult<(StatusCode, Json<EventResponse>)> {
    let event = state
        .job_applications
        .create_event(user.user_id, application_id, payload)
        .await?;

    Ok((StatusCode::CREATED, Json(event)))
}

/// `PATCH /api/v1/job-applications/{application_id}/events/{event_id}` — correct
/// an event in place.
#[utoipa::path(
    patch,
    path = "/api/v1/job-applications/{application_id}/events/{event_id}",
    tag = "job-applications",
    security(("bearer" = [])),
    params(
        ("application_id" = Uuid, Path, description = "Application identifier"),
        ("event_id" = Uuid, Path, description = "Event identifier, addressable only inside its application"),
    ),
    request_body = UpdateEventRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The corrected event. The response carries no updated_at.", body = crate::job_applications::dto::EventResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The application or the event does not exist, is not owned by the caller, or the event does not belong to that application.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn update_event(
    State(state): State<AppState>,
    user: AuthUser,
    Path((application_id, event_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateEventRequest>,
) -> AppResult<Json<EventResponse>> {
    let event = state
        .job_applications
        .update_event(user.user_id, application_id, event_id, payload)
        .await?;

    Ok(Json(event))
}

/// `DELETE /api/v1/job-applications/{application_id}/events/{event_id}` —
/// delete one event.
#[utoipa::path(
    delete,
    path = "/api/v1/job-applications/{application_id}/events/{event_id}",
    tag = "job-applications",
    security(("bearer" = [])),
    params(
        ("application_id" = Uuid, Path, description = "Application identifier"),
        ("event_id" = Uuid, Path, description = "Event identifier, addressable only inside its application"),
    ),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The event was deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The application or the event does not exist, is not owned by the caller, or the event does not belong to that application.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn delete_event(
    State(state): State<AppState>,
    user: AuthUser,
    Path((application_id, event_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    state
        .job_applications
        .delete_event(user.user_id, application_id, event_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

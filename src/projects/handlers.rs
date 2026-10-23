//! HTTP handlers for the projects endpoints.

use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::api::extract::{Json, Path, Query};
use crate::api::middleware::AuthUser;
use crate::error::AppResult;
use crate::pagination::PaginationQuery;
use crate::projects::dto::{
    CreateProjectRequest, ProjectPage, ProjectQuery, ProjectResponse, ProjectsListQuery,
    UpdateProjectRequest,
};
use crate::state::AppState;

/// `GET /api/v1/projects` — list the caller's projects.
#[utoipa::path(
    get,
    path = "/api/v1/projects",
    tag = "projects",
    security(("bearer" = [])),
    params(ProjectQuery, PaginationQuery),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's projects.", body = crate::projects::dto::ProjectPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn list_projects(
    State(state): State<AppState>,
    user: AuthUser,
    Query(list): Query<ProjectsListQuery>,
) -> AppResult<Json<ProjectPage>> {
    let (query, pagination) = list.into_parts();
    let page = state.projects.list(user.user_id, query, pagination).await?;
    Ok(Json(ProjectPage::from(page)))
}

/// `POST /api/v1/projects` — create a project.
#[utoipa::path(
    post,
    path = "/api/v1/projects",
    tag = "projects",
    security(("bearer" = [])),
    request_body = CreateProjectRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The project was created.", body = crate::projects::dto::ProjectResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn create_project(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateProjectRequest>,
) -> AppResult<(StatusCode, Json<ProjectResponse>)> {
    let project = state.projects.create(user.user_id, payload).await?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// `GET /api/v1/projects/{project_id}` — fetch one project.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}",
    tag = "projects",
    security(("bearer" = [])),
    params(("project_id" = Uuid, Path, description = "Project identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The project.", body = crate::projects::dto::ProjectResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn get_project(
    State(state): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
) -> AppResult<Json<ProjectResponse>> {
    let project = state.projects.get(user.user_id, project_id).await?;
    Ok(Json(project))
}

/// `PATCH /api/v1/projects/{project_id}` — partially update a project.
#[utoipa::path(
    patch,
    path = "/api/v1/projects/{project_id}",
    tag = "projects",
    security(("bearer" = [])),
    params(("project_id" = Uuid, Path, description = "Project identifier")),
    request_body = UpdateProjectRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated project.", body = crate::projects::dto::ProjectResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn update_project(
    State(state): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<UpdateProjectRequest>,
) -> AppResult<Json<ProjectResponse>> {
    let project = state
        .projects
        .update(user.user_id, project_id, payload)
        .await?;
    Ok(Json(project))
}

/// `DELETE /api/v1/projects/{project_id}` — delete a project and its tasks.
#[utoipa::path(
    delete,
    path = "/api/v1/projects/{project_id}",
    tag = "projects",
    security(("bearer" = [])),
    params(("project_id" = Uuid, Path, description = "Project identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The project and its tasks were deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn delete_project(
    State(state): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    state.projects.delete(user.user_id, project_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

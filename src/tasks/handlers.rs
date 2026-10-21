//! HTTP handlers for the task endpoints.

use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::api::extract::{Json, Path, Query};
use crate::api::middleware::AuthUser;
use crate::error::AppResult;
use crate::pagination::PaginationQuery;
use crate::state::AppState;
use crate::tasks::dto::{CreateTaskRequest, TaskPage, TaskQuery, TaskResponse, UpdateTaskRequest};
use crate::tasks::repository::TaskRepository;
use crate::tasks::service::TaskService;

/// Builds the task service from the shared state.
fn service(state: &AppState) -> TaskService {
    TaskService::new(TaskRepository::new(state.db.clone()))
}

/// `GET /api/v1/projects/{project_id}/tasks` — list a project's tasks.
#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/tasks",
    tag = "tasks",
    security(("bearer" = [])),
    params(("project_id" = Uuid, Path, description = "Project identifier"), TaskQuery, PaginationQuery),
    responses(
        (status = 200, description = "The project's tasks.", body = crate::tasks::dto::TaskPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn list_tasks(
    State(state): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
    Query(query): Query<TaskQuery>,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<Json<TaskPage>> {
    let page = service(&state)
        .list_for_project(user.user_id, project_id, query, pagination)
        .await?;
    Ok(Json(TaskPage::from(page)))
}

/// `POST /api/v1/projects/{project_id}/tasks` — create a task in a project.
#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/tasks",
    tag = "tasks",
    security(("bearer" = [])),
    params(("project_id" = Uuid, Path, description = "Project identifier")),
    request_body = CreateTaskRequest,
    responses(
        (status = 201, description = "The task was created.", body = crate::tasks::dto::TaskResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn create_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<CreateTaskRequest>,
) -> AppResult<(StatusCode, Json<TaskResponse>)> {
    let task = service(&state)
        .create(user.user_id, project_id, payload)
        .await?;
    Ok((StatusCode::CREATED, Json(task)))
}

/// `GET /api/v1/tasks/{task_id}` — fetch one task.
#[utoipa::path(
    get,
    path = "/api/v1/tasks/{task_id}",
    tag = "tasks",
    security(("bearer" = [])),
    params(("task_id" = Uuid, Path, description = "Task identifier")),
    responses(
        (status = 200, description = "The task.", body = crate::tasks::dto::TaskResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn get_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(task_id): Path<Uuid>,
) -> AppResult<Json<TaskResponse>> {
    let task = service(&state).get(user.user_id, task_id).await?;
    Ok(Json(task))
}

/// `PATCH /api/v1/tasks/{task_id}` — partially update a task.
#[utoipa::path(
    patch,
    path = "/api/v1/tasks/{task_id}",
    tag = "tasks",
    security(("bearer" = [])),
    params(("task_id" = Uuid, Path, description = "Task identifier")),
    request_body = UpdateTaskRequest,
    responses(
        (status = 200, description = "The updated task.", body = crate::tasks::dto::TaskResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn update_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(payload): Json<UpdateTaskRequest>,
) -> AppResult<Json<TaskResponse>> {
    let task = service(&state)
        .update(user.user_id, task_id, payload)
        .await?;
    Ok(Json(task))
}

/// `DELETE /api/v1/tasks/{task_id}` — delete a task.
#[utoipa::path(
    delete,
    path = "/api/v1/tasks/{task_id}",
    tag = "tasks",
    security(("bearer" = [])),
    params(("task_id" = Uuid, Path, description = "Task identifier")),
    responses(
        (status = 204, description = "The task was deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(task_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service(&state).delete(user.user_id, task_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

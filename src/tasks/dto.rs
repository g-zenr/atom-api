//! Public contracts for the `tasks` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::pagination::{Page, Pagination, PaginationQuery};
use crate::tasks::models::{Task, TaskPriority, TaskStatus};

/// Maximum accepted task title length.
///
/// Mirrors the database constraint `tasks_title_length_chk`
/// (`CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200)`) in
/// `migrations/0003_projects_and_tasks.sql`. The migration is the enforced
/// copy; this constant is the application-side mirror of it.
pub const MAX_TITLE_LENGTH: usize = 200;
/// Maximum accepted task description length.
///
/// Mirrors the database constraint `tasks_description_length_chk`
/// (`CHECK (CHAR_LENGTH(description) <= 10000)`) in
/// `migrations/0003_projects_and_tasks.sql`. The migration is the enforced
/// copy; this constant is the application-side mirror of it.
pub const MAX_DESCRIPTION_LENGTH: usize = 10_000;

/// The public representation of a task.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// The project the task belongs to.
    pub project_id: Uuid,
    /// Task title.
    pub title: String,
    /// Task description.
    pub description: String,
    /// Current lifecycle state.
    pub status: TaskStatus,
    /// Current priority.
    pub priority: TaskPriority,
    /// Manual ordering key inside the project.
    pub position: f64,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
    /// Completion timestamp. Present only when the task is done.
    pub completed_at: Option<DateTime<Utc>>,
}

/// Builds the public representation of a task.
///
/// The model is consumed rather than borrowed, so that the strings of a
/// listed page are moved into the response instead of being copied.
impl From<Task> for TaskResponse {
    fn from(task: Task) -> Self {
        Self {
            id: task.id,
            project_id: task.project_id,
            title: task.title,
            description: task.description,
            status: task.status,
            priority: task.priority,
            position: task.position,
            created_at: task.created_at,
            updated_at: task.updated_at,
            completed_at: task.completed_at,
        }
    }
}

/// Request body for creating a task.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateTaskRequest {
    /// Task title.
    #[schema(example = "Draft the authentication ADR")]
    pub title: String,
    /// Task description. Defaults to an empty string.
    #[schema(example = "Record the refresh-token rotation decision.")]
    #[serde(default)]
    pub description: String,
    /// Initial status. Defaults to `open`.
    pub status: Option<TaskStatus>,
    /// Initial priority. Defaults to `medium`.
    pub priority: Option<TaskPriority>,
    /// Manual ordering key. Defaults to the end of the list.
    pub position: Option<f64>,
}

/// Request body for partially updating a task.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateTaskRequest {
    /// New title.
    pub title: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New status. Setting `done` stamps `completed_at`; any other value clears it.
    pub status: Option<TaskStatus>,
    /// New priority.
    pub priority: Option<TaskPriority>,
    /// New ordering key.
    pub position: Option<f64>,
}

/// Query parameters accepted by `GET /api/v1/projects/{project_id}/tasks`.
///
/// Only the feature filters live here. The pagination parameters live in
/// [`crate::pagination::PaginationQuery`], and the handler reads both halves
/// from a single [`TasksListQuery`]: declaring `limit` and `offset` in both
/// places would publish them twice in the generated OpenAPI document.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaskQuery {
    /// Filter by status.
    pub status: Option<TaskStatus>,
    /// Filter by priority.
    pub priority: Option<TaskPriority>,
}

/// The whole query string of `GET /api/v1/projects/{project_id}/tasks`,
/// deserialized once.
///
/// The handler takes a single `Query<…>` extractor over this type. The query
/// string is therefore parsed exactly once, and [`Self::into_parts`] splits the
/// result into the pair the service layer already works with.
///
/// The fields are written out instead of composing [`TaskQuery`] and
/// [`PaginationQuery`] with `#[serde(flatten)]`: the extractor deserializes
/// with `serde_urlencoded`, whose map access yields strings, so a flattened
/// `Option<i64>` would fail at run time. The duplication is the price of
/// parsing the string once.
///
/// It derives `Deserialize` only, and is deliberately absent from the
/// generated OpenAPI document: the published parameter set stays [`TaskQuery`]
/// and [`PaginationQuery`], which the handler's `utoipa` attribute names.
#[derive(Debug, Clone, Deserialize)]
pub struct TasksListQuery {
    /// Maximum number of tasks to return.
    pub limit: Option<i64>,
    /// Number of tasks to skip.
    pub offset: Option<i64>,
    /// Filter by status.
    pub status: Option<TaskStatus>,
    /// Filter by priority.
    pub priority: Option<TaskPriority>,
}

impl TasksListQuery {
    /// Splits the parsed query into its filter and pagination halves.
    pub fn into_parts(self) -> (TaskQuery, PaginationQuery) {
        let Self {
            limit,
            offset,
            status,
            priority,
        } = self;

        (
            TaskQuery { status, priority },
            PaginationQuery { limit, offset },
        )
    }
}

/// The response body of `GET /api/v1/projects/{project_id}/tasks`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskPage {
    /// The tasks on this page.
    pub items: Vec<TaskResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<TaskResponse>> for TaskPage {
    fn from(page: Page<TaskResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

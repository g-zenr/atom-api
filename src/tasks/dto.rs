//! Public contracts for the `tasks` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::pagination::{Page, Pagination};
use crate::tasks::models::{Task, TaskPriority, TaskStatus};

/// Maximum accepted task title length.
pub const MAX_TITLE_LENGTH: usize = 200;
/// Maximum accepted task description length.
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

impl From<&Task> for TaskResponse {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id,
            project_id: task.project_id,
            title: task.title.clone(),
            description: task.description.clone(),
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
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaskQuery {
    /// Filter by status.
    pub status: Option<TaskStatus>,
    /// Filter by priority.
    pub priority: Option<TaskPriority>,
    /// Maximum number of items to return.
    pub limit: Option<i64>,
    /// Number of items to skip.
    pub offset: Option<i64>,
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

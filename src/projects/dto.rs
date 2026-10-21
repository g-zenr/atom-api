//! Public contracts for the `projects` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::pagination::{Page, Pagination};
use crate::projects::models::Project;

/// Maximum accepted project name length.
///
/// Mirrors the database constraint `projects_name_length_chk`
/// (`CHECK (CHAR_LENGTH(name) BETWEEN 1 AND 120)`) in
/// `migrations/0003_projects_and_tasks.sql`. The migration is the enforced
/// copy; this constant is the application-side mirror of it.
pub const MAX_NAME_LENGTH: usize = 120;

/// Maximum accepted project description length.
///
/// Mirrors the database constraint `projects_description_length_chk`
/// (`CHECK (CHAR_LENGTH(description) <= 5000)`) in
/// `migrations/0003_projects_and_tasks.sql`. The migration is the enforced
/// copy; this constant is the application-side mirror of it.
pub const MAX_DESCRIPTION_LENGTH: usize = 5_000;

/// The public representation of a project.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// Project name.
    pub name: String,
    /// Project description.
    pub description: String,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

impl From<&Project> for ProjectResponse {
    fn from(project: &Project) -> Self {
        Self {
            id: project.id,
            name: project.name.clone(),
            description: project.description.clone(),
            created_at: project.created_at,
            updated_at: project.updated_at,
        }
    }
}

/// Request body for creating a project.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    /// Project name.
    #[schema(example = "ATOM Phase 1")]
    pub name: String,
    /// Project description. Defaults to an empty string.
    #[schema(example = "Backend foundation for the personal workspace platform.")]
    #[serde(default)]
    pub description: String,
}

/// Request body for partially updating a project.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateProjectRequest {
    /// New name.
    pub name: Option<String>,
    /// New description.
    pub description: Option<String>,
}

/// Query parameters accepted by `GET /api/v1/projects`.
///
/// Only the feature filters live here. The pagination parameters live in
/// [`crate::pagination::PaginationQuery`], which the handler extracts
/// separately: declaring `limit` and `offset` in both places would publish
/// them twice in the generated OpenAPI document.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProjectQuery {
    /// Free-text search over name and description.
    #[param(example = "phase")]
    pub search: Option<String>,
}

/// The response body of `GET /api/v1/projects`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectPage {
    /// The projects on this page.
    pub items: Vec<ProjectResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<ProjectResponse>> for ProjectPage {
    fn from(page: Page<ProjectResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

//! Public contracts for the `projects` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::pagination::{Page, Pagination, PaginationQuery};
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

/// Builds the public representation of a project.
///
/// The model is consumed rather than borrowed, so that the strings of a
/// listed page are moved into the response instead of being copied.
impl From<Project> for ProjectResponse {
    fn from(project: Project) -> Self {
        Self {
            id: project.id,
            name: project.name,
            description: project.description,
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
/// [`crate::pagination::PaginationQuery`], and the handler reads both halves
/// from a single [`ProjectsListQuery`]: declaring `limit` and `offset` in both
/// places would publish them twice in the generated OpenAPI document.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProjectQuery {
    /// Free-text search over name and description.
    ///
    /// At most 200 characters. A longer term is a validation failure.
    #[param(example = "phase")]
    pub search: Option<String>,
}

/// The whole query string of `GET /api/v1/projects`, deserialized once.
///
/// The handler takes a single `Query<…>` extractor over this type. The query
/// string is therefore parsed exactly once, and [`Self::into_parts`] splits the
/// result into the pair the service layer already works with.
///
/// The fields are written out instead of composing [`ProjectQuery`] and
/// [`PaginationQuery`] with `#[serde(flatten)]`: the extractor deserializes
/// with `serde_urlencoded`, whose map access yields strings, so a flattened
/// `Option<i64>` would fail at run time. The duplication is the price of
/// parsing the string once.
///
/// It derives `Deserialize` only, and is deliberately absent from the
/// generated OpenAPI document: the published parameter set stays
/// [`ProjectQuery`] and [`PaginationQuery`], which the handler's `utoipa`
/// attribute names.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectsListQuery {
    /// Maximum number of projects to return.
    pub limit: Option<i64>,
    /// Number of projects to skip.
    pub offset: Option<i64>,
    /// Free-text search over name and description.
    pub search: Option<String>,
}

impl ProjectsListQuery {
    /// Splits the parsed query into its filter and pagination halves.
    pub fn into_parts(self) -> (ProjectQuery, PaginationQuery) {
        let Self {
            limit,
            offset,
            search,
        } = self;

        (ProjectQuery { search }, PaginationQuery { limit, offset })
    }
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

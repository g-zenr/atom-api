//! Project persistence.
//!
//! Every statement is scoped by `user_id`. Deleting a project cascades to its
//! tasks at the database level; see `docs/features/projects/specification.md`
//! for the product decision behind that behaviour.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::projects::models::Project;

/// The page statement for a listing without a search term.
///
/// The ownership predicate is written out in the statement, not injected by a
/// helper, so that a reviewer can check it at a glance (ADR-0005).
const PAGE_WITHOUT_SEARCH: &str = r#"
    SELECT id, user_id, name, description, created_at, updated_at
    FROM projects
    WHERE user_id = $1
    ORDER BY created_at DESC, id DESC
    LIMIT $2 OFFSET $3
"#;

/// The page statement for a listing with a search term.
///
/// The search clause is a static fragment: the term is a bound parameter and
/// never reaches the SQL text. The clause is *not* guarded by
/// `($2::text IS NULL OR name ILIKE ...)`, because a disjunction against a
/// parameter hides the selectivity of the filter from the planner. The
/// listing without a search term is the common case, and it is the one the
/// planner can serve from the owner's own index.
const PAGE_WITH_SEARCH: &str = r#"
    SELECT id, user_id, name, description, created_at, updated_at
    FROM projects
    WHERE user_id = $1
      AND (name ILIKE '%' || $2 || '%'
           OR description ILIKE '%' || $2 || '%')
    ORDER BY created_at DESC, id DESC
    LIMIT $3 OFFSET $4
"#;

/// The count statement for a listing without a search term.
const COUNT_WITHOUT_SEARCH: &str = r#"
    SELECT COUNT(*)
    FROM projects
    WHERE user_id = $1
"#;

/// The count statement for a listing with a search term.
///
/// The filter is the same filter as in [`PAGE_WITH_SEARCH`], so that the
/// reported total is the size of the set the page was taken from.
const COUNT_WITH_SEARCH: &str = r#"
    SELECT COUNT(*)
    FROM projects
    WHERE user_id = $1
      AND (name ILIKE '%' || $2 || '%'
           OR description ILIKE '%' || $2 || '%')
"#;

/// Persistence operations for projects.
#[derive(Debug, Clone)]
pub struct ProjectRepository {
    pool: PgPool,
}

impl ProjectRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts a project.
    pub async fn insert(
        &self,
        id: Uuid,
        user_id: Uuid,
        name: &str,
        description: &str,
    ) -> AppResult<Project> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            INSERT INTO projects (id, user_id, name, description)
            VALUES ($1, $2, $3, $4)
            RETURNING id, user_id, name, description, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;

        Ok(project)
    }

    /// Finds a project owned by `user_id`.
    pub async fn find_owned(&self, id: Uuid, user_id: Uuid) -> AppResult<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, user_id, name, description, created_at, updated_at
            FROM projects
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(project)
    }

    /// Lists the projects owned by `user_id`.
    ///
    /// The listing is two statements: the page and the count of the whole
    /// filtered set. The statement that is used depends on whether a search
    /// term is present, so that the listing without a term is served from
    /// the owner's own index.
    pub async fn list_owned(
        &self,
        user_id: Uuid,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<Project>, i64)> {
        let (items, total) = match search {
            Some(term) => {
                let items = sqlx::query_as::<_, Project>(PAGE_WITH_SEARCH)
                    .bind(user_id)
                    .bind(term)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await?;

                let total = sqlx::query_scalar::<_, i64>(COUNT_WITH_SEARCH)
                    .bind(user_id)
                    .bind(term)
                    .fetch_one(&self.pool)
                    .await?;

                (items, total)
            }
            None => {
                let items = sqlx::query_as::<_, Project>(PAGE_WITHOUT_SEARCH)
                    .bind(user_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await?;

                let total = sqlx::query_scalar::<_, i64>(COUNT_WITHOUT_SEARCH)
                    .bind(user_id)
                    .fetch_one(&self.pool)
                    .await?;

                (items, total)
            }
        };

        Ok((items, total))
    }

    /// Updates a project owned by `user_id`.
    pub async fn update_owned(
        &self,
        id: Uuid,
        user_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> AppResult<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            UPDATE projects
            SET name        = COALESCE($3, name),
                description = COALESCE($4, description),
                updated_at  = NOW()
            WHERE id = $1 AND user_id = $2
            RETURNING id, user_id, name, description, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(name)
        .bind(description)
        .fetch_optional(&self.pool)
        .await?;

        Ok(project)
    }

    /// Deletes a project owned by `user_id`.
    pub async fn delete_owned(&self, id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM projects WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

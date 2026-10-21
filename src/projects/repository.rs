//! Project persistence.
//!
//! Every statement is scoped by `user_id`. Deleting a project cascades to its
//! tasks at the database level; see `docs/features/projects/specification.md`
//! for the product decision behind that behaviour.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::projects::models::Project;

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
    pub async fn list_owned(
        &self,
        user_id: Uuid,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<Project>, i64)> {
        let search = search.map(str::trim).filter(|value| !value.is_empty());

        let items = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, user_id, name, description, created_at, updated_at
            FROM projects
            WHERE user_id = $1
              AND ($2::text IS NULL
                   OR name ILIKE '%' || $2 || '%'
                   OR description ILIKE '%' || $2 || '%')
            ORDER BY created_at DESC, id DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(user_id)
        .bind(search)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM projects
            WHERE user_id = $1
              AND ($2::text IS NULL
                   OR name ILIKE '%' || $2 || '%'
                   OR description ILIKE '%' || $2 || '%')
            "#,
        )
        .bind(user_id)
        .bind(search)
        .fetch_one(&self.pool)
        .await?;

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

//! Task persistence.
//!
//! Tasks are always reached through their project, and the project is always
//! reached through its owner. Every statement therefore carries the caller's
//! `user_id`, which makes cross-account access impossible at the SQL level
//! rather than at the handler level.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::tasks::models::{Task, TaskPriority, TaskStatus};

/// Filters applied to a task listing.
#[derive(Debug, Clone, Default)]
pub struct TaskFilters {
    pub status: Option<TaskStatus>,
    pub priority: Option<TaskPriority>,
}

/// The values needed to create a task.
#[derive(Debug, Clone)]
pub struct NewTask {
    pub id: Uuid,
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub position: Option<f64>,
}

/// The mutable fields of a task. `None` means "leave unchanged".
#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub priority: Option<TaskPriority>,
    pub position: Option<f64>,
}

/// Persistence operations for tasks.
#[derive(Debug, Clone)]
pub struct TaskRepository {
    pool: PgPool,
}

impl TaskRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts a task into a project owned by `user_id`.
    ///
    /// The ownership check and the insert are one statement, so a task can
    /// never be attached to a project the caller does not own.
    pub async fn insert(&self, task: NewTask) -> AppResult<Option<Task>> {
        let NewTask {
            id,
            user_id,
            project_id,
            title,
            description,
            status,
            priority,
            position,
        } = task;

        let inserted = sqlx::query_as::<_, Task>(
            r#"
            INSERT INTO tasks
                (id, project_id, title, description, status, priority, position, completed_at)
            SELECT $1, p.id, $4, $5, $6, $7,
                   COALESCE($8, (SELECT COALESCE(MAX(position), 0) + 1000 FROM tasks WHERE project_id = p.id)),
                   CASE WHEN $6 = 'done' THEN NOW() ELSE NULL END
            FROM projects p
            WHERE p.id = $2 AND p.user_id = $3
            ON CONFLICT DO NOTHING
            RETURNING id, project_id, title, description, status, priority, position,
                      created_at, updated_at, completed_at
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(user_id)
        .bind(title)
        .bind(description)
        .bind(status)
        .bind(priority)
        .bind(position)
        .fetch_optional(&self.pool)
        .await?;

        Ok(inserted)
    }

    /// Finds a task whose project is owned by `user_id`.
    pub async fn find_owned(&self, id: Uuid, user_id: Uuid) -> AppResult<Option<Task>> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            SELECT t.id, t.project_id, t.title, t.description, t.status, t.priority,
                   t.position, t.created_at, t.updated_at, t.completed_at
            FROM tasks t
            JOIN projects p ON p.id = t.project_id
            WHERE t.id = $1 AND p.user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(task)
    }

    /// Lists the tasks of a project owned by `user_id`.
    ///
    /// `None` means the project does not exist, or is not owned by the
    /// caller. The empty result and the missing project are told apart by a
    /// follow-up statement, so that the two cases can be answered
    /// differently; the difference is observable.
    pub async fn list_for_project(
        &self,
        user_id: Uuid,
        project_id: Uuid,
        filters: &TaskFilters,
        limit: i64,
        offset: i64,
    ) -> AppResult<Option<(Vec<Task>, i64)>> {
        let rows = sqlx::query_as::<_, Task>(
            r#"
            SELECT t.id, t.project_id, t.title, t.description, t.status, t.priority,
                   t.position, t.created_at, t.updated_at, t.completed_at
            FROM tasks t
            JOIN projects p ON p.id = t.project_id
            WHERE p.id = $1 AND p.user_id = $2
              AND ($3::task_status IS NULL OR t.status = $3)
              AND ($4::task_priority IS NULL OR t.priority = $4)
            ORDER BY t.position ASC, t.created_at ASC, t.id ASC
            LIMIT $5 OFFSET $6
            "#,
        )
        .bind(project_id)
        .bind(user_id)
        .bind(filters.status)
        .bind(filters.priority)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            // The empty page raises two questions: does the project exist,
            // and how many tasks match the filter. One statement answers
            // both, so that the empty case costs two round trips instead of
            // three. The distinction between "no such project" and "a
            // project with no tasks" is observable, so it is preserved
            // exactly.
            let (exists, total) = sqlx::query_as::<_, (bool, i64)>(
                r#"
                SELECT
                    EXISTS(SELECT 1 FROM projects WHERE id = $1 AND user_id = $2),
                    (SELECT COUNT(*)
                     FROM tasks t
                     JOIN projects p ON p.id = t.project_id
                     WHERE p.id = $1 AND p.user_id = $2
                       AND ($3::task_status IS NULL OR t.status = $3)
                       AND ($4::task_priority IS NULL OR t.priority = $4))
                "#,
            )
            .bind(project_id)
            .bind(user_id)
            .bind(filters.status)
            .bind(filters.priority)
            .fetch_one(&self.pool)
            .await?;

            if !exists {
                return Ok(None);
            }

            return Ok(Some((rows, total)));
        }

        let total = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM tasks t
            JOIN projects p ON p.id = t.project_id
            WHERE p.id = $1 AND p.user_id = $2
              AND ($3::task_status IS NULL OR t.status = $3)
              AND ($4::task_priority IS NULL OR t.priority = $4)
            "#,
        )
        .bind(project_id)
        .bind(user_id)
        .bind(filters.status)
        .bind(filters.priority)
        .fetch_one(&self.pool)
        .await?;

        Ok(Some((rows, total)))
    }

    /// Updates a task whose project is owned by `user_id`.
    ///
    /// `completed_at` is maintained by the database: it is set when the status
    /// becomes `done` and cleared when it leaves `done`.
    pub async fn update_owned(
        &self,
        id: Uuid,
        user_id: Uuid,
        patch: TaskPatch,
    ) -> AppResult<Option<Task>> {
        let TaskPatch {
            title,
            description,
            status,
            priority,
            position,
        } = patch;

        let task = sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks t
            SET title       = COALESCE($3, t.title),
                description = COALESCE($4, t.description),
                status      = COALESCE($5, t.status),
                priority    = COALESCE($6, t.priority),
                position    = COALESCE($7, t.position),
                completed_at = CASE
                    WHEN $5 = 'done' THEN COALESCE(t.completed_at, NOW())
                    WHEN $5 IS NOT NULL AND t.status = 'done' THEN NULL
                    ELSE t.completed_at
                END,
                updated_at  = NOW()
            FROM projects p
            WHERE t.id = $1 AND t.project_id = p.id AND p.user_id = $2
            RETURNING t.id, t.project_id, t.title, t.description, t.status, t.priority,
                      t.position, t.created_at, t.updated_at, t.completed_at
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(title)
        .bind(description)
        .bind(status)
        .bind(priority)
        .bind(position)
        .fetch_optional(&self.pool)
        .await?;

        Ok(task)
    }

    /// Deletes a task whose project is owned by `user_id`.
    pub async fn delete_owned(&self, id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM tasks t
            USING projects p
            WHERE t.id = $1 AND t.project_id = p.id AND p.user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

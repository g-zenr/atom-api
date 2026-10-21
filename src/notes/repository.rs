//! Note persistence.
//!
//! Every statement is scoped by `user_id`. A note belonging to another user
//! is therefore invisible to the repository, which is what makes the
//! cross-account isolation guarantee enforceable in one place.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::notes::models::Note;

/// Filters applied to a note listing.
#[derive(Debug, Clone, Default)]
pub struct NoteFilters {
    /// Free-text search term.
    pub search: Option<String>,
    /// Archive state filter. `None` excludes archived notes.
    pub archived: Option<bool>,
}

/// Persistence operations for notes.
#[derive(Debug, Clone)]
pub struct NoteRepository {
    pool: PgPool,
}

impl NoteRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts a note.
    pub async fn insert(
        &self,
        id: Uuid,
        user_id: Uuid,
        title: &str,
        content: &str,
    ) -> AppResult<Note> {
        let note = sqlx::query_as::<_, Note>(
            r#"
            INSERT INTO notes (id, user_id, title, content)
            VALUES ($1, $2, $3, $4)
            RETURNING id, user_id, title, content, is_archived, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(title)
        .bind(content)
        .fetch_one(&self.pool)
        .await?;

        Ok(note)
    }

    /// Finds a note owned by `user_id`.
    pub async fn find_owned(&self, id: Uuid, user_id: Uuid) -> AppResult<Option<Note>> {
        let note = sqlx::query_as::<_, Note>(
            r#"
            SELECT id, user_id, title, content, is_archived, created_at, updated_at
            FROM notes
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(note)
    }

    /// Lists the notes owned by `user_id`.
    ///
    /// The archive filter is tri-state: `None` and `Some(false)` both return
    /// only active notes, `Some(true)` returns only archived notes.
    pub async fn list_owned(
        &self,
        user_id: Uuid,
        filters: &NoteFilters,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<Note>, i64)> {
        let search = filters
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let items = sqlx::query_as::<_, Note>(
            r#"
            SELECT id, user_id, title, content, is_archived, created_at, updated_at
            FROM notes
            WHERE user_id = $1
              AND (COALESCE($2::boolean, FALSE) = is_archived)
              AND ($3::text IS NULL OR search_vector @@ plainto_tsquery('simple', $3))
            ORDER BY created_at DESC, id DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(user_id)
        .bind(filters.archived)
        .bind(search)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM notes
            WHERE user_id = $1
              AND (COALESCE($2::boolean, FALSE) = is_archived)
              AND ($3::text IS NULL OR search_vector @@ plainto_tsquery('simple', $3))
            "#,
        )
        .bind(user_id)
        .bind(filters.archived)
        .bind(search)
        .fetch_one(&self.pool)
        .await?;

        Ok((items, total))
    }

    /// Updates a note owned by `user_id` and returns the new state.
    pub async fn update_owned(
        &self,
        id: Uuid,
        user_id: Uuid,
        title: Option<&str>,
        content: Option<&str>,
        is_archived: Option<bool>,
    ) -> AppResult<Option<Note>> {
        let note = sqlx::query_as::<_, Note>(
            r#"
            UPDATE notes
            SET title        = COALESCE($3, title),
                content      = COALESCE($4, content),
                is_archived  = COALESCE($5, is_archived),
                updated_at   = NOW()
            WHERE id = $1 AND user_id = $2
            RETURNING id, user_id, title, content, is_archived, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(title)
        .bind(content)
        .bind(is_archived)
        .fetch_optional(&self.pool)
        .await?;

        Ok(note)
    }

    /// Deletes a note owned by `user_id`.
    pub async fn delete_owned(&self, id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM notes WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

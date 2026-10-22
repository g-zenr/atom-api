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

/// The page statement for a listing without a search term.
///
/// The ownership predicate is written out in the statement, not injected by a
/// helper, so that a reviewer can check it at a glance (ADR-0005).
///
/// The archive filter is a plain equality against a bound boolean. A runtime
/// expression such as `COALESCE($2::boolean, FALSE) = is_archived` hides the
/// selectivity of the filter from the planner, which then assumes that the
/// filter matches most of the owner's rows and stops using the index that
/// allows the listing to stop early.
const PAGE_WITHOUT_SEARCH: &str = r#"
    SELECT id, user_id, title, content, is_archived, created_at, updated_at
    FROM notes
    WHERE user_id = $1
      AND is_archived = $2
    ORDER BY created_at DESC, id DESC
    LIMIT $3 OFFSET $4
"#;

/// The page statement for a listing with a search term.
///
/// The search clause is a static fragment: the term is a bound parameter and
/// never reaches the SQL text. The clause is *not* guarded by
/// `($3::text IS NULL OR search_vector @@ ...)`, because a disjunction
/// against a parameter cannot be an index qual under a generic plan, and the
/// `notes_search_vector_idx` GIN index would then never be used. The two
/// statements are identical apart from the presence of the clause.
const PAGE_WITH_SEARCH: &str = r#"
    SELECT id, user_id, title, content, is_archived, created_at, updated_at
    FROM notes
    WHERE user_id = $1
      AND is_archived = $2
      AND search_vector @@ plainto_tsquery('simple', $3)
    ORDER BY created_at DESC, id DESC
    LIMIT $4 OFFSET $5
"#;

/// The count statement for a listing without a search term.
const COUNT_WITHOUT_SEARCH: &str = r#"
    SELECT COUNT(*)
    FROM notes
    WHERE user_id = $1
      AND is_archived = $2
"#;

/// The count statement for a listing with a search term.
///
/// The filter is the same filter as in [`PAGE_WITH_SEARCH`], so that the
/// reported total is the size of the set the page was taken from.
const COUNT_WITH_SEARCH: &str = r#"
    SELECT COUNT(*)
    FROM notes
    WHERE user_id = $1
      AND is_archived = $2
      AND search_vector @@ plainto_tsquery('simple', $3)
"#;

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
    /// only active notes, `Some(true)` returns only archived notes. The
    /// three states are resolved to two before the statement is built.
    ///
    /// The listing is two statements: the page and the count of the whole
    /// filtered set. The statement that is used depends on whether a search
    /// term is present, so that the text search can be an index qual.
    pub async fn list_owned(
        &self,
        user_id: Uuid,
        filters: &NoteFilters,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<Note>, i64)> {
        // The service has already trimmed the term and turned an empty one
        // into "no filter".
        let search = filters.search.as_deref();
        // "No archive filter" and "not archived" are the same request.
        let archived = filters.archived.unwrap_or(false);

        let (items, total) = match search {
            Some(term) => {
                let items = sqlx::query_as::<_, Note>(PAGE_WITH_SEARCH)
                    .bind(user_id)
                    .bind(archived)
                    .bind(term)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await?;

                let total = sqlx::query_scalar::<_, i64>(COUNT_WITH_SEARCH)
                    .bind(user_id)
                    .bind(archived)
                    .bind(term)
                    .fetch_one(&self.pool)
                    .await?;

                (items, total)
            }
            None => {
                let items = sqlx::query_as::<_, Note>(PAGE_WITHOUT_SEARCH)
                    .bind(user_id)
                    .bind(archived)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await?;

                let total = sqlx::query_scalar::<_, i64>(COUNT_WITHOUT_SEARCH)
                    .bind(user_id)
                    .bind(archived)
                    .fetch_one(&self.pool)
                    .await?;

                (items, total)
            }
        };

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

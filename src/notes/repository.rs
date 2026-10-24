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

/// The lexical candidate set of a hybrid search.
///
/// It is deliberately the **same** full-text predicate as [`PAGE_WITH_SEARCH`],
/// so the two modes cannot disagree about what matches, and the same order, so
/// the two pages are drawn from the same sequence. Only the identifiers are
/// read: the bodies are loaded for the page that is actually returned, which
/// keeps the memory of one hybrid search bounded by `ml.candidate_limit`
/// identifiers instead of `ml.candidate_limit` note bodies.
const LEXICAL_CANDIDATES: &str = r#"
    SELECT id
    FROM notes
    WHERE user_id = $1
      AND is_archived = FALSE
      AND search_vector @@ plainto_tsquery('simple', $2)
    ORDER BY created_at DESC, id DESC
    LIMIT $3
"#;

/// Loads the notes of one page by identifier.
///
/// The predicate is `user_id = $1` and not only `id = ANY($2)`: the identifier
/// list was produced by an owner-scoped query, and it is scoped again here,
/// because a list is an input and not an authorisation.
const FIND_MANY_OWNED: &str = r#"
    SELECT id, user_id, title, content, is_archived, created_at, updated_at
    FROM notes
    WHERE user_id = $1
      AND id = ANY($2)
"#;

/// Every note of the caller, for a reindex.
///
/// The order is `id`, which is stable, so two runs visit the sources in the
/// same sequence and an interrupted run resumes in a predictable place.
const EMBEDDABLE_OWNED: &str = r#"
    SELECT id, user_id, title, content, is_archived, created_at, updated_at
    FROM notes
    WHERE user_id = $1
    ORDER BY id
"#;

/// Deletes a note owned by the caller, and its embeddings, in one statement.
///
/// `embeddings.source_id` is polymorphic and carries no foreign key
/// ([`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §3), so
/// the cascade the schema cannot express is written here. One statement means
/// one transaction, which is the only shape that makes the guarantee real: two
/// repository calls would be two commits with an orphan between them.
///
/// The trailing `SELECT EXISTS` returns the scalar the method already returns;
/// `rows_affected()` cannot be read from a statement that ends in a `SELECT`.
const DELETE_NOTE_OWNED_WITH_EMBEDDINGS: &str = r#"
    WITH removed AS (
        DELETE FROM notes
         WHERE id = $1 AND user_id = $2
         RETURNING id
    ), purged AS (
        DELETE FROM embeddings
         WHERE user_id = $2
           AND source_type = 'note'
           AND source_id IN (SELECT id FROM removed)
    )
    SELECT EXISTS (SELECT 1 FROM removed)
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
    ///
    /// The note and its embeddings are removed by one statement, so a reader
    /// can never observe the note gone and its vector still present. The
    /// embeddings of `source_type = 'note'` are the only ones touched: the
    /// column is polymorphic, so the predicate is what names the table the
    /// identifiers belong to.
    pub async fn delete_owned(&self, id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let removed = sqlx::query_scalar::<_, bool>(DELETE_NOTE_OWNED_WITH_EMBEDDINGS)
            .bind(id)
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(removed)
    }

    /// The identifiers of the caller's lexical matches, newest first.
    ///
    /// The set is bounded by `limit`, which is `ml.candidate_limit`: the
    /// re-rank in Rust is therefore bounded by configuration, never by the size
    /// of the corpus.
    pub async fn lexical_candidates(
        &self,
        user_id: Uuid,
        search: &str,
        limit: i64,
    ) -> AppResult<Vec<Uuid>> {
        let ids = sqlx::query_scalar::<_, Uuid>(LEXICAL_CANDIDATES)
            .bind(user_id)
            .bind(search)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(ids)
    }

    /// Loads the caller's notes with the given identifiers.
    ///
    /// The result is unordered: the caller already has the order it asked for,
    /// and re-sorting here would make the ranking depend on the database's
    /// collation.
    pub async fn find_many_owned(&self, user_id: Uuid, ids: &[Uuid]) -> AppResult<Vec<Note>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let notes = sqlx::query_as::<_, Note>(FIND_MANY_OWNED)
            .bind(user_id)
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;

        Ok(notes)
    }

    /// Every note of the caller, for a reindex.
    pub async fn embeddable_owned(&self, user_id: Uuid) -> AppResult<Vec<Note>> {
        let notes = sqlx::query_as::<_, Note>(EMBEDDABLE_OWNED)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(notes)
    }
}

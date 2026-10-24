//! Persistence for the `embeddings` table.
//!
//! Every statement is scoped by `user_id`, which is what makes an embedding as
//! private as the note it was derived from: a vector is a compressed
//! representation of the note, so a statement that could read another account's
//! row would be a data leak even if the row were never returned
//! ([`ADR-0005`](../../docs/decisions/ADR-0005-ownership-enforced-in-sql.md)).
//!
//! Two rules from
//! [`docs/features/ml/architecture.md`](../../docs/features/ml/architecture.md)
//! §3 are enforced by the statements rather than by their callers:
//!
//! * **The write proves the source belongs to the caller.** `source_id` is
//!   polymorphic and carries no foreign key, so the insert is an `INSERT …
//!   SELECT … WHERE EXISTS (…)` over the source table. A source that vanished
//!   between the read and the write inserts nothing instead of an orphan.
//! * **The read never re-normalises.** The stored vector is unit length by
//!   construction (ADR-0008 §9); a row whose length is not `dim` is skipped by
//!   the service, not repaired here.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;

/// What an embedding was derived from.
///
/// The column is `TEXT` with a `CHECK`, not a PostgreSQL enum
/// ([`schema.md`](../../docs/database/schema.md) §11.7): the value is a closed
/// set of two, and an enum type would be a second migration to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceType {
    /// A row of `notes`.
    Note,
    /// A row of `job_applications`.
    Application,
}

impl SourceType {
    /// The stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Application => "application",
        }
    }

    /// Parses a stored value. An unknown value is `None`; the `CHECK` makes one
    /// unreachable, so it is a defensive branch rather than a state.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "note" => Some(Self::Note),
            "application" => Some(Self::Application),
            _ => None,
        }
    }
}

/// One stored vector, as the ranker reads it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredVector {
    /// The note or application the vector describes.
    pub source_id: Uuid,
    /// The vector. Unit length by construction; the service still checks the
    /// length against `dim`.
    pub vector: Vec<f32>,
}

/// The caller's note vectors, for `semantic` and for a match.
///
/// The join is what makes `pagination.total` honest in `semantic` mode: a row
/// whose note is gone (an orphan a delete left behind) is not a note the caller
/// can be shown, so it is not counted. Archived notes are excluded for the same
/// reason `lexical` mode excludes them — the three modes must not disagree
/// about which notes are candidates.
///
/// `e.source_id = n.id AND n.user_id = e.user_id` is the ownership predicate of
/// the join; `e.user_id = $1` is the predicate of the scan.
const NOTE_VECTORS: &str = r#"
    SELECT e.source_id, e.vector
    FROM embeddings e
    JOIN notes n
      ON n.id = e.source_id
     AND n.user_id = e.user_id
    WHERE e.user_id = $1
      AND e.source_type = 'note'
      AND e.model_id = $2
      AND n.is_archived = FALSE
    ORDER BY e.source_id
"#;

/// The vectors of one bounded candidate set, for `hybrid`.
///
/// The candidate identifiers were produced by the owner-scoped lexical query,
/// and this statement scopes them again: a candidate list is an input, not an
/// authorisation.
const VECTORS_FOR_SOURCES: &str = r#"
    SELECT source_id, vector
    FROM embeddings
    WHERE user_id = $1
      AND source_type = $2
      AND model_id = $3
      AND source_id = ANY($4)
    ORDER BY source_id
"#;

/// The keys of the rows a reindex may skip.
const CURRENT_SOURCES: &str = r#"
    SELECT source_type, source_id
    FROM embeddings
    WHERE user_id = $1
      AND model_id = $2
      AND dim = $3
    ORDER BY source_type, source_id
"#;

/// Inserts or replaces a note's vector, but only for a note the caller owns.
///
/// The `WHERE EXISTS` is the whole ownership check of the write path: the
/// polymorphic column has no foreign key to prove it, so the statement reads the
/// source table itself. `rows_affected()` is therefore `0` for a note that is
/// absent, foreign or was deleted in the meantime — a normal outcome for a
/// reindex that races a delete, never an error.
const UPSERT_NOTE: &str = r#"
    INSERT INTO embeddings (id, user_id, source_type, source_id, model_id, dim, vector)
    SELECT $1, $2, 'note', $3, $4, $5, $6
    WHERE EXISTS (
        SELECT 1 FROM notes WHERE id = $3 AND user_id = $2
    )
    ON CONFLICT (user_id, source_type, source_id, model_id)
    DO UPDATE SET vector = EXCLUDED.vector, dim = EXCLUDED.dim, updated_at = NOW()
"#;

/// The same statement for an application.
const UPSERT_APPLICATION: &str = r#"
    INSERT INTO embeddings (id, user_id, source_type, source_id, model_id, dim, vector)
    SELECT $1, $2, 'application', $3, $4, $5, $6
    WHERE EXISTS (
        SELECT 1 FROM job_applications WHERE id = $3 AND user_id = $2
    )
    ON CONFLICT (user_id, source_type, source_id, model_id)
    DO UPDATE SET vector = EXCLUDED.vector, dim = EXCLUDED.dim, updated_at = NOW()
"#;

/// Deletes the caller's embeddings whose source no longer exists.
///
/// This is the backstop the polymorphic column needs
/// ([`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §3): the
/// source delete purges its own rows, and this sweep removes whatever a delete
/// outside the service left behind. It is owner-scoped, so it can never remove
/// another account's row even when both point at the same identifier.
const DELETE_ORPHANS: &str = r#"
    WITH removed AS (
        DELETE FROM embeddings e
         WHERE e.user_id = $1
           AND (
                (e.source_type = 'note'
                 AND NOT EXISTS (
                     SELECT 1 FROM notes n WHERE n.id = e.source_id AND n.user_id = $1
                 ))
             OR (e.source_type = 'application'
                 AND NOT EXISTS (
                     SELECT 1 FROM job_applications a
                      WHERE a.id = e.source_id AND a.user_id = $1
                 ))
           )
         RETURNING 1
    )
    SELECT COUNT(*) FROM removed
"#;

/// Persistence operations for embeddings.
#[derive(Debug, Clone)]
pub struct MlRepository {
    pool: PgPool,
}

impl MlRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Every note vector of the caller for one model.
    pub async fn note_vectors(&self, owner: Uuid, model_id: &str) -> AppResult<Vec<StoredVector>> {
        let rows = sqlx::query_as::<_, StoredVector>(NOTE_VECTORS)
            .bind(owner)
            .bind(model_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows)
    }

    /// The vectors of one bounded candidate set.
    pub async fn vectors_for_sources(
        &self,
        owner: Uuid,
        source_type: SourceType,
        model_id: &str,
        source_ids: &[Uuid],
    ) -> AppResult<Vec<StoredVector>> {
        let rows = sqlx::query_as::<_, StoredVector>(VECTORS_FOR_SOURCES)
            .bind(owner)
            .bind(source_type.as_str())
            .bind(model_id)
            .bind(source_ids)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows)
    }

    /// The `(source_type, source_id)` keys the caller already has for one model
    /// and dimension. A reindex skips them.
    pub async fn current_sources(
        &self,
        owner: Uuid,
        model_id: &str,
        dimension: i32,
    ) -> AppResult<Vec<(String, Uuid)>> {
        let rows = sqlx::query_as::<_, (String, Uuid)>(CURRENT_SOURCES)
            .bind(owner)
            .bind(model_id)
            .bind(dimension)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows)
    }

    /// Writes one note vector, if the note still belongs to the caller.
    ///
    /// Returns `true` when a row was written or replaced.
    pub async fn upsert_note(
        &self,
        owner: Uuid,
        note_id: Uuid,
        model_id: &str,
        vector: &[f32],
    ) -> AppResult<bool> {
        self.upsert(
            UPSERT_NOTE,
            Uuid::new_v4(),
            owner,
            note_id,
            model_id,
            vector,
        )
        .await
    }

    /// Writes one application vector, if the application still belongs to the
    /// caller.
    pub async fn upsert_application(
        &self,
        owner: Uuid,
        application_id: Uuid,
        model_id: &str,
        vector: &[f32],
    ) -> AppResult<bool> {
        self.upsert(
            UPSERT_APPLICATION,
            Uuid::new_v4(),
            owner,
            application_id,
            model_id,
            vector,
        )
        .await
    }

    /// Removes the caller's embeddings whose source is gone, returning how many
    /// rows were deleted.
    pub async fn delete_orphans(&self, owner: Uuid) -> AppResult<i64> {
        let deleted = sqlx::query_scalar::<_, i64>(DELETE_ORPHANS)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok(deleted)
    }

    /// The shared half of the two guarded upserts.
    async fn upsert(
        &self,
        statement: &str,
        id: Uuid,
        owner: Uuid,
        source_id: Uuid,
        model_id: &str,
        vector: &[f32],
    ) -> AppResult<bool> {
        // The declared dimension is the array's length, not the configured one:
        // the statement binds one vector, and `embeddings_dimension_chk` is what
        // would reject a mismatch. `i32` is the column's type.
        let dimension = i32::try_from(vector.len()).unwrap_or(i32::MAX);

        let result = sqlx::query(statement)
            .bind(id)
            .bind(owner)
            .bind(source_id)
            .bind(model_id)
            .bind(dimension)
            .bind(vector.to_vec())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

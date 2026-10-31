//! Refresh-session persistence.
//!
//! Only the SHA-256 hash of a refresh token is stored. Rotation links the
//! superseded session to its replacement so that reuse of an already-rotated
//! token can be detected.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::models::RefreshSession;
use crate::error::AppResult;

/// The values of a refresh session about to be stored.
///
/// The first session of a token pair and the replacement of a rotation carry
/// the same fields, so the plain insert and the transactional rotation share
/// one parameter shape and cannot drift apart.
#[derive(Debug, Clone, Copy)]
pub struct NewRefreshSession<'a> {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: &'a str,
    pub expires_at: DateTime<Utc>,
    pub user_agent: Option<&'a str>,
    pub ip_address: Option<&'a str>,
}

/// Persistence operations for refresh sessions.
#[derive(Debug, Clone)]
pub struct RefreshSessionRepository {
    pool: PgPool,
}

impl RefreshSessionRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Stores a new refresh session.
    pub async fn insert(&self, session: NewRefreshSession<'_>) -> AppResult<RefreshSession> {
        Self::insert_on(&self.pool, session).await
    }

    /// Stores a new refresh session on an explicit connection or transaction.
    ///
    /// The transactional rotation shares this statement with the plain
    /// insert, so the two cannot drift apart.
    async fn insert_on<'e, E>(
        executor: E,
        session: NewRefreshSession<'_>,
    ) -> AppResult<RefreshSession>
    where
        E: sqlx::PgExecutor<'e>,
    {
        let stored = sqlx::query_as::<_, RefreshSession>(
            r#"
            INSERT INTO refresh_sessions
                (id, user_id, token_hash, expires_at, user_agent, ip_address)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, user_id, token_hash, expires_at, revoked_at, created_at,
                      replaced_by_session_id, user_agent, ip_address
            "#,
        )
        .bind(session.id)
        .bind(session.user_id)
        .bind(session.token_hash)
        .bind(session.expires_at)
        .bind(session.user_agent)
        .bind(session.ip_address)
        .fetch_one(executor)
        .await?;

        Ok(stored)
    }

    /// Finds a session by its token hash.
    pub async fn find_by_token_hash(&self, token_hash: &str) -> AppResult<Option<RefreshSession>> {
        let session = sqlx::query_as::<_, RefreshSession>(
            r#"
            SELECT id, user_id, token_hash, expires_at, revoked_at, created_at,
                   replaced_by_session_id, user_agent, ip_address
            FROM refresh_sessions
            WHERE token_hash = $1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    /// Claims an active session for rotation on an explicit connection or
    /// transaction.
    ///
    /// The update only matches a session that has not been revoked, so two
    /// concurrent requests presenting the same refresh token cannot both
    /// succeed: the loser sees zero affected rows. The owner is part of the
    /// predicate, so the mutation stays scoped to the caller's own row.
    ///
    /// Returns `true` when the session was claimed by this call.
    async fn claim<'e, E>(
        executor: E,
        id: Uuid,
        user_id: Uuid,
        replaced_by: Uuid,
    ) -> AppResult<bool>
    where
        E: sqlx::PgExecutor<'e>,
    {
        let result = sqlx::query(
            r#"
            UPDATE refresh_sessions
            SET revoked_at = NOW(), replaced_by_session_id = $3
            WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(replaced_by)
        .execute(executor)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    /// Rotates a session inside one transaction.
    ///
    /// Inserts the replacement session and claims the presented session as a
    /// single unit of work, so a rotation is all-or-nothing: the two
    /// statements either both land or neither does.
    ///
    /// The claim only matches a session that is still active, so two
    /// concurrent requests presenting the same refresh token cannot both
    /// succeed. When the claim loses that race it matches zero rows and the
    /// transaction is rolled back, so the replacement inserted moments
    /// earlier does not survive: no orphan replacement row is ever left
    /// behind.
    ///
    /// Returns `true` when this call won the rotation and `false` when the
    /// presented session had already been claimed by another request. The
    /// caller treats `false` as reuse.
    pub async fn rotate(
        &self,
        id: Uuid,
        user_id: Uuid,
        replacement: NewRefreshSession<'_>,
    ) -> AppResult<bool> {
        let mut transaction = self.pool.begin().await?;

        // The replacement row is written first, because the rotation link is
        // a foreign key to it.
        Self::insert_on(&mut *transaction, replacement).await?;

        // Claim the presented session inside the same transaction.
        let claimed = Self::claim(&mut *transaction, id, user_id, replacement.id).await?;

        if !claimed {
            // The lost race: undo the replacement insert, leave the presented
            // session exactly as it was found, and let the caller run the
            // reuse-detection path.
            transaction.rollback().await?;
            return Ok(false);
        }

        transaction.commit().await?;

        Ok(true)
    }

    /// Revokes a session that belongs to `user_id`.
    ///
    /// The owner is part of the predicate, so the repository cannot revoke a
    /// session that belongs to somebody else even if it is handed a foreign
    /// session id.
    pub async fn revoke(&self, id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query(
            "UPDATE refresh_sessions SET revoked_at = NOW() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    /// Revokes every active session belonging to a user.
    ///
    /// Used on logout-all and when token reuse is detected.
    pub async fn revoke_all_for_user(&self, user_id: Uuid) -> AppResult<u64> {
        let result = sqlx::query(
            "UPDATE refresh_sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Deletes sessions that expired more than `grace` ago.
    pub async fn delete_expired(&self, grace: chrono::Duration) -> AppResult<u64> {
        let cutoff = Utc::now() - grace;
        let result = sqlx::query("DELETE FROM refresh_sessions WHERE expires_at < $1")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}

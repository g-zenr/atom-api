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
    pub async fn insert(
        &self,
        id: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> AppResult<RefreshSession> {
        let session = sqlx::query_as::<_, RefreshSession>(
            r#"
            INSERT INTO refresh_sessions
                (id, user_id, token_hash, expires_at, user_agent, ip_address)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, user_id, token_hash, expires_at, revoked_at, created_at,
                      replaced_by_session_id, user_agent, ip_address
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(user_agent)
        .bind(ip_address)
        .fetch_one(&self.pool)
        .await?;

        Ok(session)
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

    /// Atomically claims an active session for rotation.
    ///
    /// The update only matches a session that has not been revoked, so two
    /// concurrent requests presenting the same refresh token cannot both
    /// succeed: the loser sees zero affected rows.
    ///
    /// Returns `true` when the session was claimed by this call.
    pub async fn claim(&self, id: Uuid, user_id: Uuid, replaced_by: Uuid) -> AppResult<bool> {
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
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
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

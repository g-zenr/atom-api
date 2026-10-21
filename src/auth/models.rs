//! Persistence models for the `auth` module.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// A refresh-session row.
#[derive(Debug, Clone, FromRow)]
pub struct RefreshSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub replaced_by_session_id: Option<Uuid>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
}

impl RefreshSession {
    /// Whether the session may still be exchanged for tokens.
    pub fn is_usable_at(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }
}

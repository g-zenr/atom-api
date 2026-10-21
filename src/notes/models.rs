//! Persistence models for the `notes` module.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// A note row.
#[derive(Debug, Clone, FromRow)]
pub struct Note {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub content: String,
    pub is_archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

//! Persistence models for the `projects` module.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// A project row.
#[derive(Debug, Clone, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

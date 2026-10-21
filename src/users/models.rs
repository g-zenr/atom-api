//! Persistence models for the `users` module.
//!
//! These structs mirror table columns and are never returned to clients
//! directly. The public representation lives in `dto.rs`.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// A user row.
#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

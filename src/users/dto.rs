//! Public contracts for the `users` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::users::models::User;

/// The public representation of a user.
///
/// It deliberately contains no password hash and no session material.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// Normalized email address.
    pub email: String,
    /// Display name chosen by the user.
    pub display_name: String,
    /// Whether the account is enabled.
    pub is_active: bool,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            is_active: user.is_active,
            created_at: user.created_at,
        }
    }
}

/// Request body for registration.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// Email address. Normalized to lower case before storage.
    #[schema(example = "ada@example.com")]
    pub email: String,
    /// Plain-text password. Never logged.
    #[schema(example = "a-long-passphrase")]
    pub password: String,
    /// Display name.
    #[schema(example = "Ada")]
    pub display_name: String,
}

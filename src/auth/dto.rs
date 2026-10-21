//! Public contracts for the `auth` module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::users::dto::UserResponse;

/// Request body for `POST /api/v1/auth/login`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Registered email address.
    #[schema(example = "ada@example.com")]
    pub email: String,
    /// Plain-text password.
    #[schema(example = "a-long-passphrase")]
    pub password: String,
}

/// Request body for `POST /api/v1/auth/refresh`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RefreshRequest {
    /// The refresh token issued earlier.
    #[schema(example = "0nQ8...")]
    pub refresh_token: String,
}

/// Request body for `POST /api/v1/auth/logout`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LogoutRequest {
    /// The refresh token to revoke.
    #[schema(example = "0nQ8...")]
    pub refresh_token: String,
}

/// The token pair returned by register, login and refresh.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TokenPair {
    /// Short-lived bearer token for the `Authorization` header.
    pub access_token: String,
    /// Opaque, single-use refresh token.
    pub refresh_token: String,
    /// Access token type. Always `Bearer`.
    pub token_type: String,
    /// Access token lifetime in seconds.
    pub expires_in: i64,
    /// Absolute expiry of the access token.
    pub expires_at: DateTime<Utc>,
}

/// The response body of register, login and refresh.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthResponse {
    /// The issued tokens.
    pub tokens: TokenPair,
    /// The authenticated user.
    pub user: UserResponse,
}

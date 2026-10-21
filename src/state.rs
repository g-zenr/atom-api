//! Shared application state.
//!
//! `AppState` is cheap to clone (all fields are `Arc`-backed) and is passed
//! explicitly to handlers. There is no global mutable state.

use std::sync::Arc;

use sqlx::PgPool;

use crate::auth::password::PasswordHasher;
use crate::auth::repository::RefreshSessionRepository;
use crate::auth::service::AuthService;
use crate::auth::token::TokenService;
use crate::config::Config;
use crate::ratelimit::RateLimiter;
use crate::users::repository::UserRepository;
use crate::users::service::UserService;

/// Dependencies shared by every handler.
#[derive(Clone)]
pub struct AppState {
    /// Validated, immutable configuration.
    pub config: Arc<Config>,
    /// Connection pool for PostgreSQL.
    pub db: PgPool,
    /// In-process rate limiter used by the authentication endpoints.
    pub rate_limiter: Arc<RateLimiter>,
    /// Access-token issuing and verification.
    pub tokens: Arc<TokenService>,
    /// Argon2id password hashing.
    pub passwords: Arc<PasswordHasher>,
    /// Authentication use cases.
    pub auth: Arc<AuthService>,
}

impl AppState {
    /// Builds the shared state from a validated configuration and a pool.
    pub fn new(config: Config, db: PgPool) -> Self {
        let rate_limiter = Arc::new(RateLimiter::new(
            config.rate_limit.max_requests,
            config.rate_limit.window,
        ));
        let tokens = Arc::new(TokenService::new(
            &config.auth.jwt_secret,
            &config.auth.issuer,
            &config.auth.audience,
            config.auth.access_token_ttl,
        ));
        let passwords = Arc::new(PasswordHasher::new(config.auth.argon2));

        let user_repository = UserRepository::new(db.clone());
        let user_service = UserService::new(user_repository.clone());
        let session_repository = RefreshSessionRepository::new(db.clone());

        let auth = Arc::new(AuthService::new(
            user_service,
            user_repository,
            session_repository,
            tokens.clone(),
            passwords.clone(),
            config.auth.refresh_token_ttl,
        ));

        Self {
            config: Arc::new(config),
            db,
            rate_limiter,
            tokens,
            passwords,
            auth,
        }
    }

    /// Convenience accessor for the authentication use cases.
    pub fn auth_service(&self) -> &AuthService {
        &self.auth
    }

    /// Deletes refresh sessions that expired more than `grace` ago.
    ///
    /// Returns the number of removed rows.
    pub async fn prune_expired_sessions(
        &self,
        grace: chrono::Duration,
    ) -> Result<u64, sqlx::Error> {
        self.auth
            .prune_expired_sessions(grace)
            .await
            .map_err(|error| match error {
                crate::error::AppError::Database(error) => error,
                other => sqlx::Error::Protocol(other.to_string()),
            })
    }
}

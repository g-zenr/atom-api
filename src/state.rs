//! Shared application state.
//!
//! `AppState` is cheap to clone (all fields are `Arc`-backed or hold a
//! `PgPool` handle) and is passed explicitly to handlers. There is no global
//! mutable state.
//!
//! The services are built **once**, at start-up, and are never rebuilt per
//! request. The configuration therefore cannot change while the process runs.

use std::sync::Arc;

use sqlx::PgPool;

use crate::auth::password::PasswordHasher;
use crate::auth::repository::RefreshSessionRepository;
use crate::auth::service::AuthService;
use crate::auth::token::TokenService;
use crate::config::Config;
use crate::notes::repository::NoteRepository;
use crate::notes::service::NoteService;
use crate::projects::repository::ProjectRepository;
use crate::projects::service::ProjectService;
use crate::ratelimit::RateLimiter;
use crate::tasks::repository::TaskRepository;
use crate::tasks::service::TaskService;
use crate::users::repository::UserRepository;
use crate::users::service::UserService;

/// Dependencies shared by every handler.
#[derive(Clone)]
pub struct AppState {
    /// The validated, immutable configuration.
    pub config: Arc<Config>,
    /// The connection pool for PostgreSQL.
    pub db: PgPool,
    /// The in-process rate limiter used by the authentication endpoints.
    pub rate_limiter: Arc<RateLimiter>,
    /// Access-token issuing and verification.
    pub tokens: Arc<TokenService>,
    /// Argon2id password hashing.
    pub passwords: Arc<PasswordHasher>,
    /// The authentication use cases.
    pub auth: AuthService,
    /// The note use cases.
    pub notes: NoteService,
    /// The project use cases.
    pub projects: ProjectService,
    /// The task use cases.
    pub tasks: TaskService,
}

impl AppState {
    /// Builds the shared state from a validated configuration and a pool.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::AppError`] when the password hasher cannot be
    /// built from the configured Argon2 parameters.
    pub fn new(config: Config, db: PgPool) -> Result<Self, crate::error::AppError> {
        let messages = config.messages.clone();

        let rate_limiter = Arc::new(RateLimiter::new(
            config.api.rate_limit.max_requests,
            config.api.rate_limit.window,
        ));

        let tokens = Arc::new(TokenService::new(
            &config.auth.jwt_secret,
            &config.auth.issuer,
            &config.auth.audience,
            config.auth.access_token_ttl,
            config.auth.clock_skew,
        ));

        let passwords = Arc::new(PasswordHasher::new(config.auth.argon2)?);

        let user_repository = UserRepository::new(db.clone());
        let user_service = UserService::new(
            user_repository.clone(),
            messages.clone(),
            config.auth.password,
        );

        let auth = AuthService::new(
            user_service,
            tokens.clone(),
            passwords.clone(),
            &db,
            messages.clone(),
            config.auth.refresh_token_ttl,
        );

        let notes = NoteService::new(NoteRepository::new(db.clone()), messages.clone());
        let projects = ProjectService::new(ProjectRepository::new(db.clone()), messages.clone());
        let tasks = TaskService::new(TaskRepository::new(db.clone()), messages.clone());

        Ok(Self {
            config: Arc::new(config),
            db,
            rate_limiter,
            tokens,
            passwords,
            auth,
            notes,
            projects,
            tasks,
        })
    }

    /// The authentication use cases.
    pub fn auth_service(&self) -> &AuthService {
        &self.auth
    }

    /// The refresh-session repository.
    pub fn sessions(&self) -> RefreshSessionRepository {
        RefreshSessionRepository::new(self.db.clone())
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

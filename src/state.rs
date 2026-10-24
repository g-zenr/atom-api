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
use crate::auth::service::AuthService;
use crate::auth::token::TokenService;
use crate::config::Config;
use crate::job_applications::repository::JobRepository;
use crate::job_applications::service::JobService;
use crate::ml::MlService;
use crate::ml::repository::MlRepository;
use crate::notes::repository::NoteRepository;
use crate::notes::service::NoteService;
use crate::projects::repository::ProjectRepository;
use crate::projects::service::ProjectService;
use crate::ratelimit::RateLimiter;
use crate::reimbursements::repository::ClaimRepository;
use crate::reimbursements::service::ClaimService;
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
    /// The authentication use cases.
    pub auth: AuthService,
    /// The note use cases.
    pub notes: NoteService,
    /// The project use cases.
    pub projects: ProjectService,
    /// The task use cases.
    pub tasks: TaskService,
    /// The expense claim use cases.
    pub reimbursements: ClaimService,
    /// The company, job-application and event use cases.
    pub job_applications: JobService,
    /// The machine-learning use cases and the embedder selected at start-up.
    pub ml: MlService,
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
            tokens,
            passwords,
            &db,
            messages.clone(),
            config.auth.refresh_token_ttl,
        );

        let note_repository = NoteRepository::new(db.clone());
        let job_repository = JobRepository::new(db.clone());

        let notes = NoteService::new(
            note_repository.clone(),
            messages.clone(),
            config.api.pagination,
        );

        // The embedder is selected exactly once, here. `select` cannot fail:
        // a missing model file is the `Disabled` state, not a start-up error
        // (ADR-0008 §8.3).
        let ml = MlService::new(
            MlRepository::new(db.clone()),
            note_repository,
            job_repository.clone(),
            crate::ml::embedder::select(&config.ml),
            messages.clone(),
            config.ml.clone(),
            config.api.pagination,
        );
        let projects = ProjectService::new(
            ProjectRepository::new(db.clone()),
            messages.clone(),
            config.api.pagination,
        );
        let tasks = TaskService::new(
            TaskRepository::new(db.clone()),
            messages.clone(),
            config.api.pagination,
        );
        let reimbursements = ClaimService::new(
            ClaimRepository::new(db.clone()),
            messages.clone(),
            config.api.pagination,
        );
        let job_applications =
            JobService::new(job_repository, messages.clone(), config.api.pagination);

        Ok(Self {
            config: Arc::new(config),
            db,
            rate_limiter,
            auth,
            notes,
            projects,
            tasks,
            reimbursements,
            job_applications,
            ml,
        })
    }
}

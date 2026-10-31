//! Authentication use cases.
//!
//! Owns registration, login, refresh-token rotation, logout and session
//! revocation. The rules enforced here are recorded in
//! `docs/features/authentication/specification.md`.
//!
//! Every client-facing string comes from the message catalogue, so the wording
//! of an error is defined once for the whole API.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::dto::{AuthResponse, LoginRequest, TokenPair};
use crate::auth::password::PasswordHasher;
use crate::auth::repository::{NewRefreshSession, RefreshSessionRepository};
use crate::auth::token::{
    IssuedRefreshToken, TokenService, generate_refresh_token, hash_refresh_token,
};
use crate::config::messages::Messages;
use crate::error::{AppError, AppResult};
use crate::users::dto::{RegisterRequest, UserResponse};
use crate::users::service::UserService;

/// Client metadata recorded alongside a session.
#[derive(Debug, Clone, Default)]
pub struct ClientMeta {
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
}

/// Authentication use cases.
#[derive(Clone)]
pub struct AuthService {
    users: UserService,
    sessions: RefreshSessionRepository,
    tokens: Arc<TokenService>,
    passwords: Arc<PasswordHasher>,
    messages: Arc<Messages>,
    refresh_ttl: Duration,
}

// The service holds a signing secret and a password hasher, so the derived
// representation would print more than it should.
impl std::fmt::Debug for AuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthService")
            .field("refresh_ttl", &self.refresh_ttl)
            .finish_non_exhaustive()
    }
}

impl AuthService {
    /// Builds the service.
    pub fn new(
        users: UserService,
        tokens: Arc<TokenService>,
        passwords: Arc<PasswordHasher>,
        pool: &PgPool,
        messages: Arc<Messages>,
        refresh_ttl: Duration,
    ) -> Self {
        Self {
            users,
            sessions: RefreshSessionRepository::new(pool.clone()),
            tokens,
            passwords,
            messages,
            refresh_ttl,
        }
    }

    /// Registers a new user and issues an initial token pair.
    pub async fn register(
        &self,
        request: RegisterRequest,
        meta: &ClientMeta,
    ) -> AppResult<AuthResponse> {
        let email = self.users.validate_email(&request.email)?;
        self.users.validate_password(&request.password)?;
        let display_name = self.users.validate_display_name(&request.display_name)?;

        // Argon2id is CPU-bound. Hashing it on a Tokio worker thread would
        // stall that worker for the whole hash, so it is moved to the blocking
        // pool. See ADR-0002: the rule that a request path performs no
        // blocking work is load-bearing.
        let password_hash = self.hash_password(&request.password).await?;

        let user = self
            .users
            .create_user(email, password_hash, display_name)
            .await?;

        let tokens = self.issue_token_pair(user.id, &user.email, meta).await?;

        Ok(AuthResponse {
            tokens,
            user: UserResponse::from(&user),
        })
    }

    /// Authenticates a user and issues a token pair.
    pub async fn login(&self, request: LoginRequest, meta: &ClientMeta) -> AppResult<AuthResponse> {
        let email = UserService::normalize_email(&request.email);

        let user = self.users.find_user_by_email(&email).await?;

        let Some(user) = user else {
            // Spend the same time hashing as a real attempt would, so that the
            // response time does not reveal whether the account exists.
            self.verify_password(&request.password, self.passwords.dummy_hash())
                .await?;
            return Err(AppError::Unauthorized(
                self.messages.auth.invalid_credentials.clone(),
            ));
        };

        if !self
            .verify_password(&request.password, &user.password_hash)
            .await?
        {
            return Err(AppError::Unauthorized(
                self.messages.auth.invalid_credentials.clone(),
            ));
        }

        if !user.is_active {
            return Err(AppError::Forbidden(
                self.messages.auth.account_disabled.clone(),
            ));
        }

        let tokens = self.issue_token_pair(user.id, &user.email, meta).await?;

        Ok(AuthResponse {
            tokens,
            user: UserResponse::from(&user),
        })
    }

    /// Hashes a password on the blocking pool.
    async fn hash_password(&self, password: &str) -> AppResult<String> {
        let hasher = self.passwords.clone();
        let password = password.to_string();

        tokio::task::spawn_blocking(move || hasher.hash(&password))
            .await
            .map_err(|error| AppError::Internal(format!("hashing task failed: {error}")))?
    }

    /// Verifies a password on the blocking pool.
    ///
    /// The result is a boolean, not an error: a malformed stored hash is a
    /// failed verification, not a failed request.
    async fn verify_password(&self, password: &str, stored_hash: &str) -> AppResult<bool> {
        let hasher = self.passwords.clone();
        let password = password.to_string();
        let stored_hash = stored_hash.to_string();

        tokio::task::spawn_blocking(move || hasher.verify(&password, &stored_hash))
            .await
            .map_err(|error| AppError::Internal(format!("verification task failed: {error}")))
    }

    /// Exchanges a refresh token for a new token pair.
    ///
    /// The presented token is revoked and replaced inside one transaction, so
    /// the rotation is all-or-nothing: a rotation that does not complete
    /// leaves no replacement row behind. Presenting an already revoked token
    /// is treated as reuse: every session of that user is revoked and the
    /// request fails.
    pub async fn refresh(&self, refresh_token: &str, meta: &ClientMeta) -> AppResult<AuthResponse> {
        let token_hash = hash_refresh_token(refresh_token);

        let session = self
            .sessions
            .find_by_token_hash(&token_hash)
            .await?
            .ok_or_else(|| {
                AppError::Unauthorized(self.messages.auth.refresh_token_invalid.clone())
            })?;

        if session.revoked_at.is_some() {
            // Reuse of a rotated token: assume the token was stolen and drop
            // every session belonging to the user.
            self.sessions.revoke_all_for_user(session.user_id).await?;
            return Err(AppError::Unauthorized(
                self.messages.auth.refresh_token_reused.clone(),
            ));
        }

        if !session.is_usable_at(Utc::now()) {
            return Err(AppError::Unauthorized(
                self.messages.auth.refresh_token_expired.clone(),
            ));
        }

        // A missing account is an invalid credential, not a missing resource:
        // the caller presented a token, not a path to a user.
        let user = match self.users.get_user(session.user_id).await {
            Ok(user) => user,
            Err(AppError::NotFound(_)) => {
                return Err(AppError::Unauthorized(
                    self.messages.auth.refresh_token_invalid.clone(),
                ));
            }
            Err(error) => return Err(error),
        };

        if !user.is_active {
            self.sessions.revoke_all_for_user(user.id).await?;
            return Err(AppError::Forbidden(
                self.messages.auth.account_disabled.clone(),
            ));
        }

        let issued = generate_refresh_token();

        // The replacement inherits the deadline of the session it replaces.
        //
        // ADR-0003 requires one *absolute* refresh lifetime: rotating the
        // token must not extend it, otherwise a client that keeps rotating
        // holds a session forever. The deadline is the one recorded when the
        // session was created, so it is the same value for every link of the
        // chain.
        let expires_at = session.expires_at;

        // The rotation is one transaction (Q-19; ADR-0003 as amended): the
        // replacement insert and the claim of the presented session either
        // both land or neither does.
        //
        // The claim still only matches a session that is still active, so two
        // concurrent requests carrying the same token cannot both succeed —
        // and the loser's transaction is rolled back, so no replacement row
        // survives the lost race.
        let replacement = NewRefreshSession {
            id: Uuid::new_v4(),
            user_id: session.user_id,
            token_hash: &issued.token_hash,
            expires_at,
            user_agent: meta.user_agent.as_deref(),
            ip_address: meta.ip_address.as_deref(),
        };

        let won = self
            .sessions
            .rotate(session.id, session.user_id, replacement)
            .await?;

        if !won {
            self.sessions.revoke_all_for_user(session.user_id).await?;
            return Err(AppError::Unauthorized(
                self.messages.auth.refresh_token_reused.clone(),
            ));
        }

        let tokens = self.token_pair(&issued, user.id, &user.email)?;

        Ok(AuthResponse {
            tokens,
            user: UserResponse::from(&user),
        })
    }

    /// Revokes the caller's own refresh session.
    pub async fn logout(&self, user_id: Uuid, refresh_token: &str) -> AppResult<()> {
        let token_hash = hash_refresh_token(refresh_token);

        let Some(session) = self.sessions.find_by_token_hash(&token_hash).await? else {
            // Logout is idempotent: an unknown token is not an error.
            return Ok(());
        };

        // A caller may only revoke its own sessions.
        if session.user_id != user_id {
            return Err(AppError::Forbidden(
                self.messages.auth.not_own_session.clone(),
            ));
        }

        self.sessions.revoke(session.id, user_id).await?;
        Ok(())
    }

    /// Revokes every active session of a user.
    pub async fn revoke_all_sessions(&self, user_id: Uuid) -> AppResult<u64> {
        self.sessions.revoke_all_for_user(user_id).await
    }

    /// Deletes refresh sessions that expired more than `grace` ago.
    ///
    /// Returns the number of removed rows.
    pub async fn prune_expired_sessions(&self, grace: chrono::Duration) -> AppResult<u64> {
        self.sessions.delete_expired(grace).await
    }

    /// Loads the authenticated user.
    pub async fn current_user(&self, user_id: Uuid) -> AppResult<UserResponse> {
        self.users.get_user_response(user_id).await
    }

    /// Issues an access/refresh pair and stores the refresh session.
    async fn issue_token_pair(
        &self,
        user_id: Uuid,
        email: &str,
        meta: &ClientMeta,
    ) -> AppResult<TokenPair> {
        let issued = generate_refresh_token();
        let session_id = Uuid::new_v4();
        let expires_at = self.refresh_expiry();

        self.new_session(session_id, user_id, &issued.token_hash, expires_at, meta)
            .await?;

        self.token_pair(&issued, user_id, email)
    }

    /// Writes the first refresh-session row of a token pair.
    ///
    /// The replacement of a rotation does not go through here: it is written
    /// by the repository's transactional `rotate`, which claims the presented
    /// session in the same transaction. The deadline is a parameter, and a
    /// rotation passes the deadline of the session it replaces, which is what
    /// keeps the absolute lifetime absolute.
    async fn new_session(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        meta: &ClientMeta,
    ) -> AppResult<()> {
        self.sessions
            .insert(NewRefreshSession {
                id: session_id,
                user_id,
                token_hash,
                expires_at,
                user_agent: meta.user_agent.as_deref(),
                ip_address: meta.ip_address.as_deref(),
            })
            .await?;

        Ok(())
    }

    /// Builds the token pair for a refresh token that was just issued.
    ///
    /// The lifetime in the response comes from the token service, so that the
    /// advertised lifetime and the lifetime inside the token cannot drift
    /// apart.
    fn token_pair(
        &self,
        issued: &IssuedRefreshToken,
        user_id: Uuid,
        email: &str,
    ) -> AppResult<TokenPair> {
        let access = self.tokens.issue_access_token(user_id, email)?;

        Ok(TokenPair {
            access_token: access.token,
            refresh_token: issued.token.clone(),
            token_type: "Bearer".to_string(),
            expires_in: self.tokens.access_ttl().as_secs() as i64,
            expires_at: access.expires_at,
        })
    }

    /// The absolute expiry of a refresh session.
    ///
    /// The configured lifetime is bounded by the configuration validation, so
    /// the conversion to a `chrono` duration cannot fail. If it ever did, the
    /// fallback is a zero-length session rather than an unbounded one.
    fn refresh_expiry(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::from_std(self.refresh_ttl).unwrap_or_default()
    }

    /// Validates the credentials carried by a bearer token.
    ///
    /// Every failure mode produces the same error, so that a caller cannot
    /// tell a bad signature from an unknown account.
    pub async fn authenticate_bearer(&self, token: &str) -> AppResult<UserResponse> {
        let claims = self
            .tokens
            .decode_access_token(token)
            .map_err(|_| self.invalid_access_token())?;

        // The subject is typed, so there is nothing to parse: a malformed one
        // never reaches this point.
        let user_id = claims.sub;

        // A missing account is an invalid credential, not a missing resource:
        // the caller presented a token, not a path to a user.
        let user = match self.users.get_user(user_id).await {
            Ok(user) => user,
            Err(AppError::NotFound(_)) => return Err(self.invalid_access_token()),
            Err(error) => return Err(error),
        };

        if !user.is_active {
            return Err(AppError::Forbidden(
                self.messages.auth.account_disabled.clone(),
            ));
        }

        Ok(UserResponse::from(&user))
    }

    /// The error for a bearer token that cannot be trusted.
    fn invalid_access_token(&self) -> AppError {
        AppError::Unauthorized(self.messages.auth.access_token_invalid.clone())
    }
}

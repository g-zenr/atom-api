//! Authentication use cases.
//!
//! Owns registration, login, refresh-token rotation, logout and session
//! revocation. The rules enforced here are recorded in
//! `docs/features/authentication/specification.md`.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::auth::dto::{AuthResponse, LoginRequest, TokenPair};
use crate::auth::password::PasswordHasher;
use crate::auth::repository::RefreshSessionRepository;
use crate::auth::token::{TokenService, generate_refresh_token, hash_refresh_token};
use crate::error::{AppError, AppResult};
use crate::users::dto::{RegisterRequest, UserResponse};
use crate::users::repository::UserRepository;
use crate::users::service::{
    UserService, validate_display_name, validate_email, validate_password,
};

/// Client metadata recorded alongside a session.
#[derive(Debug, Clone, Default)]
pub struct ClientMeta {
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
}

/// The message returned for every failed authentication attempt.
///
/// A single generic message prevents an attacker from learning whether an
/// email address is registered.
const GENERIC_LOGIN_FAILURE: &str = "Invalid email or password.";

/// Authentication use cases.
#[derive(Clone)]
pub struct AuthService {
    users: UserService,
    user_repository: UserRepository,
    sessions: RefreshSessionRepository,
    tokens: Arc<TokenService>,
    passwords: Arc<PasswordHasher>,
    refresh_ttl: Duration,
}

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
        user_repository: UserRepository,
        sessions: RefreshSessionRepository,
        tokens: Arc<TokenService>,
        passwords: Arc<PasswordHasher>,
        refresh_ttl: Duration,
    ) -> Self {
        Self {
            users,
            user_repository,
            sessions,
            tokens,
            passwords,
            refresh_ttl,
        }
    }

    /// Registers a new user and issues an initial token pair.
    pub async fn register(
        &self,
        request: RegisterRequest,
        meta: &ClientMeta,
    ) -> AppResult<AuthResponse> {
        let email = validate_email(&request.email)?;
        validate_password(&request.password)?;
        let display_name = validate_display_name(&request.display_name)?;

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
        let email = crate::users::service::normalize_email(&request.email);

        let user = self.user_repository.find_by_email(&email).await?;

        let Some(user) = user else {
            // Spend the same time hashing as a real attempt would, so that the
            // response time does not reveal whether the account exists.
            self.verify_password(&request.password, self.passwords.dummy_hash())
                .await?;
            return Err(AppError::Unauthorized(GENERIC_LOGIN_FAILURE));
        };

        if !self
            .verify_password(&request.password, &user.password_hash)
            .await?
        {
            return Err(AppError::Unauthorized(GENERIC_LOGIN_FAILURE));
        }

        if !user.is_active {
            return Err(AppError::Forbidden(
                "This account is disabled. Contact an administrator.",
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
    /// The presented token is revoked and replaced. Presenting an already
    /// revoked token is treated as reuse: every session of that user is
    /// revoked and the request fails.
    pub async fn refresh(&self, refresh_token: &str, meta: &ClientMeta) -> AppResult<AuthResponse> {
        let token_hash = hash_refresh_token(refresh_token);

        let session = self
            .sessions
            .find_by_token_hash(&token_hash)
            .await?
            .ok_or_else(|| AppError::Unauthorized("The refresh token is invalid."))?;

        if session.revoked_at.is_some() {
            // Reuse of a rotated token: assume the token was stolen and drop
            // every session belonging to the user.
            self.sessions.revoke_all_for_user(session.user_id).await?;
            return Err(AppError::Unauthorized(
                "The refresh token has already been used. All sessions have been revoked.",
            ));
        }

        if !session.is_usable_at(Utc::now()) {
            return Err(AppError::Unauthorized("The refresh token has expired."));
        }

        let Some(user) = self.user_repository.find_by_id(session.user_id).await? else {
            return Err(AppError::Unauthorized("The refresh token is invalid."));
        };

        if !user.is_active {
            self.sessions.revoke_all_for_user(user.id).await?;
            return Err(AppError::Forbidden(
                "This account is disabled. Contact an administrator.",
            ));
        }

        let issued = generate_refresh_token();
        let new_session_id = Uuid::new_v4();
        let expires_at = Utc::now()
            + chrono::Duration::from_std(self.refresh_ttl)
                .unwrap_or_else(|_| chrono::Duration::days(14));

        // The replacement row is written first, because the rotation link is a
        // foreign key to it.
        self.sessions
            .insert(
                new_session_id,
                user.id,
                &issued.token_hash,
                expires_at,
                meta.user_agent.as_deref(),
                meta.ip_address.as_deref(),
            )
            .await?;

        // Claim the old session atomically.
        //
        // The UPDATE only matches a row that is still active, so two concurrent
        // requests carrying the same token cannot both succeed: the loser sees
        // zero affected rows and is treated as a reuse. Without this, a stolen
        // token could be raced against the legitimate client and keep a usable
        // session without ever triggering the mass revocation.
        if !self
            .sessions
            .claim(session.id, session.user_id, new_session_id)
            .await?
        {
            self.sessions.revoke_all_for_user(session.user_id).await?;
            return Err(AppError::Unauthorized(
                "The refresh token has already been used. All sessions have been revoked.",
            ));
        }

        let access = self
            .tokens
            .issue_access_token(&user.id.to_string(), &user.email)?;

        Ok(AuthResponse {
            tokens: TokenPair {
                access_token: access.token,
                refresh_token: issued.token,
                token_type: "Bearer".to_string(),
                expires_in: self.tokens.access_ttl().as_secs() as i64,
                expires_at: access.expires_at,
            },
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
                "You may only revoke your own sessions.",
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
        let expires_at = Utc::now()
            + chrono::Duration::from_std(self.refresh_ttl)
                .unwrap_or_else(|_| chrono::Duration::days(14));

        self.sessions
            .insert(
                session_id,
                user_id,
                &issued.token_hash,
                expires_at,
                meta.user_agent.as_deref(),
                meta.ip_address.as_deref(),
            )
            .await?;

        let access = self
            .tokens
            .issue_access_token(&user_id.to_string(), email)?;

        Ok(TokenPair {
            access_token: access.token,
            refresh_token: issued.token,
            token_type: "Bearer".to_string(),
            expires_in: self.tokens.access_ttl().as_secs() as i64,
            expires_at: access.expires_at,
        })
    }

    /// Validates the credentials carried by a bearer token.
    pub async fn authenticate_bearer(&self, token: &str) -> AppResult<UserResponse> {
        let claims = self.tokens.decode_access_token(token)?;
        let user_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| AppError::Unauthorized("The access token is invalid or expired."))?;

        let user = self.user_repository.find_by_id(user_id).await?;

        let Some(user) = user else {
            return Err(AppError::Unauthorized(
                "The access token is invalid or expired.",
            ));
        };

        if !user.is_active {
            return Err(AppError::Forbidden(
                "This account is disabled. Contact an administrator.",
            ));
        }

        Ok(UserResponse::from(&user))
    }
}

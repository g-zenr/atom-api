//! Cross-cutting HTTP concerns: authentication extraction, client identity
//! and rate-limit enforcement.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::Json;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ratelimit::Decision;
use crate::state::AppState;

/// The authenticated caller.
///
/// Extracting this type guarantees that the request carried a valid, unexpired
/// bearer token for an active account. Handlers that take `AuthUser` cannot be
/// reached without credentials.
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// The authenticated user's id.
    pub user_id: Uuid,
    /// The authenticated user's email address.
    pub email: String,
}

/// The message returned for every authentication failure.
const UNAUTHORIZED: &str = "Authentication is required. Provide a Bearer access token.";

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(AppError::Unauthorized(UNAUTHORIZED))?;

        let value = header
            .to_str()
            .map_err(|_| AppError::Unauthorized(UNAUTHORIZED))?;

        let token = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or(AppError::Unauthorized(UNAUTHORIZED))?;

        let user = state.auth_service().authenticate_bearer(token).await?;

        Ok(AuthUser {
            user_id: user.id,
            email: user.email,
        })
    }
}

/// Extracts the client IP address from the connection information.
///
/// Returns `None` when the server was not started with connect info, in which
/// case rate limiting falls back to a single shared bucket.
///
/// The `X-Forwarded-For` header is deliberately **not** consulted: it is
/// client-controlled unless a trusted reverse proxy overwrites it, so using it
/// would let a caller forge the rate-limit key.
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub Option<IpAddr>);

impl ClientIp {
    /// The client address, if it is known.
    pub fn get(&self) -> Option<IpAddr> {
        self.0
    }
}

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(socket)| socket.ip());

        Ok(ClientIp(ip))
    }
}

/// The key used when the client address is unknown.
const UNKNOWN_CLIENT: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// Enforces the configured authentication rate limit.
///
/// # Errors
///
/// Returns [`AppError::RateLimited`] when the caller exceeded the limit.
pub fn enforce_rate_limit(state: &AppState, client: ClientIp) -> AppResult<()> {
    let key = client.get().unwrap_or(UNKNOWN_CLIENT);

    match state.rate_limiter.check(key) {
        Decision::Allowed => Ok(()),
        Decision::Rejected {
            retry_after_seconds,
        } => Err(AppError::RateLimited {
            retry_after_seconds,
        }),
    }
}

/// The response returned for an unmatched route.
pub async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(crate::error::ErrorBody {
            error: crate::error::ErrorDetail {
                code: "not_found",
                message: "The requested route does not exist.".to_string(),
                details: Vec::new(),
            },
        }),
    )
}

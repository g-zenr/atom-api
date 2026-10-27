//! Cross-cutting HTTP concerns: authentication extraction, client identity
//! and rate-limit enforcement.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::extract::FromRequestParts;
use axum::extract::{ConnectInfo, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::api::RequestContext;
use crate::error::AppError;
use crate::ratelimit::Decision;
use crate::state::AppState;

/// The authenticated caller.
///
/// Extracting this type guarantees that the request carried a valid, unexpired
/// bearer credential for an active account. Handlers that take `AuthUser` cannot
/// be reached without credentials.
///
/// Two kinds of credential are accepted, and the prefix of the token is the
/// whole of the routing decision:
///
/// * an `atom_…` **API key**, resolved through
///   [`crate::api_keys::service::ApiKeyService`] against the `api_keys` table;
/// * anything else is a **user JWT**, verified by
///   [`crate::auth::service::AuthService::authenticate_bearer`].
///
/// The two paths converge on the same type, so every route is reachable with
/// either credential: an API key is not a reduced principal. That is what lets
/// an unattended workflow rotate its own key.
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// The authenticated user's id.
    pub user_id: Uuid,
    /// The authenticated user's email address.
    pub email: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let messages = parts
            .extensions
            .get::<RequestContext>()
            .map(|context| context.messages.clone())
            .ok_or_else(|| AppError::Internal("the request context is missing".to_string()))?;

        let header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or_else(|| AppError::Unauthorized(messages.general.unauthorized.clone()))?;

        let value = header
            .to_str()
            .map_err(|_| AppError::Unauthorized(messages.general.unauthorized.clone()))?;

        let token = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| AppError::Unauthorized(messages.general.unauthorized.clone()))?;

        // An API key is recognised by its prefix and by nothing else. A JWT is
        // base64url text whose first segment is a JSON header, so it can never
        // begin with `atom_`; the check therefore cannot steal a valid JWT, and
        // a garbage `atom_…` value is routed here and refused as a credential
        // rather than handed to the JWT decoder. See `api_keys::secret`.
        //
        // The `map_err` is the same pair as the JWT path below, for the same
        // reason: the status and the wording are the published contract, and
        // `AuthUser` is the one place that decides them. An inactive account's
        // key is a `403` with the message the JWT path uses for that account, so
        // one account state has one answer (amendment `A-C-K1-1`).
        if crate::api_keys::secret::is_api_key(token) {
            let caller = state
                .api_keys
                .authenticate(token)
                .await
                .map_err(|error| match error {
                    AppError::Unauthorized(_) => {
                        AppError::Unauthorized(messages.auth.access_token_invalid.clone())
                    }
                    AppError::Forbidden(message) => AppError::Forbidden(message),
                    other => other,
                })?;

            return Ok(AuthUser {
                user_id: caller.user_id,
                email: caller.email,
            });
        }

        let user = state
            .auth
            .authenticate_bearer(token)
            .await
            .map_err(|error| match error {
                AppError::Unauthorized(_) => {
                    AppError::Unauthorized(messages.auth.access_token_invalid.clone())
                }
                AppError::Forbidden(message) => AppError::Forbidden(message),
                other => other,
            })?;

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
pub fn enforce_rate_limit(state: &AppState, client: ClientIp) -> Result<(), AppError> {
    let key = client.get().unwrap_or(UNKNOWN_CLIENT);

    let messages = &state.config.messages;

    match state.rate_limiter.check(key) {
        Decision::Allowed => Ok(()),
        Decision::Rejected {
            retry_after_seconds,
        } => Err(AppError::RateLimited {
            retry_after_seconds,
            message: messages.general.rate_limited.clone(),
        }),
    }
}

/// The response returned for an unmatched route.
pub async fn not_found(State(state): State<AppState>) -> Response {
    AppError::NotFound(state.config.messages.general.route_not_found.clone()).into_response()
}

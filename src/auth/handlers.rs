//! HTTP handlers for the authentication endpoints.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};

use crate::api::extract::Json;
use crate::api::middleware::{AuthUser, ClientIp, enforce_rate_limit};
use crate::auth::dto::{AuthResponse, LoginRequest, LogoutRequest, RefreshRequest};
use crate::auth::service::ClientMeta;
use crate::error::AppResult;
use crate::state::AppState;
use crate::users::dto::{RegisterRequest, UserResponse};

/// Builds the client metadata recorded with a session.
fn meta(headers: &HeaderMap, peer: ClientIp) -> ClientMeta {
    ClientMeta {
        user_agent: headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.chars().take(255).collect()),
        ip_address: peer.get().map(|ip| ip.to_string()),
    }
}

/// `POST /api/v1/auth/register` — creates an account.
#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "The account was created.", body = crate::auth::dto::AuthResponse),
        (status = 400, description = "The request is malformed."),
        (status = 409, description = "The resource conflicts with the current state.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 429, description = "Too many attempts.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    peer: ClientIp,
    headers: HeaderMap,
    Json(payload): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<AuthResponse>)> {
    enforce_rate_limit(&state, peer)?;

    let response = state
        .auth_service()
        .register(payload, &meta(&headers, peer))
        .await?;

    Ok((StatusCode::CREATED, Json(response)))
}

/// `POST /api/v1/auth/login` — exchanges credentials for tokens.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authentication succeeded.", body = crate::auth::dto::AuthResponse),
        (status = 400, description = "The request is malformed."),
        (status = 403, description = "The account is disabled, or the operation is not permitted.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 429, description = "Too many attempts.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    peer: ClientIp,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    enforce_rate_limit(&state, peer)?;

    let response = state
        .auth_service()
        .login(payload, &meta(&headers, peer))
        .await?;

    Ok(Json(response))
}

/// `POST /api/v1/auth/refresh` — rotates the token pair.
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "The token pair was rotated.", body = crate::auth::dto::AuthResponse),
        (status = 400, description = "The request is malformed."),
        (status = 403, description = "The account is disabled, or the operation is not permitted.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 429, description = "Too many attempts.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn refresh(
    State(state): State<AppState>,
    peer: ClientIp,
    headers: HeaderMap,
    Json(payload): Json<RefreshRequest>,
) -> AppResult<Json<AuthResponse>> {
    enforce_rate_limit(&state, peer)?;

    let response = state
        .auth_service()
        .refresh(&payload.refresh_token, &meta(&headers, peer))
        .await?;

    Ok(Json(response))
}

/// `POST /api/v1/auth/logout` — revokes the caller's refresh session.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    security(("bearer" = [])),
    request_body = LogoutRequest,
    responses(
        (status = 204, description = "The session was revoked."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 403, description = "The account is disabled, or the operation is not permitted.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn logout(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<LogoutRequest>,
) -> AppResult<StatusCode> {
    state
        .auth_service()
        .logout(user.user_id, &payload.refresh_token)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/auth/me` — the authenticated user's profile.
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The authenticated user.", body = crate::users::dto::UserResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 403, description = "The account is disabled, or the operation is not permitted.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<UserResponse>> {
    let response = state.auth_service().current_user(user.user_id).await?;
    Ok(Json(response))
}

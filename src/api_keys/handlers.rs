//! HTTP handlers for the API-key endpoints.
//!
//! Each handler extracts, calls one service method and picks the status. No
//! handler holds SQL, compares ownership or builds a response by hand: the
//! error envelope and the status code both come from [`crate::error::AppError`].
//!
//! All three routes are authenticated by [`AuthUser`], so a user JWT and an API
//! key are equally able to manage keys. That is deliberate: a workflow must be
//! able to rotate its own credential without a human, and the alternative — a
//! route only opening to one credential kind — would force the workflow to hold
//! a refresh token, which is the problem API keys exist to solve.
//!
//! The `utoipa` paths are absolute because the derive runs at compile time and
//! cannot read [`crate::api::routes::API_PREFIX`]; the agreement test compares
//! the generated document with the route table in both directions, which is
//! what keeps the two from drifting apart (amendment `A-C-D0-2`).

use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::api::extract::{Json, Path, Query};
use crate::api::middleware::AuthUser;
use crate::api_keys::dto::{ApiKeyCreatedResponse, ApiKeyPage, CreateApiKeyRequest};
use crate::error::AppResult;
use crate::pagination::PaginationQuery;
use crate::state::AppState;

/// `POST /api/v1/api-keys` — create a key and return its secret, once.
#[utoipa::path(
    post,
    path = "/api/v1/api-keys",
    tag = "api-keys",
    security(("bearer" = [])),
    request_body = CreateApiKeyRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The key was created. This is the only response that carries the secret.", body = crate::api_keys::dto::ApiKeyCreatedResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn create_api_key(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateApiKeyRequest>,
) -> AppResult<(StatusCode, Json<ApiKeyCreatedResponse>)> {
    let created = state.api_keys.create(user.user_id, payload).await?;

    Ok((StatusCode::CREATED, Json(created)))
}

/// `GET /api/v1/api-keys` — list the caller's keys, newest first.
///
/// The response never contains a secret or a hash: the listing shape has no
/// field for either.
#[utoipa::path(
    get,
    path = "/api/v1/api-keys",
    tag = "api-keys",
    security(("bearer" = [])),
    params(PaginationQuery),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's keys, newest first.", body = crate::api_keys::dto::ApiKeyPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn list_api_keys(
    State(state): State<AppState>,
    user: AuthUser,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<Json<ApiKeyPage>> {
    let page = state.api_keys.list(user.user_id, pagination).await?;

    Ok(Json(ApiKeyPage::from(page)))
}

/// `DELETE /api/v1/api-keys/{key_id}` — revoke one of the caller's keys.
///
/// Revocation is a stamp, not a delete: the row stays in the listing with its
/// `revoked_at`. A second call is `204` as well and keeps the first instant.
#[utoipa::path(
    delete,
    path = "/api/v1/api-keys/{key_id}",
    tag = "api-keys",
    security(("bearer" = [])),
    params(("key_id" = Uuid, Path, description = "API key identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The key is revoked. A key that was already revoked is still 204."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The key does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn revoke_api_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(key_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    state.api_keys.revoke(user.user_id, key_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

//! HTTP handlers for the claim endpoints.
//!
//! Each handler extracts, calls one service method and picks the status. No
//! handler holds SQL, compares ownership or builds a response by hand: the
//! error envelope and the status code both come from [`crate::error::AppError`].
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
use crate::error::AppResult;
use crate::pagination::PaginationQuery;
use crate::reimbursements::dto::{
    ClaimDetailResponse, ClaimPage, ClaimQuery, ClaimsListQuery, CreateClaimRequest,
    UpdateClaimRequest,
};
use crate::state::AppState;

/// `GET /api/v1/claims` — list the caller's claims.
#[utoipa::path(
    get,
    path = "/api/v1/claims",
    tag = "claims",
    security(("bearer" = [])),
    params(
        ClaimQuery,
        PaginationQuery
    ),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's claims, newest first.", body = crate::reimbursements::dto::ClaimPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn list_claims(
    State(state): State<AppState>,
    user: AuthUser,
    Query(list): Query<ClaimsListQuery>,
) -> AppResult<Json<ClaimPage>> {
    let (query, pagination) = list.into_parts();
    let page = state
        .reimbursements
        .list(user.user_id, query, pagination)
        .await?;

    Ok(Json(ClaimPage::from(page)))
}

/// `POST /api/v1/claims` — create a draft claim.
#[utoipa::path(
    post,
    path = "/api/v1/claims",
    tag = "claims",
    security(("bearer" = [])),
    request_body = CreateClaimRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The claim was created.", body = crate::reimbursements::dto::ClaimDetailResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The project does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn create_claim(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateClaimRequest>,
) -> AppResult<(StatusCode, Json<ClaimDetailResponse>)> {
    let claim = state.reimbursements.create(user.user_id, payload).await?;

    Ok((StatusCode::CREATED, Json(claim)))
}

/// `GET /api/v1/claims/{claim_id}` — read one claim with its items.
#[utoipa::path(
    get,
    path = "/api/v1/claims/{claim_id}",
    tag = "claims",
    security(("bearer" = [])),
    params(("claim_id" = Uuid, Path, description = "Claim identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The claim and its items.", body = crate::reimbursements::dto::ClaimDetailResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn get_claim(
    State(state): State<AppState>,
    user: AuthUser,
    Path(claim_id): Path<Uuid>,
) -> AppResult<Json<ClaimDetailResponse>> {
    let claim = state.reimbursements.get(user.user_id, claim_id).await?;

    Ok(Json(claim))
}

/// `PATCH /api/v1/claims/{claim_id}` — partially update a claim.
#[utoipa::path(
    patch,
    path = "/api/v1/claims/{claim_id}",
    tag = "claims",
    security(("bearer" = [])),
    params(("claim_id" = Uuid, Path, description = "Claim identifier")),
    request_body = UpdateClaimRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated claim and its items.", body = crate::reimbursements::dto::ClaimDetailResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim or the project does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn update_claim(
    State(state): State<AppState>,
    user: AuthUser,
    Path(claim_id): Path<Uuid>,
    Json(payload): Json<UpdateClaimRequest>,
) -> AppResult<Json<ClaimDetailResponse>> {
    let claim = state
        .reimbursements
        .update(user.user_id, claim_id, payload)
        .await?;

    Ok(Json(claim))
}

/// `DELETE /api/v1/claims/{claim_id}` — delete a claim and its items.
#[utoipa::path(
    delete,
    path = "/api/v1/claims/{claim_id}",
    tag = "claims",
    security(("bearer" = [])),
    params(("claim_id" = Uuid, Path, description = "Claim identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The claim and its items were deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn delete_claim(
    State(state): State<AppState>,
    user: AuthUser,
    Path(claim_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    state.reimbursements.delete(user.user_id, claim_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

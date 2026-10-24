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
    CreateItemRequest, ItemResponse, UpdateClaimRequest, UpdateItemRequest,
};
use crate::reimbursements::service::LifecycleAction;
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

/// `POST /api/v1/claims/{claim_id}/items` — add a line item to a draft claim.
#[utoipa::path(
    post,
    path = "/api/v1/claims/{claim_id}/items",
    tag = "claims",
    security(("bearer" = [])),
    params(("claim_id" = Uuid, Path, description = "Claim identifier")),
    request_body = CreateItemRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The item was added and the claim's total was recomputed.", body = crate::reimbursements::dto::ItemResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The claim is not a draft.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn create_item(
    State(state): State<AppState>,
    user: AuthUser,
    Path(claim_id): Path<Uuid>,
    Json(payload): Json<CreateItemRequest>,
) -> AppResult<(StatusCode, Json<ItemResponse>)> {
    let item = state
        .reimbursements
        .create_item(user.user_id, claim_id, payload)
        .await?;

    Ok((StatusCode::CREATED, Json(item)))
}

/// `PATCH /api/v1/claims/{claim_id}/items/{item_id}` — partially update an item.
#[utoipa::path(
    patch,
    path = "/api/v1/claims/{claim_id}/items/{item_id}",
    tag = "claims",
    security(("bearer" = [])),
    params(
        ("claim_id" = Uuid, Path, description = "Claim identifier"),
        ("item_id" = Uuid, Path, description = "Item identifier, addressable only inside its claim"),
    ),
    request_body = UpdateItemRequest,
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated item; the claim's total was recomputed.", body = crate::reimbursements::dto::ItemResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim or the item does not exist, is not owned by the caller, or the item does not belong to that claim.", body = crate::error::ErrorBody),
        (status = 409, description = "The claim is not a draft.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn update_item(
    State(state): State<AppState>,
    user: AuthUser,
    Path((claim_id, item_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateItemRequest>,
) -> AppResult<Json<ItemResponse>> {
    let item = state
        .reimbursements
        .update_item(user.user_id, claim_id, item_id, payload)
        .await?;

    Ok(Json(item))
}

/// `DELETE /api/v1/claims/{claim_id}/items/{item_id}` — delete an item.
#[utoipa::path(
    delete,
    path = "/api/v1/claims/{claim_id}/items/{item_id}",
    tag = "claims",
    security(("bearer" = [])),
    params(
        ("claim_id" = Uuid, Path, description = "Claim identifier"),
        ("item_id" = Uuid, Path, description = "Item identifier, addressable only inside its claim"),
    ),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The item was deleted and the claim's total was recomputed."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim or the item does not exist, is not owned by the caller, or the item does not belong to that claim.", body = crate::error::ErrorBody),
        (status = 409, description = "The claim is not a draft.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn delete_item(
    State(state): State<AppState>,
    user: AuthUser,
    Path((claim_id, item_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    state
        .reimbursements
        .delete_item(user.user_id, claim_id, item_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/claims/{claim_id}/submit` — submit a draft, or reopen a
/// rejected claim.
#[utoipa::path(
    post,
    path = "/api/v1/claims/{claim_id}/submit",
    tag = "claims",
    security(("bearer" = [])),
    params(("claim_id" = Uuid, Path, description = "Claim identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The claim in its new status: submitted, or draft when a rejected claim was reopened.", body = crate::reimbursements::dto::ClaimDetailResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The claim is submitted, approved or paid.", body = crate::error::ErrorBody),
        (status = 422, description = "A draft has no items, or a total of zero.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn submit_claim(
    State(state): State<AppState>,
    user: AuthUser,
    Path(claim_id): Path<Uuid>,
) -> AppResult<Json<ClaimDetailResponse>> {
    let claim = state
        .reimbursements
        .transition(user.user_id, claim_id, LifecycleAction::Submit)
        .await?;

    Ok(Json(claim))
}

/// `POST /api/v1/claims/{claim_id}/approve` — approve a submitted claim.
#[utoipa::path(
    post,
    path = "/api/v1/claims/{claim_id}/approve",
    tag = "claims",
    security(("bearer" = [])),
    params(("claim_id" = Uuid, Path, description = "Claim identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The approved claim.", body = crate::reimbursements::dto::ClaimDetailResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The claim is not submitted.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn approve_claim(
    State(state): State<AppState>,
    user: AuthUser,
    Path(claim_id): Path<Uuid>,
) -> AppResult<Json<ClaimDetailResponse>> {
    let claim = state
        .reimbursements
        .transition(user.user_id, claim_id, LifecycleAction::Approve)
        .await?;

    Ok(Json(claim))
}

/// `POST /api/v1/claims/{claim_id}/reject` — reject a submitted claim.
#[utoipa::path(
    post,
    path = "/api/v1/claims/{claim_id}/reject",
    tag = "claims",
    security(("bearer" = [])),
    params(("claim_id" = Uuid, Path, description = "Claim identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The rejected claim.", body = crate::reimbursements::dto::ClaimDetailResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The claim is not submitted.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn reject_claim(
    State(state): State<AppState>,
    user: AuthUser,
    Path(claim_id): Path<Uuid>,
) -> AppResult<Json<ClaimDetailResponse>> {
    let claim = state
        .reimbursements
        .transition(user.user_id, claim_id, LifecycleAction::Reject)
        .await?;

    Ok(Json(claim))
}

/// `POST /api/v1/claims/{claim_id}/pay` — mark an approved claim paid.
#[utoipa::path(
    post,
    path = "/api/v1/claims/{claim_id}/pay",
    tag = "claims",
    security(("bearer" = [])),
    params(("claim_id" = Uuid, Path, description = "Claim identifier")),
    responses(

        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The paid claim.", body = crate::reimbursements::dto::ClaimDetailResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The claim does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The claim is not approved. A paid claim is terminal, so a second pay is also 409.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),

    )
)]
pub async fn pay_claim(
    State(state): State<AppState>,
    user: AuthUser,
    Path(claim_id): Path<Uuid>,
) -> AppResult<Json<ClaimDetailResponse>> {
    let claim = state
        .reimbursements
        .transition(user.user_id, claim_id, LifecycleAction::Pay)
        .await?;

    Ok(Json(claim))
}

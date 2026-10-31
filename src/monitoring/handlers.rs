//! HTTP handlers for the price-monitoring endpoints.
//!
//! Each handler extracts, calls one service method and picks the status. No
//! handler holds SQL, compares ownership or builds a response by hand: the
//! error envelope and the status code both come from [`crate::error::AppError`].
//!
//! Two status decisions belong to this module and nowhere else:
//!
//! * **`POST /listings/{listing_id}/observations` answers `201` for a written
//!   observation and `200` for an absorbed duplicate** (`ADR-0012` §Decision
//!   5). The idempotency identity is the client-supplied fingerprint, and a
//!   retried workflow must be able to tell "recorded" from "already
//!   recorded" without a second read. The body is the same
//!   [`ObservationResponse`] in both cases — the status code is the whole
//!   difference. A duplicate is deliberately **absorbed, not refused**:
//!   refusing a retry would punish exactly the behaviour the fingerprint
//!   exists to make safe, and the workflow's next run would re-send it.
//! * **`POST /automation/runs` answers `201`** with the run in its
//!   `running` state; the workflow finishes it with the patch.
//!
//! The `utoipa` paths are absolute because the derive runs at compile time
//! and cannot read [`crate::api::routes::API_PREFIX`]; the agreement test
//! compares the generated document with the route table in both directions,
//! which is what keeps the two from drifting apart.

use axum::extract::State;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::api::extract::{Json, Path, Query};
use crate::api::middleware::AuthUser;
use crate::error::AppResult;
use crate::monitoring::dto::{
    AlertPage, AlertResponse, AlertsListQuery, CreateListingRequest, CreateObservationRequest,
    CreateProductRequest, CreateRuleRequest, FinishRunRequest, ListingPage, ListingResponse,
    ObservationPage, ObservationResponse, ProductsListQuery, RulePage, RuleResponse, RunPage,
    RunResponse, TrackedProductPage, TrackedProductResponse, UpdateAlertRequest,
    UpdateListingRequest, UpdateProductRequest, UpdateRuleRequest,
};
use crate::pagination::PaginationQuery;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Tracked products
// ---------------------------------------------------------------------------

/// `GET /api/v1/products` — list the caller's tracked products.
#[utoipa::path(
    get,
    path = "/api/v1/products",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        crate::monitoring::dto::ProductQuery,
        PaginationQuery
    ),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's tracked products, newest first.", body = crate::monitoring::dto::TrackedProductPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn list_products(
    State(state): State<AppState>,
    user: AuthUser,
    Query(list): Query<ProductsListQuery>,
) -> AppResult<Json<TrackedProductPage>> {
    let (query, pagination) = list.into_parts();
    let page = state
        .monitoring
        .list_products(user.user_id, query, pagination)
        .await?;

    Ok(Json(page))
}

/// `POST /api/v1/products` — track a product.
#[utoipa::path(
    post,
    path = "/api/v1/products",
    tag = "monitoring",
    security(("bearer" = [])),
    request_body = CreateProductRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The product is now tracked, active.", body = crate::monitoring::dto::TrackedProductResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 409, description = "A product with this name and configuration is already tracked.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn create_product(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateProductRequest>,
) -> AppResult<(StatusCode, Json<TrackedProductResponse>)> {
    let product = state
        .monitoring
        .create_product(user.user_id, payload)
        .await?;

    Ok((StatusCode::CREATED, Json(product)))
}

/// `GET /api/v1/products/{product_id}` — read one tracked product.
#[utoipa::path(
    get,
    path = "/api/v1/products/{product_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("product_id" = Uuid, Path, description = "Tracked product identifier")),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The tracked product.", body = crate::monitoring::dto::TrackedProductResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The product does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn get_product(
    State(state): State<AppState>,
    user: AuthUser,
    Path(product_id): Path<Uuid>,
) -> AppResult<Json<TrackedProductResponse>> {
    let product = state
        .monitoring
        .get_product(user.user_id, product_id)
        .await?;

    Ok(Json(product))
}

/// `PATCH /api/v1/products/{product_id}` — partially update a tracked
/// product. The identity is re-derived from the effective fields.
#[utoipa::path(
    patch,
    path = "/api/v1/products/{product_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("product_id" = Uuid, Path, description = "Tracked product identifier")),
    request_body = UpdateProductRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated product.", body = crate::monitoring::dto::TrackedProductResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The product does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The patch collides with another product of the same owner.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn update_product(
    State(state): State<AppState>,
    user: AuthUser,
    Path(product_id): Path<Uuid>,
    Json(payload): Json<UpdateProductRequest>,
) -> AppResult<Json<TrackedProductResponse>> {
    let product = state
        .monitoring
        .update_product(user.user_id, product_id, payload)
        .await?;

    Ok(Json(product))
}

/// `DELETE /api/v1/products/{product_id}` — untrack a product, with its
/// listings, observations, rules and alerts.
#[utoipa::path(
    delete,
    path = "/api/v1/products/{product_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("product_id" = Uuid, Path, description = "Tracked product identifier")),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The product and everything under it were deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The product does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_product(
    State(state): State<AppState>,
    user: AuthUser,
    Path(product_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    state
        .monitoring
        .delete_product(user.user_id, product_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Product listings
// ---------------------------------------------------------------------------

/// `GET /api/v1/products/{product_id}/listings` — list a product's listings.
#[utoipa::path(
    get,
    path = "/api/v1/products/{product_id}/listings",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        ("product_id" = Uuid, Path, description = "Tracked product identifier"),
        PaginationQuery
    ),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The product's listings, newest first.", body = crate::monitoring::dto::ListingPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The product does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn list_listings(
    State(state): State<AppState>,
    user: AuthUser,
    Path(product_id): Path<Uuid>,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<Json<ListingPage>> {
    let page = state
        .monitoring
        .list_listings(user.user_id, product_id, pagination)
        .await?;

    Ok(Json(page))
}

/// `POST /api/v1/products/{product_id}/listings` — track a listing.
#[utoipa::path(
    post,
    path = "/api/v1/products/{product_id}/listings",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("product_id" = Uuid, Path, description = "Tracked product identifier")),
    request_body = CreateListingRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The listing is now tracked.", body = crate::monitoring::dto::ListingResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The product does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "A listing with this URL is already tracked.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn create_listing(
    State(state): State<AppState>,
    user: AuthUser,
    Path(product_id): Path<Uuid>,
    Json(payload): Json<CreateListingRequest>,
) -> AppResult<(StatusCode, Json<ListingResponse>)> {
    let listing = state
        .monitoring
        .create_listing(user.user_id, product_id, payload)
        .await?;

    Ok((StatusCode::CREATED, Json(listing)))
}

/// `PATCH /api/v1/products/{product_id}/listings/{listing_id}` — report on a
/// listing: its external identifier, its availability, its validation state.
#[utoipa::path(
    patch,
    path = "/api/v1/products/{product_id}/listings/{listing_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        ("product_id" = Uuid, Path, description = "Tracked product identifier"),
        ("listing_id" = Uuid, Path, description = "Listing identifier, addressable only inside its product"),
    ),
    request_body = UpdateListingRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated listing.", body = crate::monitoring::dto::ListingResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The listing does not exist under that product, or either is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn update_listing(
    State(state): State<AppState>,
    user: AuthUser,
    Path((product_id, listing_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateListingRequest>,
) -> AppResult<Json<ListingResponse>> {
    let listing = state
        .monitoring
        .update_listing(user.user_id, product_id, listing_id, payload)
        .await?;

    Ok(Json(listing))
}

/// `DELETE /api/v1/products/{product_id}/listings/{listing_id}` — untrack a
/// listing, with its observations and their alerts.
#[utoipa::path(
    delete,
    path = "/api/v1/products/{product_id}/listings/{listing_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        ("product_id" = Uuid, Path, description = "Tracked product identifier"),
        ("listing_id" = Uuid, Path, description = "Listing identifier, addressable only inside its product"),
    ),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The listing and everything under it were deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The listing does not exist under that product, or either is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_listing(
    State(state): State<AppState>,
    user: AuthUser,
    Path((product_id, listing_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    state
        .monitoring
        .delete_listing(user.user_id, product_id, listing_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Price observations
// ---------------------------------------------------------------------------

/// `POST /api/v1/listings/{listing_id}/observations` — record one collected
/// price, idempotently.
///
/// The idempotency identity is the body's `fingerprint` (`ADR-0012` §Decision
/// 5). A body whose fingerprint was already recorded for this listing is
/// **absorbed**: the endpoint answers `200` with the **existing** observation
/// and writes nothing, so a retried workflow cannot double-write and cannot
/// re-fire the rule it already fired. A new observation answers `201`. The
/// rule evaluation runs in the same transaction as the insert; when the
/// product has an enabled rule and the observation triggers it, the alert
/// exists when the `201` is answered.
#[utoipa::path(
    post,
    path = "/api/v1/listings/{listing_id}/observations",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("listing_id" = Uuid, Path, description = "Listing identifier")),
    request_body = CreateObservationRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The observation was recorded; the product's enabled rule was evaluated in the same transaction.", body = crate::monitoring::dto::ObservationResponse),
        (status = 200, description = "The fingerprint was already recorded: the existing observation is returned unchanged, and nothing was written.", body = crate::monitoring::dto::ObservationResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The listing, or the run named in the body, does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn create_observation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(listing_id): Path<Uuid>,
    Json(payload): Json<CreateObservationRequest>,
) -> AppResult<(StatusCode, Json<ObservationResponse>)> {
    let recorded = state
        .monitoring
        .record_observation(user.user_id, listing_id, payload)
        .await?;

    let status = if recorded.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((status, Json(recorded.observation)))
}

/// `GET /api/v1/listings/{listing_id}/observations` — a listing's price
/// history, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/listings/{listing_id}/observations",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        ("listing_id" = Uuid, Path, description = "Listing identifier"),
        PaginationQuery
    ),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The listing's observations, newest first.", body = crate::monitoring::dto::ObservationPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The listing does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn list_observations(
    State(state): State<AppState>,
    user: AuthUser,
    Path(listing_id): Path<Uuid>,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<Json<ObservationPage>> {
    let page = state
        .monitoring
        .list_observations(user.user_id, listing_id, pagination)
        .await?;

    Ok(Json(page))
}

/// `GET /api/v1/listings/{listing_id}/observations/latest` — the newest
/// observation of one listing.
///
/// A listing with no observations answers `404` on the observation; a
/// foreign or absent listing answers `404` on the listing. Both are `404`,
/// because the caller may not learn which of the two it is.
#[utoipa::path(
    get,
    path = "/api/v1/listings/{listing_id}/observations/latest",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("listing_id" = Uuid, Path, description = "Listing identifier")),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The newest observation of the listing.", body = crate::monitoring::dto::ObservationResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The listing has no observation yet, or does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn latest_observation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(listing_id): Path<Uuid>,
) -> AppResult<Json<ObservationResponse>> {
    let observation = state
        .monitoring
        .latest_observation(user.user_id, listing_id)
        .await?;

    Ok(Json(observation))
}

// ---------------------------------------------------------------------------
// Monitoring rules
// ---------------------------------------------------------------------------

/// `GET /api/v1/products/{product_id}/rules` — list a product's rules.
///
/// At most one rule can be on the page: the schema allows one rule per
/// product.
#[utoipa::path(
    get,
    path = "/api/v1/products/{product_id}/rules",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        ("product_id" = Uuid, Path, description = "Tracked product identifier"),
        PaginationQuery
    ),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The product's rules, newest first; at most one.", body = crate::monitoring::dto::RulePage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The product does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn list_rules(
    State(state): State<AppState>,
    user: AuthUser,
    Path(product_id): Path<Uuid>,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<Json<RulePage>> {
    let page = state
        .monitoring
        .list_rules(user.user_id, product_id, pagination)
        .await?;

    Ok(Json(page))
}

/// `POST /api/v1/products/{product_id}/rules` — create the product's rule.
///
/// A product has at most one rule: a second create is a `409`.
#[utoipa::path(
    post,
    path = "/api/v1/products/{product_id}/rules",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("product_id" = Uuid, Path, description = "Tracked product identifier")),
    request_body = CreateRuleRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The rule was created, enabled, on the inbox channel.", body = crate::monitoring::dto::RuleResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The product does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The product already has a rule.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn create_rule(
    State(state): State<AppState>,
    user: AuthUser,
    Path(product_id): Path<Uuid>,
    Json(payload): Json<CreateRuleRequest>,
) -> AppResult<(StatusCode, Json<RuleResponse>)> {
    let rule = state
        .monitoring
        .create_rule(user.user_id, product_id, payload)
        .await?;

    Ok((StatusCode::CREATED, Json(rule)))
}

/// `PATCH /api/v1/products/{product_id}/rules/{rule_id}` — partially update
/// the rule: the switch, the target, or the percent drop.
#[utoipa::path(
    patch,
    path = "/api/v1/products/{product_id}/rules/{rule_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        ("product_id" = Uuid, Path, description = "Tracked product identifier"),
        ("rule_id" = Uuid, Path, description = "Rule identifier, addressable only inside its product"),
    ),
    request_body = UpdateRuleRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated rule.", body = crate::monitoring::dto::RuleResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The rule does not exist under that product, or either is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn update_rule(
    State(state): State<AppState>,
    user: AuthUser,
    Path((product_id, rule_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateRuleRequest>,
) -> AppResult<Json<RuleResponse>> {
    let rule = state
        .monitoring
        .update_rule(user.user_id, product_id, rule_id, payload)
        .await?;

    Ok(Json(rule))
}

/// `DELETE /api/v1/products/{product_id}/rules/{rule_id}` — delete the rule,
/// with the alerts it raised.
#[utoipa::path(
    delete,
    path = "/api/v1/products/{product_id}/rules/{rule_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        ("product_id" = Uuid, Path, description = "Tracked product identifier"),
        ("rule_id" = Uuid, Path, description = "Rule identifier, addressable only inside its product"),
    ),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 204, description = "The rule and its alerts were deleted."),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The rule does not exist under that product, or either is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_rule(
    State(state): State<AppState>,
    user: AuthUser,
    Path((product_id, rule_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    state
        .monitoring
        .delete_rule(user.user_id, product_id, rule_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Automation runs
// ---------------------------------------------------------------------------

/// `POST /api/v1/automation/runs` — start a run.
///
/// The body, if any, is ignored: a run carries no client-supplied state, and
/// its shape is fixed by the schema. The `201` response is the run in its
/// `running` state; the workflow finishes it with the patch.
#[utoipa::path(
    post,
    path = "/api/v1/automation/runs",
    tag = "monitoring",
    security(("bearer" = [])),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 201, description = "The run was started and is running.", body = crate::monitoring::dto::RunResponse),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn start_run(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<(StatusCode, Json<RunResponse>)> {
    let run = state.monitoring.start_run(user.user_id).await?;

    Ok((StatusCode::CREATED, Json(run)))
}

/// `PATCH /api/v1/automation/runs/{run_id}` — finish a running run.
///
/// The `outcome` is required and must be one of the three terminal values.
/// A run that has already finished is a `409`; the guard is the statement's
/// `WHERE outcome = 'running'`, so two concurrent finishes cannot both win.
#[utoipa::path(
    patch,
    path = "/api/v1/automation/runs/{run_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("run_id" = Uuid, Path, description = "Automation run identifier")),
    request_body = FinishRunRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The finished run.", body = crate::monitoring::dto::RunResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The run does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 409, description = "The run has already finished.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn finish_run(
    State(state): State<AppState>,
    user: AuthUser,
    Path(run_id): Path<Uuid>,
    Json(payload): Json<FinishRunRequest>,
) -> AppResult<Json<RunResponse>> {
    let run = state
        .monitoring
        .finish_run(user.user_id, run_id, payload)
        .await?;

    Ok(Json(run))
}

/// `GET /api/v1/automation/runs` — list the caller's runs, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/automation/runs",
    tag = "monitoring",
    security(("bearer" = [])),
    params(PaginationQuery),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's automation runs, newest first.", body = crate::monitoring::dto::RunPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn list_runs(
    State(state): State<AppState>,
    user: AuthUser,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<Json<RunPage>> {
    let page = state.monitoring.list_runs(user.user_id, pagination).await?;

    Ok(Json(page))
}

// ---------------------------------------------------------------------------
// Price alerts
// ---------------------------------------------------------------------------

/// `GET /api/v1/alerts` — list the caller's alerts, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/alerts",
    tag = "monitoring",
    security(("bearer" = [])),
    params(
        crate::monitoring::dto::AlertQuery,
        PaginationQuery
    ),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The caller's alerts, newest first.", body = crate::monitoring::dto::AlertPage),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn list_alerts(
    State(state): State<AppState>,
    user: AuthUser,
    Query(list): Query<AlertsListQuery>,
) -> AppResult<Json<AlertPage>> {
    let (query, pagination) = list.into_parts();
    let page = state
        .monitoring
        .list_alerts(user.user_id, query, pagination)
        .await?;

    Ok(Json(page))
}

/// `GET /api/v1/alerts/{alert_id}` — read one alert.
#[utoipa::path(
    get,
    path = "/api/v1/alerts/{alert_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("alert_id" = Uuid, Path, description = "Alert identifier")),
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The alert.", body = crate::monitoring::dto::AlertResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The alert does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn get_alert(
    State(state): State<AppState>,
    user: AuthUser,
    Path(alert_id): Path<Uuid>,
) -> AppResult<Json<AlertResponse>> {
    let alert = state.monitoring.get_alert(user.user_id, alert_id).await?;

    Ok(Json(alert))
}

/// `PATCH /api/v1/alerts/{alert_id}` — mark an alert read or unread, and
/// record its delivery status.
#[utoipa::path(
    patch,
    path = "/api/v1/alerts/{alert_id}",
    tag = "monitoring",
    security(("bearer" = [])),
    params(("alert_id" = Uuid, Path, description = "Alert identifier")),
    request_body = UpdateAlertRequest,
    responses(
        (status = 408, description = "The request timed out.", body = crate::error::ErrorBody),
        (status = 200, description = "The updated alert.", body = crate::monitoring::dto::AlertResponse),
        (status = 400, description = "The request is malformed."),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The alert does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn update_alert(
    State(state): State<AppState>,
    user: AuthUser,
    Path(alert_id): Path<Uuid>,
    Json(payload): Json<UpdateAlertRequest>,
) -> AppResult<Json<AlertResponse>> {
    let alert = state
        .monitoring
        .update_alert(user.user_id, alert_id, payload)
        .await?;

    Ok(Json(alert))
}

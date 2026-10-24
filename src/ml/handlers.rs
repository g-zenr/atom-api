//! HTTP handlers for the machine-learning endpoints.
//!
//! Extraction, status codes and serialization only. Every branch about a model
//! lives in [`crate::ml::service`], so a handler cannot invent a `500` for a
//! state the service already knows how to degrade.
//!
//! The published codes are
//! [`docs/features/ml/api.md`](../../docs/features/ml/api.md):
//!
//! | Operation | Success | Refusals |
//! | --------- | ------- | -------- |
//! | `POST /notes/search` | `200` | `400`, `401`, `422`, `500` — never `408`, `409` or `503` |
//! | `POST /job-applications/{id}/match` | `200` | `400`, `401`, `404`, `409`, `422`, `408`, `500` |
//! | `POST /ml/reindex` | `200` | `401`, `409`, `408`, `500` |

use axum::extract::State;
use uuid::Uuid;

use crate::api::extract::{Json, Path, Query};
use crate::api::middleware::AuthUser;
use crate::error::AppResult;
use crate::ml::dto::{MatchRequest, MatchResponse, ReindexResponse, SearchRequest, SearchResponse};
use crate::pagination::PaginationQuery;
use crate::state::AppState;

/// `POST /api/v1/notes/search` — search the caller's notes in one of three
/// modes.
///
/// A scored mode whose model is disabled, missing or too slow answers `200`
/// with lexical results and the four degradation fields set. It never answers
/// `408`, `409` or `503`: a missing optional model is a configuration state,
/// not an outage of a working endpoint.
#[utoipa::path(
    post,
    path = "/api/v1/notes/search",
    tag = "ml",
    security(("bearer" = [])),
    params(PaginationQuery),
    request_body = SearchRequest,
    responses(
        (status = 200, description = "The search results. `degraded` says whether a scored mode fell back to lexical.", body = crate::ml::dto::SearchResponse),
        (status = 400, description = "The request is malformed.", body = crate::error::ErrorBody),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation: an empty or over-long query, or an unknown mode.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn search_notes(
    State(state): State<AppState>,
    user: AuthUser,
    Query(pagination): Query<PaginationQuery>,
    Json(payload): Json<SearchRequest>,
) -> AppResult<Json<SearchResponse>> {
    let response = state.ml.search(user.user_id, payload, pagination).await?;

    Ok(Json(response))
}

/// `POST /api/v1/job-applications/{application_id}/match` — rank the caller's
/// notes against one job application.
///
/// A match has no lexical fallback: an unavailable model is `409`, and an
/// inference that overruns `ml.timeout_seconds` is `408`. Neither is a `500`.
#[utoipa::path(
    post,
    path = "/api/v1/job-applications/{application_id}/match",
    tag = "ml",
    security(("bearer" = [])),
    params(("application_id" = Uuid, Path, description = "Job application identifier")),
    request_body = MatchRequest,
    responses(
        (status = 200, description = "The notes ranked against the application, best first.", body = crate::ml::dto::MatchResponse),
        (status = 400, description = "The request is malformed.", body = crate::error::ErrorBody),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 404, description = "The resource does not exist or is not owned by the caller.", body = crate::error::ErrorBody),
        (status = 408, description = "The inference exceeded the configured budget and a match has no fallback.", body = crate::error::ErrorBody),
        (status = 409, description = "Machine learning is unavailable, so no ranking can be produced.", body = crate::error::ErrorBody),
        (status = 413, description = "The request body is too large.", body = crate::error::ErrorBody),
        (status = 422, description = "The request failed validation: a lexical or unknown mode, or a limit outside 1..=50.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn match_application(
    State(state): State<AppState>,
    user: AuthUser,
    Path(application_id): Path<Uuid>,
    Json(payload): Json<MatchRequest>,
) -> AppResult<Json<MatchResponse>> {
    let response = state
        .ml
        .match_application(user.user_id, application_id, payload)
        .await?;

    Ok(Json(response))
}

/// `POST /api/v1/ml/reindex` — rebuild the caller's embeddings.
///
/// Idempotent and owner-scoped. A body is not read: the operation is defined
/// entirely by the caller's identity. Reindexing with ML disabled is `409`,
/// because the call cannot do what it says.
#[utoipa::path(
    post,
    path = "/api/v1/ml/reindex",
    tag = "ml",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "How many rows the run wrote, skipped and removed.", body = crate::ml::dto::ReindexResponse),
        (status = 401, description = "Authentication is required.", body = crate::error::ErrorBody),
        (status = 408, description = "The run exceeded the configured request timeout; the rows already written stay.", body = crate::error::ErrorBody),
        (status = 409, description = "Machine learning is unavailable, so reindexing cannot do what it says.", body = crate::error::ErrorBody),
        (status = 500, description = "An unexpected failure occurred.", body = crate::error::ErrorBody),
    )
)]
pub async fn reindex(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<ReindexResponse>> {
    let response = state.ml.reindex(user.user_id).await?;

    Ok(Json(response))
}

//! Liveness and readiness endpoints.
//!
//! Both endpoints live outside `/api/v1` and outside the authentication
//! boundary: an orchestrator must be able to probe the process without
//! credentials.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

/// Readiness detail for each downstream dependency.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyChecks {
    /// `"ok"` when the database answered the probe query.
    pub database: String,
}

/// Readiness response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyResponse {
    /// `"ready"` or `"degraded"`.
    pub status: String,
    /// Per-dependency detail.
    pub checks: ReadyChecks,
}

/// Liveness response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct LiveResponse {
    /// Always `"live"` when the process can serve requests.
    pub status: String,
}

/// `GET /health/live` — process liveness.
#[utoipa::path(
    get,
    path = "/health/live",
    tag = "health",
    responses(
        (status = 200, description = "The process is alive.", body = LiveResponse),
    )
)]
pub async fn live() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(LiveResponse {
            status: "live".into(),
        }),
    )
}

/// `GET /health/ready` — readiness, including database connectivity.
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    responses(
        (status = 200, description = "All dependencies are reachable.", body = ReadyResponse),
        (status = 503, description = "A required dependency is unavailable.", body = ReadyResponse),
    )
)]
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    // A one-second timeout keeps a stalled database from hanging the probe.
    let database_ok = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db),
    )
    .await
    .is_ok_and(|result| result.is_ok());

    if database_ok {
        (
            StatusCode::OK,
            Json(ReadyResponse {
                status: "ready".into(),
                checks: ReadyChecks {
                    database: "ok".into(),
                },
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadyResponse {
                status: "degraded".into(),
                checks: ReadyChecks {
                    database: "unavailable".into(),
                },
            }),
        )
    }
}

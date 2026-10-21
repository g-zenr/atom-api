//! HTTP layer: routing, middleware and shared HTTP concerns.
//!
//! The router is assembled once in [`router`]. Every layer is explicit so
//! that the security posture of the API is readable in one place.
//!
//! `Router::layer` wraps the routes registered so far, so the **last**
//! `.layer()` call is the **outermost**. The effective order, from the
//! outside in, is:
//!
//! ```text
//! CORS -> trace -> body limit -> request id (set, propagate) -> timeout -> panic guard -> route
//! ```
//!
//! The request-id pair is deliberately adjacent and in that order: the
//! setter runs first and puts the id on the request, the propagator then
//! copies it to the response. `docs/architecture/request-lifecycle.md` §2
//! describes the same order in more detail.

pub mod extract;
pub mod middleware;
pub mod routes;

use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::error::AppError;
use crate::health;
use crate::state::AppState;

/// Builds the complete application router.
pub fn router(state: AppState) -> Router {
    let config = state.config.clone();

    // The OpenAPI document itself is served by the Swagger UI router, which
    // registers `/api/v1/openapi.json` from the URL configured in
    // `routes::swagger_ui`. Registering it here as well would create two
    // handlers for the same method and path.
    let api = routes::api_routes();

    let mut app = Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .nest("/api/v1", api)
        .merge(routes::swagger_ui())
        .fallback(routes::not_found)
        .with_state(state);

    app = app
        .layer(CatchPanicLayer::custom(routes::handle_panic))
        // The timeout is applied before the request-id pair, which makes it
        // inner to both of them: a timed-out request therefore still carries
        // its correlation id on the response.
        .layer(axum::middleware::from_fn(timeout_middleware))
        // The extension is applied after the middleware in the chain, which
        // makes it the outer of the two: the extension therefore inserts the
        // timeout before the middleware reads it.
        .layer(axum::Extension(config.request_timeout))
        // The setter is applied *before* the propagator, which makes it the
        // outer of the two: the setter therefore puts the id on the request
        // before the propagator reads it. The propagator copies the id from
        // the request to the response, so a response produced by an inner
        // layer - a timeout, for example - still carries it.
        .layer(PropagateRequestIdLayer::new(x_request_id()))
        .layer(SetRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
            MakeRequestUuid,
        ))
        .layer(axum::extract::DefaultBodyLimit::max(config.max_body_bytes))
        .layer(TraceLayer::new_for_http());

    if let Some(cors) = cors_layer(&config) {
        app = app.layer(cors);
    }

    app
}

/// The timeout used when the router did not install one.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Abandons a request that exceeds the configured timeout.
///
/// The timeout comes from the request extensions, where the router puts it.
/// The timeout is reported through the standard error envelope, so a client
/// that parses the envelope does not have to special-case this response.
pub async fn timeout_middleware(request: Request, next: Next) -> Response {
    let timeout = request
        .extensions()
        .get::<Duration>()
        .copied()
        .unwrap_or(DEFAULT_REQUEST_TIMEOUT);

    match tokio::time::timeout(timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => AppError::Timeout.into_response(),
    }
}

/// Builds the CORS layer from the configured allowlist.
///
/// `None` means CORS is disabled entirely, which is the correct behaviour for
/// a deployment that is not reachable from a browser.
fn cors_layer(config: &Config) -> Option<CorsLayer> {
    if config.cors_allowed_origins.is_empty() {
        return None;
    }

    let allow_any = config
        .cors_allowed_origins
        .iter()
        .any(|origin| origin == "*");

    // A wildcard origin combined with credentials is rejected by browsers and
    // is a security foot-gun. Refuse it at start-up instead of shipping it.
    assert!(
        !allow_any,
        "APP_CORS_ALLOWED_ORIGINS must list explicit origins, not '*'"
    );

    let origins: Vec<axum::http::HeaderValue> = config
        .cors_allowed_origins
        .iter()
        .map(|origin| {
            origin
                .parse::<axum::http::HeaderValue>()
                .unwrap_or_else(|_| {
                    panic!("APP_CORS_ALLOWED_ORIGINS contains an invalid origin: {origin}")
                })
        })
        .collect();

    Some(
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PATCH,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::header::ACCEPT,
            ])
            .max_age(std::time::Duration::from_secs(600)),
    )
}

/// The header name used for the request correlation id.
fn x_request_id() -> axum::http::HeaderName {
    axum::http::HeaderName::from_static("x-request-id")
}

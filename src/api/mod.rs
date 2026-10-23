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
//! CORS -> trace -> body limit -> request id (set, then propagate) -> request
//!         context -> timeout -> panic guard -> route
//! ```
//!
//! The request-id pair is deliberately adjacent and in that order: the setter
//! runs first and puts the id on the request, the propagator then copies it to
//! the response. A response produced by an inner layer - a timeout, for
//! example - still carries the id.
//!
//! `docs/architecture/request-lifecycle.md` describes the same order in more
//! detail.

pub mod extract;
pub mod middleware;
pub mod routes;

use std::sync::Arc;
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
use crate::config::RequestLimits;
use crate::config::messages::Messages;
use crate::error::AppError;
use crate::health;
use crate::state::AppState;

/// The per-request context the layers and the extractors need.
///
/// It is installed once, by the router, and read from the request extensions.
/// The alternative - reading the environment or the configuration file per
/// request - is what this type exists to prevent.
///
/// The limits it carries were resolved once, at router construction, from the
/// configuration the process validated at start-up. A handler and the timeout
/// middleware therefore read the same value, and neither can disagree with the
/// other or with what was validated.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// The client-facing message catalogue.
    pub messages: Arc<Messages>,
    /// The request limits.
    pub limits: RequestLimits,
}

impl RequestContext {
    /// Builds the context from the resolved configuration.
    pub fn new(config: &Config) -> Self {
        Self {
            messages: config.messages.clone(),
            limits: config.app.limits.clone(),
        }
    }
}

/// Builds the complete application router.
pub fn router(state: AppState) -> Router {
    let config = state.config.clone();

    let mut app = Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .nest(routes::API_PREFIX, routes::api_routes())
        .fallback(middleware::not_found);

    if config.api.docs_enabled {
        app = mount_docs(app, &config);
    }

    app = app
        .layer(CatchPanicLayer::custom(routes::handle_panic))
        // The timeout is applied before the request-id pair, which makes it
        // inner to both of them: a timed-out request therefore still carries
        // its correlation id on the response.
        .layer(axum::middleware::from_fn(timeout_middleware))
        .layer(axum::Extension(RequestContext::new(&config)))
        .layer(PropagateRequestIdLayer::new(x_request_id()))
        .layer(SetRequestIdLayer::new(x_request_id(), MakeRequestUuid))
        .layer(axum::extract::DefaultBodyLimit::max(
            config.app.limits.max_body_bytes,
        ))
        .layer(TraceLayer::new_for_http());

    if let Some(cors) = cors_layer(&config) {
        app = app.layer(cors);
    }

    app.with_state(state)
}

/// Mounts the API explorer, when the binary was built with the `docs` feature.
///
/// The Swagger UI asset bundle is 10.73 MiB and a deployment that serves no
/// documentation should not carry it, so the dependency is optional. With the
/// feature on, the explorer is merged into the application router exactly as
/// before.
#[cfg(feature = "docs")]
fn mount_docs(app: Router<AppState>, config: &Config) -> Router<AppState> {
    app.merge(routes::swagger_ui(config))
}

/// Explains, once, why the configured documentation route is not served.
///
/// A build without the `docs` feature cannot mount the explorer — the route
/// and its assets are not compiled in — but the configuration can still ask
/// for it, because `api.docs_enabled` is a deployment setting. Silently
/// returning `404` for `/api/v1/docs` would leave an operator guessing whether
/// the route is misconfigured or the binary is wrong, so the reason is logged
/// once, at router construction, and start-up continues: a missing explorer is
/// not a reason to refuse to serve the rest of the API.
#[cfg(not(feature = "docs"))]
fn mount_docs(app: Router<AppState>, _config: &Config) -> Router<AppState> {
    tracing::warn!(
        "api.docs_enabled is true, but the documentation route is not compiled into this \
         binary: it was built without the `docs` Cargo feature, so /api/v1/docs and \
         /api/v1/openapi.json will return 404. Build with the default features, or with \
         `--features docs`, to serve them."
    );
    app
}

/// Abandons a request that exceeds the configured timeout.
///
/// The timeout is reported through the standard error envelope, so a client
/// that parses the envelope does not have to special-case this response.
pub async fn timeout_middleware(request: Request, next: Next) -> Response {
    // The router installs the request context before this middleware runs,
    // so a missing context is a wiring error rather than a client error.
    let Some(context) = request.extensions().get::<RequestContext>().cloned() else {
        return AppError::Internal("the request context is missing".to_string()).into_response();
    };

    match tokio::time::timeout(context.limits.request_timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => AppError::Timeout(context.messages.general.timeout.clone()).into_response(),
    }
}

/// The lifetime of the CORS preflight cache, in seconds.
///
/// It is a constant and not a setting: the value is a browser-side cache
/// policy, it does not change what the API does with a request, and a
/// deployment has no reason to tune it. The browser caps the lifetime anyway,
/// so a larger value would buy nothing.
const CORS_PREFLIGHT_MAX_AGE_SECONDS: u64 = 600;

/// Builds the CORS layer from the configured allowlist.
///
/// `None` means CORS is disabled entirely, which is the correct behaviour for
/// an API that is not reachable from a browser. The allowlist itself was
/// validated when the configuration was loaded, so a wildcard cannot reach
/// this point.
fn cors_layer(config: &Config) -> Option<CorsLayer> {
    let origins = &config.api.cors.allowed_origins;
    if origins.is_empty() {
        return None;
    }

    // The allowlist was validated when the configuration was loaded, so every
    // origin parses. The fallback is a plain header value rather than a
    // panic, so that a mistake can never take the process down at runtime.
    let origins: Vec<axum::http::HeaderValue> = origins
        .iter()
        .map(|origin| {
            origin
                .parse::<axum::http::HeaderValue>()
                .unwrap_or_else(|_| axum::http::HeaderValue::from_static(""))
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
            .max_age(Duration::from_secs(CORS_PREFLIGHT_MAX_AGE_SECONDS)),
    )
}

/// The header name used for the request correlation id.
fn x_request_id() -> axum::http::HeaderName {
    axum::http::HeaderName::from_static("x-request-id")
}

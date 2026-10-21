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
use crate::config::messages::Messages;
use crate::error::AppError;
use crate::health;
use crate::state::AppState;

/// The per-request context the layers and the extractors need.
///
/// It is installed once, by the router, and read from the request extensions.
/// The alternative - reading the environment or the configuration file per
/// request - is what this type exists to prevent.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// The client-facing message catalogue.
    pub messages: Arc<Messages>,
    /// The request limits.
    pub settings: RequestSettings,
}

/// The request limits that the layers enforce.
#[derive(Debug, Clone, Copy)]
pub struct RequestSettings {
    /// The largest accepted request body, in bytes.
    pub max_body_bytes: usize,
    /// How long a request may take.
    pub request_timeout: Duration,
}

impl RequestContext {
    /// Builds the context from the resolved configuration.
    pub fn new(config: &Config) -> Self {
        Self {
            messages: config.messages.clone(),
            settings: RequestSettings {
                max_body_bytes: config.app.limits.max_body_bytes,
                request_timeout: config.app.limits.request_timeout,
            },
        }
    }
}

/// Builds the complete application router.
pub fn router(state: AppState) -> Router {
    let config = state.config.clone();

    let mut app = Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .nest("/api/v1", routes::api_routes())
        .fallback(middleware::not_found);

    if config.api.docs_enabled {
        app = app.merge(routes::swagger_ui(&config));
    }

    app = app
        .layer(CatchPanicLayer::custom(routes::handle_panic))
        // The timeout is applied before the request-id pair, which makes it
        // inner to both of them: a timed-out request therefore still carries
        // its correlation id on the response.
        .layer(axum::middleware::from_fn(timeout_middleware))
        .layer(axum::Extension(RequestContext::new(&config)))
        .layer(PropagateRequestIdLayer::new(x_request_id()))
        .layer(SetRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
            MakeRequestUuid,
        ))
        .layer(axum::extract::DefaultBodyLimit::max(
            config.app.limits.max_body_bytes,
        ))
        .layer(TraceLayer::new_for_http());

    if let Some(cors) = cors_layer(&config) {
        app = app.layer(cors);
    }

    app.with_state(state)
}

/// Abandons a request that exceeds the configured timeout.
///
/// The timeout is reported through the standard error envelope, so a client
/// that parses the envelope does not have to special-case this response.
pub async fn timeout_middleware(request: Request, next: Next) -> Response {
    let context = request.extensions().get::<RequestContext>().cloned();

    let (timeout, messages) = match &context {
        Some(context) => (
            context.settings.request_timeout,
            Some(context.messages.clone()),
        ),
        None => (crate::config::DEFAULT_REQUEST_TIMEOUT, None),
    };

    match tokio::time::timeout(timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => {
            let message = messages
                .as_ref()
                .map(|messages| messages.general.timeout.clone())
                .unwrap_or_else(|| "The request timed out.".to_string());
            AppError::Timeout(message).into_response()
        }
    }
}

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
            .max_age(std::time::Duration::from_secs(600)),
    )
}

/// The header name used for the request correlation id.
fn x_request_id() -> axum::http::HeaderName {
    axum::http::HeaderName::from_static("x-request-id")
}

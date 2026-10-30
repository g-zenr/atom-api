//! Integration tests for the router's CORS behaviour.
//!
//! CORS is an enforcement point for browsers, not an authentication control:
//! the server answers every request, and the `Access-Control-*` headers decide
//! only whether a browser lets its own page read the answer. The tests pin
//! exactly that split — an allowed origin receives the grant, a disallowed
//! origin receives a perfectly normal answer without the grant, and an empty
//! allowlist disables the layer entirely, which is the documented behaviour of
//! `cors_layer` in `src/api/mod.rs`.
//!
//! The allowlist is read from `api.cors.allowed_origins` once, at router
//! construction, so the tests supply it through the harness's configuration
//! override path (`state_with` on a modified `test_config()`), the same
//! mechanism every other middleware test uses. No handler, database row or
//! token is involved: `/health/live` is enough to observe the headers.

pub mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use tower::ServiceExt;

use atom_api::state::AppState;

use common::{app, state_with, test_config};

/// The one origin the allowlist names in the configured tests.
const ALLOWED_ORIGIN: &str = "https://app.example.test";

/// An origin the allowlist does not name.
const DISALLOWED_ORIGIN: &str = "https://unrelated.example.test";

/// The header the grant is carried in.
const ALLOW_ORIGIN: &str = "access-control-allow-origin";

/// A state whose CORS allowlist names exactly [`ALLOWED_ORIGIN`].
///
/// `state_with` is the same override path the rate-limit and pagination tests
/// use: the configuration is loaded from the real files and modified before the
/// router is built, because that is when the layer is built from it.
async fn state_with_one_allowed_origin() -> AppState {
    let mut config = test_config();
    config.api.cors.allowed_origins = vec![ALLOWED_ORIGIN.to_string()];
    state_with(config).await
}

/// The shipped default: an empty allowlist, which disables CORS entirely.
async fn state_with_empty_allowlist() -> AppState {
    let mut config = test_config();
    // The shipped `config/api.toml` already ships an empty list; setting it
    // again keeps this test independent of what a future default does.
    config.api.cors.allowed_origins = Vec::new();
    state_with(config).await
}

/// Sends a request to the router and returns the raw response, headers
/// included.
///
/// The harness's `send` parses the body and drops the headers; CORS lives in
/// the headers, so these tests need the response itself.
async fn send_raw(app: &Router, request: Request<Body>) -> Response {
    app.clone()
        .oneshot(request)
        .await
        .expect("the router must answer")
}

/// A GET with an optional `Origin` header.
fn get_with_origin(uri: &str, origin: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(Method::GET).uri(uri);
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }

    builder
        .body(Body::empty())
        .expect("the request must be valid")
}

/// A preflight the way a browser sends it: `OPTIONS`, with the method and
/// headers of the intended request named in the `Access-Control-Request-*`
/// headers.
fn preflight(uri: &str, origin: &str) -> Request<Body> {
    Request::builder()
        .method(Method::OPTIONS)
        .uri(uri)
        .header("origin", origin)
        .header("access-control-request-method", "GET")
        .header("access-control-request-headers", "authorization")
        .body(Body::empty())
        .expect("the request must be valid")
}

/// The value of a response header, when it is present and valid text.
fn header_value(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// The members of a comma-separated response header, trimmed.
fn header_members(response: &Response, name: &str) -> Vec<String> {
    header_value(response, name)
        .map(|value| {
            value
                .split(',')
                .map(|item| item.trim().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Asserts that no CORS header at all is present on the response.
fn assert_no_cors_headers(response: &Response, context: &str) {
    for name in [
        ALLOW_ORIGIN,
        "access-control-allow-methods",
        "access-control-allow-headers",
        "access-control-max-age",
    ] {
        assert!(
            header_value(response, name).is_none(),
            "{context}: {name} must not be present"
        );
    }
}

#[tokio::test]
async fn a_request_from_an_allowed_origin_carries_the_cors_grant() {
    let state = state_with_one_allowed_origin().await;
    let app = app(state);

    let response = send_raw(&app, get_with_origin("/health/live", Some(ALLOWED_ORIGIN))).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_value(&response, ALLOW_ORIGIN).as_deref(),
        Some(ALLOWED_ORIGIN),
        "an allowed origin must be echoed in access-control-allow-origin"
    );
}

#[tokio::test]
async fn a_request_from_a_disallowed_origin_is_answered_normally_without_the_grant() {
    let state = state_with_one_allowed_origin().await;
    let app = app(state);

    let response = send_raw(
        &app,
        get_with_origin("/health/live", Some(DISALLOWED_ORIGIN)),
    )
    .await;

    // The endpoint answers exactly as it would for any other client: CORS is
    // an enforcement point for browsers, not an authentication control. The
    // only difference is the missing grant, which is what the browser acts on.
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        header_value(&response, ALLOW_ORIGIN).is_none(),
        "a disallowed origin must not receive access-control-allow-origin"
    );
}

#[tokio::test]
async fn a_request_without_an_origin_carries_no_cors_headers() {
    let state = state_with_one_allowed_origin().await;
    let app = app(state);

    // A same-origin request carries no `Origin` header at all, and the grant
    // is decided per request, not added blanket to every response.
    let response = send_raw(&app, get_with_origin("/health/live", None)).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_no_cors_headers(&response, "a request without an Origin header");
}

#[tokio::test]
async fn a_preflight_from_an_allowed_origin_is_answered_with_the_full_grant() {
    let state = state_with_one_allowed_origin().await;
    let app = app(state);

    let response = send_raw(&app, preflight("/health/live", ALLOWED_ORIGIN)).await;

    // The layer answers the preflight itself, before the route runs, so the
    // status is the layer's own and not the route's.
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_value(&response, ALLOW_ORIGIN).as_deref(),
        Some(ALLOWED_ORIGIN),
        "the preflight must echo the allowed origin"
    );

    // The methods and headers the API accepts are advertised, so the browser
    // needs no second round trip to learn that a `Bearer` token is welcome.
    let methods = header_members(&response, "access-control-allow-methods");
    for method in ["GET", "POST", "PATCH", "PUT", "DELETE", "OPTIONS"] {
        assert!(
            methods.iter().any(|advertised| advertised == method),
            "the preflight must advertise {method}; got {methods:?}"
        );
    }

    let headers = header_members(&response, "access-control-allow-headers");
    assert!(
        headers
            .iter()
            .any(|advertised| advertised.eq_ignore_ascii_case("authorization")),
        "the preflight must allow the authorization header; got {headers:?}"
    );

    // The preflight cache lifetime is the router's documented constant
    // (`CORS_PREFLIGHT_MAX_AGE_SECONDS` in `src/api/mod.rs`).
    assert_eq!(
        header_value(&response, "access-control-max-age").as_deref(),
        Some("600"),
        "the preflight cache must be the documented 600 seconds"
    );
}

#[tokio::test]
async fn a_preflight_from_a_disallowed_origin_is_answered_without_the_grant() {
    let state = state_with_one_allowed_origin().await;
    let app = app(state);

    let response = send_raw(&app, preflight("/health/live", DISALLOWED_ORIGIN)).await;

    // The refusal is the missing echo, not an error status: a browser that
    // receives no `access-control-allow-origin` refuses to send the real
    // request, and the server itself never sees it.
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        header_value(&response, ALLOW_ORIGIN).is_none(),
        "a disallowed origin must not receive the preflight grant"
    );
}

#[tokio::test]
async fn an_empty_allowlist_disables_cors_entirely() {
    let state = state_with_empty_allowlist().await;
    let app = app(state);

    // A plain GET from any origin: answered normally, without a single CORS
    // header, because no layer is built at all when the allowlist is empty.
    let get = send_raw(&app, get_with_origin("/health/live", Some(ALLOWED_ORIGIN))).await;
    assert_eq!(get.status(), StatusCode::OK);
    assert_no_cors_headers(&get, "a GET with an empty allowlist");

    // With the layer gone there is no preflight handler either: `OPTIONS`
    // reaches the route itself, and the router's normal answer to an
    // `OPTIONS` on a GET-only route is 405.
    let options = send_raw(&app, preflight("/health/live", ALLOWED_ORIGIN)).await;
    assert_eq!(options.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_no_cors_headers(&options, "an OPTIONS with an empty allowlist");
}

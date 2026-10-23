//! The API explorer route exists only when the `docs` Cargo feature is on.
//!
//! Finding `F-44` made `utoipa-swagger-ui` optional, so the route can now be
//! compiled out. That is only safe if somebody proves the route is still there
//! when the feature *is* on — otherwise a future change could disable the
//! explorer for every default build and nothing would notice.
//!
//! The whole file is gated: with `--no-default-features` it compiles to nothing,
//! which is the correct assertion for a binary that has no route to test.

#![cfg(feature = "docs")]

// `pub` for the same reason the other test binaries use it: the dead-code lint
// fires per binary, and this file uses only part of the shared harness.
pub mod common;

use axum::http::{Method, StatusCode};
use common::{app, request, send, test_state};

#[tokio::test]
async fn the_explorer_and_its_assets_are_served() {
    let state = test_state().await;
    let app = app(state);

    // `utoipa-swagger-ui` redirects the directory without its trailing slash.
    let response = send(&app, request(Method::GET, "/api/v1/docs", None, None)).await;
    response.expect_status(StatusCode::SEE_OTHER);

    let response = send(&app, request(Method::GET, "/api/v1/docs/", None, None)).await;
    response.expect_status(StatusCode::OK);

    // An asset, not just the shell: this is what proves the bundle was embedded
    // rather than merely routed.
    let response = send(
        &app,
        request(Method::GET, "/api/v1/docs/swagger-ui.css", None, None),
    )
    .await;
    response.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn the_openapi_document_is_served() {
    let state = test_state().await;
    let app = app(state);

    let response = send(
        &app,
        request(Method::GET, "/api/v1/openapi.json", None, None),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    // The title has exactly one source, `api.title` in `config/api.toml`.
    assert_eq!(
        body["info"]["title"], "ATOM API",
        "the document must carry the configured title"
    );
    assert!(
        body["paths"].is_object(),
        "the document must describe the routes"
    );
}

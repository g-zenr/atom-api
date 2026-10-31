//! HTTP-level tests for the price-monitoring endpoints.
//!
//! The feature publishes twenty-two operations under `/api/v1`: five product
//! routes, four listing routes, three observation routes, four rule routes,
//! three automation-run routes and three alert routes. This file holds the
//! acceptance tests for them, grouped the way the feature documents group
//! them: product CRUD, listing CRUD, the idempotent observation and the rule
//! that fires on it, the run lifecycle, the alert read, pagination, the
//! credential, and the ownership rule on every id-taking verb.
//!
//! Three rules shape the file, exactly as `tests/claims_api.rs` states them:
//!
//! * **Behaviour, not implementation.** Every assertion is on the status code
//!   and the observable body, never on a log line or a timing value.
//! * **Independence.** The suite shares one real, disposable PostgreSQL
//!   database. Every test registers its own accounts, and every count is
//!   restricted to the ids that test created.
//! * **The mutating call is never the evidence.** Where an invariant is about
//!   stored state — the absorbed fingerprint, the single alert, the derived
//!   identity — the assertion is on a later read of the resource.
//!
//! The constraint-level evidence (the `CHECK`s, the composite foreign keys,
//! the idempotency index) is in `tests/monitoring_schema.rs`, which speaks
//! SQL to the database; this file drives the router.

pub mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestResponse, app, assert_pagination, register_user, request, send, test_state};
use serde_json::{Value, json};
use uuid::Uuid;

/// The collection paths of the feature.
const PRODUCTS: &str = "/api/v1/products";
const LISTINGS_OF: &str = "/api/v1/products";
const OBSERVATIONS_OF: &str = "/api/v1/listings";
const RULES_OF: &str = "/api/v1/products";
const RUNS: &str = "/api/v1/automation/runs";
const ALERTS: &str = "/api/v1/alerts";

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

/// Creates a product through the API and returns the `201` body.
async fn create_product(app: &Router, token: &str, brand: &str, model: &str) -> Value {
    create_product_with_specs(app, token, brand, model, "").await
}

/// Creates a product with a configuration, and returns the `201` body.
async fn create_product_with_specs(
    app: &Router,
    token: &str,
    brand: &str,
    model: &str,
    specs: &str,
) -> Value {
    send(
        app,
        request(
            Method::POST,
            PRODUCTS,
            Some(token),
            Some(json!({ "brand": brand, "model": model, "specs": specs })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED)
}

/// Creates a listing of a product and returns the `201` body.
async fn create_listing(
    app: &Router,
    token: &str,
    product_id: &str,
    retailer: &str,
    url: &str,
    currency: &str,
) -> Value {
    send(
        app,
        request(
            Method::POST,
            &format!("{LISTINGS_OF}/{product_id}/listings"),
            Some(token),
            Some(json!({
                "retailer": retailer,
                "url": url,
                "currency": currency,
            })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED)
}

/// Creates the product's one rule and returns the `201` body.
async fn create_rule(app: &Router, token: &str, product_id: &str, body: Value) -> Value {
    send(
        app,
        request(
            Method::POST,
            &format!("{RULES_OF}/{product_id}/rules"),
            Some(token),
            Some(body),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED)
}

/// Posts one observation and returns the raw response.
async fn post_observation(
    app: &Router,
    token: &str,
    listing_id: &str,
    body: Value,
) -> TestResponse {
    send(
        app,
        request(
            Method::POST,
            &format!("{OBSERVATIONS_OF}/{listing_id}/observations"),
            Some(token),
            Some(body),
        ),
    )
    .await
}

/// The body of a minimal observation.
fn observation_body(amount: &str, fingerprint: &str) -> Value {
    json!({
        "amount": amount,
        "currency": "EUR",
        "observed_at": "2026-10-30T09:00:00Z",
        "fingerprint": fingerprint,
    })
}

/// Posts one observation and asserts the `201` of a new row.
async fn record_observation(app: &Router, token: &str, listing_id: &str, body: Value) -> Value {
    post_observation(app, token, listing_id, body)
        .await
        .expect_status(StatusCode::CREATED)
}

/// Starts a run and returns the `201` body.
async fn start_run(app: &Router, token: &str) -> Value {
    send(
        app,
        request(Method::POST, RUNS, Some(token), Some(json!({}))),
    )
    .await
    .expect_status(StatusCode::CREATED)
}

/// Reads the raw response of a `GET` on an arbitrary path.
async fn get(app: &Router, token: &str, path: &str) -> TestResponse {
    send(app, request(Method::GET, path, Some(token), None)).await
}

/// Reads the raw response of a `PATCH` with a body.
async fn patch(app: &Router, token: &str, path: &str, body: Value) -> TestResponse {
    send(app, request(Method::PATCH, path, Some(token), Some(body))).await
}

/// Reads the raw response of a `DELETE`.
async fn delete(app: &Router, token: &str, path: &str) -> TestResponse {
    send(app, request(Method::DELETE, path, Some(token), None)).await
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

/// Asserts the stable error code of the standard envelope.
fn assert_error(body: &Value, code: &str) {
    assert_eq!(
        body["error"]["code"], code,
        "unexpected error code; the envelope was {body}"
    );
}

/// Asserts the stable code and that the first violation names `field`.
fn assert_field_error(body: &Value, code: &str, field: &str) {
    assert_error(body, code);

    let details = body["error"]["details"]
        .as_array()
        .unwrap_or_else(|| panic!("the error must carry field details: {body}"));

    assert!(
        !details.is_empty(),
        "the validation failure must name `{field}`: {body}"
    );
    assert_eq!(
        details[0]["field"], field,
        "the violation must name `{field}`: {body}"
    );
}

/// Asserts that a value is a money **string** at scale four, and returns it.
fn assert_money(value: &Value, what: &str) -> String {
    assert!(
        !value.is_number(),
        "{what} must be a JSON string, never a number: {value}"
    );

    let text = value
        .as_str()
        .unwrap_or_else(|| panic!("{what} must be a JSON string with four decimals: {value}"));

    let (integer, fraction) = text
        .split_once('.')
        .unwrap_or_else(|| panic!("{what} must carry a decimal point: {text:?}"));

    assert!(
        !integer.is_empty() && integer.len() <= 15 && integer.chars().all(|c| c.is_ascii_digit()),
        "{what} must have one to fifteen integer digits: {text:?}"
    );
    assert_eq!(
        fraction.len(),
        4,
        "{what} must carry exactly four decimals: {text:?}"
    );
    assert!(
        fraction.chars().all(|c| c.is_ascii_digit()),
        "{what}: {text:?}"
    );

    text.to_string()
}

/// Builds a unique listing URL for one test.
fn listing_url(path: &str) -> String {
    format!("https://monitoring.test/{path}/{}", Uuid::new_v4().simple())
}

// ---------------------------------------------------------------------------
// Products: CRUD and the derived identity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_product_is_created_active_with_a_derived_identity() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-product-create").await;

    let body = create_product_with_specs(
        &app,
        &user.access_token,
        "  Lenovo ",
        "ThinkPad X1 Carbon",
        "32GB RAM, 1TB SSD",
    )
    .await;

    assert_eq!(body["brand"], "Lenovo", "the brand is stored trimmed");
    assert_eq!(body["model"], "ThinkPad X1 Carbon");
    assert_eq!(body["specs"], "32GB RAM, 1TB SSD");
    assert_eq!(body["status"], "active", "a product is born active");
    assert_eq!(
        body["normalized_name"], "lenovo thinkpad x1 carbon",
        "the identity is derived, trimmed and lower-cased"
    );

    let hash = body["config_hash"].as_str().expect("a config hash");
    assert_eq!(
        hash.len(),
        64,
        "the configuration hash is a SHA-256 hex digest"
    );
    assert!(
        hash.bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "the digest is lower case: {hash}"
    );

    // The stored state is the evidence: a later read sees the same identity.
    let read = get(
        &app,
        &user.access_token,
        &format!("{PRODUCTS}/{}", body["id"].as_str().unwrap()),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(read["normalized_name"], body["normalized_name"]);
    assert_eq!(read["config_hash"], hash);
}

#[tokio::test]
async fn a_duplicate_product_identity_is_a_409_and_a_new_configuration_is_not() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-product-dupe").await;

    create_product_with_specs(&app, &user.access_token, "Lenovo", "X1", "16GB RAM").await;

    let duplicate = send(
        &app,
        request(
            Method::POST,
            PRODUCTS,
            Some(&user.access_token),
            Some(json!({ "brand": "  lenovo ", "model": "X1", "specs": "16GB RAM" })),
        ),
    )
    .await
    .expect_status(StatusCode::CONFLICT);
    assert_error(&duplicate, "conflict");

    // A different configuration is a different product, even for the same
    // model: the identity is (name, configuration), not the name alone.
    send(
        &app,
        request(
            Method::POST,
            PRODUCTS,
            Some(&user.access_token),
            Some(json!({ "brand": "Lenovo", "model": "X1", "specs": "32GB RAM" })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);
}

#[tokio::test]
async fn a_product_patch_merges_fields_and_rederives_the_identity() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-product-patch").await;

    let product = create_product_with_specs(&app, &user.access_token, "Lenovo", "X1", "").await;
    let product_id = product["id"].as_str().unwrap();

    // An omitted field is left untouched; a changed one re-derives.
    let patched = patch(
        &app,
        &user.access_token,
        &format!("{PRODUCTS}/{product_id}"),
        json!({ "model": "X1 Carbon", "status": "paused" }),
    )
    .await
    .expect_status(StatusCode::OK);

    assert_eq!(patched["brand"], "Lenovo", "the omitted brand is untouched");
    assert_eq!(patched["model"], "X1 Carbon");
    assert_eq!(patched["status"], "paused");
    assert_eq!(
        patched["normalized_name"], "lenovo x1 carbon",
        "the identity follows the effective model"
    );

    // `""` is the clear of the configuration text; the identity does not move.
    let cleared = patch(
        &app,
        &user.access_token,
        &format!("{PRODUCTS}/{product_id}"),
        json!({ "specs": "" }),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(cleared["specs"], "");
    assert_eq!(cleared["normalized_name"], patched["normalized_name"]);
}

#[tokio::test]
async fn a_product_name_is_validated_before_any_statement_runs() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-product-validate").await;

    for (field, body) in [
        ("brand", json!({ "brand": "   ", "model": "X1" })),
        ("brand", json!({ "brand": "x".repeat(101), "model": "X1" })),
        ("model", json!({ "brand": "Lenovo", "model": "" })),
        (
            "model",
            json!({ "brand": "Lenovo", "model": "x".repeat(101) }),
        ),
    ] {
        let response = send(
            &app,
            request(Method::POST, PRODUCTS, Some(&user.access_token), Some(body)),
        )
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&response, "validation_failed", field);
    }
}

#[tokio::test]
async fn deleting_a_product_removes_everything_under_it() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-product-delete").await;
    let token = user.access_token.as_str();

    let product = create_product(&app, token, "Lenovo", "X1").await;
    let product_id = product["id"].as_str().unwrap();

    let listing = create_listing(
        &app,
        token,
        product_id,
        "Retailer",
        &listing_url("delete"),
        "EUR",
    )
    .await;
    create_rule(
        &app,
        token,
        product_id,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;
    record_observation(
        &app,
        token,
        listing["id"].as_str().unwrap(),
        observation_body("499.0000", "delete-obs-1"),
    )
    .await;

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 1,
        "the fixture raised one alert"
    );

    delete(&app, token, &format!("{PRODUCTS}/{product_id}"))
        .await
        .expect_status(StatusCode::NO_CONTENT);

    get(&app, token, &format!("{PRODUCTS}/{product_id}"))
        .await
        .expect_status(StatusCode::NOT_FOUND);
    // The listing route answers `404` on the product, because the product is
    // gone; the alert list is empty, because the cascade removed the alert.
    get(&app, token, &format!("{LISTINGS_OF}/{product_id}/listings"))
        .await
        .expect_status(StatusCode::NOT_FOUND);

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 0,
        "the product's alerts went with it: {alerts}"
    );
}

// ---------------------------------------------------------------------------
// Listings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_listing_is_created_with_its_defaults_and_a_normalised_currency() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-listing-create").await;

    let product = create_product(&app, &user.access_token, "Lenovo", "X1").await;
    let url = listing_url("listing-create");

    let listing = create_listing(
        &app,
        &user.access_token,
        product["id"].as_str().unwrap(),
        "Notebooksbilliger",
        &url,
        "eur",
    )
    .await;

    assert_eq!(listing["retailer"], "Notebooksbilliger");
    assert_eq!(listing["url"], url);
    assert_eq!(listing["currency"], "EUR", "the code is stored upper-cased");
    assert_eq!(
        listing["availability"], "unknown",
        "a listing starts unknown"
    );
    assert_eq!(
        listing["validation_state"], "valid",
        "a listing starts valid"
    );
    assert_eq!(
        listing["last_observed_at"],
        Value::Null,
        "no observation has been written yet"
    );

    // The stored state is the evidence.
    let read = get(
        &app,
        &user.access_token,
        &format!("{LISTINGS_OF}/{}/listings", product["id"].as_str().unwrap()),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(read["pagination"]["total"], 1);
    assert_eq!(read["items"][0]["url"], url);
}

#[tokio::test]
async fn a_duplicate_listing_url_is_a_409_for_the_same_owner_and_fine_for_another() {
    let app = app(test_state().await);
    let owner = register_user(&app, "mon-listing-dupe-a").await;
    let other = register_user(&app, "mon-listing-dupe-b").await;

    let product = create_product(&app, &owner.access_token, "Lenovo", "X1").await;
    let other_product = create_product(&app, &other.access_token, "Dell", "XPS").await;
    let url = listing_url("listing-dupe");

    create_listing(
        &app,
        &owner.access_token,
        product["id"].as_str().unwrap(),
        "Retailer",
        &url,
        "EUR",
    )
    .await;

    let duplicate = send(
        &app,
        request(
            Method::POST,
            &format!("{LISTINGS_OF}/{}/listings", product["id"].as_str().unwrap()),
            Some(&owner.access_token),
            Some(json!({ "retailer": "Retailer", "url": url, "currency": "EUR" })),
        ),
    )
    .await
    .expect_status(StatusCode::CONFLICT);
    assert_error(&duplicate, "conflict");

    // The same URL under another owner is a different row: the unique index
    // is per owner.
    create_listing(
        &app,
        &other.access_token,
        other_product["id"].as_str().unwrap(),
        "Retailer",
        &url,
        "EUR",
    )
    .await;
}

#[tokio::test]
async fn the_listing_patch_carries_the_tri_state_and_the_two_reports() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-listing-patch").await;

    let product = create_product(&app, &user.access_token, "Lenovo", "X1").await;
    let product_id = product["id"].as_str().unwrap();
    let listing = create_listing(
        &app,
        &user.access_token,
        product_id,
        "Retailer",
        &listing_url("patch"),
        "EUR",
    )
    .await;
    let listing_id = listing["id"].as_str().unwrap();
    let path = format!("{LISTINGS_OF}/{product_id}/listings/{listing_id}");

    // Absent leaves the external identifier untouched.
    let kept = patch(&app, &user.access_token, &path, json!({}))
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(kept["external_id"], Value::Null);

    // A value sets it, `null` clears it. The evidence is the collection read:
    // a listing has no single-resource read, so the state after each write is
    // the listing inside the product's collection.
    let listings_of = format!("{LISTINGS_OF}/{product_id}/listings");
    patch(
        &app,
        &user.access_token,
        &path,
        json!({ "external_id": "SKU-1" }),
    )
    .await
    .expect_status(StatusCode::OK);
    let set = get(&app, &user.access_token, &listings_of)
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(set["items"][0]["external_id"], "SKU-1");

    // The two reports the workflow writes, in one patch.
    let reported = patch(
        &app,
        &user.access_token,
        &path,
        json!({ "external_id": null, "availability": "unavailable", "validation_state": "stale" }),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(
        reported["external_id"],
        Value::Null,
        "an explicit null clears"
    );
    assert_eq!(reported["availability"], "unavailable");
    assert_eq!(reported["validation_state"], "stale");

    let stored = get(&app, &user.access_token, &listings_of)
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(stored["items"][0]["availability"], "unavailable");
    assert_eq!(stored["items"][0]["validation_state"], "stale");
}

#[tokio::test]
async fn a_listing_is_addressable_only_inside_its_product() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-listing-scope").await;
    let token = user.access_token.as_str();

    let first = create_product(&app, token, "Lenovo", "X1").await;
    let second = create_product(&app, token, "Dell", "XPS").await;
    let listing = create_listing(
        &app,
        token,
        first["id"].as_str().unwrap(),
        "Retailer",
        &listing_url("scope"),
        "EUR",
    )
    .await;

    // The listing exists, but under the first product: addressing it through
    // the second is a `404`, for both verbs that take a listing id.
    let wrong_product = format!("{LISTINGS_OF}/{}/listings", second["id"].as_str().unwrap());
    let patch_response = patch(
        &app,
        token,
        &format!("{wrong_product}/{}", listing["id"].as_str().unwrap()),
        json!({ "availability": "available" }),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&patch_response, "not_found");

    delete(
        &app,
        token,
        &format!("{wrong_product}/{}", listing["id"].as_str().unwrap()),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);

    // The correct address still works, so the refusals above were the
    // address's and not the row's.
    patch(
        &app,
        token,
        &format!(
            "{LISTINGS_OF}/{}/listings/{}",
            first["id"].as_str().unwrap(),
            listing["id"].as_str().unwrap()
        ),
        json!({ "availability": "available" }),
    )
    .await
    .expect_status(StatusCode::OK);
}

#[tokio::test]
async fn a_listing_url_and_currency_are_validated() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-listing-validate").await;

    let product = create_product(&app, &user.access_token, "Lenovo", "X1").await;
    let path = format!("{LISTINGS_OF}/{}/listings", product["id"].as_str().unwrap());

    for url in [
        "example.test/listing",
        "/relative/path",
        "javascript:alert(1)",
        "ftp://example.test/a",
        "https://",
        "",
        "   ",
    ] {
        let response = send(
            &app,
            request(
                Method::POST,
                &path,
                Some(&user.access_token),
                Some(json!({ "retailer": "Retailer", "url": url, "currency": "EUR" })),
            ),
        )
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&response, "validation_failed", "url");
    }

    let response = send(
        &app,
        request(
            Method::POST,
            &path,
            Some(&user.access_token),
            Some(json!({
                "retailer": "Retailer",
                "url": listing_url("valid"),
                "currency": "EURO",
            })),
        ),
    )
    .await
    .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&response, "validation_failed", "currency");
}

// ---------------------------------------------------------------------------
// Observations: the history, the idempotency, the validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_observation_history_is_newest_first_and_paginated() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-obs-history").await;
    let token = user.access_token.as_str();

    let product = create_product(&app, token, "Lenovo", "X1").await;
    let listing = create_listing(
        &app,
        token,
        product["id"].as_str().unwrap(),
        "Retailer",
        &listing_url("history"),
        "EUR",
    )
    .await;
    let listing_id = listing["id"].as_str().unwrap();

    for (index, (day, amount)) in [(28, "1200.0000"), (29, "1150.0000"), (30, "1100.0000")]
        .iter()
        .enumerate()
    {
        record_observation(
            &app,
            token,
            listing_id,
            json!({
                "amount": amount,
                "currency": "EUR",
                "observed_at": format!("2026-10-{day}T09:00:00Z"),
                "fingerprint": format!("history-{index}"),
            }),
        )
        .await;
    }

    let page = get(
        &app,
        token,
        &format!("{OBSERVATIONS_OF}/{listing_id}/observations"),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_pagination(&page, 3, 20, 0);

    let amounts: Vec<&str> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["amount"].as_str().unwrap())
        .collect();
    assert_eq!(
        amounts,
        ["1100.0000", "1150.0000", "1200.0000"],
        "newest first"
    );

    let second_page = get(
        &app,
        token,
        &format!("{OBSERVATIONS_OF}/{listing_id}/observations?limit=2&offset=1"),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_pagination(&second_page, 3, 2, 1);
    assert_eq!(second_page["items"].as_array().unwrap().len(), 2);
}

/// The idempotency rule of `ADR-0012` §Decision 5, driven through the API: the
/// same fingerprint is answered `200` with the **existing** observation, and
/// the history still holds one row.
#[tokio::test]
async fn the_same_fingerprint_is_absorbed_with_a_200_and_one_row() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-obs-idempotent").await;
    let token = user.access_token.as_str();

    let product = create_product(&app, token, "Lenovo", "X1").await;
    let listing = create_listing(
        &app,
        token,
        product["id"].as_str().unwrap(),
        "Retailer",
        &listing_url("idempotent"),
        "EUR",
    )
    .await;
    let listing_id = listing["id"].as_str().unwrap();

    let first = post_observation(
        &app,
        token,
        listing_id,
        observation_body("1499.0000", "same-fingerprint"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let retry = post_observation(
        &app,
        token,
        listing_id,
        // A different amount under the same fingerprint is still the same
        // observation to the idempotency index: the fingerprint is the
        // identity, and the stored row wins.
        json!({
            "amount": "1599.0000",
            "currency": "EUR",
            "observed_at": "2026-10-30T09:00:00Z",
            "fingerprint": "same-fingerprint",
        }),
    )
    .await
    .expect_status(StatusCode::OK);

    assert_eq!(
        retry["id"], first["id"],
        "the retry returns the existing observation"
    );
    assert_eq!(retry["amount"], first["amount"], "nothing was rewritten");
    assert_eq!(retry["created_at"], first["created_at"]);

    let history = get(
        &app,
        token,
        &format!("{OBSERVATIONS_OF}/{listing_id}/observations"),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(
        history["pagination"]["total"], 1,
        "a retried workflow cannot double-write: {history}"
    );
}

#[tokio::test]
async fn an_observation_amount_is_a_four_decimal_string_and_the_fingerprint_is_bounded() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-obs-validate").await;
    let token = user.access_token.as_str();

    let product = create_product(&app, token, "Lenovo", "X1").await;
    let listing = create_listing(
        &app,
        token,
        product["id"].as_str().unwrap(),
        "Retailer",
        &listing_url("obs-validate"),
        "EUR",
    )
    .await;
    let listing_id = listing["id"].as_str().unwrap();

    for amount in ["12.5", "12.500", "12.50000", "-1.0000", "abc", "1,000.0000"] {
        let response = post_observation(
            &app,
            token,
            listing_id,
            json!({
                "amount": amount,
                "currency": "EUR",
                "observed_at": "2026-10-30T09:00:00Z",
                "fingerprint": "validation-obs",
            }),
        )
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&response, "validation_failed", "amount");
    }

    // A JSON number is refused by the extractor before the service runs: the
    // wire form of money is a string (`ADR-0007`).
    let number = send(
        &app,
        request(
            Method::POST,
            &format!("{OBSERVATIONS_OF}/{listing_id}/observations"),
            Some(token),
            Some(json!({
                "amount": 12.5,
                "currency": "EUR",
                "observed_at": "2026-10-30T09:00:00Z",
                "fingerprint": "validation-obs-number",
            })),
        ),
    )
    .await
    .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&number, "validation_failed");

    for fingerprint in ["", "   ", "x".repeat(201).as_str()] {
        let response = post_observation(
            &app,
            token,
            listing_id,
            json!({
                "amount": "1.0000",
                "currency": "EUR",
                "observed_at": "2026-10-30T09:00:00Z",
                "fingerprint": fingerprint,
            }),
        )
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&response, "validation_failed", "fingerprint");
    }
}

#[tokio::test]
async fn the_latest_observation_is_the_newest_one_and_a_404_when_there_is_none() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-obs-latest").await;
    let token = user.access_token.as_str();

    let product = create_product(&app, token, "Lenovo", "X1").await;
    let listing = create_listing(
        &app,
        token,
        product["id"].as_str().unwrap(),
        "Retailer",
        &listing_url("latest"),
        "EUR",
    )
    .await;
    let listing_id = listing["id"].as_str().unwrap();

    // A listing with no observations is a `404` on the observation.
    let empty = get(
        &app,
        token,
        &format!("{OBSERVATIONS_OF}/{listing_id}/observations/latest"),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&empty, "not_found");

    record_observation(
        &app,
        token,
        listing_id,
        json!({
            "amount": "100.0000",
            "currency": "EUR",
            "observed_at": "2026-10-29T09:00:00Z",
            "fingerprint": "latest-1",
        }),
    )
    .await;
    let newest = record_observation(
        &app,
        token,
        listing_id,
        json!({
            "amount": "90.0000",
            "currency": "EUR",
            "observed_at": "2026-10-30T09:00:00Z",
            "fingerprint": "latest-2",
        }),
    )
    .await;

    let latest = get(
        &app,
        token,
        &format!("{OBSERVATIONS_OF}/{listing_id}/observations/latest"),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(
        latest["id"], newest["id"],
        "the newest, not the first written"
    );
}

#[tokio::test]
async fn a_foreign_listing_is_a_404_on_every_observation_verb() {
    let app = app(test_state().await);
    let owner = register_user(&app, "mon-obs-owner").await;
    let intruder = register_user(&app, "mon-obs-intruder").await;

    let product = create_product(&app, &owner.access_token, "Lenovo", "X1").await;
    let listing = create_listing(
        &app,
        &owner.access_token,
        product["id"].as_str().unwrap(),
        "Retailer",
        &listing_url("foreign"),
        "EUR",
    )
    .await;
    let listing_id = listing["id"].as_str().unwrap();

    let posted = post_observation(
        &app,
        &intruder.access_token,
        listing_id,
        observation_body("1.0000", "intruder-obs"),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&posted, "not_found");

    let listed = get(
        &app,
        &intruder.access_token,
        &format!("{OBSERVATIONS_OF}/{listing_id}/observations"),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&listed, "not_found");

    let latest = get(
        &app,
        &intruder.access_token,
        &format!("{OBSERVATIONS_OF}/{listing_id}/observations/latest"),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&latest, "not_found");
}

#[tokio::test]
async fn an_observation_can_name_a_run_and_a_foreign_run_is_a_404() {
    let app = app(test_state().await);
    let owner = register_user(&app, "mon-obs-run-owner").await;
    let intruder = register_user(&app, "mon-obs-run-intruder").await;

    let product = create_product(&app, &owner.access_token, "Lenovo", "X1").await;
    let listing = create_listing(
        &app,
        &owner.access_token,
        product["id"].as_str().unwrap(),
        "Retailer",
        &listing_url("obs-run"),
        "EUR",
    )
    .await;
    let listing_id = listing["id"].as_str().unwrap();

    let run = start_run(&app, &owner.access_token).await;

    let recorded = record_observation(
        &app,
        &owner.access_token,
        listing_id,
        json!({
            "amount": "10.0000",
            "currency": "EUR",
            "observed_at": "2026-10-30T09:00:00Z",
            "fingerprint": "obs-with-run",
            "run_id": run["id"],
        }),
    )
    .await;
    assert_eq!(
        recorded["run_id"], run["id"],
        "the run is recorded with the row"
    );

    // Another account's run is a `404` on the run, never a foreign key error.
    let foreign_run = start_run(&app, &intruder.access_token).await;
    let refused = post_observation(
        &app,
        &owner.access_token,
        listing_id,
        json!({
            "amount": "11.0000",
            "currency": "EUR",
            "observed_at": "2026-10-30T10:00:00Z",
            "fingerprint": "obs-foreign-run",
            "run_id": foreign_run["id"],
        }),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&refused, "not_found");
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_rule_is_created_enabled_and_a_second_one_is_refused() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-rule-create").await;
    let token = user.access_token.as_str();

    let product = create_product(&app, token, "Lenovo", "X1").await;
    let product_id = product["id"].as_str().unwrap();

    let rule = create_rule(
        &app,
        token,
        product_id,
        json!({ "target_amount": "1000.0000", "target_currency": "eur", "percent_drop": 15 }),
    )
    .await;

    assert_eq!(
        rule["target_currency"], "EUR",
        "the code is stored upper-cased"
    );
    assert_money(&rule["target_amount"], "target_amount");
    assert_eq!(rule["enabled"], true, "a rule is born enabled");
    assert_eq!(rule["notify_via"], "inbox", "the one channel there is");
    assert_eq!(rule["percent_drop"], 15);

    let duplicate = send(
        &app,
        request(
            Method::POST,
            &format!("{RULES_OF}/{product_id}/rules"),
            Some(token),
            Some(json!({ "target_amount": "900.0000", "target_currency": "EUR" })),
        ),
    )
    .await
    .expect_status(StatusCode::CONFLICT);
    assert_error(&duplicate, "conflict");

    // The refusal was real: the listing of the rule collection holds one row.
    let rules = get(&app, token, &format!("{RULES_OF}/{product_id}/rules"))
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(rules["pagination"]["total"], 1);
}

#[tokio::test]
async fn the_rule_patch_toggles_the_switch_the_target_and_the_percent_drop() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-rule-patch").await;
    let token = user.access_token.as_str();

    let product = create_product(&app, token, "Lenovo", "X1").await;
    let product_id = product["id"].as_str().unwrap();
    let rule = create_rule(
        &app,
        token,
        product_id,
        json!({ "target_amount": "1000.0000", "target_currency": "EUR", "percent_drop": 15 }),
    )
    .await;
    let rule_id = rule["id"].as_str().unwrap();
    let path = format!("{RULES_OF}/{product_id}/rules/{rule_id}");

    let disabled = patch(&app, token, &path, json!({ "enabled": false }))
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(disabled["enabled"], false);

    // The percent drop is a tri-state: absent leaves it, `null` clears it.
    let kept = patch(&app, token, &path, json!({ "target_amount": "900.0000" }))
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(
        kept["percent_drop"], 15,
        "an omitted tri-state is untouched"
    );
    assert_money(&kept["target_amount"], "target_amount");

    let cleared = patch(&app, token, &path, json!({ "percent_drop": null }))
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(
        cleared["percent_drop"],
        Value::Null,
        "an explicit null clears"
    );
    assert_eq!(cleared["target_amount"], "900.0000", "the target survived");
}

#[tokio::test]
async fn a_rule_target_and_percent_drop_are_validated() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-rule-validate").await;
    let token = user.access_token.as_str();

    let product = create_product(&app, token, "Lenovo", "X1").await;
    let product_id = product["id"].as_str().unwrap();

    for percent_drop in [0, -5, 100, 1000] {
        let response = send(
            &app,
            request(
                Method::POST,
                &format!("{RULES_OF}/{product_id}/rules"),
                Some(token),
                Some(json!({
                    "target_amount": "500.0000",
                    "target_currency": "EUR",
                    "percent_drop": percent_drop,
                })),
            ),
        )
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&response, "validation_failed", "percent_drop");
    }

    let response = send(
        &app,
        request(
            Method::POST,
            &format!("{RULES_OF}/{product_id}/rules"),
            Some(token),
            Some(json!({ "target_amount": "500", "target_currency": "EUR" })),
        ),
    )
    .await
    .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&response, "validation_failed", "target_amount");
}

#[tokio::test]
async fn a_rule_is_addressable_only_inside_its_product_and_its_owner() {
    let app = app(test_state().await);
    let owner = register_user(&app, "mon-rule-owner").await;
    let intruder = register_user(&app, "mon-rule-intruder").await;
    let token = owner.access_token.as_str();

    let first = create_product(&app, token, "Lenovo", "X1").await;
    let second = create_product(&app, token, "Dell", "XPS").await;
    let rule = create_rule(
        &app,
        token,
        first["id"].as_str().unwrap(),
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;
    let rule_id = rule["id"].as_str().unwrap();

    // The same owner's other product does not address the rule either.
    let wrong_product = format!("{RULES_OF}/{}/rules", second["id"].as_str().unwrap());
    patch(
        &app,
        token,
        &format!("{wrong_product}/{rule_id}"),
        json!({ "enabled": false }),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    delete(&app, token, &format!("{wrong_product}/{rule_id}"))
        .await
        .expect_status(StatusCode::NOT_FOUND);

    // Another account gets the same `404` for the same address.
    let foreign = patch(
        &app,
        &intruder.access_token,
        &format!(
            "{RULES_OF}/{}/rules/{}",
            first["id"].as_str().unwrap(),
            rule_id
        ),
        json!({ "enabled": false }),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&foreign, "not_found");
}

// ---------------------------------------------------------------------------
// The rule evaluation and the alerts
// ---------------------------------------------------------------------------

/// The full trigger fixture: a product, a listing and the rule. Returns the
/// ids the observation tests need.
async fn trigger_fixture(app: &Router, token: &str, rule: Value) -> (String, String, String) {
    let product = create_product(app, token, "Lenovo", "X1").await;
    let product_id = product["id"].as_str().unwrap().to_string();
    let listing = create_listing(
        app,
        token,
        &product_id,
        "Retailer",
        &listing_url("trigger"),
        "EUR",
    )
    .await;
    let listing_id = listing["id"].as_str().unwrap().to_string();
    let rule_id = create_rule(app, token, &product_id, rule).await;
    let rule_id = rule_id["id"].as_str().unwrap().to_string();

    (product_id, listing_id, rule_id)
}

#[tokio::test]
async fn a_reached_target_raises_exactly_one_alert_and_a_repeat_raises_none() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-trigger-target").await;
    let token = user.access_token.as_str();

    let (_, listing_id, _) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;

    // Above the target: nothing.
    record_observation(
        &app,
        token,
        &listing_id,
        observation_body("999.0000", "target-obs-1"),
    )
    .await;
    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 0,
        "no alert above the target"
    );

    // At the target: one alert, in the same transaction as the insert.
    let triggering = record_observation(
        &app,
        token,
        &listing_id,
        observation_body("499.9900", "target-obs-2"),
    )
    .await;

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 1,
        "one alert for one trigger"
    );
    assert_eq!(alerts["items"][0]["observation_id"], triggering["id"]);
    assert_eq!(alerts["items"][0]["notification_status"], "pending");
    assert_eq!(alerts["items"][0]["read_at"], Value::Null);

    // A different fingerprint below the target is a **new** observation, and
    // the rule fires again — but on a new observation, so a new alert is
    // representable. The unrepresentable duplicate is the same observation
    // firing twice, which the absorbed fingerprint covers below.
    let again = record_observation(
        &app,
        token,
        &listing_id,
        observation_body("480.0000", "target-obs-3"),
    )
    .await;

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 2,
        "a new observation may fire again"
    );
    assert_eq!(alerts["items"][0]["observation_id"], again["id"]);

    // The retried write of the triggering observation is absorbed, so it does
    // not raise a second alert for the same (rule, observation) pair.
    let retry = post_observation(
        &app,
        token,
        &listing_id,
        observation_body("499.9900", "target-obs-2"),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(retry["id"], triggering["id"]);

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 2,
        "the retried write raised no duplicate alert: {alerts}"
    );
}

#[tokio::test]
async fn a_percent_drop_against_the_previous_observation_raises_an_alert() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-trigger-percent").await;
    let token = user.access_token.as_str();

    // A target no reachable price meets, so the only live trigger is the
    // percentage.
    let (_, listing_id, _) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "1.0000", "target_currency": "EUR", "percent_drop": 10 }),
    )
    .await;

    record_observation(
        &app,
        token,
        &listing_id,
        observation_body("1000.0000", "percent-obs-1"),
    )
    .await;

    // A 5% drop: below the threshold, no alert.
    record_observation(
        &app,
        token,
        &listing_id,
        observation_body("955.0000", "percent-obs-2"),
    )
    .await;
    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(alerts["pagination"]["total"], 0, "a 4.5% drop is not 10%");

    // A 15% drop from the previous observation: one alert.
    record_observation(
        &app,
        token,
        &listing_id,
        observation_body("810.0000", "percent-obs-3"),
    )
    .await;
    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 1,
        "810 against 955 is a 15% drop"
    );
}

#[tokio::test]
async fn a_price_in_another_currency_does_not_meet_the_target() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-trigger-currency").await;
    let token = user.access_token.as_str();

    let (_, listing_id, _) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;

    // At the target amount, but in another currency: the rule never converts,
    // so it never fires. The disagreement is data, not a conversion input.
    record_observation(
        &app,
        token,
        &listing_id,
        json!({
            "amount": "400.0000",
            "currency": "USD",
            "observed_at": "2026-10-30T09:00:00Z",
            "fingerprint": "currency-obs-1",
        }),
    )
    .await;

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 0,
        "a USD price does not meet a EUR target: {alerts}"
    );
}

#[tokio::test]
async fn a_disabled_rule_does_not_fire() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-trigger-disabled").await;
    let token = user.access_token.as_str();

    let (product_id, listing_id, rule_id) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;

    patch(
        &app,
        token,
        &format!("{RULES_OF}/{product_id}/rules/{rule_id}"),
        json!({ "enabled": false }),
    )
    .await
    .expect_status(StatusCode::OK);

    record_observation(
        &app,
        token,
        &listing_id,
        observation_body("100.0000", "disabled-obs-1"),
    )
    .await;

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 0,
        "a disabled rule is not evaluated"
    );
}

#[tokio::test]
async fn an_observation_exactly_at_the_target_fires_the_rule() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-trigger-equality").await;
    let token = user.access_token.as_str();

    let (_, listing_id, rule_id) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;

    // The boundary case: the price equals the target exactly. The SQL
    // predicate is `target_amount >= price`, so "at the target" fires — a
    // regression to a strict `>` would pass every other trigger test in
    // this file and silently change the product behaviour this rule
    // names ("at or below").
    let triggering = record_observation(
        &app,
        token,
        &listing_id,
        observation_body("500.0000", "target-equality-obs-1"),
    )
    .await;

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 1,
        "equality meets the target: {alerts}"
    );
    assert_eq!(alerts["items"][0]["observation_id"], triggering["id"]);
    assert_eq!(alerts["items"][0]["rule_id"], rule_id);
}

// ---------------------------------------------------------------------------
// Concurrent ingestion
// ---------------------------------------------------------------------------
//
// The two guarantees the observation transaction is built for — the
// `FOR UPDATE` serialisation and the `ON CONFLICT` absorption — are
// properties of the final state, so both tests race two real requests and
// then verify the database, not just the answers.
//
// The race is the same deterministic mechanism as
// `tests/auth_api.rs`'s refresh race: both request futures are created
// before either is awaited and driven together by `tokio::join!`, so both
// requests are in flight at once — each enters its observation
// transaction while the other is inside its own. No sleep synchronises
// them; none is needed, because every assertion below holds for **any**
// interleaving: the listing's row lock makes the loser wait, and the
// asserted end state is which requests were issued, not which one won
// the lock. What the test cannot prove is that both requests were
// between their BEGIN and COMMIT at the same wall-clock instant — the
// residual timing dependence of any in-process race — which the lock
// makes irrelevant to the outcome it pins.

/// Counts a listing's observations, spoken in SQL to the same database the
/// handler wrote to.
async fn observation_rows(pool: &sqlx::PgPool, listing_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM price_observations WHERE listing_id = $1")
        .bind(Uuid::parse_str(listing_id).expect("the listing id is a UUID"))
        .fetch_one(pool)
        .await
        .expect("the observation count must be readable")
}

/// Counts a rule's alerts, spoken in SQL to the same database the handler
/// wrote to.
async fn alert_rows(pool: &sqlx::PgPool, rule_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM price_alerts WHERE rule_id = $1")
        .bind(Uuid::parse_str(rule_id).expect("the rule id is a UUID"))
        .fetch_one(pool)
        .await
        .expect("the alert count must be readable")
}

#[tokio::test]
async fn two_concurrent_distinct_fingerprints_both_write_and_both_fire() {
    let state = test_state().await;
    let app = app(state.clone());
    let user = register_user(&app, "mon-race-distinct").await;
    let token = user.access_token.as_str();

    let (_, listing_id, rule_id) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;

    // Two different collections of the same listing, issued at the same
    // time. Both prices are below the target, so the rule's semantics say
    // both fire — whatever order the row lock grants.
    let first = post_observation(
        &app,
        token,
        &listing_id,
        observation_body("100.0000", "race-distinct-obs-1"),
    );
    let second = post_observation(
        &app,
        token,
        &listing_id,
        observation_body("120.0000", "race-distinct-obs-2"),
    );
    let (first, second) = tokio::join!(first, second);

    // Distinct fingerprints are distinct observations: both answers are
    // `201`, and neither may absorb the other.
    let first = first.expect_status(StatusCode::CREATED);
    let second = second.expect_status(StatusCode::CREATED);
    assert_ne!(
        first["id"], second["id"],
        "two distinct collections are two rows: {first} / {second}"
    );

    // The final database state carries both rows — no silent absorption —
    // and the rule fired on each of them, because the serialised
    // evaluation reads the other's committed row, not a stale previous.
    assert_eq!(
        observation_rows(&state.db, &listing_id).await,
        2,
        "both concurrent observations are stored"
    );
    assert_eq!(
        alert_rows(&state.db, &rule_id).await,
        2,
        "the rule fired once per stored observation"
    );

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        alerts["pagination"]["total"], 2,
        "the API view agrees with the SQL view: {alerts}"
    );
}

#[tokio::test]
async fn two_concurrent_same_fingerprint_writes_leave_exactly_one_row() {
    let state = test_state().await;
    let app = app(state.clone());
    let user = register_user(&app, "mon-race-same").await;
    let token = user.access_token.as_str();

    let (_, listing_id, rule_id) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;

    // The same collection delivered twice at the same moment — a retried
    // workflow whose two attempts overlap. The idempotency identity is the
    // fingerprint, so the two writers race on one row.
    let body = observation_body("100.0000", "race-same-obs-1");
    let first = post_observation(&app, token, &listing_id, body.clone());
    let second = post_observation(&app, token, &listing_id, body);
    let (first, second) = tokio::join!(first, second);

    // Exactly one write wins (`201`) and the loser is absorbed (`200`)
    // with the winner's row — never a second `201`, never an error. Which
    // of the two is which depends on who wins the row lock, so the bodies
    // are assigned by their answers, not by position.
    let statuses = [first.status, second.status];
    let created = statuses
        .iter()
        .filter(|status| **status == StatusCode::CREATED)
        .count();
    let absorbed = statuses
        .iter()
        .filter(|status| **status == StatusCode::OK)
        .count();
    assert_eq!(created, 1, "exactly one write may win: {statuses:?}");
    assert_eq!(
        absorbed, 1,
        "the loser must be absorbed, not refused: {statuses:?}"
    );

    let (winner, absorbed_row) = if first.status == StatusCode::CREATED {
        (
            first.expect_status(StatusCode::CREATED),
            second.expect_status(StatusCode::OK),
        )
    } else {
        (
            second.expect_status(StatusCode::CREATED),
            first.expect_status(StatusCode::OK),
        )
    };
    assert_eq!(
        winner["id"], absorbed_row["id"],
        "the absorption returns the row that was written: {winner} / {absorbed_row}"
    );

    // The final database state: one observation, one alert — the absorbed
    // retry raised nothing, because the evaluation runs for the write that
    // created the row only.
    assert_eq!(
        observation_rows(&state.db, &listing_id).await,
        1,
        "the idempotency index absorbs the concurrent duplicate"
    );
    assert_eq!(
        alert_rows(&state.db, &rule_id).await,
        1,
        "the absorbed retry raised no duplicate alert"
    );
}

#[tokio::test]
async fn the_unread_filter_and_the_read_mark_follow_the_patch() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-alert-read").await;
    let token = user.access_token.as_str();

    let (_, listing_id, _) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;
    record_observation(
        &app,
        token,
        &listing_id,
        observation_body("400.0000", "read-obs-1"),
    )
    .await;

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    let alert_id = alerts["items"][0]["id"].as_str().unwrap().to_string();

    let unread = get(&app, token, &format!("{ALERTS}?unread=true"))
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(unread["pagination"]["total"], 1, "a fresh alert is unread");

    let marked = patch(
        &app,
        token,
        &format!("{ALERTS}/{alert_id}"),
        json!({ "read": true }),
    )
    .await
    .expect_status(StatusCode::OK);
    assert!(
        marked["read_at"].is_string(),
        "the read mark is a timestamp: {marked}"
    );
    assert_eq!(
        marked["notification_status"], "pending",
        "untouched by this patch"
    );

    let unread = get(&app, token, &format!("{ALERTS}?unread=true"))
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(
        unread["pagination"]["total"], 0,
        "the read alert is out of the filter"
    );

    let all = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    assert_eq!(
        all["pagination"]["total"], 1,
        "the unread filter is optional"
    );

    // Un-reading is a real state change, not a second read mark.
    let unmarked = patch(
        &app,
        token,
        &format!("{ALERTS}/{alert_id}"),
        json!({ "read": false }),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(unmarked["read_at"], Value::Null);

    let delivered = patch(
        &app,
        token,
        &format!("{ALERTS}/{alert_id}"),
        json!({ "notification_status": "delivered" }),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_eq!(delivered["notification_status"], "delivered");
}

#[tokio::test]
async fn a_foreign_alert_is_a_404_on_read_and_on_the_mark() {
    let app = app(test_state().await);
    let owner = register_user(&app, "mon-alert-owner").await;
    let intruder = register_user(&app, "mon-alert-intruder").await;
    let token = owner.access_token.as_str();

    let (_, listing_id, _) = trigger_fixture(
        &app,
        token,
        json!({ "target_amount": "500.0000", "target_currency": "EUR" }),
    )
    .await;
    record_observation(
        &app,
        token,
        &listing_id,
        observation_body("400.0000", "foreign-alert-obs"),
    )
    .await;

    let alerts = get(&app, token, ALERTS).await.expect_status(StatusCode::OK);
    let alert_id = alerts["items"][0]["id"].as_str().unwrap();

    let read = get(
        &app,
        &intruder.access_token,
        &format!("{ALERTS}/{alert_id}"),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&read, "not_found");

    let marked = patch(
        &app,
        &intruder.access_token,
        &format!("{ALERTS}/{alert_id}"),
        json!({ "read": true }),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&marked, "not_found");

    // The mark did not leak: the owner still sees the alert unread.
    let unread = get(&app, token, &format!("{ALERTS}?unread=true"))
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(unread["pagination"]["total"], 1);
}

// ---------------------------------------------------------------------------
// Automation runs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_run_is_started_running_and_finished_once() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-run-lifecycle").await;
    let token = user.access_token.as_str();

    let started = start_run(&app, token).await;
    assert_eq!(started["outcome"], "running");
    assert_eq!(started["finished_at"], Value::Null);
    assert_eq!(started["observations"], 0);
    assert_eq!(started["alerts"], 0);
    assert_eq!(started["failures"], 0);
    assert_eq!(started["error_summary"], Value::Null);
    let run_id = started["id"].as_str().unwrap().to_string();

    let finished = patch(
        &app,
        token,
        &format!("{RUNS}/{run_id}"),
        json!({
            "outcome": "partial",
            "observations": 12,
            "alerts": 1,
            "failures": 2,
            "error_summary": "  two sources refused  ",
        }),
    )
    .await
    .expect_status(StatusCode::OK);

    assert_eq!(finished["outcome"], "partial");
    assert!(finished["finished_at"].is_string(), "the finish is stamped");
    assert_eq!(finished["observations"], 12);
    assert_eq!(finished["alerts"], 1);
    assert_eq!(finished["failures"], 2);
    assert_eq!(
        finished["error_summary"], "two sources refused",
        "the summary is stored trimmed"
    );

    // A second finish is refused: the guard is the statement's.
    let again = patch(
        &app,
        token,
        &format!("{RUNS}/{run_id}"),
        json!({ "outcome": "ok" }),
    )
    .await
    .expect_status(StatusCode::CONFLICT);
    assert_error(&again, "conflict");

    // The stored state is the evidence, read through the collection: a run
    // has no single-resource read, and the finish is what the patch returned.
    let runs = get(&app, token, RUNS).await.expect_status(StatusCode::OK);
    assert_eq!(runs["pagination"]["total"], 1);
    let stored = &runs["items"][0];
    assert_eq!(stored["id"], started["id"]);
    assert_eq!(stored["outcome"], "partial");
    assert_eq!(stored["error_summary"], "two sources refused");

    start_run(&app, token).await;
    let runs = get(&app, token, RUNS).await.expect_status(StatusCode::OK);
    assert_eq!(runs["pagination"]["total"], 2);

    // Two runs started within the same test can share a `started_at` at the
    // database's resolution, so the newest-first order is asserted on the
    // outcome set, not on a position that a tie could flip.
    let mut outcomes: Vec<&str> = runs["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["outcome"].as_str().unwrap())
        .collect();
    outcomes.sort_unstable();
    assert_eq!(outcomes, ["partial", "running"]);
}

#[tokio::test]
async fn a_run_finish_is_validated_and_the_summary_is_a_tri_state() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-run-validate").await;
    let token = user.access_token.as_str();

    let started = start_run(&app, token).await;
    let run_id = started["id"].as_str().unwrap().to_string();
    let path = format!("{RUNS}/{run_id}");

    let running = patch(&app, token, &path, json!({ "outcome": "running" }))
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&running, "validation_failed", "outcome");

    for count in ["observations", "failures"] {
        let response = patch(
            &app,
            token,
            &path,
            json!({ "outcome": "failed", (count): -1 }),
        )
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&response, "validation_failed", count);
    }

    let summary = patch(
        &app,
        token,
        &path,
        json!({ "outcome": "failed", "error_summary": "" }),
    )
    .await
    .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&summary, "validation_failed", "error_summary");

    // The refusal was real: the run is still running, with its default counts.
    let read = get(&app, token, RUNS).await.expect_status(StatusCode::OK);
    assert_eq!(read["items"][0]["outcome"], "running");

    // Set the summary while finishing one run; omitting it leaves the default
    // untouched on the next one. (A summary can only be written by the finish
    // itself — a finished run refuses a second patch with a `409` — so the
    // clear half of the tri-state is pinned by the service's unit tests.)
    patch(
        &app,
        token,
        &path,
        json!({ "outcome": "failed", "error_summary": "no source answered" }),
    )
    .await
    .expect_status(StatusCode::OK);

    let read = get(&app, token, RUNS).await.expect_status(StatusCode::OK);
    let finished = read["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["id"] == started["id"])
        .unwrap_or_else(|| panic!("the finished run must be in the owner's history: {read}"));
    assert_eq!(finished["error_summary"], "no source answered");

    let second = start_run(&app, token).await;
    let second_path = format!("{RUNS}/{}", second["id"].as_str().unwrap());
    patch(&app, token, &second_path, json!({ "outcome": "ok" }))
        .await
        .expect_status(StatusCode::OK);

    let second_read = get(&app, token, RUNS).await.expect_status(StatusCode::OK);
    let second_finished = second_read["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["id"] == second["id"])
        .unwrap_or_else(|| panic!("the second run must be in the owner's history: {second_read}"));
    assert_eq!(
        second_finished["error_summary"],
        Value::Null,
        "an omitted tri-state leaves the summary untouched"
    );
}

#[tokio::test]
async fn a_foreign_run_is_a_404() {
    let app = app(test_state().await);
    let owner = register_user(&app, "mon-run-owner").await;
    let intruder = register_user(&app, "mon-run-intruder").await;

    let started = start_run(&app, &owner.access_token).await;
    let run_id = started["id"].as_str().unwrap();

    let finished = patch(
        &app,
        &intruder.access_token,
        &format!("{RUNS}/{run_id}"),
        json!({ "outcome": "failed" }),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&finished, "not_found");

    // The run is untouched, so the refusal was the ownership predicate's.
    let read = get(&app, &owner.access_token, RUNS)
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(read["items"][0]["outcome"], "running");
}

// ---------------------------------------------------------------------------
// Products: pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_product_listing_is_paginated_and_filtered() {
    let app = app(test_state().await);
    let user = register_user(&app, "mon-products-page").await;
    let token = user.access_token.as_str();

    let first = create_product(&app, token, "Acer", "Aspire").await;
    create_product(&app, token, "Dell", "XPS").await;
    let lenovo = create_product(&app, token, "Lenovo", "ThinkPad").await;

    let page = get(&app, token, &format!("{PRODUCTS}?limit=2&offset=1"))
        .await
        .expect_status(StatusCode::OK);
    assert_pagination(&page, 3, 2, 1);
    assert_eq!(page["items"][0]["brand"], "Dell", "newest first");

    let filtered = get(
        &app,
        token,
        &format!("{PRODUCTS}?status=active&search=lenovo"),
    )
    .await
    .expect_status(StatusCode::OK);
    assert_pagination(&filtered, 1, 20, 0);
    assert_eq!(filtered["items"][0]["id"], lenovo["id"]);
    assert_ne!(filtered["items"][0]["id"], first["id"]);
}

// ---------------------------------------------------------------------------
// The credential
// ---------------------------------------------------------------------------

/// Every monitoring operation is private: an unauthenticated call is a `401`
/// with the standard envelope, before the body is read.
#[tokio::test]
async fn the_monitoring_endpoints_require_a_credential() {
    let app = app(test_state().await);

    let paths = [
        (Method::GET, PRODUCTS.to_string()),
        (Method::POST, PRODUCTS.to_string()),
        (Method::GET, format!("{PRODUCTS}/{}", Uuid::new_v4())),
        (Method::PATCH, format!("{PRODUCTS}/{}", Uuid::new_v4())),
        (Method::DELETE, format!("{PRODUCTS}/{}", Uuid::new_v4())),
        (
            Method::GET,
            format!("{PRODUCTS}/{}/listings", Uuid::new_v4()),
        ),
        (
            Method::POST,
            format!("{PRODUCTS}/{}/listings", Uuid::new_v4()),
        ),
        (
            Method::PATCH,
            format!("{PRODUCTS}/{}/listings/{}", Uuid::new_v4(), Uuid::new_v4()),
        ),
        (
            Method::DELETE,
            format!("{PRODUCTS}/{}/listings/{}", Uuid::new_v4(), Uuid::new_v4()),
        ),
        (
            Method::POST,
            format!("{OBSERVATIONS_OF}/{}/observations", Uuid::new_v4()),
        ),
        (
            Method::GET,
            format!("{OBSERVATIONS_OF}/{}/observations", Uuid::new_v4()),
        ),
        (
            Method::GET,
            format!("{OBSERVATIONS_OF}/{}/observations/latest", Uuid::new_v4()),
        ),
        (Method::GET, format!("{RULES_OF}/{}/rules", Uuid::new_v4())),
        (Method::POST, format!("{RULES_OF}/{}/rules", Uuid::new_v4())),
        (
            Method::PATCH,
            format!("{RULES_OF}/{}/rules/{}", Uuid::new_v4(), Uuid::new_v4()),
        ),
        (
            Method::DELETE,
            format!("{RULES_OF}/{}/rules/{}", Uuid::new_v4(), Uuid::new_v4()),
        ),
        (Method::POST, RUNS.to_string()),
        (Method::GET, RUNS.to_string()),
        (Method::PATCH, format!("{RUNS}/{}", Uuid::new_v4())),
        (Method::GET, ALERTS.to_string()),
        (Method::GET, format!("{ALERTS}/{}", Uuid::new_v4())),
        (Method::PATCH, format!("{ALERTS}/{}", Uuid::new_v4())),
    ];

    assert_eq!(paths.len(), 22, "every monitoring operation is swept");

    for (method, path) in paths {
        let response = send(&app, request(method.clone(), &path, None, None)).await;

        assert_eq!(
            response.status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} must answer 401 to an unauthenticated call: {}",
            response.body
        );
        assert_error(&response.body, "unauthorized");
    }
}

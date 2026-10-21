//! Shared helpers for the integration test suite.
//!
//! Every integration test runs against a real PostgreSQL database. The harness
//! applies the migrations, creates a unique account per test and calls the
//! router directly.
//!
//! The configuration starts from [`atom_api::Config::defaults`], which loads
//! the real `config/` files. The harness then weakens three settings so that
//! the suite stays fast and deterministic. Nothing else is changed, so a
//! broken configuration file fails the suite rather than a deployment.

#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

use atom_api::api;
use atom_api::config::Config;
use atom_api::state::AppState;

/// Environment variable holding the URL of the disposable test database.
pub const TEST_DATABASE_URL_VAR: &str = "TEST_DATABASE_URL";

/// The issuer and the audience the test tokens are signed with.
const TEST_ISSUER: &str = "atom-api-test";

/// A response captured from the API under test.
#[derive(Debug)]
pub struct TestResponse {
    pub status: StatusCode,
    pub body: Value,
}

impl TestResponse {
    /// Asserts the status code and returns the parsed body.
    pub fn expect_status(self, expected: StatusCode) -> Value {
        assert_eq!(
            self.status,
            expected,
            "unexpected status; body was {}",
            serde_json::to_string_pretty(&self.body).unwrap_or_default()
        );
        self.body
    }
}

/// The base configuration for the test suite.
///
/// The starting point is the real configuration, so the tests exercise the
/// real files and the real parser. Only the settings that would make the
/// suite slow or flaky are changed.
pub fn test_config() -> Config {
    let mut config = Config::defaults().expect("the base configuration must be valid");

    config.database.url = test_database_url();

    // A secret for the tests. It is not a placeholder: it is long enough to
    // satisfy the production rule as well.
    config.auth.jwt_secret = "test-signing-secret-that-is-long-enough-32b".to_string();
    config.auth.issuer = TEST_ISSUER.to_string();
    config.auth.audience = TEST_ISSUER.to_string();

    // Cheap Argon2 parameters, so that the suite is fast. The logic is
    // proven, never the cost.
    config.auth.argon2.memory_kib = 8_192;
    config.auth.argon2.iterations = 1;
    config.auth.argon2.parallelism = 1;

    // A raised rate limit, so that the suite is never throttled. The limiter
    // itself has its own test, with its own limit.
    config.api.rate_limit.max_requests = 10_000;

    config
}

/// The URL of the disposable test database.
///
/// # Panics
///
/// Panics when `TEST_DATABASE_URL` is unset, or when the database name does
/// not contain `test`: a production database must never be truncated by an
/// accidental `cargo test`.
pub fn test_database_url() -> String {
    let url = std::env::var(TEST_DATABASE_URL_VAR).unwrap_or_else(|_| {
        panic!(
            "{TEST_DATABASE_URL_VAR} must be set to a disposable test database, e.g. \
             postgres://atom:atom@127.0.0.1:5432/atom_test"
        )
    });

    assert!(
        database_name(&url).contains("test"),
        "refusing to run integration tests against '{url}': the database name must contain 'test'"
    );

    url
}

/// Extracts the database name from a PostgreSQL URL.
fn database_name(url: &str) -> String {
    let without_scheme = url.rsplit("://").next().unwrap_or(url);
    let path = without_scheme.split('/').nth(1).unwrap_or("");
    path.split('?').next().unwrap_or("").to_string()
}

/// Connects to the test database, applies the migrations and builds the state.
pub async fn state_with(config: Config) -> AppState {
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .acquire_timeout(config.database.acquire_timeout)
        .connect(&config.database.url)
        .await
        .expect("the test database must be reachable");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations must apply to the test database");

    AppState::new(config, pool).expect("the state must be built")
}

/// The default test state.
pub async fn test_state() -> AppState {
    state_with(test_config()).await
}

/// A test state with a specific authentication rate limit.
pub async fn test_state_with_rate_limit(max_requests: u32) -> AppState {
    let mut config = test_config();
    config.api.rate_limit.max_requests = max_requests;
    state_with(config).await
}

/// A test state with a specific request body limit.
pub async fn test_state_with_body_limit(max_body_bytes: usize) -> AppState {
    let mut config = test_config();
    config.app.limits.max_body_bytes = max_body_bytes;
    state_with(config).await
}

/// Builds the application router for the given state.
pub fn app(state: AppState) -> Router {
    api::router(state)
}

/// Sends a request to the router and parses the response body.
pub async fn send(app: &Router, request: Request<Body>) -> TestResponse {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("the router must answer");

    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the response body must be readable")
        .to_bytes();

    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()))
    };

    TestResponse { status, body }
}

/// Builds a JSON request with an optional bearer token.
pub fn request(
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).unwrap())
        }
        None => Body::empty(),
    };

    builder.body(body).expect("the request must be valid")
}

/// Generates a unique email address for a test user.
pub fn unique_email(prefix: &str) -> String {
    format!("{prefix}-{}@example.test", Uuid::new_v4())
}

/// Registers a user and returns the account with its tokens.
pub async fn register_user(app: &Router, prefix: &str) -> TestUser {
    let email = unique_email(prefix);
    let password = "Test-Password-123".to_string();

    let response = send(
        app,
        request(
            Method::POST,
            "/api/v1/auth/register",
            None,
            Some(json!({
                "email": email,
                "password": password,
                "display_name": "Test User",
            })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::CREATED);

    TestUser {
        id: Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap(),
        email,
        password,
        access_token: body["tokens"]["access_token"].as_str().unwrap().to_string(),
        refresh_token: body["tokens"]["refresh_token"]
            .as_str()
            .unwrap()
            .to_string(),
    }
}

/// Logs a user in and returns the token pair.
pub async fn login_user(app: &Router, user: &TestUser) -> String {
    let response = send(
        app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": user.email, "password": user.password })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::OK);
    body["tokens"]["access_token"].as_str().unwrap().to_string()
}

/// Creates a project owned by the given user and returns its id.
pub async fn create_project(app: &Router, user: &TestUser, name: &str) -> Uuid {
    let response = send(
        app,
        request(
            Method::POST,
            "/api/v1/projects",
            Some(&user.access_token),
            Some(json!({ "name": name, "description": "created by the test suite" })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::CREATED);
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

/// Creates a task inside a project and returns its id.
pub async fn create_task(app: &Router, user: &TestUser, project_id: Uuid, title: &str) -> Uuid {
    let response = send(
        app,
        request(
            Method::POST,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            Some(json!({ "title": title })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::CREATED);
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

/// Asserts the pagination block of a list response.
pub fn assert_pagination(body: &Value, total: i64, limit: i64, offset: i64) {
    assert_eq!(body["pagination"]["total"], total, "unexpected total");
    assert_eq!(body["pagination"]["limit"], limit, "unexpected limit");
    assert_eq!(body["pagination"]["offset"], offset, "unexpected offset");
    assert_eq!(
        body["pagination"]["has_more"],
        offset + limit < total,
        "unexpected has_more"
    );
}

/// The stored hash of a refresh token, as the database records it.
pub fn hash_of(refresh_token: &str) -> String {
    atom_api::auth::token::hash_refresh_token(refresh_token)
}

/// A registered user together with its tokens.
#[derive(Debug, Clone)]
pub struct TestUser {
    pub id: Uuid,
    pub email: String,
    pub password: String,
    pub access_token: String,
    pub refresh_token: String,
}

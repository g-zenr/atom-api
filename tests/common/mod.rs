//! Shared helpers for the integration test suite.
//!
//! Every integration test runs against a real PostgreSQL database. To make
//! that safe, the harness refuses to start unless the target database name
//! contains `test`; a production database can therefore never be truncated by
//! an accidental `cargo test` run.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

use atom_api::api;
use atom_api::config::{
    Argon2Params, AuthConfig, Config, DatabaseConfig, Environment, RateLimitConfig,
};
use atom_api::state::AppState;

/// Environment variable holding the URL of the disposable test database.
pub const TEST_DATABASE_URL_VAR: &str = "TEST_DATABASE_URL";

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

/// Builds a test configuration pointing at the disposable test database.
///
/// # Panics
///
/// Panics when `TEST_DATABASE_URL` is unset, does not point at a database
/// whose name contains `test`, or cannot be parsed.
pub fn test_config() -> Config {
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

    Config {
        environment: Environment::Development,
        bind_addr: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        cors_allowed_origins: Vec::new(),
        max_body_bytes: 1024 * 1024,
        request_timeout: Duration::from_secs(30),
        log_filter: "warn".to_string(),
        database: DatabaseConfig {
            url,
            max_connections: 5,
            acquire_timeout: Duration::from_secs(5),
        },
        auth: AuthConfig {
            jwt_secret: "test-signing-secret-that-is-long-enough-32b".to_string(),
            access_token_ttl: Duration::from_secs(900),
            refresh_token_ttl: Duration::from_secs(86_400),
            issuer: "atom-api-test".to_string(),
            audience: "atom-api-test".to_string(),
            argon2: Argon2Params {
                // Cheap parameters keep the suite fast. They are not used for
                // any security claim.
                memory_kib: 8_192,
                iterations: 1,
                parallelism: 1,
            },
        },
        rate_limit: RateLimitConfig {
            max_requests: 10_000,
            window: Duration::from_secs(60),
        },
    }
}

/// Extracts the database name from a PostgreSQL URL.
fn database_name(url: &str) -> String {
    let without_scheme = url.rsplit("://").next().unwrap_or(url);
    let path = without_scheme.split('/').nth(1).unwrap_or("");
    path.split('?').next().unwrap_or("").to_string()
}

/// Connects to the test database, applies migrations and returns the state.
pub async fn test_state() -> AppState {
    let config = test_config();

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

    AppState::new(config, pool)
}

/// Connects to the test database with a specific authentication rate limit.
///
/// Used to test the rate limiter without waiting for a real window to elapse.
pub async fn test_state_with_rate_limit(max_requests: u32) -> AppState {
    let mut config = test_config();
    config.rate_limit.max_requests = max_requests;

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

    AppState::new(config, pool)
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

/// A registered user together with their tokens.
#[derive(Debug, Clone)]
pub struct TestUser {
    pub id: Uuid,
    pub email: String,
    pub password: String,
    pub access_token: String,
    pub refresh_token: String,
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

/// Connects to the test database with a specific request body limit.
pub async fn test_state_with_body_limit(max_body_bytes: usize) -> AppState {
    let mut config = test_config();
    config.max_body_bytes = max_body_bytes;

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

    AppState::new(config, pool)
}

/// The stored hash of a refresh token, as the database records it.
pub fn hash_of(refresh_token: &str) -> String {
    atom_api::auth::token::hash_refresh_token(refresh_token)
}

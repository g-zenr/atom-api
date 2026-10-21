//! Integration tests for the authentication endpoints.

mod common;

use axum::http::{Method, StatusCode};
use common::{app, create_project, register_user, request, send, test_state, unique_email};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn registration_returns_a_token_pair_and_a_safe_user() {
    let state = test_state().await;
    let app = app(state);

    let email = unique_email("register");
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/register",
            None,
            Some(json!({
                "email": email,
                "password": "Strong-Password-42",
                "display_name": "Ada",
            })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::CREATED);

    assert_eq!(body["user"]["email"], email);
    assert_eq!(body["user"]["display_name"], "Ada");
    assert!(body["user"]["id"].is_string());
    assert!(body["user"].get("password_hash").is_none());
    assert!(body["tokens"]["access_token"].is_string());
    assert!(body["tokens"]["refresh_token"].is_string());
    assert_eq!(body["tokens"]["token_type"], "Bearer");
    assert!(body["tokens"]["expires_in"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn registration_normalises_the_email_address() {
    let state = test_state().await;
    let app = app(state);

    let email = unique_email("Normalise");
    let upper = email.to_uppercase();

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/register",
            None,
            Some(json!({
                "email": upper,
                "password": "Strong-Password-42",
                "display_name": "Ada",
            })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::CREATED);
    assert_eq!(body["user"]["email"], email.to_lowercase());
}

#[tokio::test]
async fn duplicate_registration_is_a_conflict() {
    let state = test_state().await;
    let app = app(state);

    let email = unique_email("duplicate");
    let payload = json!({
        "email": email,
        "password": "Strong-Password-42",
        "display_name": "Ada",
    });

    send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/register",
            None,
            Some(payload.clone()),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let response = send(
        &app,
        request(Method::POST, "/api/v1/auth/register", None, Some(payload)),
    )
    .await;

    let body = response.expect_status(StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
}

#[tokio::test]
async fn a_weak_password_is_rejected_with_field_details() {
    let state = test_state().await;
    let app = app(state);

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/register",
            None,
            Some(json!({
                "email": unique_email("weak"),
                "password": "short",
                "display_name": "Ada",
            })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert!(body["error"]["details"].is_array());
}

#[tokio::test]
async fn login_succeeds_with_the_right_password() {
    let state = test_state().await;
    let app = app(state);

    let user = register_user(&app, "login-ok").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": user.email, "password": user.password })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["user"]["id"], user.id.to_string());
    assert!(body["tokens"]["access_token"].is_string());
}

#[tokio::test]
async fn login_fails_with_a_generic_message() {
    let state = test_state().await;
    let app = app(state);

    let user = register_user(&app, "login-bad").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": user.email, "password": "Wrong-Password-1" })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
    assert!(
        !body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("password is wrong"),
        "the failure message must not reveal which credential was wrong"
    );
}

#[tokio::test]
async fn an_unknown_email_produces_the_same_failure_as_a_wrong_password() {
    let state = test_state().await;
    let app = app(state);

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({
                "email": unique_email("nobody"),
                "password": "Wrong-Password-1",
            })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn refresh_rotates_the_token_pair() {
    let state = test_state().await;
    let app = app(state);

    let user = register_user(&app, "refresh").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::OK);
    let new_refresh = body["tokens"]["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(new_refresh, user.refresh_token);
}

#[tokio::test]
async fn reusing_a_rotated_refresh_token_revokes_every_session() {
    let state = test_state().await;
    let app = app(state);

    let user = register_user(&app, "reuse").await;

    // Rotate once.
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    )
    .await;
    let rotated = response.expect_status(StatusCode::OK);
    let new_refresh = rotated["tokens"]["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Reuse the old token: this is treated as a compromise.
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    )
    .await;
    response.expect_status(StatusCode::UNAUTHORIZED);

    // Every session of the user must now be invalid.
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": new_refresh })),
        ),
    )
    .await;
    response.expect_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_revokes_the_session() {
    let state = test_state().await;
    let app = app(state);

    let user = register_user(&app, "logout").await;

    send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/logout",
            Some(&user.access_token),
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    )
    .await
    .expect_status(StatusCode::NO_CONTENT);

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    )
    .await;
    response.expect_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_routes_reject_missing_and_invalid_credentials() {
    let state = test_state().await;
    let app = app(state);

    send(&app, request(Method::GET, "/api/v1/auth/me", None, None))
        .await
        .expect_status(StatusCode::UNAUTHORIZED);

    send(
        &app,
        request(Method::GET, "/api/v1/auth/me", Some("not-a-token"), None),
    )
    .await
    .expect_status(StatusCode::UNAUTHORIZED);

    send(&app, request(Method::GET, "/api/v1/notes", None, None))
        .await
        .expect_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_current_user_endpoint_returns_the_caller() {
    let state = test_state().await;
    let app = app(state);

    let user = register_user(&app, "me").await;

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/auth/me",
            Some(&user.access_token),
            None,
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["id"], user.id.to_string());
    assert_eq!(body["email"], user.email);
    assert!(body.get("password_hash").is_none());
}

#[tokio::test]
async fn an_account_cannot_log_out_another_accounts_session() {
    let state = test_state().await;
    let app = app(state);

    let first = register_user(&app, "owner").await;
    let second = register_user(&app, "attacker").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/logout",
            Some(&second.access_token),
            Some(json!({ "refresh_token": first.refresh_token })),
        ),
    )
    .await;

    response.expect_status(StatusCode::FORBIDDEN);

    // The victim's session must still work.
    send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": first.refresh_token })),
        ),
    )
    .await
    .expect_status(StatusCode::OK);
}

#[tokio::test]
async fn the_authentication_endpoints_are_rate_limited() {
    // Two attempts are allowed; the third must be refused.
    let state = common::test_state_with_rate_limit(2).await;
    let app = app(state);

    async fn attempt(app: &axum::Router) -> common::TestResponse {
        send(
            app,
            request(
                Method::POST,
                "/api/v1/auth/login",
                None,
                Some(json!({ "email": "nobody@example.test", "password": "Wrong-Password-1" })),
            ),
        )
        .await
    }

    // The first two attempts are rejected for the credentials, not the limit.
    attempt(&app).await.expect_status(StatusCode::UNAUTHORIZED);
    attempt(&app).await.expect_status(StatusCode::UNAUTHORIZED);

    // The third exceeds the limit.
    let refused = attempt(&app).await;
    let body = refused.expect_status(StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"]["code"], "rate_limited");
}

#[tokio::test]
async fn a_disabled_account_cannot_authenticate() {
    let state = test_state().await;
    let app = app(state.clone());
    let user = register_user(&app, "disabled").await;

    sqlx::query("UPDATE users SET is_active = FALSE WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await
        .expect("the account must be deactivated");

    // login
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": user.email, "password": user.password })),
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");

    // the current-user endpoint
    send(
        &app,
        request(
            Method::GET,
            "/api/v1/auth/me",
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::FORBIDDEN);

    // refresh
    send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    )
    .await
    .expect_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_expired_refresh_token_is_rejected() {
    let state = test_state().await;
    let app = app(state.clone());
    let user = register_user(&app, "expired-refresh").await;

    sqlx::query(
        "UPDATE refresh_sessions SET expires_at = NOW() - INTERVAL '1 hour' WHERE token_hash = $1",
    )
    .bind(common::hash_of(&user.refresh_token))
    .execute(&state.db)
    .await
    .expect("the session must be expired");

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::UNAUTHORIZED);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("expired"),
        "the message must say the token expired: {}",
        body["error"]["message"]
    );
}

#[tokio::test]
async fn an_expired_access_token_is_rejected() {
    let state = test_state().await;
    let app = app(state.clone());
    let user = register_user(&app, "expired-access").await;

    // A token that was valid an hour ago, signed with the configured secret.
    let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
    let claims = atom_api::auth::token::AccessTokenClaims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        iss: "atom-api-test".to_string(),
        aud: "atom-api-test".to_string(),
        exp: hour_ago.timestamp(),
        iat: hour_ago.timestamp(),
        jti: "expired-access-token".to_string(),
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
    )
    .expect("the token must be signed");

    send(
        &app,
        request(Method::GET, "/api/v1/auth/me", Some(&token), None),
    )
    .await
    .expect_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoking_every_session_invalidates_the_refresh_tokens() {
    let state = test_state().await;
    let app = app(state.clone());
    let user = register_user(&app, "revoke-all").await;

    // A second, independent session: a fresh login, not a rotation.
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": user.email, "password": user.password })),
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    let second_refresh = body["tokens"]["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    let revoked = state
        .auth_service()
        .revoke_all_sessions(user.id)
        .await
        .expect("the sessions must be revoked");
    assert_eq!(revoked, 2, "both active sessions must be revoked");

    for token in [&user.refresh_token, &second_refresh] {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/auth/refresh",
                None,
                Some(json!({ "refresh_token": token })),
            ),
        )
        .await
        .expect_status(StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn a_concurrent_refresh_of_the_same_token_succeeds_only_once() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "refresh-race").await;

    // Two requests carrying the same refresh token, at the same time.
    let refresh_one = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    );
    let refresh_two = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": user.refresh_token })),
        ),
    );

    let (first, second) = tokio::join!(refresh_one, refresh_two);

    let statuses = [first.status, second.status];
    let successes = statuses
        .iter()
        .filter(|status| **status == StatusCode::OK)
        .count();
    let refusals = statuses
        .iter()
        .filter(|status| **status == StatusCode::UNAUTHORIZED)
        .count();

    assert_eq!(successes, 1, "exactly one rotation may win: {statuses:?}");
    assert_eq!(refusals, 1, "the loser must be refused: {statuses:?}");
}

#[tokio::test]
async fn logout_with_an_unknown_refresh_token_is_accepted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "logout-unknown").await;

    send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/logout",
            Some(&user.access_token),
            Some(json!({ "refresh_token": "a-token-that-never-existed" })),
        ),
    )
    .await
    .expect_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_malformed_json_body_is_a_bad_request() {
    let state = test_state().await;
    let app = app(state);

    let body = axum::body::Body::from("{ this is not json");
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/register")
        .header("content-type", "application/json")
        .body(body)
        .unwrap();

    let response = send(&app, request).await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn an_unknown_query_parameter_value_is_a_bad_request() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "query-enum").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks?status=not-a-status"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn an_oversized_body_is_refused() {
    let state = common::test_state_with_body_limit(64).await;
    let app = app(state);

    let long = "x".repeat(512);
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/register",
            None,
            Some(json!({ "email": "big@example.test", "password": "Strong-Password-42", "display_name": long })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "payload_too_large");
}

#[tokio::test]
async fn an_unknown_route_returns_the_error_envelope() {
    let state = test_state().await;
    let app = app(state);

    let response = send(
        &app,
        request(Method::GET, "/api/v1/there-is-no-such-route", None, None),
    )
    .await;

    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    assert!(body["error"]["message"].is_string());
}

#[tokio::test]
async fn a_generated_request_id_is_propagated_to_the_response() {
    let state = test_state().await;
    let app = app(state);

    // No `x-request-id` header is sent, so the server generates one.
    let raw = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/health/live")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        raw.headers().get("x-request-id").is_some(),
        "a generated correlation id must be echoed on the response"
    );
}

#[tokio::test]
async fn the_request_id_is_echoed_on_the_response() {
    let state = test_state().await;
    let app = app(state);

    let request = axum::http::Request::builder()
        .method(Method::GET)
        .uri("/health/live")
        .header("x-request-id", "test-correlation-id")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("test-correlation-id")
    );
}

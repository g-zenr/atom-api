//! HTTP-level tests for the scoped API-key endpoints (`C-K1`).
//!
//! `C-K1` publishes three operations under `/api/v1/api-keys` and adds a second
//! kind of bearer credential: a key a machine client can hold, which is
//! distinguishable from a user JWT by its `atom_` prefix. The ten tests below
//! are the ones the contract fixes, one per behaviour, and each asserts the
//! observable answer rather than the implementation behind it:
//!
//! * **The secret is evidence, not a field.** "Returned once" is proven by
//!   taking the one response that carries it, then serializing every later
//!   response this suite can reach and asserting the secret string is absent
//!   from it *and* that no string anywhere is shaped like one. Asserting that
//!   a listing has no `key` field would pass a response that leaked the secret
//!   under another name.
//! * **A credential is proven by what it can reach.** `a_key_authenticates_a_request`
//!   calls `GET /api/v1/auth/me` and asserts the same account comes back, then
//!   reads that account's notes through the key. A `200` alone would pass a
//!   middleware that authenticated everybody as somebody.
//! * **The throttle is proven by moving the clock, not by waiting.** Waiting an
//!   hour is not a test, so `last_used_is_recorded_once_an_hour` writes the
//!   stamp back two hours with SQL and observes the next use move it. The
//!   assertion is a comparison between two stored instants, never a measured
//!   duration.
//!
//! Two states cannot be produced through the API in this chain and are built
//! with SQL against the same database, exactly as `tests/claims_api.rs` does: an
//! inactive account (no endpoint deactivates one) and a stale `last_used_at` (no
//! endpoint writes it). The SQL only builds the state; every assertion is still
//! on the API's answer.
//!
//! The page type is read through [`keys_of`], which accepts the two spellings a
//! page can have, because the contract names `ApiKeyPage` without fixing whether
//! the keys sit under `items` or are the body. What the contract does fix - that
//! no secret is present and which keys the page carries - is asserted directly.

pub mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use chrono::{DateTime, Duration, Utc};
use common::{TestResponse, app, register_user, request, send, test_state};
use serde_json::{Value, json};
use uuid::Uuid;

/// The collection path of the feature.
const KEYS: &str = "/api/v1/api-keys";

/// The path whose answer names the authenticated account.
const ME: &str = "/api/v1/auth/me";

/// The prefix every key secret carries, so the middleware can tell it from a JWT.
const SECRET_PREFIX: &str = "atom_";

/// The number of hex characters a secret carries after the prefix.
const SECRET_HEX_LENGTH: usize = 64;

/// The number of hex characters the published `prefix` repeats.
const PREFIX_HEX_LENGTH: usize = 8;

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

/// Posts a create request and returns the raw response.
async fn post_key(app: &Router, token: &str, name: &str) -> TestResponse {
    send(
        app,
        request(
            Method::POST,
            KEYS,
            Some(token),
            Some(json!({ "name": name })),
        ),
    )
    .await
}

/// Creates a key through the API and returns the `201` body.
async fn create_key(app: &Router, token: &str, name: &str) -> Value {
    post_key(app, token, name)
        .await
        .expect_status(StatusCode::CREATED)
}

/// Lists the caller's keys and returns the raw response.
async fn list_keys(app: &Router, token: &str) -> TestResponse {
    send(app, request(Method::GET, KEYS, Some(token), None)).await
}

/// Deletes one key and returns the raw response.
async fn delete_key(app: &Router, token: &str, key_id: &str) -> TestResponse {
    send(
        app,
        request(
            Method::DELETE,
            &format!("{KEYS}/{key_id}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Calls the account endpoint with a credential.
///
/// The credential is passed as the bearer token unchanged, so the same helper
/// sends a JWT and an API key. Which kind it is, is the middleware's decision
/// and exactly what these tests observe.
async fn me(app: &Router, credential: &str) -> TestResponse {
    send(app, request(Method::GET, ME, Some(credential), None)).await
}

// ---------------------------------------------------------------------------
// Body helpers
// ---------------------------------------------------------------------------

/// Asserts the stable error code of the standard envelope.
fn assert_error(body: &Value, code: &str) {
    assert_eq!(
        body["error"]["code"], code,
        "unexpected error code; the envelope was {body}"
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "every error envelope carries a non-empty message: {body}"
    );
}

/// The public identifier of a created key.
fn id_of(key: &Value) -> String {
    key["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the response must carry an id: {key}"))
        .to_string()
}

/// The secret of a create response.
fn secret_of(created: &Value) -> String {
    created["key"]
        .as_str()
        .unwrap_or_else(|| panic!("the create response must carry the secret once: {created}"))
        .to_string()
}

/// Whether a string is shaped like a key secret: the prefix and 64 lower-case
/// hex characters.
fn is_secret_shaped(text: &str) -> bool {
    let Some(hex) = text.strip_prefix(SECRET_PREFIX) else {
        return false;
    };

    hex.len() == SECRET_HEX_LENGTH
        && hex
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, 'a'..='f'))
}

/// Asserts that no part of a response carries a secret.
///
/// The serialized form is the evidence, not a field name: a response that
/// published the secret under `token`, `value` or inside a message is the
/// contract broken just the same, and a field-name assertion would pass it.
fn assert_no_secret(body: &Value, secret: &str, what: &str) {
    assert_secret_absent(body, secret, what);

    fn visit(value: &Value, what: &str) {
        match value {
            Value::String(text) => assert!(
                !is_secret_shaped(text),
                "the {what} response carries a value shaped like a key secret: {text:?}"
            ),
            Value::Array(items) => {
                for item in items {
                    visit(item, what);
                }
            }
            Value::Object(fields) => {
                for item in fields.values() {
                    visit(item, what);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    visit(body, what);
}

/// Asserts that one specific secret is absent from a serialized response.
///
/// Separated from [`assert_no_secret`] for the responses that legitimately
/// carry a secret of their own - the create response of a second key - where
/// "nothing shaped like a secret may appear" would be the wrong assertion.
fn assert_secret_absent(body: &Value, secret: &str, what: &str) {
    let rendered = serde_json::to_string(body)
        .unwrap_or_else(|error| panic!("the {what} response must serialize: {error}"));

    assert!(
        !rendered.contains(secret),
        "the {what} response carries the key secret: {rendered}"
    );
}

/// The keys a listing carries.
///
/// The contract publishes the page type without fixing whether the keys sit
/// under an `items` field or are the body itself, so both published spellings
/// are read. An empty page, or one under a name neither spelling uses, is a
/// failure with the body in the message rather than a silently empty list.
fn keys_of(page: &Value) -> Vec<Value> {
    if let Some(items) = page.get("items").and_then(Value::as_array) {
        return items.clone();
    }

    page.as_array().cloned().unwrap_or_else(|| {
        panic!(
            "the key listing must carry this account's keys, under `items` or as the body: {page}"
        )
    })
}

/// One listed key, by id.
fn find_key(page: &Value, key_id: &str) -> Value {
    keys_of(page)
        .into_iter()
        .find(|entry| entry["id"].as_str() == Some(key_id))
        .unwrap_or_else(|| panic!("the listing must carry the key {key_id}: {page}"))
}

/// Parses an RFC 3339 timestamp from a response body.
fn instant(key: &Value, field: &str) -> DateTime<Utc> {
    let text = key[field].as_str().unwrap_or_else(|| {
        panic!(
            "{field} must be an RFC 3339 string, but it is {}",
            key[field]
        )
    });

    DateTime::parse_from_rfc3339(text)
        .unwrap_or_else(|error| panic!("{field} must be RFC 3339 ({text:?}): {error}"))
        .with_timezone(&Utc)
}

/// The identifier of a note, or any other created resource.
fn note_id(note: &Value) -> String {
    note["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the create response must carry an id: {note}"))
        .to_string()
}

// ---------------------------------------------------------------------------
// I1 - the secret is returned once
// ---------------------------------------------------------------------------

/// The secret appears in the create response and nowhere else.
///
/// The evidence is every later response this test can reach: the listing before
/// the key has been used, the listing after it has been used (which is the one
/// write the key itself can trigger), the create response of a second key, and
/// an error envelope. Each is serialized and searched for the secret string and
/// for anything shaped like one.
///
/// `C-K1` publishes no read-one-key operation, so there is no detail path to
/// call: the listing is the only published read of a key, and the absence of a
/// detail route is asserted by `tests/openapi_contract.rs`, which would fail if
/// one appeared without being added to the contract.
#[tokio::test]
async fn a_key_is_returned_once_and_never_again() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "key-once").await;
    let other = register_user(&app, "key-once-other").await;

    // The one response that carries the secret.
    let created = create_key(&app, &user.access_token, "n8n workflow").await;

    let key_id = id_of(&created);
    let secret = secret_of(&created);

    assert_eq!(created["name"], "n8n workflow");

    let prefix = created["prefix"]
        .as_str()
        .unwrap_or_else(|| panic!("the create response must carry the prefix: {created}"));
    assert!(
        secret.starts_with(prefix),
        "the published prefix must be the start of the secret: {prefix:?} against {secret:?}"
    );
    assert_eq!(
        prefix.len(),
        SECRET_PREFIX.len() + PREFIX_HEX_LENGTH,
        "the published prefix is the `atom_` marker and eight hex characters: {prefix:?}"
    );
    assert!(
        is_secret_shaped(&secret),
        "a key secret is `atom_` followed by 64 lower-case hex characters: {secret:?}"
    );
    assert!(
        created["created_at"].is_string(),
        "the create response must carry the creation instant: {created}"
    );

    // Neither the digest nor the owner is on the wire, in any response.
    for absent in ["key_hash", "hash", "user_id"] {
        assert!(
            !created
                .as_object()
                .unwrap_or_else(|| panic!("the create response must be an object: {created}"))
                .contains_key(absent),
            "the create response must not publish `{absent}`: {created}"
        );
    }

    // The listing, before the key has ever been used.
    let page = list_keys(&app, &user.access_token)
        .await
        .expect_status(StatusCode::OK);
    assert_no_secret(&page, &secret, "listing");

    let listed = find_key(&page, &key_id);
    assert_eq!(listed["prefix"], prefix);
    assert_eq!(listed["name"], "n8n workflow");
    assert!(
        listed["last_used_at"].is_null(),
        "a key that has never been used carries no stamp: {listed}"
    );
    assert!(
        listed["revoked_at"].is_null(),
        "a fresh key is not revoked: {listed}"
    );

    // The listing again, after a use - the state in which a careless
    // implementation might carry the credential it just read.
    me(&app, &secret).await.expect_status(StatusCode::OK);

    let page = list_keys(&app, &user.access_token)
        .await
        .expect_status(StatusCode::OK);
    assert_no_secret(&page, &secret, "listing after a use");
    assert!(
        find_key(&page, &key_id)["last_used_at"].is_string(),
        "using the key must leave a readable stamp: {page}"
    );

    // A second key's create response carries the second secret and not the
    // first: the assertion is the narrower one, because that response is
    // *supposed* to carry a secret - its own.
    let second = create_key(&app, &user.access_token, "second workflow").await;
    assert_ne!(secret_of(&second), secret);
    assert_secret_absent(&second, &secret, "create response of another key");

    // An error envelope carries no secret either. The identifier exists for
    // nobody, so this is the published `404`.
    let body = delete_key(&app, &user.access_token, &Uuid::new_v4().to_string())
        .await
        .expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");
    assert_no_secret(&body, &secret, "not_found envelope");

    // Another account's listing never carried it, in either state.
    let page = list_keys(&app, &other.access_token)
        .await
        .expect_status(StatusCode::OK);
    assert_no_secret(&page, &secret, "another account's listing");
    assert!(
        !keys_of(&page)
            .iter()
            .any(|entry| entry["id"].as_str() == Some(key_id.as_str())),
        "another account's listing must not carry this key"
    );
}

// ---------------------------------------------------------------------------
// I1/I6 - the key is a credential for its owner
// ---------------------------------------------------------------------------

/// The key authenticates a real request as the account that created it.
///
/// The evidence is the account the answer names, not the status code: a
/// middleware that authenticated every key as the same user, or as anonymous,
/// would still answer `200` here if only the status were asserted. The key also
/// reads that account's rows, which is what "scoped to its owner" means in
/// practice and what a workflow needs.
#[tokio::test]
async fn a_key_authenticates_a_request() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "key-auth").await;
    let other = register_user(&app, "key-auth-other").await;

    let secret = secret_of(&create_key(&app, &user.access_token, "machine client").await);

    // The account behind the credential.
    let profile = me(&app, &secret).await.expect_status(StatusCode::OK);
    assert_eq!(
        profile["id"],
        user.id.to_string(),
        "the key must authenticate as the account that created it: {profile}"
    );
    assert_eq!(profile["email"], user.email);
    assert_eq!(profile["display_name"], "Test User");
    assert_ne!(
        profile["id"],
        other.id.to_string(),
        "the key must not authenticate as another account"
    );

    // The owner's rows are reachable through the key.
    let note = send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&user.access_token),
            Some(json!({ "title": "written with a jwt" })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let page = send(
        &app,
        request(Method::GET, "/api/v1/notes", Some(&secret), None),
    )
    .await
    .expect_status(StatusCode::OK);

    assert_eq!(
        page["pagination"]["total"], 1,
        "the key must reach its owner's rows: {page}"
    );
    assert_eq!(page["items"][0]["id"], note_id(&note));

    // Twice, so the credential is not a one-shot.
    me(&app, &secret).await.expect_status(StatusCode::OK);
}

// ---------------------------------------------------------------------------
// I3 - a revoked key is refused
// ---------------------------------------------------------------------------

/// A key stops working the moment it is revoked, and its account does not.
#[tokio::test]
async fn a_revoked_key_is_unauthorized() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "key-revoked").await;

    let created = create_key(&app, &user.access_token, "to be revoked").await;
    let key_id = id_of(&created);
    let secret = secret_of(&created);

    // It works before the revocation, so the refusal below is the revocation
    // and not a credential that never worked.
    me(&app, &secret).await.expect_status(StatusCode::OK);

    delete_key(&app, &user.access_token, &key_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    let body = me(&app, &secret)
        .await
        .expect_status(StatusCode::UNAUTHORIZED);
    assert_error(&body, "unauthorized");

    // A second protected operation refuses it too, so the answer is the
    // credential and not one handler's decision.
    let body = send(
        &app,
        request(Method::GET, "/api/v1/notes", Some(&secret), None),
    )
    .await
    .expect_status(StatusCode::UNAUTHORIZED);
    assert_error(&body, "unauthorized");

    // Revoking one key does not disable the account: its JWT still works.
    me(&app, &user.access_token)
        .await
        .expect_status(StatusCode::OK);
}

// ---------------------------------------------------------------------------
// I4 (A-C-K1-1) - an inactive account's key is refused with 403
// ---------------------------------------------------------------------------

/// A key of a deactivated account is refused, and the refusal is not a
/// revocation.
///
/// `A-C-K1-1` fixes the answer as `403 forbidden`, the same answer the JWT path
/// already gives for the same account state, so one account state has one
/// answer whichever credential the caller holds. The test asserts that equality
/// rather than a message text: the catalogue may be reworded, but a client
/// **must not** have to special-case the credential kind to read a disabled
/// account.
///
/// There is no endpoint that deactivates an account, so the state is built with
/// SQL against the same database. The assertion is on the API's answer, and on
/// the stored row: a refusal that also revoked the key would be indistinguishable
/// from the status code alone, and would silently break the key when the account
/// is re-enabled.
#[tokio::test]
async fn a_key_of_an_inactive_account_is_forbidden() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "key-inactive").await;

    let created = create_key(&app, &user.access_token, "disabled owner").await;
    let key_id = id_of(&created);
    let secret = secret_of(&created);

    me(&app, &secret).await.expect_status(StatusCode::OK);

    sqlx::query("UPDATE users SET is_active = FALSE WHERE id = $1")
        .bind(user.id)
        .execute(&db)
        .await
        .expect("the account must be deactivated");

    let body = me(&app, &secret).await.expect_status(StatusCode::FORBIDDEN);
    assert_error(&body, "forbidden");

    // One account state, one answer: the JWT of the same inactive account is
    // refused with the same envelope (`A-C-K1-1`).
    let by_jwt = me(&app, &user.access_token)
        .await
        .expect_status(StatusCode::FORBIDDEN);
    assert_error(&by_jwt, "forbidden");
    assert_eq!(
        body["error"], by_jwt["error"],
        "the two credential kinds must answer the same way for one account state: the key was \
         refused with {body}, the JWT with {by_jwt}"
    );

    // Another protected operation answers the same way, so the refusal is the
    // credential and not one handler's decision.
    let body = send(
        &app,
        request(Method::GET, "/api/v1/notes", Some(&secret), None),
    )
    .await
    .expect_status(StatusCode::FORBIDDEN);
    assert_error(&body, "forbidden");

    // The key is still a key: refused, not revoked and not deleted.
    let revoked: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM api_keys WHERE id = $1")
            .bind(
                Uuid::parse_str(&key_id)
                    .unwrap_or_else(|error| panic!("`{key_id}` must be a UUID: {error}")),
            )
            .fetch_one(&db)
            .await
            .expect("the key row must be readable");

    assert!(
        revoked.is_none(),
        "a refused authentication must not revoke the key, but revoked_at is {revoked:?}"
    );
}

// ---------------------------------------------------------------------------
// I2 - a foreign key is not found
// ---------------------------------------------------------------------------

/// Another account's key id answers `404`, never `403`.
///
/// `C-K1` publishes no read-one-key operation, so the published id-taking
/// operation is the delete. A `403` would confirm that the row exists, which is
/// the information the ownership rule exists to withhold; the same `404` is
/// returned for an identifier that exists for nobody, so the two are
/// indistinguishable to the caller.
#[tokio::test]
async fn a_foreign_key_is_not_found() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "key-foreign-owner").await;
    let other = register_user(&app, "key-foreign-other").await;

    let created = create_key(&app, &owner.access_token, "owner's workflow").await;
    let key_id = id_of(&created);
    let secret = secret_of(&created);

    let body = delete_key(&app, &other.access_token, &key_id)
        .await
        .expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // An identifier nobody owns is the same answer.
    let body = delete_key(&app, &other.access_token, &Uuid::new_v4().to_string())
        .await
        .expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // A malformed identifier is a `400`, not a `404` and never a `500`: the
    // path segment is parsed before the row is looked for.
    let body = delete_key(&app, &other.access_token, "not-a-uuid")
        .await
        .expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");

    // The owner's key survived every attempt, and still authenticates.
    me(&app, &secret).await.expect_status(StatusCode::OK);
}

// ---------------------------------------------------------------------------
// I1 - the listing never carries a secret
// ---------------------------------------------------------------------------

/// The listing publishes metadata only, for its own account only.
///
/// Both halves matter: a listing that leaked a secret is the contract broken,
/// and a listing that returned another account's keys is the ownership rule
/// broken. The secret is searched for in the serialized response, so a leak
/// under a field name the contract does not publish is still caught.
#[tokio::test]
async fn the_listing_never_contains_a_secret() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "key-listing").await;
    let other = register_user(&app, "key-listing-other").await;

    let first = create_key(&app, &user.access_token, "first workflow").await;
    let second = create_key(&app, &user.access_token, "second workflow").await;
    let theirs = create_key(&app, &other.access_token, "somebody else").await;

    let page = list_keys(&app, &user.access_token)
        .await
        .expect_status(StatusCode::OK);

    // No secret anywhere in the response.
    for created in [&first, &second, &theirs] {
        assert_no_secret(&page, &secret_of(created), "listing");
    }

    // Both of this account's keys, and neither of the other account's.
    let listed: Vec<String> = keys_of(&page).iter().map(id_of).collect();
    assert!(
        listed.contains(&id_of(&first)) && listed.contains(&id_of(&second)),
        "the listing must carry this account's keys: {page}"
    );
    assert!(
        !listed.contains(&id_of(&theirs)),
        "the listing must not carry another account's key: {page}"
    );

    // Every entry carries the published metadata and no secret field. The
    // absence is checked on the object, so a `key: null` field is a failure
    // too: the read contract does not have the field at all.
    for entry in keys_of(&page) {
        let fields = entry
            .as_object()
            .unwrap_or_else(|| panic!("a listed key must be an object: {entry}"));

        assert!(
            !fields.contains_key("key"),
            "a listed key must not carry a `key` field: {entry}"
        );

        for published in [
            "id",
            "name",
            "prefix",
            "last_used_at",
            "created_at",
            "revoked_at",
        ] {
            assert!(
                fields.contains_key(published),
                "a listed key must carry `{published}`: {entry}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// I6 - a malformed or unknown key is a 401
// ---------------------------------------------------------------------------

/// Anything that is not a live key is `401`, never `500` and never `404`.
///
/// The set covers the shapes the middleware has to route: the marker alone, a
/// marker with something that is not hex, 68 hex characters with no marker at
/// all, a hex body of the wrong length, upper case where the contract publishes
/// lower case, and - the interesting one - a value with the **right prefix and
/// the wrong digest**, which passes every syntactic check and must still be
/// refused because no row matches it.
#[tokio::test]
async fn a_malformed_key_is_unauthorized() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "key-malformed").await;

    let created = create_key(&app, &user.access_token, "real key").await;
    let secret = secret_of(&created);

    let prefix = created["prefix"]
        .as_str()
        .unwrap_or_else(|| panic!("the create response must carry the prefix: {created}"))
        .to_string();

    let mut credentials = vec![
        // The marker on its own.
        SECRET_PREFIX.to_string(),
        // A marker followed by something that is not hex.
        "atom_zzzz".to_string(),
        // 68 hex characters, no marker: the secret without its prefix.
        "a".repeat(SECRET_HEX_LENGTH + 4),
        // One character short and one character long.
        format!("atom_{}", "a".repeat(SECRET_HEX_LENGTH - 1)),
        format!("atom_{}", "a".repeat(SECRET_HEX_LENGTH + 1)),
        // Upper case, which the contract does not publish.
        format!("atom_{}", "A".repeat(SECRET_HEX_LENGTH)),
        // The correct prefix - a real key's - with a digest no row holds.
        format!("{}{}", prefix, "0".repeat(SECRET_HEX_LENGTH)),
    ];

    // A value that is not a credential at all, in the same position.
    credentials.push("not-a-token".to_string());

    for credential in credentials {
        let body = me(&app, &credential)
            .await
            .expect_status(StatusCode::UNAUTHORIZED);
        assert_error(&body, "unauthorized");
    }

    // The real key still works, so every `401` above is the credential and not
    // this test's setup.
    me(&app, &secret).await.expect_status(StatusCode::OK);
}

// ---------------------------------------------------------------------------
// I2 - a key manages only its owner's keys
// ---------------------------------------------------------------------------

/// Another account can neither delete this account's key nor see it.
///
/// The delete is `404` rather than `403`, the listing of the other account does
/// not carry the key, and the owner's key is still live afterwards - so the
/// refusal is a refusal and not a failed write that half-ran.
#[tokio::test]
async fn a_key_cannot_manage_keys_of_another_account() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "key-manage-owner").await;
    let intruder = register_user(&app, "key-manage-intruder").await;

    let owned = create_key(&app, &owner.access_token, "owner's workflow").await;
    let owned_id = id_of(&owned);

    // DELETE
    let body = delete_key(&app, &intruder.access_token, &owned_id)
        .await
        .expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The other account's listing does not carry it.
    let page = list_keys(&app, &intruder.access_token)
        .await
        .expect_status(StatusCode::OK);
    assert!(
        !keys_of(&page)
            .iter()
            .any(|entry| entry["id"].as_str() == Some(owned_id.as_str())),
        "another account's key leaked into the listing: {page}"
    );

    // The reverse, in the other direction: a key the intruder creates does not
    // appear in the owner's listing.
    let theirs = create_key(&app, &intruder.access_token, "intruder's workflow").await;
    let theirs_id = id_of(&theirs);

    let page = list_keys(&app, &owner.access_token)
        .await
        .expect_status(StatusCode::OK);
    assert!(
        !keys_of(&page)
            .iter()
            .any(|entry| entry["id"].as_str() == Some(theirs_id.as_str())),
        "the owner's listing must not carry another account's key: {page}"
    );

    // The owner's key survived the delete attempt and still authenticates.
    me(&app, &secret_of(&owned))
        .await
        .expect_status(StatusCode::OK);
}

// ---------------------------------------------------------------------------
// I5 - the last-use stamp is throttled to once an hour
// ---------------------------------------------------------------------------

/// A use is recorded at most once an hour, and the throttle releases.
///
/// Three observations, in order: an unused key carries no stamp; the first use
/// records one; a second use inside the hour does **not** move it. The release
/// is proven by writing the stamp back two hours with SQL - waiting an hour is
/// not a test - and observing the next use move it forward by roughly the two
/// hours it was moved back. Every comparison is between two instants the
/// database wrote, so no measured duration and no clock skew between the
/// application and the test is involved.
#[tokio::test]
async fn last_used_is_recorded_once_an_hour() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "key-throttle").await;

    let created = create_key(&app, &user.access_token, "hot key").await;
    let key_id = id_of(&created);
    let secret = secret_of(&created);
    let row_id = Uuid::parse_str(&key_id)
        .unwrap_or_else(|error| panic!("`{key_id}` must be a UUID: {error}"));

    // Unused.
    let page = list_keys(&app, &user.access_token)
        .await
        .expect_status(StatusCode::OK);
    assert!(
        find_key(&page, &key_id)["last_used_at"].is_null(),
        "a key that has never been used carries no stamp: {page}"
    );

    // First use: recorded.
    me(&app, &secret).await.expect_status(StatusCode::OK);

    let page = list_keys(&app, &user.access_token)
        .await
        .expect_status(StatusCode::OK);
    let first = instant(&find_key(&page, &key_id), "last_used_at");

    // Second use, immediately: not recorded again.
    me(&app, &secret).await.expect_status(StatusCode::OK);

    let page = list_keys(&app, &user.access_token)
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(
        instant(&find_key(&page, &key_id), "last_used_at"),
        first,
        "a second use inside the hour must not move last_used_at, but it moved from {first}"
    );

    // Two hours back, so the throttle has expired. The stamp is written by SQL
    // because no endpoint writes it and waiting an hour is not a test.
    sqlx::query("UPDATE api_keys SET last_used_at = NOW() - INTERVAL '2 hours' WHERE id = $1")
        .bind(row_id)
        .execute(&db)
        .await
        .expect("the stale stamp must be written");

    let stale: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT last_used_at FROM api_keys WHERE id = $1")
            .bind(row_id)
            .fetch_one(&db)
            .await
            .expect("the stale stamp must be readable");
    let stale = stale.unwrap_or_else(|| panic!("the stale stamp must be set for {key_id}"));

    // The next use records a fresh instant.
    me(&app, &secret).await.expect_status(StatusCode::OK);

    let page = list_keys(&app, &user.access_token)
        .await
        .expect_status(StatusCode::OK);
    let fresh = instant(&find_key(&page, &key_id), "last_used_at");

    assert!(
        fresh > stale + Duration::minutes(90),
        "the first use after the hour must record a new instant: the stamp was {stale}, and it \
         reads {fresh}. A throttle longer than an hour, or none at all, produces this."
    );
    assert!(
        fresh >= first,
        "the stamp must only move forward: first {first}, now {fresh}"
    );
}

// ---------------------------------------------------------------------------
// I2 - the key acts as its owner, on the owner's rows only
// ---------------------------------------------------------------------------

/// A request authenticated by a key sees exactly that account's rows.
///
/// The key is not a superuser and not anonymous: it reaches its owner's notes,
/// cannot read another account's note by id (`404`, not `403`), and lists only
/// its owner's keys. The other account's key, used the same way, sees the
/// reverse set - so the isolation is a property of the credential and not of
/// the order the rows were written in.
#[tokio::test]
async fn a_key_is_scoped_to_its_owner() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "key-scope-owner").await;
    let intruder = register_user(&app, "key-scope-intruder").await;

    let owned_note = send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&owner.access_token),
            Some(json!({ "title": "owner's note" })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let other_note = send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&intruder.access_token),
            Some(json!({ "title": "intruder's note" })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let owner_key = secret_of(&create_key(&app, &owner.access_token, "owner's key").await);
    let intruder_key = secret_of(&create_key(&app, &intruder.access_token, "intruder's key").await);

    // The credential names its owner.
    let profile = me(&app, &owner_key).await.expect_status(StatusCode::OK);
    assert_eq!(profile["id"], owner.id.to_string());

    // The owner's collection, through the key, is the owner's.
    let page = send(
        &app,
        request(Method::GET, "/api/v1/notes", Some(&owner_key), None),
    )
    .await
    .expect_status(StatusCode::OK);

    assert_eq!(
        page["pagination"]["total"], 1,
        "the owner's key must see exactly the owner's rows: {page}"
    );
    assert_eq!(page["items"][0]["id"], note_id(&owned_note));

    // Another account's row is not addressable through this key, and the
    // refusal does not confirm that it exists.
    let body = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/notes/{}", note_id(&other_note)),
            Some(&owner_key),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The reverse direction, so the isolation is not an artefact of which
    // account was created first.
    let page = send(
        &app,
        request(Method::GET, "/api/v1/notes", Some(&intruder_key), None),
    )
    .await
    .expect_status(StatusCode::OK);

    assert_eq!(
        page["pagination"]["total"], 1,
        "the other account's key must see exactly its own rows: {page}"
    );
    assert_eq!(page["items"][0]["id"], note_id(&other_note));

    // A key manages the keys of its own owner and no others.
    let page = list_keys(&app, &owner_key)
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(
        keys_of(&page).len(),
        1,
        "a key authenticated request may only list its owner's single key: {page}"
    );
}

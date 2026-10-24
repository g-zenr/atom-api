//! HTTP-level tests for the reimbursement claim endpoints.
//!
//! `C-R2` publishes five operations under `/api/v1/claims`: list, create, read,
//! patch and delete. `C-R3`/`C-R4` publish seven more: the three item writes and
//! the four lifecycle actions. This file holds the 55 acceptance tests those
//! chains name, grouped the way the design document
//! (`docs/architecture/reimbursements-crud-design.md` §12) and
//! [`docs/features/reimbursements/api.md`] §7-§14 group them: CRUD, ownership,
//! money on the wire, `422` validation, `400`/`413` malformed, listing and
//! filters, line items and the total invariant, the lifecycle, and the
//! credential.
//!
//! Three rules shape the file:
//!
//! * **Behaviour, not implementation.** Every assertion is on the status code
//!   and the observable body, never on a log line, a timing value or a random
//!   value. The one structural assertion - the item order - is on the order of
//!   the response array, which is the published contract.
//! * **Independence.** The suite shares one real, disposable PostgreSQL
//!   database. Every test registers its own accounts, and every count is
//!   restricted to the ids that test created, so no test can see another's
//!   rows and no test truncates a table.
//! * **The mutating call is never the evidence.** Where an invariant is about
//!   stored state - the total, the lifecycle stamps - the assertion is on a
//!   later read of the resource, not on the response that wrote it.
//!
//! Where a row cannot be produced through the API in this chain - a fixed
//! `created_at` - the fixture is written as SQL against the same database,
//! exactly as `tests/claims_schema.rs` does. The assertion is still on the API
//! response: the SQL only builds the state, it never stands in for the
//! behaviour under test.

pub mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use common::{
    TestResponse, app, assert_pagination, create_project, register_user, request, send, test_state,
    test_state_with_body_limit, test_state_with_pagination,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// The collection path of the feature.
const CLAIMS: &str = "/api/v1/claims";

/// The currency most fixtures use.
const EUR: &str = "EUR";

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

/// The body of a minimal create request.
fn claim_body(title: &str, description: &str, currency: &str) -> Value {
    json!({ "title": title, "description": description, "currency": currency })
}

/// Posts a create request and returns the raw response.
async fn post_claim(app: &Router, token: &str, body: Value) -> TestResponse {
    send(app, request(Method::POST, CLAIMS, Some(token), Some(body))).await
}

/// Creates a claim through the API and returns the `201` body.
async fn create_claim(app: &Router, token: &str, title: &str) -> Value {
    let response = post_claim(app, token, claim_body(title, "", EUR)).await;
    response.expect_status(StatusCode::CREATED)
}

/// Creates a claim with a description and returns the `201` body.
async fn create_claim_with_description(
    app: &Router,
    token: &str,
    title: &str,
    description: &str,
) -> Value {
    let response = post_claim(app, token, claim_body(title, description, EUR)).await;
    response.expect_status(StatusCode::CREATED)
}

/// Reads one claim and returns the raw response.
async fn get_claim(app: &Router, token: &str, claim_id: &str) -> TestResponse {
    send(
        app,
        request(
            Method::GET,
            &format!("{CLAIMS}/{claim_id}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Patches one claim and returns the raw response.
async fn patch_claim(app: &Router, token: &str, claim_id: &str, body: Value) -> TestResponse {
    send(
        app,
        request(
            Method::PATCH,
            &format!("{CLAIMS}/{claim_id}"),
            Some(token),
            Some(body),
        ),
    )
    .await
}

/// Deletes one claim and returns the raw response.
async fn delete_claim(app: &Router, token: &str, claim_id: &str) -> TestResponse {
    send(
        app,
        request(
            Method::DELETE,
            &format!("{CLAIMS}/{claim_id}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Lists claims with the given query string (including its leading `?`).
async fn list_claims(app: &Router, token: &str, query: &str) -> TestResponse {
    send(
        app,
        request(Method::GET, &format!("{CLAIMS}{query}"), Some(token), None),
    )
    .await
}

/// Sends a request whose body is deliberately not built by a serializer.
async fn send_raw(
    app: &Router,
    method: Method,
    uri: &str,
    token: &str,
    body: &'static str,
) -> TestResponse {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("the raw request must be valid");

    send(app, request).await
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
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "every error envelope carries a non-empty message: {body}"
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
        "the violation must name `{field}`; the envelope was {body}"
    );
}

/// Asserts that a value is a money **string** at scale four, and returns it.
///
/// The type check is the point: a JSON number that happened to render
/// `149.8` is a contract violation even if it is numerically equal.
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
        "{what} must carry exactly four decimal places: {text:?}"
    );
    assert!(
        fraction.chars().all(|c| c.is_ascii_digit()),
        "{what} must carry only digits after the point: {text:?}"
    );

    text.to_string()
}

/// Asserts that both money fields of a claim response are money strings.
fn assert_claim_money(claim: &Value) {
    assert_money(&claim["total_amount"], "total_amount");

    if let Some(items) = claim["items"].as_array() {
        for (index, item) in items.iter().enumerate() {
            assert_money(&item["amount"], &format!("items[{index}].amount"));
        }
    }
}

/// The public identifier of a created claim.
fn id_of(claim: &Value) -> String {
    claim["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the response must carry an id: {claim}"))
        .to_string()
}

/// The titles of a list response, in the order the server returned them.
fn titles(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the listing must be an array envelope: {body}"))
        .iter()
        .map(|item| item["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The identifiers of a list response, in the order the server returned them.
fn ids(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the listing must be an array envelope: {body}"))
        .iter()
        .map(|item| item["id"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Parses an RFC 3339 timestamp from a response body.
fn instant(claim: &Value, field: &str) -> DateTime<Utc> {
    let text = claim[field].as_str().unwrap_or_else(|| {
        panic!(
            "{field} must be an RFC 3339 string, but it is {}",
            claim[field]
        )
    });

    DateTime::parse_from_rfc3339(text)
        .unwrap_or_else(|error| panic!("{field} must be RFC 3339 ({text:?}): {error}"))
        .with_timezone(&Utc)
}

// ---------------------------------------------------------------------------
// SQL fixtures
//
// The chain cannot produce every state through the API: `R2` writes no status
// other than `draft` and no item at all. A claim in `submitted`, an item, or a
// fixed `created_at` is therefore inserted directly, owned by the account the
// test just registered. Every fixture is scoped to that account, and the
// assertions stay on the API's response.
// ---------------------------------------------------------------------------

/// One `expense_claims` row as a fixture writes it.
struct ClaimRow<'a> {
    id: Option<Uuid>,
    title: &'a str,
    description: &'a str,
    currency: &'a str,
    total_amount: &'a str,
    status: &'a str,
    project_id: Option<Uuid>,
    submitted_at: Option<DateTime<Utc>>,
    decided_at: Option<DateTime<Utc>>,
    paid_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
}

impl Default for ClaimRow<'_> {
    fn default() -> Self {
        Self {
            id: None,
            title: "a fixture claim",
            description: "",
            currency: EUR,
            total_amount: "0",
            status: "draft",
            project_id: None,
            submitted_at: None,
            decided_at: None,
            paid_at: None,
            created_at: None,
        }
    }
}

/// One `expense_items` row as a fixture writes it.
struct ItemRow<'a> {
    description: &'a str,
    amount: &'a str,
    incurred_on: NaiveDate,
    created_at: Option<DateTime<Utc>>,
}

impl Default for ItemRow<'_> {
    fn default() -> Self {
        Self {
            description: "a fixture line item",
            amount: "1.0000",
            incurred_on: date(2026, 3, 1),
            created_at: None,
        }
    }
}

/// A calendar date, for the fixtures.
fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day)
        .unwrap_or_else(|| panic!("{year}-{month:02}-{day:02} must be a real date"))
}

/// An instant, for the fixtures.
fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap_or_else(|| panic!("{year}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"))
}

/// Inserts a claim owned by `owner` and returns its id.
async fn insert_claim(db: &PgPool, owner: Uuid, claim: &ClaimRow<'_>) -> Uuid {
    let id = claim.id.unwrap_or_else(Uuid::new_v4);

    sqlx::query(
        "INSERT INTO expense_claims
            (id, user_id, title, description, currency, total_amount, status,
             project_id, submitted_at, decided_at, paid_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5::char(3), $6::text::numeric,
                 $7::text::expense_claim_status, $8, $9, $10, $11,
                 COALESCE($12, NOW()), COALESCE($12, NOW()))",
    )
    .bind(id)
    .bind(owner)
    .bind(claim.title)
    .bind(claim.description)
    .bind(claim.currency)
    .bind(claim.total_amount)
    .bind(claim.status)
    .bind(claim.project_id)
    .bind(claim.submitted_at)
    .bind(claim.decided_at)
    .bind(claim.paid_at)
    .bind(claim.created_at)
    .execute(db)
    .await
    .unwrap_or_else(|error| panic!("the fixture claim must be inserted: {error}"));

    id
}

/// Inserts an item into `claim_id`, owned by `owner`, and returns its id.
async fn insert_item(db: &PgPool, owner: Uuid, claim_id: Uuid, item: &ItemRow<'_>) -> Uuid {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO expense_items
            (id, user_id, claim_id, description, amount, incurred_on, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5::text::numeric, $6, COALESCE($7, NOW()),
                 COALESCE($7, NOW()))",
    )
    .bind(id)
    .bind(owner)
    .bind(claim_id)
    .bind(item.description)
    .bind(item.amount)
    .bind(item.incurred_on)
    .bind(item.created_at)
    .execute(db)
    .await
    .unwrap_or_else(|error| panic!("the fixture item must be inserted: {error}"));

    id
}

// ---------------------------------------------------------------------------
// 12.1 - a claim can be created, read, changed and deleted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_claim_can_be_created_read_and_deleted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-crud").await;

    // create
    let response = post_claim(
        &app,
        &user.access_token,
        json!({
            "title": "Client visit to Berlin",
            "description": "Two days on site",
            "currency": EUR,
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);

    let claim_id = id_of(&created);
    assert_eq!(created["title"], "Client visit to Berlin");
    assert_eq!(created["description"], "Two days on site");
    assert_eq!(created["currency"], EUR);
    assert_eq!(
        created["status"], "draft",
        "a claim is created in the draft status"
    );
    assert_eq!(
        assert_money(&created["total_amount"], "total_amount"),
        "0.0000",
        "a new claim has no items, so its total is zero at scale four"
    );
    assert_eq!(
        created["items"].as_array().map(Vec::len),
        Some(0),
        "a new claim carries an empty item list"
    );
    assert!(created["submitted_at"].is_null());
    assert!(created["decided_at"].is_null());
    assert!(created["paid_at"].is_null());
    assert!(created["created_at"].is_string());
    assert!(created["updated_at"].is_string());

    // The owner is on the row and not on the wire, and the detail is flat.
    assert!(
        created.get("user_id").is_none(),
        "user_id must not be published: {created}"
    );
    assert!(
        created.get("claim").is_none(),
        "the detail response is flat, not wrapped: {created}"
    );

    // read
    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let fetched = response.expect_status(StatusCode::OK);
    assert_eq!(fetched["id"], claim_id);
    assert_eq!(fetched["description"], "Two days on site");
    assert_eq!(
        assert_money(&fetched["total_amount"], "total_amount"),
        "0.0000"
    );

    // the listing carries the same claim, without items
    let response = list_claims(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![claim_id.clone()]);
    assert_pagination(&page, 1, 20, 0);
    assert!(
        page["items"][0].get("items").is_none(),
        "the listing must not carry the detail's item list: {page}"
    );

    // delete
    delete_claim(&app, &user.access_token, &claim_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = list_claims(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

#[tokio::test]
async fn a_claim_can_be_created_with_a_project_of_the_caller() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-with-project").await;

    let project_id = create_project(&app, &user, "Berlin trip").await;

    let response = post_claim(
        &app,
        &user.access_token,
        json!({
            "title": "Client visit to Berlin",
            "description": "Two days on site",
            "currency": EUR,
            "project_id": project_id,
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);

    assert_eq!(
        created["project_id"],
        project_id.to_string(),
        "the project link must be echoed back"
    );

    let response = get_claim(&app, &user.access_token, &id_of(&created)).await;
    let fetched = response.expect_status(StatusCode::OK);
    assert_eq!(fetched["project_id"], project_id.to_string());
    assert_eq!(fetched["status"], "draft");
}

#[tokio::test]
async fn a_claim_can_be_partially_updated() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-patch").await;

    let project_id = create_project(&app, &user, "Berlin trip").await;

    let response = post_claim(
        &app,
        &user.access_token,
        json!({
            "title": "Client visit to Berlin",
            "description": "Two days on site",
            "currency": EUR,
            "project_id": project_id,
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let claim_id = id_of(&created);

    // An omitted field is untouched; `project_id: null` clears the link.
    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "title": "Client visit to Berlin (Oct)", "project_id": null }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);

    assert_eq!(updated["title"], "Client visit to Berlin (Oct)");
    assert_eq!(
        updated["description"], "Two days on site",
        "an omitted field must not be overwritten"
    );
    assert_eq!(updated["currency"], EUR);
    assert_eq!(updated["status"], "draft");
    assert!(
        updated["project_id"].is_null(),
        "`project_id: null` must clear the link, not leave it: {updated}"
    );

    // An empty description is the one empty string that means something.
    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "description": "" }),
    )
    .await;
    let cleared = response.expect_status(StatusCode::OK);

    assert_eq!(cleared["description"], "");
    assert_eq!(
        cleared["title"], "Client visit to Berlin (Oct)",
        "clearing the description must not touch the title"
    );

    // A UUID sets the link again.
    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "project_id": project_id }),
    )
    .await;
    let relinked = response.expect_status(StatusCode::OK);
    assert_eq!(relinked["project_id"], project_id.to_string());

    // An explicit null for a non-nullable field is treated as omitted, per the
    // published type (`string`, not `string or null`).
    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "title": null }),
    )
    .await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(
        unchanged["title"], "Client visit to Berlin (Oct)",
        "an explicit null title must not clear the title"
    );

    // The persisted state agrees with every response.
    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["title"], "Client visit to Berlin (Oct)");
    assert_eq!(stored["description"], "");
    assert_eq!(stored["project_id"], project_id.to_string());
}

#[tokio::test]
async fn a_patch_body_with_no_fields_changes_only_the_modification_time() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "claim-empty-patch").await;

    // A fixed creation instant, so "updated_at moved" is an instant comparison
    // and not a comparison against the clock resolution.
    let created_at = at(2020, 1, 1, 0, 0, 0);
    let claim_id = insert_claim(
        &db,
        user.id,
        &ClaimRow {
            title: "nothing may change",
            description: "kept verbatim",
            created_at: Some(created_at),
            ..Default::default()
        },
    )
    .await;

    let before = get_claim(&app, &user.access_token, &claim_id.to_string())
        .await
        .expect_status(StatusCode::OK);

    assert_eq!(instant(&before, "created_at"), created_at);
    assert_eq!(instant(&before, "updated_at"), created_at);

    let response = patch_claim(&app, &user.access_token, &claim_id.to_string(), json!({})).await;
    let after = response.expect_status(StatusCode::OK);

    for field in [
        "id",
        "title",
        "description",
        "currency",
        "total_amount",
        "status",
        "project_id",
        "submitted_at",
        "decided_at",
        "paid_at",
        "created_at",
    ] {
        assert_eq!(
            after[field], before[field],
            "an empty PATCH body must not change `{field}`"
        );
    }

    assert_eq!(
        instant(&after, "created_at"),
        created_at,
        "the creation instant must never be rewritten"
    );
    assert!(
        instant(&after, "updated_at") > created_at,
        "an empty PATCH body must still stamp the modification time, but it stayed at {}",
        after["updated_at"]
    );

    // Nothing changed for the next reader either.
    let stored = get_claim(&app, &user.access_token, &claim_id.to_string())
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(stored["title"], "nothing may change");
    assert_eq!(stored["description"], "kept verbatim");
}

#[tokio::test]
async fn the_claim_list_returns_the_callers_claims_newest_first() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "claim-order").await;

    // Two claims share a creation instant, so the identifier is the only
    // tie-breaker; the other two are one and two seconds away. The pair of
    // identifiers is derived from a fresh UUID, because the database is
    // shared and a fixed id would collide on a second run. Clearing the low
    // bit leaves room for `+ 1`, so the two ids are ordered by byte value.
    let base = at(2026, 5, 1, 0, 0, 0);
    let tie = Uuid::new_v4().as_u128() & !1;
    let tie_low = Uuid::from_u128(tie);
    let tie_high = Uuid::from_u128(tie + 1);

    insert_claim(
        &db,
        user.id,
        &ClaimRow {
            title: "oldest",
            created_at: Some(base),
            ..Default::default()
        },
    )
    .await;
    insert_claim(
        &db,
        user.id,
        &ClaimRow {
            id: Some(tie_low),
            title: "tie-low",
            created_at: Some(base + Duration::seconds(10)),
            ..Default::default()
        },
    )
    .await;
    insert_claim(
        &db,
        user.id,
        &ClaimRow {
            id: Some(tie_high),
            title: "tie-high",
            created_at: Some(base + Duration::seconds(10)),
            ..Default::default()
        },
    )
    .await;
    insert_claim(
        &db,
        user.id,
        &ClaimRow {
            title: "newest",
            created_at: Some(base + Duration::seconds(20)),
            ..Default::default()
        },
    )
    .await;

    let response = list_claims(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);

    assert_eq!(
        titles(&page),
        vec!["newest", "tie-high", "tie-low", "oldest"],
        "the listing must be ordered by created_at DESC, then id DESC"
    );
    assert_pagination(&page, 4, 20, 0);
}

// ---------------------------------------------------------------------------
// 12.2 - ownership: the second account gets 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_claim_of_another_user_is_invisible() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "claim-owner").await;
    let other = register_user(&app, "claim-other").await;

    let project_id = create_project(&app, &owner, "Owner's project").await;

    let response = post_claim(
        &app,
        &owner.access_token,
        json!({
            "title": "Private claim",
            "description": "belongs to one account",
            "currency": EUR,
            "project_id": project_id,
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let claim_id = id_of(&created);

    // READ
    let response = get_claim(&app, &other.access_token, &claim_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // UPDATE
    let response = patch_claim(
        &app,
        &other.access_token,
        &claim_id,
        json!({ "title": "hijacked" }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // DELETE
    let response = delete_claim(&app, &other.access_token, &claim_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // LIST: the claim is not in the other account's collection at all.
    let response = list_claims(&app, &other.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
    assert!(!ids(&page).contains(&claim_id));

    // THE PROJECT LINK: the other account cannot attach the owner's project to
    // a claim of its own, and the refusal is 404 rather than 403.
    let response = post_claim(
        &app,
        &other.access_token,
        json!({ "title": "Other's claim", "currency": EUR }),
    )
    .await;
    let other_claim = response.expect_status(StatusCode::CREATED);
    let other_claim_id = id_of(&other_claim);

    let response = patch_claim(
        &app,
        &other.access_token,
        &other_claim_id,
        json!({ "project_id": project_id }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = get_claim(&app, &other.access_token, &other_claim_id).await;
    let untouched = response.expect_status(StatusCode::OK);
    assert!(
        untouched["project_id"].is_null(),
        "a refused link must not have been written: {untouched}"
    );

    // The owner still sees the claim, unchanged.
    let response = get_claim(&app, &owner.access_token, &claim_id).await;
    let kept = response.expect_status(StatusCode::OK);
    assert_eq!(kept["title"], "Private claim");
    assert_eq!(kept["project_id"], project_id.to_string());
}

#[tokio::test]
async fn the_listing_only_contains_the_callers_claims() {
    let state = test_state().await;
    let app = app(state);
    let first = register_user(&app, "claim-list-a").await;
    let second = register_user(&app, "claim-list-b").await;

    let mut first_ids = Vec::new();
    for title in ["first alpha", "first beta"] {
        first_ids.push(id_of(&create_claim(&app, &first.access_token, title).await));
    }
    let second_id = id_of(&create_claim(&app, &second.access_token, "second gamma").await);

    let response = list_claims(&app, &first.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);

    assert_pagination(&page, 2, 20, 0);
    for id in ids(&page) {
        assert!(
            first_ids.contains(&id),
            "another account's claim leaked into the listing: {id}"
        );
    }
    assert!(!ids(&page).contains(&second_id));

    let response = list_claims(&app, &second.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![second_id]);
    assert_pagination(&page, 1, 20, 0);
}

#[tokio::test]
async fn a_claim_cannot_reference_another_users_project() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "claim-project-owner").await;
    let other = register_user(&app, "claim-project-other").await;

    let owner_project = create_project(&app, &owner, "Owner's project").await;

    // The other account's own claim starts without a link.
    let response = post_claim(
        &app,
        &other.access_token,
        json!({ "title": "Other's claim", "currency": EUR }),
    )
    .await;
    let other_claim = response.expect_status(StatusCode::CREATED);
    assert!(other_claim["project_id"].is_null());
    let other_claim_id = id_of(&other_claim);

    // CREATE with a foreign project: 404, not 403 and not 422.
    let response = post_claim(
        &app,
        &other.access_token,
        json!({
            "title": "Intruder",
            "currency": EUR,
            "project_id": owner_project,
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // PATCH with a foreign project: also 404.
    let response = patch_claim(
        &app,
        &other.access_token,
        &other_claim_id,
        json!({ "project_id": owner_project }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // A project that does not exist at all is the same answer.
    let missing = Uuid::new_v4();
    let response = post_claim(
        &app,
        &other.access_token,
        json!({ "title": "Missing project", "currency": EUR, "project_id": missing }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = patch_claim(
        &app,
        &other.access_token,
        &other_claim_id,
        json!({ "project_id": missing }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The other claim is unchanged: neither refused write landed.
    let response = get_claim(&app, &other.access_token, &other_claim_id).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert!(unchanged["project_id"].is_null());

    // The owner may link the owner's own project, so the refusals above are
    // the ownership rule and not "a claim never carries a project".
    let response = post_claim(
        &app,
        &owner.access_token,
        json!({
            "title": "Owner's claim",
            "currency": EUR,
            "project_id": owner_project,
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["project_id"], owner_project.to_string());
}

#[tokio::test]
async fn deleting_the_project_clears_the_link_and_keeps_the_claim() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-project-delete").await;

    let project_id = create_project(&app, &user, "Doomed project").await;

    let response = post_claim(
        &app,
        &user.access_token,
        json!({
            "title": "Client visit to Berlin",
            "description": "Two days on site",
            "currency": EUR,
            "project_id": project_id,
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let claim_id = id_of(&created);

    send(
        &app,
        request(
            Method::DELETE,
            &format!("/api/v1/projects/{project_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NO_CONTENT);

    // The claim survives the project, with its link cleared.
    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let fetched = response.expect_status(StatusCode::OK);
    assert!(
        fetched["project_id"].is_null(),
        "deleting the project must clear the link, not the claim: {fetched}"
    );
    assert_eq!(fetched["title"], "Client visit to Berlin");
    assert_eq!(fetched["description"], "Two days on site");

    let response = list_claims(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![claim_id]);
}

// ---------------------------------------------------------------------------
// 12.3 - money on the wire
// ---------------------------------------------------------------------------

#[tokio::test]
async fn money_round_trips_with_four_decimals() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "claim-money").await;

    // The create half: the server writes the total, and it renders at scale 4.
    let created = create_claim(&app, &user.access_token, "Empty claim").await;
    assert_eq!(
        assert_money(&created["total_amount"], "total_amount"),
        "0.0000"
    );

    // The read half: a fixture total of 149.8 is published unchanged, at the
    // scale the column renders rather than the scale it was written with.
    let claim_id = insert_claim(
        &db,
        user.id,
        &ClaimRow {
            title: "Berlin trip",
            total_amount: "149.8",
            ..Default::default()
        },
    )
    .await;

    for (amount, day) in [("125.8", 1), ("24.0", 2)] {
        insert_item(
            &db,
            user.id,
            claim_id,
            &ItemRow {
                amount,
                incurred_on: date(2026, 3, day),
                ..Default::default()
            },
        )
        .await;
    }

    let response = get_claim(&app, &user.access_token, &claim_id.to_string()).await;
    let detail = response.expect_status(StatusCode::OK);

    assert_eq!(
        assert_money(&detail["total_amount"], "total_amount"),
        "149.8000",
        "the stored total must be published at scale four"
    );

    let amounts: Vec<String> = detail["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| assert_money(&item["amount"], "items[].amount"))
        .collect();
    assert_eq!(
        amounts,
        vec!["125.8000", "24.0000"],
        "every item amount must be published at scale four"
    );
}

#[tokio::test]
async fn a_money_field_is_never_a_json_number_in_a_response() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "claim-money-type").await;

    // create
    let created = create_claim(&app, &user.access_token, "Typed claim").await;
    assert_claim_money(&created);

    // read, with items
    let claim_id = insert_claim(
        &db,
        user.id,
        &ClaimRow {
            title: "Typed claim with items",
            total_amount: "3.7500",
            ..Default::default()
        },
    )
    .await;
    for (amount, day) in [("3.7500", 1), ("0.0000", 2)] {
        insert_item(
            &db,
            user.id,
            claim_id,
            &ItemRow {
                amount,
                incurred_on: date(2026, 3, day),
                ..Default::default()
            },
        )
        .await;
    }

    let response = get_claim(&app, &user.access_token, &claim_id.to_string()).await;
    let detail = response.expect_status(StatusCode::OK);
    assert_claim_money(&detail);
    assert_eq!(
        detail["items"].as_array().map(Vec::len),
        Some(2),
        "the detail must carry both items"
    );

    // list
    let response = list_claims(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert!(
        !page["items"].as_array().unwrap().is_empty(),
        "the listing must not be empty for this account"
    );
    for claim in page["items"].as_array().unwrap() {
        assert_money(&claim["total_amount"], "items[].total_amount");
    }

    // The four decimal places of every claim total on the page.
    for claim in page["items"].as_array().unwrap() {
        let text = claim["total_amount"].as_str().unwrap();
        assert!(
            text.split_once('.')
                .is_some_and(|(_, fraction)| fraction.len() == 4),
            "a listed total must render four decimals: {text:?}"
        );
    }
}

#[tokio::test]
async fn the_total_is_not_accepted_from_the_client() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-total-owned").await;

    // A create body that carries the server-owned total.
    let response = post_claim(
        &app,
        &user.access_token,
        json!({
            "title": "Client visit to Berlin",
            "description": "Two days on site",
            "currency": EUR,
            "total_amount": "999.0000",
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let claim_id = id_of(&created);
    assert_eq!(
        assert_money(&created["total_amount"], "total_amount"),
        "0.0000",
        "a client-supplied total must not be stored"
    );

    // A JSON number in the same position is ignored as well; it must not turn
    // into a total, and it must not turn into a 422 either: the field is not
    // part of the request contract.
    let response = post_claim(
        &app,
        &user.access_token,
        json!({ "title": "Numeric total", "currency": EUR, "total_amount": 12.5 }),
    )
    .await;
    let numeric = response.expect_status(StatusCode::CREATED);
    assert_eq!(
        assert_money(&numeric["total_amount"], "total_amount"),
        "0.0000"
    );

    // The same on PATCH: accepted, ignored, unchanged.
    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "total_amount": "5.0000" }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert_eq!(
        assert_money(&updated["total_amount"], "total_amount"),
        "0.0000",
        "a PATCH must not move the total"
    );

    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(
        assert_money(&stored["total_amount"], "total_amount"),
        "0.0000"
    );
}

// ---------------------------------------------------------------------------
// 12.4 - 422 validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_claim_title_must_not_be_blank() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-title-blank").await;

    for blank in ["", "   "] {
        let response = post_claim(
            &app,
            &user.access_token,
            json!({ "title": blank, "currency": EUR }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "title");
        assert_eq!(body["error"]["details"][0]["message"], "must not be empty");
    }

    // A missing required field is a validation failure too, not a 400: the
    // body is well-formed JSON that does not match the contract.
    let response = send(
        &app,
        request(
            Method::POST,
            CLAIMS,
            Some(&user.access_token),
            Some(json!({ "currency": EUR })),
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    // The same rule on PATCH: a claim always keeps a title.
    let created = create_claim(&app, &user.access_token, "Titled claim").await;
    let claim_id = id_of(&created);

    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "title": "   " }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "title");

    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(
        unchanged["title"], "Titled claim",
        "a refused PATCH must not have changed the title"
    );

    // The design validates the body before any statement runs, so a request
    // that is both malformed and aimed at somebody else's claim is a 422 and
    // not a 404: the body never had to be read.
    let other = register_user(&app, "claim-title-blank-other").await;
    let response = patch_claim(
        &app,
        &other.access_token,
        &claim_id,
        json!({ "title": "   " }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "title");
}

#[tokio::test]
async fn a_claim_title_is_bounded() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-title-bound").await;

    // 200 characters are accepted, at the bound.
    let at_bound = "x".repeat(200);
    let created = create_claim(&app, &user.access_token, &at_bound).await;
    assert_eq!(created["title"], at_bound);

    // 201 are refused, and the field is named.
    let response = post_claim(
        &app,
        &user.access_token,
        json!({ "title": "x".repeat(201), "currency": EUR }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "title");
    assert_eq!(
        body["error"]["details"][0]["message"],
        "must be at most 200 characters long"
    );

    // The bound counts characters, not bytes: 200 two-byte characters are
    // 400 bytes and must still be accepted, because the database constraint
    // (`CHAR_LENGTH`) counts characters too.
    let wide = "é".repeat(200);
    let created = create_claim(&app, &user.access_token, &wide).await;
    assert_eq!(
        created["title"]
            .as_str()
            .map(str::chars)
            .map(Iterator::count),
        Some(200)
    );

    let response = post_claim(
        &app,
        &user.access_token,
        json!({ "title": "é".repeat(201), "currency": EUR }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "title");
}

#[tokio::test]
async fn a_claim_title_is_trimmed_before_it_is_measured() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-title-trim").await;

    // 200 characters surrounded by spaces: 204 characters on the wire, 200
    // after trimming, and therefore accepted.
    let trimmed = "x".repeat(200);
    let padded = format!("   {trimmed}   ");

    let response = post_claim(
        &app,
        &user.access_token,
        json!({ "title": padded, "currency": EUR }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(
        created["title"], trimmed,
        "the stored title must be the trimmed value"
    );

    // One character more after trimming is refused, even though the wire form
    // is only 3 characters longer than the bound.
    let response = post_claim(
        &app,
        &user.access_token,
        json!({ "title": format!("   {}   ", "x".repeat(201)), "currency": EUR }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "title");
}

#[tokio::test]
async fn a_claim_description_is_bounded() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-description").await;

    // Omitted: the published default is the empty string.
    let created = create_claim(&app, &user.access_token, "No description").await;
    assert_eq!(created["description"], "");

    // Kept verbatim: a description is not a title, so it is not trimmed.
    let created =
        create_claim_with_description(&app, &user.access_token, "Spaced", "  spaced  ").await;
    assert_eq!(created["description"], "  spaced  ");

    // 5000 characters are accepted and stored unchanged.
    let at_bound = "d".repeat(5_000);
    let created = create_claim_with_description(&app, &user.access_token, "Long", &at_bound).await;
    assert_eq!(
        created["description"].as_str().map(str::len),
        Some(5_000),
        "a description at the bound must be stored in full"
    );

    // 5001 are refused, and the field is named.
    let response = post_claim(
        &app,
        &user.access_token,
        json!({
            "title": "Too long",
            "description": "d".repeat(5_001),
            "currency": EUR,
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "description");
    assert_eq!(
        body["error"]["details"][0]["message"],
        "must be at most 5000 characters long"
    );
}

#[tokio::test]
async fn the_claim_currency_is_required_and_upper_cased() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-currency").await;

    // Missing: a 422, not a 500 from the database.
    let response = send(
        &app,
        request(
            Method::POST,
            CLAIMS,
            Some(&user.access_token),
            Some(json!({ "title": "No currency" })),
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    // Every shape that is not three letters is refused, with the field named.
    // `EURO` is four characters: `CHAR(3)` would answer with a type error and a
    // 500 if the service let it reach the statement.
    for currency in ["EU", "EURO", "eur1", "", "E1R", "EU R"] {
        let response = post_claim(
            &app,
            &user.access_token,
            json!({ "title": "Bad currency", "currency": currency }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "currency");
    }

    // Lower case is accepted and stored upper case.
    let response = post_claim(
        &app,
        &user.access_token,
        json!({ "title": "Lower case currency", "currency": "eur" }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["currency"], EUR);
    let claim_id = id_of(&created);

    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(
        stored["currency"], EUR,
        "the stored currency must be the upper-cased value"
    );
}

#[tokio::test]
async fn the_claim_currency_is_not_patchable() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-currency-patch").await;

    let created = create_claim(&app, &user.access_token, "Euro claim").await;
    let claim_id = id_of(&created);

    // The published contract (api.md §6) wins over the "422" wording of the
    // schema document: `currency` is not in the request contract, so a body
    // that carries it is accepted and ignored. The write path is what makes it
    // immutable - the column is in no SET clause.
    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "currency": "USD" }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert_eq!(
        updated["currency"], EUR,
        "a PATCH carrying a currency must leave the stored currency alone"
    );

    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "currency": "usd", "description": "still euros" }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert_eq!(updated["currency"], EUR);
    assert_eq!(updated["description"], "still euros");

    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["currency"], EUR);
}

#[tokio::test]
async fn the_claim_status_is_not_patchable() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-status-patch").await;

    let created = create_claim(&app, &user.access_token, "Draft claim").await;
    let claim_id = id_of(&created);
    assert_eq!(created["status"], "draft");

    // A body that carries the status *and* the lifecycle stamps must change
    // none of them: the only writer of these fields is the lifecycle chain.
    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({
            "status": "paid",
            "submitted_at": "2026-03-01T09:00:00Z",
            "decided_at": "2026-03-02T09:00:00Z",
            "paid_at": "2026-03-03T09:00:00Z",
        }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);

    assert_eq!(
        updated["status"], "draft",
        "a PATCH must not move the claim through its lifecycle"
    );
    assert!(updated["submitted_at"].is_null());
    assert!(updated["decided_at"].is_null());
    assert!(updated["paid_at"].is_null());

    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["status"], "draft");
    assert!(stored["submitted_at"].is_null());
}

#[tokio::test]
async fn a_project_id_that_is_not_a_uuid_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-project-type").await;

    // A well-formed body whose field has the wrong type is a 422, and the
    // violation names the body: the extractor sees the whole body, not the
    // field.
    let response = post_claim(
        &app,
        &user.access_token,
        json!({ "title": "Bad project", "currency": EUR, "project_id": "not-a-uuid" }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "body");

    let created = create_claim(&app, &user.access_token, "Titled claim").await;
    let claim_id = id_of(&created);

    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "project_id": "not-a-uuid" }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "body");

    // A numeric project id is the same failure, not a 400.
    let response = patch_claim(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "project_id": 7 }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "body");
}

#[tokio::test]
async fn a_reversed_date_range_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-date-range").await;

    let response = list_claims(
        &app,
        &user.access_token,
        "?submitted_from=2026-10-31&submitted_to=2026-10-01",
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "submitted_from");

    // Equal bounds are a single day, not a reversed range.
    let response = list_claims(
        &app,
        &user.access_token,
        "?submitted_from=2026-10-01&submitted_to=2026-10-01",
    )
    .await;
    response.expect_status(StatusCode::OK);

    // A forward range is accepted.
    let response = list_claims(
        &app,
        &user.access_token,
        "?submitted_from=2026-10-01&submitted_to=2026-10-31",
    )
    .await;
    response.expect_status(StatusCode::OK);
}

// ---------------------------------------------------------------------------
// 12.5 - 400 malformed and 413 over-size
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_claim_id_must_be_a_uuid() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-bad-id").await;

    let uri = format!("{CLAIMS}/not-a-uuid");

    let response = send(
        &app,
        request(Method::GET, &uri, Some(&user.access_token), None),
    )
    .await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");

    let response = send(
        &app,
        request(
            Method::PATCH,
            &uri,
            Some(&user.access_token),
            Some(json!({ "title": "valid body" })),
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");

    let response = send(
        &app,
        request(Method::DELETE, &uri, Some(&user.access_token), None),
    )
    .await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");
}

#[tokio::test]
async fn a_malformed_body_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-bad-body").await;

    // A body that is not JSON at all is a malformed request: 400.
    let response = send_raw(&app, Method::POST, CLAIMS, &user.access_token, "{not json").await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");

    // A well-formed body of the wrong type is a business-rule failure: 422.
    // The distinction is the one the error contract draws, and it is asserted
    // rather than assumed.
    let response = send_raw(&app, Method::POST, CLAIMS, &user.access_token, "[]").await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "body");

    let created = create_claim(&app, &user.access_token, "Titled claim").await;
    let claim_id = id_of(&created);

    let response = send_raw(
        &app,
        Method::PATCH,
        &format!("{CLAIMS}/{claim_id}"),
        &user.access_token,
        "{oops",
    )
    .await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");
}

#[tokio::test]
async fn an_unknown_status_filter_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-bad-status").await;

    // Outside the enum, so the query string never became a `ClaimStatus`. The
    // answer is 400, not 422: this is a parse failure, not a business rule.
    for value in ["archived", "Submitted", "PAID", ""] {
        let response = list_claims(&app, &user.access_token, &format!("?status={value}")).await;
        let body = response.expect_status(StatusCode::BAD_REQUEST);
        assert_error(&body, "bad_request");
    }

    // Every one of the five contract values is accepted, so the refusals above
    // are not "the filter never works".
    for value in ["draft", "submitted", "approved", "rejected", "paid"] {
        let response = list_claims(&app, &user.access_token, &format!("?status={value}")).await;
        response.expect_status(StatusCode::OK);
    }
}

#[tokio::test]
async fn a_malformed_claim_list_parameter_is_a_bad_request() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-bad-query").await;

    for query in [
        "?limit=abc",
        "?offset=1.5",
        "?limit=",
        "?submitted_from=not-a-date",
        "?submitted_from=2026-02-30",
        "?submitted_from=",
        "?submitted_to=31-10-2026",
    ] {
        let response = list_claims(&app, &user.access_token, query).await;
        let body = response.expect_status(StatusCode::BAD_REQUEST);
        assert_error(&body, "bad_request");
    }

    // The same parameters with a valid value are accepted.
    let response = list_claims(
        &app,
        &user.access_token,
        "?limit=12&offset=3&submitted_from=2026-10-01&submitted_to=2026-10-31",
    )
    .await;
    response.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn a_claims_body_over_the_limit_is_refused() {
    // A body limit far below a real claim, so the refusal is about size and
    // not about validation.
    let state = test_state_with_body_limit(256).await;
    let app = app(state);
    let user = register_user(&app, "claim-body-limit").await;

    let response = post_claim(
        &app,
        &user.access_token,
        claim_body(&"t".repeat(400), "description", EUR),
    )
    .await;
    let body = response.expect_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert_error(&body, "payload_too_large");
}

// ---------------------------------------------------------------------------
// 12.6 - listing, filters, pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_claims_listing_filters_by_status_and_currency() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "claim-filter").await;

    let submitted = at(2026, 6, 1, 9, 0, 0);
    let decided = at(2026, 6, 2, 9, 0, 0);

    for (title, currency, status) in [
        ("draft eur", "EUR", "draft"),
        ("draft usd", "USD", "draft"),
        ("submitted eur", "EUR", "submitted"),
        ("submitted usd", "USD", "submitted"),
        ("approved eur", "EUR", "approved"),
        ("approved usd", "USD", "approved"),
    ] {
        let (submitted_at, decided_at) = match status {
            "draft" => (None, None),
            "submitted" => (Some(submitted), None),
            _ => (Some(submitted), Some(decided)),
        };

        insert_claim(
            &db,
            user.id,
            &ClaimRow {
                title,
                currency,
                status,
                submitted_at,
                decided_at,
                ..Default::default()
            },
        )
        .await;
    }

    let cases: [(&str, &[&str]); 5] = [
        ("?status=draft", &["draft eur", "draft usd"]),
        ("?status=submitted", &["submitted eur", "submitted usd"]),
        ("?status=approved", &["approved eur", "approved usd"]),
        (
            "?currency=usd",
            &["draft usd", "submitted usd", "approved usd"],
        ),
        ("?status=approved&currency=EUR", &["approved eur"]),
    ];

    for (query, expected) in cases {
        let response = list_claims(&app, &user.access_token, query).await;
        let page = response.expect_status(StatusCode::OK);

        let mut returned = titles(&page);
        returned.sort();
        let mut expected = expected
            .iter()
            .map(|title| (*title).to_string())
            .collect::<Vec<_>>();
        expected.sort();

        assert_eq!(returned, expected, "unexpected titles for '{query}'");
        assert_eq!(
            page["pagination"]["total"],
            expected.len(),
            "the total must count the filtered set for '{query}'"
        );
        assert_eq!(
            page["items"].as_array().map(Vec::len),
            Some(expected.len()),
            "the page and the count must describe the same set for '{query}'"
        );
    }

    // A status nobody has is an empty page and not a 404.
    let response = list_claims(&app, &user.access_token, "?status=paid").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

#[tokio::test]
async fn the_claims_listing_filters_by_submission_dates() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "claim-dates").await;

    // The instants sit on the edges of their UTC days, which is what makes an
    // inclusive bound observable: 23:59:59 on the 10th and 00:00:00 on the
    // 20th both have to be inside a range that names those days.
    for (title, submitted_at) in [
        ("tenth", at(2026, 3, 10, 23, 59, 59)),
        ("twentieth", at(2026, 3, 20, 0, 0, 0)),
        ("thirty-first", at(2026, 3, 31, 12, 0, 0)),
    ] {
        insert_claim(
            &db,
            user.id,
            &ClaimRow {
                title,
                status: "submitted",
                submitted_at: Some(submitted_at),
                ..Default::default()
            },
        )
        .await;
    }

    // A draft has no submission instant, so any bound excludes it.
    create_claim(&app, &user.access_token, "draft").await;

    let cases: [(&str, &[&str]); 5] = [
        ("", &["tenth", "twentieth", "thirty-first", "draft"]),
        (
            "?submitted_from=2026-03-10",
            &["tenth", "twentieth", "thirty-first"],
        ),
        ("?submitted_to=2026-03-19", &["tenth"]),
        (
            "?submitted_from=2026-03-20&submitted_to=2026-03-20",
            &["twentieth"],
        ),
        (
            "?submitted_from=2026-03-31&submitted_to=2026-03-31",
            &["thirty-first"],
        ),
    ];

    for (query, expected) in cases {
        let response = list_claims(&app, &user.access_token, query).await;
        let page = response.expect_status(StatusCode::OK);

        let mut returned = titles(&page);
        returned.sort();
        let mut expected = expected
            .iter()
            .map(|title| (*title).to_string())
            .collect::<Vec<_>>();
        expected.sort();

        assert_eq!(returned, expected, "unexpected titles for '{query}'");
        assert_eq!(
            page["pagination"]["total"],
            expected.len(),
            "the total must count the filtered set for '{query}'"
        );
    }
}

#[tokio::test]
async fn the_claim_search_matches_title_and_description() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-search").await;

    create_claim_with_description(
        &app,
        &user.access_token,
        "Client visit to Berlin",
        "Two days on site",
    )
    .await;
    create_claim_with_description(
        &app,
        &user.access_token,
        "Team meeting",
        "The Berlin office",
    )
    .await;
    create_claim_with_description(&app, &user.access_token, "Groceries", "nothing relevant").await;

    // A term in the title matches, in any case.
    for term in ["berlin", "BERLIN", "Berlin"] {
        let response = list_claims(&app, &user.access_token, &format!("?search={term}")).await;
        let page = response.expect_status(StatusCode::OK);
        assert_eq!(
            page["pagination"]["total"], 2,
            "the search must match both titles and descriptions for '{term}'"
        );
    }

    // A term in the description only.
    let response = list_claims(&app, &user.access_token, "?search=site").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(titles(&page), vec!["Client visit to Berlin"]);

    // A term in a title only.
    let response = list_claims(&app, &user.access_token, "?search=meeting").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(titles(&page), vec!["Team meeting"]);

    // No match is an empty page, not a 404.
    let response = list_claims(&app, &user.access_token, "?search=nothing-matches-this").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);

    // An empty term means "no filter", the only parameter with that reading.
    let response = list_claims(&app, &user.access_token, "?search=").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 3, 20, 0);
}

#[tokio::test]
async fn the_claims_listing_honours_the_pagination_bounds() {
    // Bounds that are neither the shipped defaults nor any plausible
    // compiled-in constant, so a fallback to a literal cannot pass.
    let state = test_state_with_pagination(7, 13).await;
    let bounds = state.config.api.pagination;
    let app = app(state);
    let user = register_user(&app, "claim-bounds").await;

    for index in 0..15 {
        create_claim(&app, &user.access_token, &format!("claim {index}")).await;
    }

    // A limit above the maximum is answered with the maximum.
    let response = list_claims(&app, &user.access_token, "?limit=100000").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(page["pagination"]["limit"], bounds.max_limit);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(13));
    assert_eq!(page["pagination"]["total"], 15);
    assert_eq!(page["pagination"]["has_more"], true);

    // No limit falls back to the configured default.
    let response = list_claims(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(page["pagination"]["limit"], bounds.default_limit);
    assert_eq!(page["pagination"]["offset"], 0);

    // A non-positive limit does not bypass the default.
    let response = list_claims(&app, &user.access_token, "?limit=0").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(page["pagination"]["limit"], bounds.default_limit);

    // A negative offset is treated as zero.
    let response = list_claims(&app, &user.access_token, "?offset=-5").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(page["pagination"]["offset"], 0);

    // The second page continues where the first stopped; both are full pages
    // of a set larger than either.
    let response = list_claims(&app, &user.access_token, "?limit=5&offset=5").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(5));
    assert_pagination(&page, 15, 5, 5);
    assert_eq!(page["pagination"]["has_more"], true);

    // The last page ends exactly on the total.
    let response = list_claims(&app, &user.access_token, "?limit=5&offset=10").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(5));
    assert_pagination(&page, 15, 5, 10);
    assert_eq!(page["pagination"]["has_more"], false);

    // An offset past the end is an empty page, not a 404.
    let response = list_claims(&app, &user.access_token, "?limit=5&offset=100").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 15, 5, 100);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn a_currency_filter_that_is_not_three_letters_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-currency-filter").await;

    // An empty value is a 422 and not "no filter": `search` is the only
    // parameter documented to read an empty value that way.
    for value in ["EU", "EURO", "eur1", "E1R", ""] {
        let response = list_claims(&app, &user.access_token, &format!("?currency={value}")).await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "currency");
    }

    // A valid value, lower case and surrounded by spaces, is trimmed and
    // upper-cased rather than refused.
    let created = create_claim(&app, &user.access_token, "Euro claim").await;
    assert_eq!(created["currency"], EUR);

    let response = list_claims(&app, &user.access_token, "?currency=%20eur%20").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 1, 20, 0);
    assert_eq!(page["items"][0]["currency"], EUR);
}

#[tokio::test]
async fn a_search_term_over_the_bound_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-search-bound").await;

    // One character more than the bound of 200.
    let term = "x".repeat(201);
    let response = list_claims(&app, &user.access_token, &format!("?search={term}")).await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "search");
    assert_eq!(
        body["error"]["details"][0]["message"],
        "must be at most 200 characters long"
    );

    // The term at the bound is accepted.
    let term = "x".repeat(200);
    let response = list_claims(&app, &user.access_token, &format!("?search={term}")).await;
    response.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn the_claim_detail_items_are_in_chronological_order() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "claim-item-order").await;

    let claim_id = insert_claim(
        &db,
        user.id,
        &ClaimRow {
            title: "Three items",
            total_amount: "6.0000",
            ..Default::default()
        },
    )
    .await;

    // Inserted out of order on purpose: the response order must come from the
    // published (incurred_on, created_at, id) rule, not from insertion order.
    let late = insert_item(
        &db,
        user.id,
        claim_id,
        &ItemRow {
            description: "latest day",
            amount: "3.0000",
            incurred_on: date(2026, 3, 3),
            created_at: Some(at(2026, 3, 3, 10, 0, 0)),
        },
    )
    .await;
    let midday = insert_item(
        &db,
        user.id,
        claim_id,
        &ItemRow {
            description: "first day, later record",
            amount: "2.0000",
            incurred_on: date(2026, 3, 1),
            created_at: Some(at(2026, 3, 1, 12, 0, 0)),
        },
    )
    .await;
    let morning = insert_item(
        &db,
        user.id,
        claim_id,
        &ItemRow {
            description: "first day, earlier record",
            amount: "1.0000",
            incurred_on: date(2026, 3, 1),
            created_at: Some(at(2026, 3, 1, 9, 0, 0)),
        },
    )
    .await;

    let response = get_claim(&app, &user.access_token, &claim_id.to_string()).await;
    let detail = response.expect_status(StatusCode::OK);

    let returned: Vec<String> = detail["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();

    assert_eq!(
        returned,
        vec![morning.to_string(), midday.to_string(), late.to_string()],
        "the items must be ordered by incurred_on, then created_at, then id"
    );

    // Every item belongs to this claim and carries its money as a string.
    for item in detail["items"].as_array().unwrap() {
        assert_eq!(item["claim_id"], claim_id.to_string());
        assert_money(&item["amount"], "items[].amount");
    }
}

// ---------------------------------------------------------------------------
// 12.8 - line items: writing one, and the total it maintains (R3)
// ---------------------------------------------------------------------------

/// The path of one claim's item collection.
fn items_path(claim_id: &str) -> String {
    format!("{CLAIMS}/{claim_id}/items")
}

/// The path of one item inside its claim.
fn item_path(claim_id: &str, item_id: &str) -> String {
    format!("{CLAIMS}/{claim_id}/items/{item_id}")
}

/// The path of one lifecycle action.
fn action_path(claim_id: &str, action: &str) -> String {
    format!("{CLAIMS}/{claim_id}/{action}")
}

/// The body of a minimal item request.
fn item_body(description: &str, amount: &str, incurred_on: &str) -> Value {
    json!({
        "description": description,
        "amount": amount,
        "incurred_on": incurred_on,
    })
}

/// Adds an item to a claim and returns the raw response.
async fn post_item(app: &Router, token: &str, claim_id: &str, body: Value) -> TestResponse {
    send(
        app,
        request(Method::POST, &items_path(claim_id), Some(token), Some(body)),
    )
    .await
}

/// Patches one item of a claim and returns the raw response.
async fn patch_item(
    app: &Router,
    token: &str,
    claim_id: &str,
    item_id: &str,
    body: Value,
) -> TestResponse {
    send(
        app,
        request(
            Method::PATCH,
            &item_path(claim_id, item_id),
            Some(token),
            Some(body),
        ),
    )
    .await
}

/// Deletes one item of a claim and returns the raw response.
async fn delete_item(app: &Router, token: &str, claim_id: &str, item_id: &str) -> TestResponse {
    send(
        app,
        request(
            Method::DELETE,
            &item_path(claim_id, item_id),
            Some(token),
            None,
        ),
    )
    .await
}

/// Fires one lifecycle action and returns the raw response.
async fn act(app: &Router, token: &str, claim_id: &str, action: &str) -> TestResponse {
    send(
        app,
        request(
            Method::POST,
            &action_path(claim_id, action),
            Some(token),
            None,
        ),
    )
    .await
}

/// Reads one claim's detail and returns the `200` body.
async fn detail_of(app: &Router, token: &str, claim_id: &str) -> Value {
    get_claim(app, token, claim_id)
        .await
        .expect_status(StatusCode::OK)
}

/// The identifier of a created item.
fn item_id_of(item: &Value) -> String {
    item["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the item response must carry an id: {item}"))
        .to_string()
}

/// Reads a four-decimal money string as units of one ten-thousandth.
fn scaled(text: &str) -> i128 {
    let (integer, fraction) = text
        .split_once('.')
        .unwrap_or_else(|| panic!("{text:?} must carry a decimal point"));

    let integer: i128 = integer
        .parse()
        .unwrap_or_else(|error| panic!("{text:?} must have a whole part: {error}"));
    let fraction: i128 = fraction
        .parse()
        .unwrap_or_else(|error| panic!("{text:?} must have a fractional part: {error}"));

    integer * 10_000 + fraction
}

/// Renders units of one ten-thousandth as a four-decimal money string.
fn render_money(units: i128) -> String {
    format!("{}.{:04}", units / 10_000, units % 10_000)
}

/// Adds the amounts of a claim detail's items exactly, as scaled integers.
///
/// The arithmetic is on `i128` units of one ten-thousandth, never on a float.
/// The assertion this feeds is that the server's stored total and the client's
/// sum agree digit for digit, and an approximate sum could let a wrong total
/// pass a test that was supposed to catch it.
fn sum_of(items: &Value) -> String {
    let items = items
        .as_array()
        .unwrap_or_else(|| panic!("a claim detail must carry an items array: {items}"));

    let mut total: i128 = 0;
    for (index, item) in items.iter().enumerate() {
        let amount = assert_money(&item["amount"], &format!("items[{index}].amount"));
        total += scaled(&amount);
    }

    render_money(total)
}

/// Creates a claim with one item and walks it to `status` through the API.
///
/// The state is built by the operations under test rather than by SQL, so a
/// lifecycle that cannot reach one of its own states fails here instead of
/// quietly making the transition test vacuous.
async fn claim_in_status(app: &Router, token: &str, title: &str, status: &str) -> String {
    let created = create_claim(app, token, title).await;
    let claim_id = id_of(&created);

    post_item(
        app,
        token,
        &claim_id,
        item_body("One line", "10.0000", "2026-03-01"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    match status {
        "draft" => {}
        "submitted" => {
            act(app, token, &claim_id, "submit")
                .await
                .expect_status(StatusCode::OK);
        }
        "approved" => {
            act(app, token, &claim_id, "submit")
                .await
                .expect_status(StatusCode::OK);
            act(app, token, &claim_id, "approve")
                .await
                .expect_status(StatusCode::OK);
        }
        "rejected" => {
            act(app, token, &claim_id, "submit")
                .await
                .expect_status(StatusCode::OK);
            act(app, token, &claim_id, "reject")
                .await
                .expect_status(StatusCode::OK);
        }
        "paid" => {
            act(app, token, &claim_id, "submit")
                .await
                .expect_status(StatusCode::OK);
            act(app, token, &claim_id, "approve")
                .await
                .expect_status(StatusCode::OK);
            act(app, token, &claim_id, "pay")
                .await
                .expect_status(StatusCode::OK);
        }
        other => panic!("`{other}` is not one of the five published statuses"),
    }

    let claim = detail_of(app, token, &claim_id).await;
    assert_eq!(
        claim["status"], status,
        "the fixture must reach the {status} status"
    );

    claim_id
}

#[tokio::test]
async fn an_item_can_be_added_to_a_draft_claim() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-create").await;

    let created = create_claim(&app, &user.access_token, "Berlin trip").await;
    let claim_id = id_of(&created);

    // Every published field of §8, with the trimming and lower-casing the
    // contract promises: a padded description is trimmed and a mixed-case
    // category is stored as its lower-case token.
    let response = post_item(
        &app,
        &user.access_token,
        &claim_id,
        json!({
            "description": "  Train, return  ",
            "amount": "125.8000",
            "incurred_on": "2026-10-20",
            "category": "Travel",
            "receipt_url": "https://receipts.example.test/2026/10/20-train.pdf",
        }),
    )
    .await;
    let item = response.expect_status(StatusCode::CREATED);

    let item_id = item_id_of(&item);
    assert_eq!(
        item["claim_id"], claim_id,
        "the item must be addressed by the claim it was posted under"
    );
    assert_eq!(
        item["description"], "Train, return",
        "an item description is trimmed before it is stored"
    );
    assert_eq!(
        assert_money(&item["amount"], "amount"),
        "125.8000",
        "an item amount is a money string at scale four"
    );
    assert_eq!(item["incurred_on"], "2026-10-20");
    assert_eq!(
        item["category"], "travel",
        "a category is trimmed and lower-cased"
    );
    assert_eq!(
        item["receipt_url"], "https://receipts.example.test/2026/10/20-train.pdf",
        "a receipt reference is stored as it arrived"
    );
    assert!(item["created_at"].is_string());
    assert!(item["updated_at"].is_string());

    // The item is denominated in its claim, so it carries no currency, and the
    // owner is never published.
    assert!(
        item.get("currency").is_none(),
        "ItemResponse carries no currency: {item}"
    );
    assert!(
        item.get("user_id").is_none(),
        "user_id must not be published: {item}"
    );

    // The two optional fields default to null, not to an empty string.
    let response = post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Coffee", "4.5000", "2026-10-21"),
    )
    .await;
    let minimal = response.expect_status(StatusCode::CREATED);
    assert!(
        minimal["category"].is_null(),
        "an absent category must be null: {minimal}"
    );
    assert!(
        minimal["receipt_url"].is_null(),
        "an absent receipt_url must be null: {minimal}"
    );

    // The claim detail is where an item is read back.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(claim["status"], "draft");
    assert_eq!(
        claim["items"].as_array().map(Vec::len),
        Some(2),
        "the detail must list both items: {claim}"
    );
    let returned: Vec<String> = claim["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        returned.contains(&item_id),
        "the added item must be readable through the claim: {returned:?}"
    );
    assert_eq!(sum_of(&claim["items"]), "130.3000");
}

#[tokio::test]
async fn adding_an_item_recomputes_the_total() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-total").await;

    let created = create_claim(&app, &user.access_token, "Recomputed claim").await;
    let claim_id = id_of(&created);

    // Before any item the total is zero, and every add moves it by exactly the
    // amount added - read back from the claim, not from the item response.
    let before = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(
        assert_money(&before["total_amount"], "total_amount"),
        "0.0000"
    );

    let steps = [
        ("125.8000", "125.8000"),
        ("24.0000", "149.8000"),
        ("0.0001", "149.8001"),
    ];

    for (amount, expected) in steps {
        post_item(
            &app,
            &user.access_token,
            &claim_id,
            item_body("A line", amount, "2026-10-20"),
        )
        .await
        .expect_status(StatusCode::CREATED);

        let claim = detail_of(&app, &user.access_token, &claim_id).await;
        assert_eq!(
            assert_money(&claim["total_amount"], "total_amount"),
            expected,
            "the total must be the sum of the items after adding {amount}"
        );
        assert_eq!(
            sum_of(&claim["items"]),
            expected,
            "the items and the total must agree after adding {amount}"
        );
    }
}

#[tokio::test]
async fn an_item_amount_round_trips_with_four_decimals() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-money").await;

    // Every case is its own claim: the largest accepted value is only
    // representable once, and a single-item claim makes the total equal to the
    // amount, so a rounding step anywhere is visible.
    for amount in ["125.8000", "0.0000", "999999999999999.9999"] {
        let created = create_claim(&app, &user.access_token, "Round trip").await;
        let claim_id = id_of(&created);

        let response = post_item(
            &app,
            &user.access_token,
            &claim_id,
            item_body("A line", amount, "2026-10-20"),
        )
        .await;
        let item = response.expect_status(StatusCode::CREATED);

        assert_eq!(
            assert_money(&item["amount"], "amount"),
            amount,
            "the response must echo the amount at the scale it was sent with"
        );

        // The stored value, read back through a different operation.
        let claim = detail_of(&app, &user.access_token, &claim_id).await;
        assert_eq!(claim["items"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            assert_money(&claim["items"][0]["amount"], "items[0].amount"),
            amount,
            "the stored amount must be the value that was sent"
        );
        assert_eq!(
            assert_money(&claim["total_amount"], "total_amount"),
            amount,
            "a one-item claim's total is that item's amount, at scale four"
        );
    }
}

#[tokio::test]
async fn an_item_amount_must_be_a_four_decimal_string() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-amount").await;

    let created = create_claim(&app, &user.access_token, "Amount shapes").await;
    let claim_id = id_of(&created);

    // Each of these is a different way to be "numerically right and
    // contractually wrong": the wire form is a string with exactly four
    // decimals, and the server refuses rather than rounds (ADR-0007).
    for amount in ["12.5", "12.500", "1,000.0000", "1e3", "-1.0000"] {
        let response = post_item(
            &app,
            &user.access_token,
            &claim_id,
            item_body("Bad amount", amount, "2026-10-20"),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "amount");
    }

    // A JSON number in the amount position never reaches the rule: money is a
    // string in the request contract, so the body fails to deserialize.
    let response = post_item(
        &app,
        &user.access_token,
        &claim_id,
        json!({ "description": "Numeric amount", "amount": 12.5, "incurred_on": "2026-10-20" }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    // Sixteen integer digits are outside the published maximum of fifteen.
    let response = post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Too large", "9999999999999999.0000", "2026-10-20"),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "amount");

    // Nothing refused was written, and the total did not move.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(
        claim["items"].as_array().map(Vec::len),
        Some(0),
        "a refused amount must not create an item: {claim}"
    );
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "0.0000"
    );
}

#[tokio::test]
async fn an_item_cannot_be_added_to_a_submitted_claim() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-not-draft").await;

    // Every status other than `draft` refuses an item write with a conflict,
    // and the claim's own content does not move.
    for status in ["submitted", "approved", "rejected", "paid"] {
        let claim_id = claim_in_status(&app, &user.access_token, status, status).await;
        let before = detail_of(&app, &user.access_token, &claim_id).await;

        let response = post_item(
            &app,
            &user.access_token,
            &claim_id,
            item_body("Late line", "5.0000", "2026-10-22"),
        )
        .await;
        let body = response.expect_status(StatusCode::CONFLICT);
        assert_error(&body, "conflict");

        let after = detail_of(&app, &user.access_token, &claim_id).await;
        assert_eq!(
            after["items"].as_array().map(Vec::len),
            Some(1),
            "a refused item write must not add an item to a {status} claim: {after}"
        );
        assert_eq!(
            after["total_amount"], before["total_amount"],
            "a refused item write must not move the total of a {status} claim"
        );
    }
}

#[tokio::test]
async fn an_item_cannot_be_changed_or_deleted_after_submission() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-locked").await;

    let created = create_claim(&app, &user.access_token, "Locked claim").await;
    let claim_id = id_of(&created);

    let item = post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Train, return", "125.8000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);
    let item_id = item_id_of(&item);

    act(&app, &user.access_token, &claim_id, "submit")
        .await
        .expect_status(StatusCode::OK);

    let response = patch_item(
        &app,
        &user.access_token,
        &claim_id,
        &item_id,
        json!({ "amount": "1.0000" }),
    )
    .await;
    let body = response.expect_status(StatusCode::CONFLICT);
    assert_error(&body, "conflict");

    let response = delete_item(&app, &user.access_token, &claim_id, &item_id).await;
    let body = response.expect_status(StatusCode::CONFLICT);
    assert_error(&body, "conflict");

    // The item is untouched and still there: neither refused write landed.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(claim["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        claim["items"][0]["id"].as_str(),
        Some(item_id.as_str()),
        "the item must survive the refused delete"
    );
    assert_eq!(
        assert_money(&claim["items"][0]["amount"], "items[0].amount"),
        "125.8000",
        "the amount must survive the refused patch"
    );
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "125.8000"
    );
    assert_eq!(claim["status"], "submitted");
}

#[tokio::test]
async fn an_item_can_be_partially_updated() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-patch").await;

    let created = create_claim(&app, &user.access_token, "Patchable claim").await;
    let claim_id = id_of(&created);

    let item = post_item(
        &app,
        &user.access_token,
        &claim_id,
        json!({
            "description": "Train, return",
            "amount": "125.8000",
            "incurred_on": "2026-10-20",
            "category": "travel",
            "receipt_url": "https://receipts.example.test/train.pdf",
        }),
    )
    .await
    .expect_status(StatusCode::CREATED);
    let item_id = item_id_of(&item);

    // One field: everything else is left untouched.
    let response = patch_item(
        &app,
        &user.access_token,
        &claim_id,
        &item_id,
        json!({ "amount": "129.9000" }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);

    assert_eq!(assert_money(&updated["amount"], "amount"), "129.9000");
    assert_eq!(updated["description"], "Train, return");
    assert_eq!(updated["incurred_on"], "2026-10-20");
    assert_eq!(updated["category"], "travel");
    assert_eq!(
        updated["receipt_url"], "https://receipts.example.test/train.pdf",
        "an omitted receipt_url must not be cleared"
    );
    assert_eq!(updated["id"], item_id);

    // The total follows the item, because the item is the only source of it.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "129.9000",
        "an item patch must recompute the claim total"
    );

    // Another field, and the amount stays where the previous patch put it.
    let response = patch_item(
        &app,
        &user.access_token,
        &claim_id,
        &item_id,
        json!({ "description": "  Train, return (updated)  ", "incurred_on": "2026-10-19" }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert_eq!(
        updated["description"], "Train, return (updated)",
        "a patched description is trimmed"
    );
    assert_eq!(updated["incurred_on"], "2026-10-19");
    assert_eq!(
        assert_money(&updated["amount"], "amount"),
        "129.9000",
        "an omitted amount must not be rewritten"
    );

    // An explicit null clears the two nullable fields and nothing else.
    let response = patch_item(
        &app,
        &user.access_token,
        &claim_id,
        &item_id,
        json!({ "category": null, "receipt_url": null }),
    )
    .await;
    let cleared = response.expect_status(StatusCode::OK);
    assert!(
        cleared["category"].is_null(),
        "`category: null` must clear the category: {cleared}"
    );
    assert!(
        cleared["receipt_url"].is_null(),
        "`receipt_url: null` must clear the reference: {cleared}"
    );
    assert_eq!(cleared["description"], "Train, return (updated)");

    // The persisted state agrees with the last response.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    let stored = &claim["items"][0];
    assert!(stored["category"].is_null());
    assert!(stored["receipt_url"].is_null());
    assert_eq!(stored["incurred_on"], "2026-10-19");
}

#[tokio::test]
async fn deleting_an_item_recomputes_the_total() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-delete").await;

    let created = create_claim(&app, &user.access_token, "Deletable items").await;
    let claim_id = id_of(&created);

    let first = post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Train", "125.8000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);
    let first_id = item_id_of(&first);

    let second = post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Taxi", "24.0000", "2026-10-21"),
    )
    .await
    .expect_status(StatusCode::CREATED);
    let second_id = item_id_of(&second);

    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "149.8000"
    );

    // Deleting one item answers 204 with no body.
    let response = delete_item(&app, &user.access_token, &claim_id, &first_id).await;
    assert_eq!(
        response.status,
        StatusCode::NO_CONTENT,
        "a delete answers 204; body was {}",
        response.body
    );
    assert!(
        response.body.is_null(),
        "a 204 carries no body: {}",
        response.body
    );

    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "24.0000",
        "the total must be the remaining items' sum after a delete"
    );
    assert_eq!(claim["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(claim["items"][0]["id"].as_str(), Some(second_id.as_str()));

    // Deleting the last one returns the total to zero, not to null.
    delete_item(&app, &user.access_token, &claim_id, &second_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(claim["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "0.0000",
        "a claim with no items has a total of zero at scale four"
    );

    // Deleting the same item twice is a 404, not a 204.
    let response = delete_item(&app, &user.access_token, &claim_id, &second_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");
}

#[tokio::test]
async fn an_item_of_another_users_claim_is_invisible() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "item-owner").await;
    let other = register_user(&app, "item-other").await;

    let created = create_claim(&app, &owner.access_token, "Private claim").await;
    let claim_id = id_of(&created);

    let item = post_item(
        &app,
        &owner.access_token,
        &claim_id,
        item_body("Private line", "125.8000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);
    let item_id = item_id_of(&item);

    // CREATE under somebody else's claim.
    let response = post_item(
        &app,
        &other.access_token,
        &claim_id,
        item_body("Intruder line", "9.9900", "2026-10-21"),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // UPDATE and DELETE of somebody else's item.
    let response = patch_item(
        &app,
        &other.access_token,
        &claim_id,
        &item_id,
        json!({ "amount": "0.0001" }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = delete_item(&app, &other.access_token, &claim_id, &item_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The other account's collection is still empty, so the claim was not
    // adopted either.
    let response = list_claims(&app, &other.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);

    // The owner's claim and item are unchanged.
    let claim = detail_of(&app, &owner.access_token, &claim_id).await;
    assert_eq!(claim["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        claim["items"][0]["id"].as_str(),
        Some(item_id.as_str()),
        "the item must survive the other account's delete"
    );
    assert_eq!(
        assert_money(&claim["items"][0]["amount"], "items[0].amount"),
        "125.8000",
        "the amount must survive the other account's patch"
    );
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "125.8000"
    );
}

#[tokio::test]
async fn an_item_id_under_the_wrong_claim_is_not_found() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-wrong-claim").await;

    // Both claims belong to the caller, so ownership is not the reason for the
    // refusal: the item simply is not addressable under the other claim.
    let first = create_claim(&app, &user.access_token, "First claim").await;
    let first_id = id_of(&first);
    let second = create_claim(&app, &user.access_token, "Second claim").await;
    let second_id = id_of(&second);

    let item = post_item(
        &app,
        &user.access_token,
        &second_id,
        item_body("Second claim line", "42.0000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);
    let item_id = item_id_of(&item);

    // The item of the second claim, addressed under the first one.
    let response = patch_item(
        &app,
        &user.access_token,
        &first_id,
        &item_id,
        json!({ "amount": "1.0000" }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = delete_item(&app, &user.access_token, &first_id, &item_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // An item that exists nowhere is the same answer.
    let missing = Uuid::new_v4().to_string();
    let response = patch_item(
        &app,
        &user.access_token,
        &first_id,
        &missing,
        json!({ "amount": "1.0000" }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The item is still in the claim it belongs to, untouched, and the first
    // claim never gained one.
    let claim = detail_of(&app, &user.access_token, &second_id).await;
    assert_eq!(claim["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        assert_money(&claim["items"][0]["amount"], "items[0].amount"),
        "42.0000"
    );

    let empty = detail_of(&app, &user.access_token, &first_id).await;
    assert_eq!(empty["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        assert_money(&empty["total_amount"], "total_amount"),
        "0.0000"
    );
}

#[tokio::test]
async fn a_receipt_url_must_be_absolute_http_or_https() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-receipt").await;

    let created = create_claim(&app, &user.access_token, "Receipt claim").await;
    let claim_id = id_of(&created);

    // A reference with no scheme, a scheme the API never returns, a
    // protocol-relative form, a non-http scheme and a relative path.
    for receipt_url in [
        "receipts.example.test/2026/10/20.pdf",
        "ftp://receipts.example.test/2026/10/20.pdf",
        "//receipts.example.test/2026/10/20.pdf",
        "javascript:alert(1)",
        "/receipts/2026/10/20.pdf",
        "http:/receipts.example.test/2026/10/20.pdf",
    ] {
        let response = post_item(
            &app,
            &user.access_token,
            &claim_id,
            json!({
                "description": "Bad reference",
                "amount": "1.0000",
                "incurred_on": "2026-10-20",
                "receipt_url": receipt_url,
            }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "receipt_url");
    }

    // One character over the published bound of 2048.
    let over = format!("https://receipts.example.test/{}", "x".repeat(2_049 - 30));
    assert_eq!(over.chars().count(), 2_049);
    let response = post_item(
        &app,
        &user.access_token,
        &claim_id,
        json!({
            "description": "Long reference",
            "amount": "1.0000",
            "incurred_on": "2026-10-20",
            "receipt_url": over,
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "receipt_url");

    // At the bound it is accepted, so the refusal above is the length and not
    // "a long URL is never a URL".
    let at_bound = format!("https://receipts.example.test/{}", "x".repeat(2_048 - 30));
    assert_eq!(at_bound.chars().count(), 2_048);
    let response = post_item(
        &app,
        &user.access_token,
        &claim_id,
        json!({
            "description": "Long but allowed",
            "amount": "1.0000",
            "incurred_on": "2026-10-20",
            "receipt_url": at_bound,
        }),
    )
    .await;
    let accepted = response.expect_status(StatusCode::CREATED);
    assert_eq!(accepted["receipt_url"], at_bound);

    // The two schemes the contract names are accepted, and the reference is
    // never fetched - the test asserts the stored value only, because the
    // host does not resolve and the suite must not reach the network.
    let response = post_item(
        &app,
        &user.access_token,
        &claim_id,
        json!({
            "description": "Secure reference",
            "amount": "2.0000",
            "incurred_on": "2026-10-21",
            "receipt_url": "https://receipts.example.test/2026/10/21.pdf",
        }),
    )
    .await;
    response.expect_status(StatusCode::CREATED);

    let response = post_item(
        &app,
        &user.access_token,
        &claim_id,
        json!({
            "description": "Plain reference",
            "amount": "3.0000",
            "incurred_on": "2026-10-22",
            "receipt_url": "http://example.test/receipt.pdf",
        }),
    )
    .await;
    response.expect_status(StatusCode::CREATED);

    // The refused bodies created nothing.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(
        claim["items"].as_array().map(Vec::len),
        Some(3),
        "only the three accepted items may exist: {claim}"
    );
}

#[tokio::test]
async fn the_claim_total_equals_the_sum_of_its_items() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "item-total-invariant").await;

    let created = create_claim(&app, &user.access_token, "Invariant claim").await;
    let claim_id = id_of(&created);

    // ADD. The stored total is read back through the claim detail, not taken
    // from the response of the mutating call, and it is compared both with the
    // items the server returned and with an independently computed expectation.
    let mut expected: i128 = 0;
    for amount in ["125.8000", "24.0000", "0.0001"] {
        post_item(
            &app,
            &user.access_token,
            &claim_id,
            item_body("A line", amount, "2026-10-20"),
        )
        .await
        .expect_status(StatusCode::CREATED);

        expected += scaled(amount);

        let claim = detail_of(&app, &user.access_token, &claim_id).await;
        let stored = assert_money(&claim["total_amount"], "total_amount");

        assert_eq!(
            stored,
            sum_of(&claim["items"]),
            "after adding {amount} the stored total must equal the sum of the items"
        );
        assert_eq!(
            stored,
            render_money(expected),
            "after adding {amount} the stored total must equal the expected sum"
        );
    }

    assert_eq!(render_money(expected), "149.8001");

    // UPDATE. One amount is replaced; the total is read back again.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    let item_id = claim["items"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the detail must carry item ids: {claim}"))
        .to_string();
    let replaced = claim["items"][0]["amount"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    patch_item(
        &app,
        &user.access_token,
        &claim_id,
        &item_id,
        json!({ "amount": "200.0000" }),
    )
    .await
    .expect_status(StatusCode::OK);

    expected = expected - scaled(&replaced) + scaled("200.0000");

    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    let stored = assert_money(&claim["total_amount"], "total_amount");
    assert_eq!(
        stored,
        sum_of(&claim["items"]),
        "after an item update the stored total must equal the sum of the items"
    );
    assert_eq!(
        stored,
        render_money(expected),
        "after an item update the stored total must equal the expected sum"
    );
    assert_eq!(render_money(expected), "224.0001");

    // DELETE. One item is removed; the total is read back a third time.
    let removed = claim["items"][1]["amount"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let removed_id = claim["items"][1]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    delete_item(&app, &user.access_token, &claim_id, &removed_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    expected -= scaled(&removed);

    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    let stored = assert_money(&claim["total_amount"], "total_amount");
    assert_eq!(
        stored,
        sum_of(&claim["items"]),
        "after a delete the stored total must equal the sum of the items"
    );
    assert_eq!(
        stored,
        render_money(expected),
        "after a delete the stored total must equal the expected sum"
    );
    assert_eq!(render_money(expected), "200.0001");

    // Removing the rest leaves a zero total, which is still the exact sum.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    for item in claim["items"].as_array().unwrap() {
        let item_id = item["id"].as_str().unwrap_or_default();
        delete_item(&app, &user.access_token, &claim_id, item_id)
            .await
            .expect_status(StatusCode::NO_CONTENT);
    }

    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(claim["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        sum_of(&claim["items"]),
        "an empty claim's total must equal the empty sum"
    );
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "0.0000"
    );
}

// ---------------------------------------------------------------------------
// 12.9 - lifecycle actions (R4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_draft_can_be_submitted_approved_and_paid() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "lifecycle-happy").await;

    let created = create_claim(&app, &user.access_token, "Reimbursement").await;
    let claim_id = id_of(&created);

    post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Train, return", "125.8000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    // draft -> submitted
    let response = act(&app, &user.access_token, &claim_id, "submit").await;
    let submitted = response.expect_status(StatusCode::OK);

    assert_eq!(submitted["status"], "submitted");
    assert!(submitted["submitted_at"].is_string());
    assert!(submitted["decided_at"].is_null());
    assert!(submitted["paid_at"].is_null());
    assert_eq!(
        assert_money(&submitted["total_amount"], "total_amount"),
        "125.8000",
        "a transition must not disturb the total"
    );
    assert_eq!(
        submitted["items"].as_array().map(Vec::len),
        Some(1),
        "a lifecycle response is the claim detail, with its items"
    );

    // submitted -> approved
    let response = act(&app, &user.access_token, &claim_id, "approve").await;
    let approved = response.expect_status(StatusCode::OK);

    assert_eq!(approved["status"], "approved");
    assert!(approved["decided_at"].is_string());
    assert!(approved["paid_at"].is_null());
    assert_eq!(
        approved["submitted_at"], submitted["submitted_at"],
        "an approval must not move the submission instant"
    );

    // approved -> paid
    let response = act(&app, &user.access_token, &claim_id, "pay").await;
    let paid = response.expect_status(StatusCode::OK);

    assert_eq!(paid["status"], "paid");
    assert!(paid["paid_at"].is_string());
    assert_eq!(
        paid["submitted_at"], submitted["submitted_at"],
        "a payment must not move the submission instant"
    );
    assert_eq!(
        paid["decided_at"], approved["decided_at"],
        "a payment must not move the decision instant"
    );

    // Every step was committed, not only answered.
    let stored = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(stored["status"], "paid");
    assert_eq!(stored["submitted_at"], submitted["submitted_at"]);
    assert_eq!(stored["decided_at"], approved["decided_at"]);
    assert_eq!(stored["paid_at"], paid["paid_at"]);
    assert_eq!(
        assert_money(&stored["total_amount"], "total_amount"),
        "125.8000"
    );
}

#[tokio::test]
async fn a_claim_without_items_cannot_be_submitted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "lifecycle-no-items").await;

    let created = create_claim(&app, &user.access_token, "Empty claim").await;
    let claim_id = id_of(&created);
    assert_eq!(
        assert_money(&created["total_amount"], "total_amount"),
        "0.0000"
    );

    let response = act(&app, &user.access_token, &claim_id, "submit").await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "items");

    // The refusal is a refusal: the claim is still a draft with no stamps.
    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(claim["status"], "draft");
    assert!(claim["submitted_at"].is_null());
    assert!(claim["decided_at"].is_null());
    assert!(claim["paid_at"].is_null());

    // With one item the same call succeeds, so the precondition is the reason
    // and not the operation itself.
    post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("A line", "1.0000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let response = act(&app, &user.access_token, &claim_id, "submit").await;
    let submitted = response.expect_status(StatusCode::OK);
    assert_eq!(submitted["status"], "submitted");
}

#[tokio::test]
async fn a_claim_with_a_zero_total_cannot_be_submitted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "lifecycle-zero").await;

    let created = create_claim(&app, &user.access_token, "Zero claim").await;
    let claim_id = id_of(&created);

    // A zero-amount item is legal (§2.7 allows `0.0000`), so the claim does
    // have an item and the only thing wrong is the total.
    post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("A free line", "0.0000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(claim["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "0.0000"
    );

    let response = act(&app, &user.access_token, &claim_id, "submit").await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "total_amount");

    let claim = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(claim["status"], "draft");
    assert!(claim["submitted_at"].is_null());

    // The smallest representable amount is enough, so the rule is "above zero"
    // and not "at least one cent" or any other rounded boundary.
    post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("One unit", "0.0001", "2026-10-21"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let response = act(&app, &user.access_token, &claim_id, "submit").await;
    let submitted = response.expect_status(StatusCode::OK);
    assert_eq!(submitted["status"], "submitted");
    assert_eq!(
        assert_money(&submitted["total_amount"], "total_amount"),
        "0.0001"
    );
}

#[tokio::test]
async fn a_submitted_claim_cannot_be_approved_twice() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "lifecycle-double-approve").await;

    let claim_id = claim_in_status(&app, &user.access_token, "Once only", "submitted").await;

    let response = act(&app, &user.access_token, &claim_id, "approve").await;
    let approved = response.expect_status(StatusCode::OK);
    assert_eq!(approved["status"], "approved");
    let decided_at = approved["decided_at"].clone();

    let response = act(&app, &user.access_token, &claim_id, "approve").await;
    let body = response.expect_status(StatusCode::CONFLICT);
    assert_error(&body, "conflict");

    // The second call changed nothing, including the instant the first one
    // stamped.
    let stored = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(stored["status"], "approved");
    assert_eq!(
        stored["decided_at"], decided_at,
        "a refused approval must not re-stamp the decision"
    );
    assert!(stored["paid_at"].is_null());
}

#[tokio::test]
async fn every_forbidden_transition_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "lifecycle-forbidden").await;

    // One claim per status. A refusal cannot change a status, so each claim is
    // reused for every refused action aimed at its status.
    let mut claims = std::collections::BTreeMap::new();
    for status in ["draft", "submitted", "approved", "rejected", "paid"] {
        let claim_id = claim_in_status(&app, &user.access_token, status, status).await;
        claims.insert(status, claim_id);
    }

    // The twenty status/action pairs of §11-§14 minus the five the contract
    // allows: draft/submit, submitted/approve, submitted/reject,
    // approved/pay and rejected/submit (the reopen).
    let refused: [(&str, &str); 15] = [
        ("draft", "approve"),
        ("draft", "reject"),
        ("draft", "pay"),
        ("submitted", "submit"),
        ("submitted", "pay"),
        ("approved", "submit"),
        ("approved", "approve"),
        ("approved", "reject"),
        ("rejected", "approve"),
        ("rejected", "reject"),
        ("rejected", "pay"),
        ("paid", "submit"),
        ("paid", "approve"),
        ("paid", "reject"),
        ("paid", "pay"),
    ];

    for (status, action) in refused {
        let claim_id = claims
            .get(status)
            .unwrap_or_else(|| panic!("no fixture claim for the {status} status"))
            .clone();

        let before = detail_of(&app, &user.access_token, &claim_id).await;
        assert_eq!(
            before["status"], status,
            "the fixture for {status}/{action} must start in {status}"
        );

        let response = act(&app, &user.access_token, &claim_id, action).await;
        let body = response.expect_status(StatusCode::CONFLICT);
        assert_error(&body, "conflict");

        let after = detail_of(&app, &user.access_token, &claim_id).await;
        assert_eq!(
            after["status"], status,
            "{action} on a {status} claim must be refused, but the status became {}",
            after["status"]
        );
        for field in ["submitted_at", "decided_at", "paid_at"] {
            assert_eq!(
                after[field], before[field],
                "a refused {action} on a {status} claim must not write `{field}`"
            );
        }
        assert_eq!(
            after["total_amount"], before["total_amount"],
            "a refused {action} on a {status} claim must not move the total"
        );
    }

    // The five allowed pairs are not in the refused table, and each of them is
    // exercised by its own test, so this table is the complement and not a
    // second opinion about the whole lifecycle.
    for (status, action) in [
        ("draft", "submit"),
        ("submitted", "approve"),
        ("submitted", "reject"),
        ("approved", "pay"),
        ("rejected", "submit"),
    ] {
        assert!(
            !refused.contains(&(status, action)),
            "{status}/{action} is an allowed transition and must not be listed as refused"
        );
    }
}

#[tokio::test]
async fn a_rejected_claim_reopens_and_can_be_resubmitted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "lifecycle-reopen").await;

    let created = create_claim(&app, &user.access_token, "Reopening claim").await;
    let claim_id = id_of(&created);

    post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Train, return", "125.8000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    act(&app, &user.access_token, &claim_id, "submit")
        .await
        .expect_status(StatusCode::OK);

    let response = act(&app, &user.access_token, &claim_id, "reject").await;
    let rejected = response.expect_status(StatusCode::OK);
    assert_eq!(rejected["status"], "rejected");
    assert!(rejected["submitted_at"].is_string());
    assert!(rejected["decided_at"].is_string());

    // `submit` on a rejected claim reopens it. The `draft` arm of
    // `expense_claims_lifecycle_timestamps_chk` requires all three stamps to be
    // null, so a partial clear is a 23514 at commit; a wrong answer here is a
    // 500 and this assertion is what makes it fail as a test rather than as a
    // server error nobody read.
    let response = act(&app, &user.access_token, &claim_id, "submit").await;
    let reopened = response.expect_status(StatusCode::OK);

    assert_eq!(
        reopened["status"], "draft",
        "a rejected claim reopens to draft"
    );
    assert!(
        reopened["submitted_at"].is_null(),
        "the reopen must clear submitted_at, but it is {}",
        reopened["submitted_at"]
    );
    assert!(
        reopened["decided_at"].is_null(),
        "the reopen must clear decided_at, but it is {}",
        reopened["decided_at"]
    );
    assert!(reopened["paid_at"].is_null());

    // The same nulls are what the row holds, not only what the response said.
    let stored = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(stored["status"], "draft");
    assert!(stored["submitted_at"].is_null());
    assert!(stored["decided_at"].is_null());
    assert!(stored["paid_at"].is_null());
    assert_eq!(
        stored["items"].as_array().map(Vec::len),
        Some(1),
        "the items survive the reopen"
    );

    // A reopened claim is a draft again, so it can be edited and resubmitted.
    post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Taxi", "24.0000", "2026-10-21"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let response = act(&app, &user.access_token, &claim_id, "submit").await;
    let resubmitted = response.expect_status(StatusCode::OK);

    assert_eq!(resubmitted["status"], "submitted");
    assert!(resubmitted["submitted_at"].is_string());
    assert!(
        resubmitted["decided_at"].is_null(),
        "a resubmission is not a decision: {}",
        resubmitted["decided_at"]
    );
    assert_eq!(
        assert_money(&resubmitted["total_amount"], "total_amount"),
        "149.8000",
        "both items count after a reopen and a resubmit"
    );
}

#[tokio::test]
async fn paid_is_terminal() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "lifecycle-terminal").await;

    let claim_id = claim_in_status(&app, &user.access_token, "Terminal claim", "paid").await;

    let before = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(before["status"], "paid");

    // No action moves a paid claim, including another payment.
    for action in ["submit", "approve", "reject", "pay"] {
        let response = act(&app, &user.access_token, &claim_id, action).await;
        let body = response.expect_status(StatusCode::CONFLICT);
        assert_error(&body, "conflict");

        let after = detail_of(&app, &user.access_token, &claim_id).await;
        assert_eq!(
            after["status"], "paid",
            "{action} must not move a paid claim, but the status became {}",
            after["status"]
        );
        for field in ["submitted_at", "decided_at", "paid_at", "total_amount"] {
            assert_eq!(
                after[field], before[field],
                "a refused {action} on a paid claim must not write `{field}`"
            );
        }
    }

    // The claim is still readable, with its items and its three stamps.
    let stored = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(stored["status"], "paid");
    assert!(stored["submitted_at"].is_string());
    assert!(stored["decided_at"].is_string());
    assert!(stored["paid_at"].is_string());
    assert_eq!(stored["items"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn a_lifecycle_action_on_another_users_claim_is_not_found() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "lifecycle-owner").await;
    let other = register_user(&app, "lifecycle-other").await;

    // `approved` is the sharpest fixture: with ownership ignored, `pay` would
    // succeed and `approve` would answer 409, so every answer other than 404
    // is visibly different from "not the caller's".
    let claim_id = claim_in_status(&app, &owner.access_token, "Owner's claim", "approved").await;

    let before = detail_of(&app, &owner.access_token, &claim_id).await;

    for action in ["submit", "approve", "reject", "pay"] {
        let response = act(&app, &other.access_token, &claim_id, action).await;
        let body = response.expect_status(StatusCode::NOT_FOUND);
        assert_error(&body, "not_found");
    }

    // The owner's claim is untouched, and the other account owns nothing.
    let after = detail_of(&app, &owner.access_token, &claim_id).await;
    assert_eq!(after["status"], "approved");
    for field in ["submitted_at", "decided_at", "paid_at"] {
        assert_eq!(
            after[field], before[field],
            "a foreign lifecycle action must not write `{field}`"
        );
    }

    let response = list_claims(&app, &other.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

#[tokio::test]
async fn the_lifecycle_timestamps_are_set_by_the_transition() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "lifecycle-stamps").await;

    let created = create_claim(&app, &user.access_token, "Stamped claim").await;
    let claim_id = id_of(&created);

    post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Train, return", "125.8000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);

    // A draft carries no stamp at all.
    let draft = detail_of(&app, &user.access_token, &claim_id).await;
    assert!(draft["submitted_at"].is_null());
    assert!(draft["decided_at"].is_null());
    assert!(draft["paid_at"].is_null());

    // `submit` sets submitted_at and nothing else.
    let submitted = act(&app, &user.access_token, &claim_id, "submit")
        .await
        .expect_status(StatusCode::OK);
    let submitted_at = instant(&submitted, "submitted_at");
    assert!(submitted["decided_at"].is_null());
    assert!(submitted["paid_at"].is_null());

    // `approve` adds decided_at and leaves submitted_at where it was.
    let approved = act(&app, &user.access_token, &claim_id, "approve")
        .await
        .expect_status(StatusCode::OK);
    let decided_at = instant(&approved, "decided_at");
    assert_eq!(
        instant(&approved, "submitted_at"),
        submitted_at,
        "an approval must not restamp the submission"
    );
    assert!(approved["paid_at"].is_null());
    assert!(
        decided_at >= submitted_at,
        "a decision cannot predate the submission ({decided_at} < {submitted_at})"
    );

    // `pay` adds paid_at and leaves the other two where they were.
    let paid = act(&app, &user.access_token, &claim_id, "pay")
        .await
        .expect_status(StatusCode::OK);
    let paid_at = instant(&paid, "paid_at");
    assert_eq!(instant(&paid, "submitted_at"), submitted_at);
    assert_eq!(instant(&paid, "decided_at"), decided_at);
    assert!(
        paid_at >= decided_at,
        "a payment cannot predate the decision ({paid_at} < {decided_at})"
    );

    // `reject` is the other decision: it stamps decided_at too, and it stamps
    // nothing else.
    let rejected_claim = claim_in_status(&app, &user.access_token, "Rejected", "submitted").await;
    let rejected = act(&app, &user.access_token, &rejected_claim, "reject")
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(rejected["status"], "rejected");
    assert!(rejected["submitted_at"].is_string());
    assert!(rejected["decided_at"].is_string());
    assert!(
        rejected["paid_at"].is_null(),
        "a rejection is not a payment: {}",
        rejected["paid_at"]
    );
    assert!(
        instant(&rejected, "decided_at") >= instant(&rejected, "submitted_at"),
        "a rejection cannot predate the submission"
    );

    // The three instants of the paid claim are the stored ones, and the last
    // transition is what stamped the last one.
    let stored = detail_of(&app, &user.access_token, &claim_id).await;
    assert_eq!(instant(&stored, "submitted_at"), submitted_at);
    assert_eq!(instant(&stored, "decided_at"), decided_at);
    assert_eq!(instant(&stored, "paid_at"), paid_at);
}

// ---------------------------------------------------------------------------
// 12.7 - the credential
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_claim_routes_require_a_credential() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "claim-credential").await;

    // A real claim with a real item, so an authenticated DELETE would have
    // something to remove and the 401 cannot be confused with a 404.
    let created = create_claim(&app, &user.access_token, "Guarded claim").await;
    let claim_id = id_of(&created);

    let item = post_item(
        &app,
        &user.access_token,
        &claim_id,
        item_body("Guarded line", "10.0000", "2026-10-20"),
    )
    .await
    .expect_status(StatusCode::CREATED);
    let item_id = item_id_of(&item);

    // All twelve operations of the feature: the five claim routes and the
    // seven that write an item or move the lifecycle.
    let operations: [(Method, String, Option<Value>); 12] = [
        (Method::GET, CLAIMS.to_string(), None),
        (
            Method::POST,
            CLAIMS.to_string(),
            Some(claim_body("Intruder", "", EUR)),
        ),
        (Method::GET, format!("{CLAIMS}/{claim_id}"), None),
        (
            Method::PATCH,
            format!("{CLAIMS}/{claim_id}"),
            Some(json!({ "title": "hijacked" })),
        ),
        (Method::DELETE, format!("{CLAIMS}/{claim_id}"), None),
        (
            Method::POST,
            items_path(&claim_id),
            Some(item_body("Intruder line", "1.0000", "2026-10-20")),
        ),
        (
            Method::PATCH,
            item_path(&claim_id, &item_id),
            Some(json!({ "amount": "0.0001" })),
        ),
        (Method::DELETE, item_path(&claim_id, &item_id), None),
        (
            Method::POST,
            action_path(&claim_id, "submit"),
            Some(json!({})),
        ),
        (
            Method::POST,
            action_path(&claim_id, "approve"),
            Some(json!({})),
        ),
        (
            Method::POST,
            action_path(&claim_id, "reject"),
            Some(json!({})),
        ),
        (Method::POST, action_path(&claim_id, "pay"), Some(json!({}))),
    ];

    for (method, uri, body) in operations {
        // No token at all, and a token that is not one.
        for token in [None, Some("not-a-valid-token")] {
            let response = send(&app, request(method.clone(), &uri, token, body.clone())).await;
            let body = response.expect_status(StatusCode::UNAUTHORIZED);

            assert_error(&body, "unauthorized");
            assert!(
                body["error"].get("details").is_none(),
                "a 401 carries no field details: {body}"
            );
        }
    }

    // Nothing the unauthenticated calls sent landed: the claim is still a
    // draft holding the one item it started with, with its own title and
    // amount, and it never left `draft`.
    let response = get_claim(&app, &user.access_token, &claim_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["title"], "Guarded claim");
    assert_eq!(
        stored["status"], "draft",
        "no unauthenticated call may move the lifecycle"
    );
    assert_eq!(stored["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(stored["items"][0]["id"].as_str(), Some(item_id.as_str()));
    assert_eq!(
        assert_money(&stored["items"][0]["amount"], "items[0].amount"),
        "10.0000"
    );
}

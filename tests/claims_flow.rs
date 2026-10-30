//! The whole reimbursement feature in one journey, and the ownership sweep.
//!
//! `C-R2`-`C-R4` prove one operation at a time: `tests/claims_api.rs` asserts
//! each of the twelve operations, and `tests/claims_schema.rs` the tables
//! underneath them. Two properties are not visible from any single operation,
//! and they are what this file exists for:
//!
//! * **The journey.** A claim that is created, filled, submitted, approved and
//!   paid in one run must still carry the right total, the right status and the
//!   right stamps at every step. A per-operation test can pass while the state
//!   the *next* step depends on is lost between two calls: the stored total,
//!   the immutable `created_at`, the `submitted_at` an approval must not
//!   restamp, and the fact that `paid` is terminal for every write and not only
//!   for the one action a per-operation test fires.
//! * **The sweep.** Ownership must be enforced by every one of the twelve
//!   operations, and it must be enforced as `404` even where the claim's own
//!   state would otherwise answer `409`. A test that fires one foreign read
//!   proves one of twelve.
//!
//! The rules this file follows, in the spirit of `AGENTS.md` §8:
//!
//! * every assertion is on the status code and the observable body - never on a
//!   log line, a timing value or a random value;
//! * where a value is **stored** - the total, the stamps - the assertion is on
//!   a fresh read, never on the response of the call that wrote it;
//! * the sums are computed independently, as `i128` units of one ten-thousandth,
//!   never by trusting the arithmetic of the server that is under test;
//! * the shared database is never truncated and no count is global: every
//!   fixture belongs to an account this file registered, so every count is
//!   restricted to that account by construction;
//! * nothing waits on a timer. There is no `sleep` here, and none is needed:
//!   every transition is a synchronous request whose `200` is the condition.

pub mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use chrono::{DateTime, Utc};
use common::{
    TestResponse, TestUser, app, assert_pagination, create_project, login_user, register_user,
    request, send, test_state,
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection};
use std::process::Command;
use uuid::Uuid;

/// The collection path of the feature.
const CLAIMS: &str = "/api/v1/claims";

/// The authenticated-account path, used to read back who registered and who
/// logged in.
const ME: &str = "/api/v1/auth/me";

/// The currency the journey uses.
const EUR: &str = "EUR";

/// The published development account the seeder writes (`C-S1`).
const SEEDED_EMAIL: &str = "owner@atom.local";

/// The published development password of the two seeded accounts (`C-S1`).
const SEEDED_PASSWORD: &str = "atom-dev-password-1234";

/// The published display name of [`SEEDED_EMAIL`] (`C-S1`).
const SEEDED_DISPLAY_NAME: &str = "Owner";

/// The fields of `UserResponse`, sorted.
const USER_FIELDS: [&str; 5] = ["created_at", "display_name", "email", "id", "is_active"];

/// The fields of `ClaimResponse`, sorted. The listing returns exactly these:
/// a listed claim carries no `items`.
const CLAIM_FIELDS: [&str; 12] = [
    "created_at",
    "currency",
    "decided_at",
    "description",
    "id",
    "paid_at",
    "project_id",
    "status",
    "submitted_at",
    "title",
    "total_amount",
    "updated_at",
];

/// The fields of `ClaimDetailResponse`, sorted: [`CLAIM_FIELDS`] plus `items`.
const CLAIM_DETAIL_FIELDS: [&str; 13] = [
    "created_at",
    "currency",
    "decided_at",
    "description",
    "id",
    "items",
    "paid_at",
    "project_id",
    "status",
    "submitted_at",
    "title",
    "total_amount",
    "updated_at",
];

/// The fields of `ItemResponse`, sorted.
const ITEM_FIELDS: [&str; 9] = [
    "amount",
    "category",
    "claim_id",
    "created_at",
    "description",
    "id",
    "incurred_on",
    "receipt_url",
    "updated_at",
];

/// The fields of the list envelope, sorted.
const PAGE_FIELDS: [&str; 2] = ["items", "pagination"];

/// The fields of the pagination block, sorted.
const PAGINATION_FIELDS: [&str; 4] = ["has_more", "limit", "offset", "total"];

/// One line item of the journey, exactly as the client sends it.
struct LineItem {
    /// Not trimmed: the service trims it, and the assertion is on what comes
    /// back.
    description: &'static str,
    amount: &'static str,
    incurred_on: &'static str,
    category: Option<&'static str>,
    receipt_url: Option<&'static str>,
}

/// The three items the journey adds.
///
/// Three properties are deliberate:
///
/// * the amounts do not add up to a round number (`149.8001`), so a total that
///   was rounded, truncated or recomputed as a float is visible;
/// * the dates are **not** in the order the items are posted, so the documented
///   `incurred_on, created_at, id` order of the detail is asserted rather than
///   assumed;
/// * one description is padded and one category is mixed case, so the trimming
///   and lower-casing the contract promises are asserted on the stored value.
const JOURNEY_ITEMS: [LineItem; 3] = [
    LineItem {
        description: "Train, return",
        amount: "125.8000",
        incurred_on: "2026-10-21",
        category: Some("Travel"),
        receipt_url: Some("https://receipts.example.test/2026/10/21-train.pdf"),
    },
    LineItem {
        description: "  Taxi to the venue  ",
        amount: "24.0000",
        incurred_on: "2026-10-19",
        category: None,
        receipt_url: None,
    },
    LineItem {
        description: "Coffee with the client",
        amount: "0.0001",
        incurred_on: "2026-10-20",
        category: Some(" Meals "),
        receipt_url: None,
    },
];

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

/// The path of one claim.
fn claim_path(claim_id: &str) -> String {
    format!("{CLAIMS}/{claim_id}")
}

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

/// Sends one operation and returns the raw response.
async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> TestResponse {
    send(app, request(method, uri, Some(token), body)).await
}

/// Builds the body of a create request.
fn claim_body(title: &str, description: &str, currency: &str) -> Value {
    json!({ "title": title, "description": description, "currency": currency })
}

/// Builds the body of a minimal item request.
fn item_body(description: &str, amount: &str, incurred_on: &str) -> Value {
    json!({
        "description": description,
        "amount": amount,
        "incurred_on": incurred_on,
    })
}

/// Creates a claim through the API and returns the `201` body.
async fn create_claim(app: &Router, token: &str, title: &str) -> Value {
    let response = call(
        app,
        Method::POST,
        CLAIMS,
        token,
        Some(claim_body(title, "", EUR)),
    )
    .await;

    response.expect_status(StatusCode::CREATED)
}

/// Lists claims with the given query string, including its leading `?`.
async fn list_claims(app: &Router, token: &str, query: &str) -> TestResponse {
    call(app, Method::GET, &format!("{CLAIMS}{query}"), token, None).await
}

/// Reads one claim and returns the `200` body.
async fn read_claim(app: &Router, token: &str, claim_id: &str) -> Value {
    let response = call(app, Method::GET, &claim_path(claim_id), token, None).await;

    response.expect_status(StatusCode::OK)
}

/// Reads the authenticated account and returns the `200` body.
async fn read_account(app: &Router, token: &str) -> Value {
    let response = call(app, Method::GET, ME, token, None).await;

    response.expect_status(StatusCode::OK)
}

/// Fires one lifecycle action, which the caller expects to be accepted.
async fn transition(app: &Router, token: &str, claim_id: &str, action: &str) -> Value {
    let response = call(
        app,
        Method::POST,
        &action_path(claim_id, action),
        token,
        None,
    )
    .await;

    response.expect_status(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

/// The field names of a JSON object, sorted.
fn fields_of(value: &Value) -> Vec<String> {
    let mut fields: Vec<String> = value
        .as_object()
        .unwrap_or_else(|| panic!("expected a JSON object, but the body was {value}"))
        .keys()
        .cloned()
        .collect();

    fields.sort_unstable();
    fields
}

/// Asserts that a value is a money **string** at scale four, and returns it.
///
/// The type check is the point: a JSON number that happened to render `149.8`
/// is a contract violation even when it is numerically equal.
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

/// Parses an RFC 3339 timestamp out of a response body.
fn instant(value: &Value, field: &str) -> DateTime<Utc> {
    let text = value[field].as_str().unwrap_or_else(|| {
        panic!(
            "{field} must be an RFC 3339 string, but it is {}",
            value[field]
        )
    });

    DateTime::parse_from_rfc3339(text)
        .unwrap_or_else(|error| panic!("{field} must be RFC 3339 ({text:?}): {error}"))
        .with_timezone(&Utc)
}

/// Parses a UUID out of a response body's field.
fn uuid_of(value: &Value, field: &str) -> Uuid {
    let text = value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} must be a UUID string, but it is {}", value[field]));

    Uuid::parse_str(text)
        .unwrap_or_else(|error| panic!("{field} must be a UUID ({text:?}): {error}"))
}

/// The identifier of a claim or item response, as the wire spells it.
fn id_of(value: &Value) -> String {
    uuid_of(value, "id").to_string()
}

/// The identifiers of a claim detail's items, in the order the server returned
/// them.
fn ids_of(items: &Value) -> Vec<String> {
    items
        .as_array()
        .unwrap_or_else(|| panic!("a claim detail must carry an items array: {items}"))
        .iter()
        .map(id_of)
        .collect()
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
/// The arithmetic is on `i128` units of one ten-thousandth, never on a float:
/// the assertion this feeds is that the stored total and an independent sum
/// agree digit for digit, and an approximate sum could let a wrong total pass
/// the test that was written to catch it.
fn sum_of(items: &Value) -> String {
    let items = items
        .as_array()
        .unwrap_or_else(|| panic!("a claim detail must carry an items array: {items}"));

    let mut total: i128 = 0;
    for (index, item) in items.iter().enumerate() {
        total += scaled(&assert_money(
            &item["amount"],
            &format!("items[{index}].amount"),
        ));
    }

    render_money(total)
}

/// Asserts the shape every claim detail must have, and returns nothing: the
/// field-by-field assertions belong to the step that knows the values.
fn assert_claim_detail_shape(claim: &Value) {
    assert_eq!(
        fields_of(claim),
        CLAIM_DETAIL_FIELDS,
        "the claim detail must publish exactly the documented fields: {claim}"
    );
    uuid_of(claim, "id");
    assert_money(&claim["total_amount"], "total_amount");

    let items = claim["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the claim detail must carry an items array: {claim}"));
    for (index, item) in items.iter().enumerate() {
        assert_eq!(
            fields_of(item),
            ITEM_FIELDS,
            "items[{index}] must publish exactly the documented fields: {item}"
        );
        assert_money(&item["amount"], &format!("items[{index}].amount"));
        assert_eq!(
            item["claim_id"], claim["id"],
            "items[{index}] must name the claim it belongs to"
        );
    }
}

/// Asserts the standard error envelope of a `404`, and that it is not a `403`.
fn assert_not_found(body: &Value, operation: &str) {
    assert_eq!(
        body["error"]["code"], "not_found",
        "{operation} must answer `404 not_found` for a foreign claim, never `403`: {body}"
    );
    assert_eq!(
        fields_of(&body["error"]),
        ["code", "message"],
        "{operation} must answer the standard envelope, with no field details: {body}"
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "{operation} must carry a non-empty message: {body}"
    );
}

// ---------------------------------------------------------------------------
// The ownership sweep
// ---------------------------------------------------------------------------

/// One operation a second account may aim at the first account's data.
struct Attempt {
    /// What the operation is, for a failure message that names it.
    label: &'static str,
    method: Method,
    uri: String,
    body: Option<Value>,
}

/// The eleven claim-addressed operations, every one of them pointed at the
/// owner's claim, its item and its project.
///
/// The list operation is the twelfth and is not here: it cannot answer `404`,
/// because the contract publishes `200` with the caller's own set. It is
/// asserted separately, in [`assert_foreign_sweep`], by the property that does
/// hold - the second account's page is empty and its `total` counts only its
/// own claims.
fn foreign_operations(claim_id: &str, item_id: &str, project_id: Uuid) -> Vec<Attempt> {
    vec![
        Attempt {
            label: "create a claim on the owner's project",
            method: Method::POST,
            uri: CLAIMS.to_string(),
            body: Some(json!({
                "title": "Intruder claim",
                "currency": EUR,
                "project_id": project_id,
            })),
        },
        Attempt {
            label: "read the claim",
            method: Method::GET,
            uri: claim_path(claim_id),
            body: None,
        },
        Attempt {
            label: "update the claim",
            method: Method::PATCH,
            uri: claim_path(claim_id),
            body: Some(json!({ "title": "hijacked" })),
        },
        Attempt {
            label: "delete the claim",
            method: Method::DELETE,
            uri: claim_path(claim_id),
            body: None,
        },
        Attempt {
            label: "add an item",
            method: Method::POST,
            uri: items_path(claim_id),
            body: Some(item_body("Intruder line", "1.0000", "2026-10-20")),
        },
        Attempt {
            label: "update an item",
            method: Method::PATCH,
            uri: item_path(claim_id, item_id),
            body: Some(json!({ "amount": "0.0001" })),
        },
        Attempt {
            label: "delete an item",
            method: Method::DELETE,
            uri: item_path(claim_id, item_id),
            body: None,
        },
        Attempt {
            label: "submit",
            method: Method::POST,
            uri: action_path(claim_id, "submit"),
            body: None,
        },
        Attempt {
            label: "approve",
            method: Method::POST,
            uri: action_path(claim_id, "approve"),
            body: None,
        },
        Attempt {
            label: "reject",
            method: Method::POST,
            uri: action_path(claim_id, "reject"),
            body: None,
        },
        Attempt {
            label: "pay",
            method: Method::POST,
            uri: action_path(claim_id, "pay"),
            body: None,
        },
    ]
}

/// Has `other` attempt every reimbursement operation against the owner's claim,
/// and asserts each answer is a `404` with the standard envelope.
///
/// `409` is what makes this more than a smoke test: a claim that is `paid`
/// would answer `409` to `pay` and `approve` for *anyone*, so an ownership
/// check that ran after the state check would show up here as a `409` and fail.
async fn assert_foreign_sweep(
    app: &Router,
    other: &TestUser,
    claim_id: &str,
    item_id: &str,
    project_id: Uuid,
) {
    for attempt in foreign_operations(claim_id, item_id, project_id) {
        let response = call(
            app,
            attempt.method,
            &attempt.uri,
            &other.access_token,
            attempt.body,
        )
        .await;

        let body = response.expect_status(StatusCode::NOT_FOUND);
        assert_not_found(&body, attempt.label);
    }

    // The twelfth operation. The contract publishes `200` and the caller's own
    // set, so the assertion is that the owner's data is not in it and that the
    // account owns nothing - `other` was registered by this test and has never
    // created a claim.
    let page = list_claims(app, &other.access_token, "").await;
    let page = page.expect_status(StatusCode::OK);

    assert_eq!(
        fields_of(&page),
        PAGE_FIELDS,
        "the listing must answer the standard envelope: {page}"
    );
    assert_pagination(&page, 0, 20, 0);
    assert!(
        page["items"].as_array().is_some_and(Vec::is_empty),
        "the second account must not see the owner's claims in its own listing: {page}"
    );
}

// ---------------------------------------------------------------------------
// Task 1 - the whole journey
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_whole_reimbursement_journey_from_registration_to_payment() {
    let state = test_state().await;
    let app = app(state);

    // -----------------------------------------------------------------
    // 1. register
    // -----------------------------------------------------------------
    let owner = register_user(&app, "journey-owner").await;
    let registered = read_account(&app, &owner.access_token).await;

    assert_eq!(
        fields_of(&registered),
        USER_FIELDS,
        "the account must publish exactly the documented fields: {registered}"
    );
    assert_eq!(
        uuid_of(&registered, "id"),
        owner.id,
        "the credential must belong to the account that was registered"
    );
    assert_eq!(registered["email"], owner.email);
    assert_eq!(registered["display_name"], "Test User");
    assert_eq!(registered["is_active"], json!(true));
    instant(&registered, "created_at");

    // -----------------------------------------------------------------
    // 2. login
    //
    // A fresh credential for the same account: `common::login_user` asserts the
    // `200`, this asserts that the token it returned authenticates the account
    // the journey registered, and that it is a new credential rather than a
    // replay of the registration's.
    // -----------------------------------------------------------------
    let token = login_user(&app, &owner).await;

    assert_ne!(
        token, owner.access_token,
        "a login must issue a new access token"
    );
    let logged_in = read_account(&app, &token).await;
    assert_eq!(
        logged_in, registered,
        "logging in must return the account the journey registered"
    );

    // -----------------------------------------------------------------
    // 3. a project of the caller
    // -----------------------------------------------------------------
    let project_id = create_project(&app, &owner, "Client visits").await;

    // -----------------------------------------------------------------
    // 4. a draft claim
    //
    // The title is padded and the currency is lower case: both are normalised
    // by the service, so the stored values are the assertion.
    // -----------------------------------------------------------------
    let response = call(
        &app,
        Method::POST,
        CLAIMS,
        &token,
        Some(json!({
            "title": "  Client visit to Berlin  ",
            "description": "Two days on site",
            "currency": "eur",
            "project_id": project_id,
        })),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);

    assert_claim_detail_shape(&created);
    assert_eq!(created["title"], "Client visit to Berlin");
    assert_eq!(created["description"], "Two days on site");
    assert_eq!(created["currency"], EUR);
    assert_eq!(created["status"], "draft");
    assert_eq!(
        assert_money(&created["total_amount"], "total_amount"),
        "0.0000",
        "a new draft has an empty total, as a money string"
    );
    assert_eq!(created["project_id"], project_id.to_string());
    assert!(
        created["submitted_at"].is_null(),
        "a draft carries no submission stamp: {created}"
    );
    assert!(
        created["decided_at"].is_null(),
        "a draft carries no decision stamp: {created}"
    );
    assert!(
        created["paid_at"].is_null(),
        "a draft carries no payment stamp: {created}"
    );
    assert_eq!(
        created["items"],
        json!([]),
        "a claim is created without items"
    );

    let claim_id = id_of(&created);
    let created_at = instant(&created, "created_at");
    let created_updated_at = instant(&created, "updated_at");
    assert!(
        created_updated_at >= created_at,
        "a claim cannot be modified before it existed ({created_updated_at} < {created_at})"
    );

    // -----------------------------------------------------------------
    // 5. three line items
    //
    // The expected total is computed here, from the amounts the client sent,
    // as scaled integers. It is never taken from a response.
    // -----------------------------------------------------------------
    let mut expected: i128 = 0;
    let mut posted: Vec<(String, &LineItem)> = Vec::new();

    for spec in &JOURNEY_ITEMS {
        let mut body = item_body(spec.description, spec.amount, spec.incurred_on);
        if let Some(category) = spec.category {
            body["category"] = json!(category);
        }
        if let Some(receipt_url) = spec.receipt_url {
            body["receipt_url"] = json!(receipt_url);
        }

        let response = call(
            &app,
            Method::POST,
            &items_path(&claim_id),
            &token,
            Some(body),
        )
        .await;
        let item = response.expect_status(StatusCode::CREATED);

        assert_eq!(
            fields_of(&item),
            ITEM_FIELDS,
            "an item response must publish exactly the documented fields: {item}"
        );
        assert_eq!(item["claim_id"], claim_id.as_str());
        assert_eq!(
            item["description"],
            spec.description.trim(),
            "a description is stored trimmed"
        );
        assert_eq!(assert_money(&item["amount"], "amount"), spec.amount);
        assert_eq!(item["incurred_on"], spec.incurred_on);

        match spec.category {
            Some(category) => assert_eq!(
                item["category"],
                category.trim().to_lowercase(),
                "a category is stored trimmed and lower-cased"
            ),
            None => assert!(
                item["category"].is_null(),
                "an absent category is stored as null: {item}"
            ),
        }
        match spec.receipt_url {
            Some(receipt_url) => assert_eq!(item["receipt_url"], receipt_url),
            None => assert!(
                item["receipt_url"].is_null(),
                "an absent receipt reference is stored as null: {item}"
            ),
        }

        let item_created_at = instant(&item, "created_at");
        assert!(
            instant(&item, "updated_at") >= item_created_at,
            "an item cannot be modified before it existed"
        );

        expected += scaled(spec.amount);
        posted.push((id_of(&item), spec));
    }

    assert_eq!(
        render_money(expected),
        "149.8001",
        "the journey's own arithmetic must be the three amounts it posted"
    );

    // -----------------------------------------------------------------
    // 6. read the claim: the stored total is the sum of the items
    // -----------------------------------------------------------------
    let read = read_claim(&app, &token, &claim_id).await;

    assert_claim_detail_shape(&read);
    assert_eq!(
        assert_money(&read["total_amount"], "total_amount"),
        render_money(expected),
        "the stored total must equal the sum the journey computed independently"
    );
    assert_eq!(
        assert_money(&read["total_amount"], "total_amount"),
        sum_of(&read["items"]),
        "the stored total must equal the sum of the items the server returned"
    );
    assert_eq!(
        read["items"].as_array().map(Vec::len),
        Some(JOURNEY_ITEMS.len()),
        "every posted item must be stored"
    );

    // The published order of the detail is `incurred_on, created_at, id`. The
    // items were posted out of date order, so this is a real assertion.
    let incurred: Vec<&str> = read["items"]
        .as_array()
        .expect("the detail carries an items array")
        .iter()
        .map(|item| item["incurred_on"].as_str().unwrap_or_default())
        .collect();
    let mut sorted = incurred.clone();
    sorted.sort_unstable();
    assert_eq!(
        incurred, sorted,
        "the detail must order items by incurred_on"
    );
    assert_eq!(
        incurred,
        ["2026-10-19", "2026-10-20", "2026-10-21"],
        "the detail must return the three items oldest first, whatever order they were posted in"
    );

    // The stored items are the ones the three POSTs returned, with the values
    // the client sent after normalisation.
    let mut returned_ids = ids_of(&read["items"]);
    returned_ids.sort_unstable();
    let mut posted_ids: Vec<String> = posted.iter().map(|(id, _)| id.clone()).collect();
    posted_ids.sort_unstable();
    assert_eq!(
        returned_ids, posted_ids,
        "the detail must return the items that were created"
    );

    for item in read["items"].as_array().expect("an items array") {
        let id = id_of(item);
        let spec = posted
            .iter()
            .find(|(posted_id, _)| posted_id == &id)
            .map(|(_, spec)| *spec)
            .unwrap_or_else(|| panic!("item {id} was not posted by this test"));
        assert_eq!(item["description"], spec.description.trim());
        assert_eq!(assert_money(&item["amount"], "amount"), spec.amount);
        assert_eq!(item["incurred_on"], spec.incurred_on);
    }

    assert_eq!(
        read["created_at"], created["created_at"],
        "created_at is stamped once and never changes"
    );
    let updated_at = instant(&read, "updated_at");
    assert!(
        updated_at >= created_updated_at,
        "adding items must not move updated_at backwards ({updated_at} < {created_updated_at})"
    );

    // -----------------------------------------------------------------
    // 7. submit: submitted_at is set, the other two stamps stay null
    // -----------------------------------------------------------------
    let submitted = transition(&app, &token, &claim_id, "submit").await;

    assert_claim_detail_shape(&submitted);
    assert_eq!(submitted["status"], "submitted");
    let submitted_at = instant(&submitted, "submitted_at");
    assert!(
        submitted["decided_at"].is_null(),
        "a submission must not stamp a decision: {submitted}"
    );
    assert!(
        submitted["paid_at"].is_null(),
        "a submission must not stamp a payment: {submitted}"
    );
    assert_eq!(
        assert_money(&submitted["total_amount"], "total_amount"),
        render_money(expected),
        "a transition must not disturb the total"
    );
    assert_eq!(submitted["created_at"], created["created_at"]);
    assert_eq!(submitted["items"].as_array().map(Vec::len), Some(3));

    // The stamp is stored, not merely echoed: read it back.
    let stored = read_claim(&app, &token, &claim_id).await;
    assert_eq!(stored["status"], "submitted");
    assert_eq!(
        stored["submitted_at"], submitted["submitted_at"],
        "the submission instant must be the one that was stored"
    );
    assert!(stored["decided_at"].is_null());

    // -----------------------------------------------------------------
    // 8. approve: decided_at is set, submitted_at is not restamped
    // -----------------------------------------------------------------
    let approved = transition(&app, &token, &claim_id, "approve").await;

    assert_claim_detail_shape(&approved);
    assert_eq!(approved["status"], "approved");
    let decided_at = instant(&approved, "decided_at");
    assert!(
        approved["paid_at"].is_null(),
        "an approval must not stamp a payment: {approved}"
    );
    assert_eq!(
        instant(&approved, "submitted_at"),
        submitted_at,
        "an approval must not move the submission instant"
    );
    assert!(
        decided_at >= submitted_at,
        "a decision cannot predate the submission ({decided_at} < {submitted_at})"
    );
    assert_eq!(
        assert_money(&approved["total_amount"], "total_amount"),
        render_money(expected)
    );
    assert_eq!(approved["created_at"], created["created_at"]);

    // -----------------------------------------------------------------
    // 9. pay: paid_at is set, the two earlier stamps are not restamped
    // -----------------------------------------------------------------
    let paid = transition(&app, &token, &claim_id, "pay").await;

    assert_claim_detail_shape(&paid);
    assert_eq!(paid["status"], "paid");
    let paid_at = instant(&paid, "paid_at");
    assert_eq!(
        instant(&paid, "submitted_at"),
        submitted_at,
        "a payment must not move the submission instant"
    );
    assert_eq!(
        instant(&paid, "decided_at"),
        decided_at,
        "a payment must not move the decision instant"
    );
    assert!(
        paid_at >= decided_at,
        "a payment cannot predate the decision ({paid_at} < {decided_at})"
    );
    assert_eq!(
        assert_money(&paid["total_amount"], "total_amount"),
        render_money(expected)
    );

    // The stored claim is the one the transition answered with, and its total
    // is still the independently computed sum.
    let paid_detail = read_claim(&app, &token, &claim_id).await;
    assert_eq!(paid_detail, paid, "the payment must be committed");
    assert_eq!(
        assert_money(&paid_detail["total_amount"], "total_amount"),
        sum_of(&paid_detail["items"]),
        "the total must still equal the sum of the items after the lifecycle"
    );
    assert_eq!(
        assert_money(&paid_detail["total_amount"], "total_amount"),
        render_money(expected)
    );
    assert_eq!(paid_detail["items"].as_array().map(Vec::len), Some(3));

    // -----------------------------------------------------------------
    // 10. paid is terminal
    //
    // Every further write is refused, and the refused writes do not change a
    // byte of the stored claim. The comparison is on the whole detail, so a
    // stamp that moved or an item that changed is caught even though the status
    // stayed the same.
    // -----------------------------------------------------------------
    let terminal = read_claim(&app, &token, &claim_id).await;
    let first_item_id = terminal["items"]
        .as_array()
        .and_then(|items| items.first())
        .map(id_of)
        .unwrap_or_else(|| panic!("the paid claim must still carry its items: {terminal}"));

    for action in ["submit", "approve", "reject", "pay"] {
        let response = call(
            &app,
            Method::POST,
            &action_path(&claim_id, action),
            &token,
            None,
        )
        .await;
        let body = response.expect_status(StatusCode::CONFLICT);

        assert_eq!(
            body["error"]["code"], "conflict",
            "`{action}` on a paid claim must be `409 conflict`: {body}"
        );

        let after = read_claim(&app, &token, &claim_id).await;
        assert_eq!(
            after, terminal,
            "a refused `{action}` must not change the paid claim"
        );
    }

    // The item writes are refused for the same reason: the claim is not a
    // draft any more.
    let item_attempts = [
        (
            "add an item",
            Method::POST,
            items_path(&claim_id),
            Some(item_body("Late line", "1.0000", "2026-10-22")),
        ),
        (
            "update an item",
            Method::PATCH,
            item_path(&claim_id, &first_item_id),
            Some(json!({ "amount": "1.0000" })),
        ),
        (
            "delete an item",
            Method::DELETE,
            item_path(&claim_id, &first_item_id),
            None,
        ),
    ];

    for (label, method, uri, body) in item_attempts {
        let response = call(&app, method, &uri, &token, body).await;
        let body = response.expect_status(StatusCode::CONFLICT);

        assert_eq!(
            body["error"]["code"], "conflict",
            "`{label}` on a paid claim must be `409 conflict`: {body}"
        );

        let after = read_claim(&app, &token, &claim_id).await;
        assert_eq!(
            after, terminal,
            "a refused `{label}` must not change the paid claim"
        );
    }

    // -----------------------------------------------------------------
    // 11. the listing
    // -----------------------------------------------------------------
    let page = list_claims(&app, &token, "").await;
    let page = page.expect_status(StatusCode::OK);

    assert_eq!(
        fields_of(&page),
        PAGE_FIELDS,
        "the listing must answer the standard envelope: {page}"
    );
    // Restricted to the account this test registered, so `total` is exact even
    // though the database holds every other test's claims.
    assert_pagination(&page, 1, 20, 0);
    assert_eq!(
        fields_of(&page["pagination"]),
        PAGINATION_FIELDS,
        "the pagination block must publish exactly its four fields: {page}"
    );

    let listed = &page["items"][0];
    assert_eq!(
        fields_of(listed),
        CLAIM_FIELDS,
        "a listed claim carries no items: {listed}"
    );
    assert_eq!(listed["status"], "paid");
    assert_eq!(
        assert_money(&listed["total_amount"], "total_amount"),
        render_money(expected),
        "the listing must report the stored total"
    );
    for field in CLAIM_FIELDS {
        assert_eq!(
            listed[field], terminal[field],
            "the listed claim must carry the stored `{field}`"
        );
    }

    // -----------------------------------------------------------------
    // 12. the second account
    // -----------------------------------------------------------------
    let intruder = register_user(&app, "journey-intruder").await;

    assert_foreign_sweep(&app, &intruder, &claim_id, &first_item_id, project_id).await;

    // After the sweep, the owner's claim is byte-identical and still readable.
    let after_sweep = read_claim(&app, &token, &claim_id).await;
    assert_eq!(
        after_sweep, terminal,
        "a second account must not be able to change the claim"
    );
}

// ---------------------------------------------------------------------------
// Task 2 - the ownership sweep on its own
// ---------------------------------------------------------------------------

/// Creates a claim with one item and walks it to `status` through the API, and
/// returns `(claim_id, item_id)`.
///
/// The state is built by the operations under test rather than by SQL, so a
/// lifecycle that cannot reach one of its own states fails here instead of
/// quietly making the sweep vacuous. `status` is one of the five published
/// values.
async fn claim_in_status(app: &Router, token: &str, status: &str) -> (String, String) {
    let created = create_claim(app, token, &format!("A {status} claim")).await;
    let claim_id = id_of(&created);

    let response = call(
        app,
        Method::POST,
        &items_path(&claim_id),
        token,
        Some(item_body("One line", "10.0000", "2026-03-01")),
    )
    .await;
    let item_id = id_of(&response.expect_status(StatusCode::CREATED));

    match status {
        "draft" => {}
        "submitted" => {
            transition(app, token, &claim_id, "submit").await;
        }
        "approved" => {
            transition(app, token, &claim_id, "submit").await;
            transition(app, token, &claim_id, "approve").await;
        }
        "rejected" => {
            transition(app, token, &claim_id, "submit").await;
            transition(app, token, &claim_id, "reject").await;
        }
        "paid" => {
            transition(app, token, &claim_id, "submit").await;
            transition(app, token, &claim_id, "approve").await;
            transition(app, token, &claim_id, "pay").await;
        }
        other => panic!("`{other}` is not one of the five published statuses"),
    }

    let claim = read_claim(app, token, &claim_id).await;
    assert_eq!(
        claim["status"], status,
        "the fixture must reach the {status} status"
    );
    assert_eq!(
        assert_money(&claim["total_amount"], "total_amount"),
        "10.0000",
        "the fixture must hold its one item"
    );
    assert_eq!(claim["items"].as_array().map(Vec::len), Some(1));

    (claim_id, item_id)
}

#[tokio::test]
async fn the_second_account_sees_not_found_on_every_reimbursement_operation() {
    let state = test_state().await;
    let app = app(state);

    let owner = register_user(&app, "sweep-owner").await;
    let other = register_user(&app, "sweep-other").await;
    let project_id = create_project(&app, &owner, "Owner's project").await;

    // One claim in each of the five states, each holding one item.
    let mut fixtures: Vec<(&str, String, String)> = Vec::new();
    for status in ["draft", "submitted", "approved", "rejected", "paid"] {
        let (claim_id, item_id) = claim_in_status(&app, &owner.access_token, status).await;
        fixtures.push((status, claim_id, item_id));
    }

    // The owner's rows, before the sweep.
    let mut before: Vec<Value> = Vec::new();
    for (status, claim_id, _) in &fixtures {
        let detail = read_claim(&app, &owner.access_token, claim_id).await;
        assert_eq!(detail["status"], *status);
        before.push(detail);
    }

    // The other account attempts every operation against every claim.
    for (_, claim_id, item_id) in &fixtures {
        assert_foreign_sweep(&app, &other, claim_id, item_id, project_id).await;
    }

    // The owner's five claims are byte-identical: same status, same total, same
    // item count, same stamps, same item identifiers.
    for ((status, claim_id, _), before_detail) in fixtures.iter().zip(&before) {
        let after = read_claim(&app, &owner.access_token, claim_id).await;

        assert_eq!(
            after["status"], *status,
            "a foreign attempt must not move the {status} claim"
        );
        assert_eq!(
            assert_money(&after["total_amount"], "total_amount"),
            assert_money(&before_detail["total_amount"], "total_amount"),
            "a foreign attempt must not change the total of the {status} claim"
        );
        assert_eq!(
            after["items"].as_array().map(Vec::len),
            before_detail["items"].as_array().map(Vec::len),
            "a foreign attempt must not change the item count of the {status} claim"
        );
        assert_eq!(
            after, *before_detail,
            "a foreign attempt must leave the {status} claim byte-identical"
        );
    }

    // The owner still sees all five, and the other account still sees none.
    let page = list_claims(&app, &owner.access_token, "").await;
    let page = page.expect_status(StatusCode::OK);
    assert_pagination(&page, 5, 20, 0);

    let page = list_claims(&app, &other.access_token, "").await;
    let page = page.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
    assert!(page["items"].as_array().is_some_and(Vec::is_empty));
}

// ---------------------------------------------------------------------------
// Task 3 - the seeded dataset, read through the API
// ---------------------------------------------------------------------------

/// Whether the seeder has written its accounts into the test database.
///
/// The seeder suite and this file share one database but run as separate test
/// binaries, so which of them ran first is not a property this test may depend
/// on. It asks the database instead of guessing, and it asks **before** logging
/// in so that the two outcomes stay distinct: "the seeder has not run here" is
/// a skip, while "the published development password no longer authenticates"
/// is a failure. A `401` alone cannot tell them apart, which is why the check
/// is not simply "try to log in".
async fn the_seeder_has_run() -> bool {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::test_database_url())
        .await
        .expect("the test database must be reachable");

    sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM users WHERE email = $1)")
        .bind(SEEDED_EMAIL)
        .fetch_one(&pool)
        .await
        .expect("the users table must be queryable")
}

#[tokio::test]
async fn the_seeded_claims_are_readable_through_the_api() {
    let state = test_state().await;
    let app = app(state);

    // -----------------------------------------------------------------
    // The deterministic half: a claim written through the API is read back
    // through the listing with the published pagination block.
    //
    // This needs no seeder run, and it is restricted to an account this test
    // registered, so it is exact against a database that holds every other
    // test's rows.
    // -----------------------------------------------------------------
    let user = register_user(&app, "flow-list").await;

    let first = create_claim(&app, &user.access_token, "First claim").await;
    let first_id = id_of(&first);
    let second = create_claim(&app, &user.access_token, "Second claim").await;
    let second_id = id_of(&second);
    assert_ne!(first_id, second_id);

    // A page of one, twice: the two claims are both there, in two different
    // pages, and the block says so. Which of the two is first is not asserted:
    // the order is `created_at DESC, id DESC`, and two claims created within
    // the same microsecond would be ordered by a random identifier.
    for (offset, expected_has_more) in [(0, true), (1, false)] {
        let page = list_claims(
            &app,
            &user.access_token,
            &format!("?limit=1&offset={offset}"),
        )
        .await;
        let page = page.expect_status(StatusCode::OK);

        assert_eq!(fields_of(&page), PAGE_FIELDS);
        assert_pagination(&page, 2, 1, offset);

        let items = page["items"].as_array().expect("an items array");
        assert_eq!(items.len(), 1, "a page of one holds exactly one claim");
        assert_eq!(
            page["pagination"]["has_more"], expected_has_more,
            "has_more must describe the rest of the set, not the page"
        );
        assert_eq!(
            fields_of(&items[0]),
            CLAIM_FIELDS,
            "a listed claim carries no items"
        );
    }

    let page = list_claims(&app, &user.access_token, "?limit=2&offset=0").await;
    let page = page.expect_status(StatusCode::OK);
    assert_pagination(&page, 2, 2, 0);

    let mut returned: Vec<String> = page["items"]
        .as_array()
        .expect("an items array")
        .iter()
        .map(id_of)
        .collect();
    returned.sort_unstable();
    let mut expected = vec![first_id, second_id];
    expected.sort_unstable();
    assert_eq!(
        returned, expected,
        "the listing must return the two claims the account created"
    );

    // -----------------------------------------------------------------
    // The seeded half, only when the seeder has run against this database.
    // -----------------------------------------------------------------
    // The half holds the cross-process seeder lock from *before* the
    // existence check to the end of the test. The minimal-profile case below
    // resets the shared database while it holds the same key, and a reset
    // that commits between this half's check and its login deletes the
    // account under it: the login's session insert then answers the
    // foreign-key violation the API maps to `500`. Serialising the two
    // halves on the seeder key (`ATOMSEED`) closes that window without
    // changing what either asserts; the assertions here are
    // profile-agnostic, so whichever profile ran last is the one read.
    let _seeder_lock = seeder_lock().await;

    if !the_seeder_has_run().await {
        eprintln!(
            "skipping the seeded half of `the_seeded_claims_are_readable_through_the_api`: \
             no account {SEEDED_EMAIL} in this database, so `atom-seed` has not run against it"
        );
        return;
    }

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": SEEDED_EMAIL, "password": SEEDED_PASSWORD })),
        ),
    )
    .await;
    let logged_in = response.expect_status(StatusCode::OK);

    assert_eq!(logged_in["user"]["email"], SEEDED_EMAIL);
    assert_eq!(
        logged_in["user"]["display_name"], SEEDED_DISPLAY_NAME,
        "the seeded account's display name is part of the published dataset"
    );
    assert_eq!(logged_in["user"]["is_active"], json!(true));
    uuid_of(&logged_in["user"], "id");

    let seeded_token = logged_in["tokens"]["access_token"]
        .as_str()
        .unwrap_or_else(|| panic!("the login must return an access token: {logged_in}"));

    // A page of the seeded owner's claims, whatever profile the seeder last
    // wrote: the suite shares one database, so the *size* of the seeded set is
    // not asserted - only that the envelope describes it truthfully.
    let page = list_claims(&app, seeded_token, "?limit=5&offset=0").await;
    let page = page.expect_status(StatusCode::OK);

    assert_eq!(fields_of(&page), PAGE_FIELDS);
    assert_eq!(fields_of(&page["pagination"]), PAGINATION_FIELDS);

    let total = page["pagination"]["total"]
        .as_i64()
        .unwrap_or_else(|| panic!("the pagination block must carry a total: {page}"));
    assert!(total >= 0, "a total cannot be negative: {page}");
    assert_eq!(page["pagination"]["limit"], 5);
    assert_eq!(page["pagination"]["offset"], 0);
    assert_eq!(
        page["pagination"]["has_more"],
        total > 5,
        "has_more must describe the rest of the set: {page}"
    );

    let items = page["items"].as_array().expect("an items array");
    assert_eq!(
        i64::try_from(items.len()).unwrap_or(-1),
        total.min(5),
        "a page of five holds five claims, or the whole set when it is smaller: {page}"
    );
    for item in items {
        assert_eq!(fields_of(item), CLAIM_FIELDS);
        assert_money(&item["total_amount"], "total_amount");
        assert!(
            matches!(
                item["status"].as_str(),
                Some("draft" | "submitted" | "approved" | "rejected" | "paid")
            ),
            "a seeded claim must be in one of the five published states: {item}"
        );
    }

    // The seeded total is read back through the API and checked against an
    // independent sum of the seeded items - the invariant `C-R1` makes the
    // seeder maintain, proven through the HTTP contract rather than through SQL.
    if let Some(first_claim) = items.first() {
        let claim_id = id_of(first_claim);
        let detail = read_claim(&app, seeded_token, &claim_id).await;

        assert_claim_detail_shape(&detail);
        assert_eq!(
            assert_money(&detail["total_amount"], "total_amount"),
            sum_of(&detail["items"]),
            "a seeded claim's stored total must equal the sum of its items"
        );
    }
}

// ---------------------------------------------------------------------------
// The guaranteed seeded read
// ---------------------------------------------------------------------------

/// The advisory-lock key every copy of the suite that drives the seeder uses.
///
/// The bytes spell `ATOMSEED` - the same key [`tests/seeder.rs`] holds for its
/// own runs - so two processes that drive the seeder cannot overlap even when
/// their test binaries do.
const SEEDER_LOCK_KEY: i64 = 0x4154_4f4d_5345_4544;

/// Holds the cross-process seeder lock for the caller's whole test.
///
/// The lock lives on a PostgreSQL session of its own, so PostgreSQL releases
/// it when the connection closes: a crashed test cannot leave it behind. It
/// serialises this file's seed run against `tests/seeder.rs`'s runs, which
/// hold the same key, and against the seeded half of
/// `the_seeded_claims_are_readable_through_the_api`, which takes it for the
/// same reason: no seeder run may reset the shared database while that half
/// is between its existence check and its login.
async fn seeder_lock() -> PgConnection {
    let mut session = PgConnection::connect(&common::test_database_url())
        .await
        .expect("the test database must be reachable for the seeder lock");

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SEEDER_LOCK_KEY)
        .execute(&mut session)
        .await
        .expect("the seeder advisory lock must be acquirable");

    session
}

/// The guarantee the opportunistic seeded read cannot make: this case runs
/// `atom-seed` itself, so the rows it asserts exist because this test put them
/// there (`testing.md` §3 item 1).
///
/// The invocation is the one `tests/seeder.rs` drives: the real binary, the
/// shared disposable database in `DATABASE_URL` and `APP_ENV=dev`. The
/// `--allow-non-empty` flag is what lets the run reach the data at all - the
/// suite's database holds every other test's accounts, and the seeder's `G3`
/// guard refuses a non-empty database without it. `--reset --profile minimal`
/// then makes the seeded set **exact** (`docs/database/seeding.md` §2 and
/// §7.4): one claim in total, and row `i` belongs to seeded user `i % 2`, so
/// the owner account holds that one claim. The profile's status cycle starts
/// at index 0, which is a `draft`, and the first claim of a profile carries no
/// items - so the published total is the empty sum, `"0.0000"`.
#[tokio::test]
async fn the_seeded_minimal_profile_is_readable_through_the_api() {
    let _lock = seeder_lock().await;

    let mut command = Command::new(env!("CARGO_BIN_EXE_atom-seed"));
    command
        .args(["--profile", "minimal", "--reset", "--allow-non-empty"])
        .env("DATABASE_URL", common::test_database_url())
        .env("APP_ENV", "dev");

    let output = command
        .output()
        .expect("the atom-seed binary must be runnable");
    assert!(
        output.status.success(),
        "the minimal seed run must exit 0: {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let state = test_state().await;
    let app = app(state);

    // The published development credential authenticates the account the run
    // just wrote.
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": SEEDED_EMAIL, "password": SEEDED_PASSWORD })),
        ),
    )
    .await;
    let logged_in = response.expect_status(StatusCode::OK);
    assert_eq!(logged_in["user"]["email"], SEEDED_EMAIL);

    let seeded_token = logged_in["tokens"]["access_token"]
        .as_str()
        .unwrap_or_else(|| panic!("the login must return an access token: {logged_in}"));

    // The listing is exactly the minimal profile's set for this account.
    let page = list_claims(&app, seeded_token, "?limit=5&offset=0").await;
    let page = page.expect_status(StatusCode::OK);

    assert_eq!(fields_of(&page), PAGE_FIELDS);
    assert_pagination(&page, 1, 5, 0);
    assert_eq!(
        page["pagination"]["has_more"], false,
        "one claim leaves nothing after the page: {page}"
    );

    let items = page["items"].as_array().expect("an items array");
    assert_eq!(
        items.len(),
        1,
        "the minimal profile seeds exactly one claim, and it belongs to this account: {page}"
    );
    assert_eq!(fields_of(&items[0]), CLAIM_FIELDS);
    assert_eq!(
        items[0]["status"], "draft",
        "the profile's status cycle starts at index 0, which is a draft: {page}"
    );
    assert_eq!(
        assert_money(&items[0]["total_amount"], "total_amount"),
        "0.0000",
        "the first claim of a profile is a draft with no items: {page}"
    );

    // The detail agrees with the listing, on a fresh read: no items, and the
    // stored total is the empty sum.
    let claim_id = id_of(&items[0]);
    let detail = read_claim(&app, seeded_token, &claim_id).await;

    assert_eq!(fields_of(&detail), CLAIM_DETAIL_FIELDS);
    assert_eq!(detail["status"], "draft");
    assert_eq!(
        detail["items"].as_array().map(Vec::len),
        Some(0),
        "the minimal profile's claim carries no items: {detail}"
    );
    assert_eq!(
        assert_money(&detail["total_amount"], "total_amount"),
        "0.0000"
    );
}

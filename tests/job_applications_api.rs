//! HTTP-level tests for the job application endpoints.
//!
//! Chains `J2`, `J3` and `J4` publish fourteen operations under
//! `/api/v1/companies` and `/api/v1/job-applications`: five company routes, five
//! application routes and four event routes. The published contract is
//! [`docs/features/job-applications/api.md`], and the rule the whole feature
//! turns on is the stage machine - seven stages, 49 ordered pairs, of which
//! **12** are allowed edges, **7** are accepted no-ops and **30** are refused.
//!
//! The file is grouped the way the contract groups it: companies, applications,
//! the stage machine, the cross-links to Phase 1, events, salary money,
//! ownership, and the shared `400`/`401`/`404`/`413`/`422` sweep that every one
//! of the fourteen operations owes.
//!
//! Three rules shape the file:
//!
//! * **Behaviour, not implementation.** Every assertion is on the status code
//!   and the observable body. Nothing asserts a log line, a timing value or a
//!   random value. The one structural assertion - the event order - is on the
//!   order of the response array, which is the published contract.
//! * **Independence.** The suite shares one real, disposable PostgreSQL
//!   database. Every test registers its own accounts, and every count is
//!   restricted to the ids that test created, so no test can see another's rows
//!   and no test truncates a table.
//! * **The mutating call is never the evidence.** Where the rule is about
//!   stored state (a refused transition wrote nothing, a cascade ran), the
//!   assertion is on a later read of the resource, never on the response that
//!   wrote it.
//!
//! Where a row cannot be produced through the API in this chain - an exact
//! `created_at`, so that the `created_at DESC, id DESC` order is decided rather
//! than hoped for - the fixture is written as SQL against the same database,
//! exactly as `tests/claims_api.rs` does. The assertion is still on the API
//! response: the SQL only builds the state, it never stands in for the
//! behaviour under test.

pub mod common;

use std::collections::BTreeMap;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::{DateTime, TimeZone, Utc};
use common::{
    TestResponse, app, assert_pagination, create_project, create_task, register_user, request,
    send, test_state, test_state_with_body_limit,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// The company collection path.
const COMPANIES: &str = "/api/v1/companies";

/// The application collection path.
const APPLICATIONS: &str = "/api/v1/job-applications";

/// The seven stages, in the order the contract lists them.
const ALL_STAGES: [&str; 7] = [
    "wishlist",
    "applied",
    "screening",
    "interview",
    "offer",
    "rejected",
    "withdrawn",
];

/// The terminal stages: no move out of any of them is accepted.
const TERMINAL_STAGES: [&str; 3] = ["offer", "rejected", "withdrawn"];

/// The twelve permitted edges of the stage machine.
///
/// The four forward edges and `rejected`/`withdrawn` from each of the four
/// non-terminal stages. `allow_dead_code` would be a lie: the test asserts the
/// length, so a table that lost a row fails.
const ALLOWED_EDGES: [(&str, &str); 12] = [
    ("wishlist", "applied"),
    ("wishlist", "rejected"),
    ("wishlist", "withdrawn"),
    ("applied", "screening"),
    ("applied", "rejected"),
    ("applied", "withdrawn"),
    ("screening", "interview"),
    ("screening", "rejected"),
    ("screening", "withdrawn"),
    ("interview", "offer"),
    ("interview", "rejected"),
    ("interview", "withdrawn"),
];

/// The thirty refused pairs: everything that is neither an edge nor a self-pair.
const REFUSED_EDGES: [(&str, &str); 30] = [
    // a skipped stage
    ("wishlist", "screening"),
    ("wishlist", "interview"),
    ("wishlist", "offer"),
    // backwards
    ("applied", "wishlist"),
    ("applied", "interview"),
    ("applied", "offer"),
    ("screening", "wishlist"),
    ("screening", "applied"),
    ("screening", "offer"),
    ("interview", "wishlist"),
    ("interview", "applied"),
    ("interview", "screening"),
    // every move out of a terminal stage, including between two terminals
    ("offer", "wishlist"),
    ("offer", "applied"),
    ("offer", "screening"),
    ("offer", "interview"),
    ("offer", "rejected"),
    ("offer", "withdrawn"),
    ("rejected", "wishlist"),
    ("rejected", "applied"),
    ("rejected", "screening"),
    ("rejected", "interview"),
    ("rejected", "offer"),
    ("rejected", "withdrawn"),
    ("withdrawn", "wishlist"),
    ("withdrawn", "applied"),
    ("withdrawn", "screening"),
    ("withdrawn", "interview"),
    ("withdrawn", "offer"),
    ("withdrawn", "rejected"),
];

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

/// The body of a minimal company create request.
fn company_body(name: &str) -> Value {
    json!({ "name": name })
}

/// Posts a company and returns the raw response.
async fn post_company(app: &Router, token: &str, body: Value) -> TestResponse {
    send(
        app,
        request(Method::POST, COMPANIES, Some(token), Some(body)),
    )
    .await
}

/// Creates a company through the API and returns the `201` body.
async fn create_company(app: &Router, token: &str, name: &str) -> Value {
    post_company(app, token, company_body(name))
        .await
        .expect_status(StatusCode::CREATED)
}

/// Reads one company and returns the raw response.
async fn get_company(app: &Router, token: &str, company_id: &str) -> TestResponse {
    send(
        app,
        request(
            Method::GET,
            &format!("{COMPANIES}/{company_id}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Patches one company and returns the raw response.
async fn patch_company(app: &Router, token: &str, company_id: &str, body: Value) -> TestResponse {
    send(
        app,
        request(
            Method::PATCH,
            &format!("{COMPANIES}/{company_id}"),
            Some(token),
            Some(body),
        ),
    )
    .await
}

/// Deletes one company and returns the raw response.
async fn delete_company(app: &Router, token: &str, company_id: &str) -> TestResponse {
    send(
        app,
        request(
            Method::DELETE,
            &format!("{COMPANIES}/{company_id}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Lists companies with the given query string (including its leading `?`).
async fn list_companies(app: &Router, token: &str, query: &str) -> TestResponse {
    send(
        app,
        request(
            Method::GET,
            &format!("{COMPANIES}{query}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Posts an application and returns the raw response.
async fn post_application(app: &Router, token: &str, body: Value) -> TestResponse {
    send(
        app,
        request(Method::POST, APPLICATIONS, Some(token), Some(body)),
    )
    .await
}

/// Creates a wishlist application through the API and returns the `201` body.
async fn create_application(
    app: &Router,
    token: &str,
    company_id: &str,
    role_title: &str,
) -> Value {
    post_application(
        app,
        token,
        json!({ "company_id": company_id, "role_title": role_title }),
    )
    .await
    .expect_status(StatusCode::CREATED)
}

/// Reads one application and returns the raw response.
async fn get_application(app: &Router, token: &str, application_id: &str) -> TestResponse {
    send(
        app,
        request(
            Method::GET,
            &format!("{APPLICATIONS}/{application_id}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Patches one application and returns the raw response.
async fn patch_application(
    app: &Router,
    token: &str,
    application_id: &str,
    body: Value,
) -> TestResponse {
    send(
        app,
        request(
            Method::PATCH,
            &format!("{APPLICATIONS}/{application_id}"),
            Some(token),
            Some(body),
        ),
    )
    .await
}

/// Deletes one application and returns the raw response.
async fn delete_application(app: &Router, token: &str, application_id: &str) -> TestResponse {
    send(
        app,
        request(
            Method::DELETE,
            &format!("{APPLICATIONS}/{application_id}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Lists applications with the given query string (including its leading `?`).
async fn list_applications(app: &Router, token: &str, query: &str) -> TestResponse {
    send(
        app,
        request(
            Method::GET,
            &format!("{APPLICATIONS}{query}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// The path of one application's event collection.
fn events_path(application_id: &str) -> String {
    format!("{APPLICATIONS}/{application_id}/events")
}

/// The path of one event inside its application.
fn event_path(application_id: &str, event_id: &str) -> String {
    format!("{APPLICATIONS}/{application_id}/events/{event_id}")
}

/// The body of a minimal event create request.
fn event_body(kind: &str, occurred_at: &str, summary: &str) -> Value {
    json!({ "kind": kind, "occurred_at": occurred_at, "summary": summary })
}

/// Posts an event and returns the raw response.
async fn post_event(app: &Router, token: &str, application_id: &str, body: Value) -> TestResponse {
    send(
        app,
        request(
            Method::POST,
            &events_path(application_id),
            Some(token),
            Some(body),
        ),
    )
    .await
}

/// Adds an event through the API and returns the `201` body.
async fn create_event(
    app: &Router,
    token: &str,
    application_id: &str,
    kind: &str,
    occurred_at: &str,
    summary: &str,
) -> Value {
    post_event(
        app,
        token,
        application_id,
        event_body(kind, occurred_at, summary),
    )
    .await
    .expect_status(StatusCode::CREATED)
}

/// Lists one application's events with the given query string.
async fn list_events(app: &Router, token: &str, application_id: &str, query: &str) -> TestResponse {
    send(
        app,
        request(
            Method::GET,
            &format!("{}{query}", events_path(application_id)),
            Some(token),
            None,
        ),
    )
    .await
}

/// Patches one event and returns the raw response.
async fn patch_event(
    app: &Router,
    token: &str,
    application_id: &str,
    event_id: &str,
    body: Value,
) -> TestResponse {
    send(
        app,
        request(
            Method::PATCH,
            &event_path(application_id, event_id),
            Some(token),
            Some(body),
        ),
    )
    .await
}

/// Deletes one event and returns the raw response.
async fn delete_event(
    app: &Router,
    token: &str,
    application_id: &str,
    event_id: &str,
) -> TestResponse {
    send(
        app,
        request(
            Method::DELETE,
            &event_path(application_id, event_id),
            Some(token),
            None,
        ),
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
/// `60000` is a contract violation even if it is numerically equal.
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

/// The public identifier of a created resource.
fn id_of(resource: &Value) -> String {
    resource["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the response must carry an id: {resource}"))
        .to_string()
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

/// The company names of a list response, in the order the server returned them.
fn names(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the listing must be an array envelope: {body}"))
        .iter()
        .map(|item| item["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The role titles of an application list response, in the server's order.
fn titles(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the listing must be an array envelope: {body}"))
        .iter()
        .map(|item| item["role_title"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The summaries of an event list response, in the server's order.
fn summaries(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the listing must be an array envelope: {body}"))
        .iter()
        .map(|item| item["summary"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Parses an RFC 3339 timestamp from a response body.
fn instant(resource: &Value, field: &str) -> DateTime<Utc> {
    let text = resource[field].as_str().unwrap_or_else(|| {
        panic!(
            "{field} must be an RFC 3339 string, but it is {}",
            resource[field]
        )
    });

    DateTime::parse_from_rfc3339(text)
        .unwrap_or_else(|error| panic!("{field} must be RFC 3339 ({text:?}): {error}"))
        .with_timezone(&Utc)
}

/// A calendar instant, for the fixtures.
fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap_or_else(|| panic!("{year}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"))
}

/// Pins an application's creation instant, so that a list order is decided.
///
/// The API never accepts `created_at` from a client, which is correct, so the
/// only way to observe the `created_at DESC, id DESC` order is to build the
/// instant directly. This is the single SQL fixture in the file: it writes the
/// state, the assertion is still on the API's answer.
async fn pin_created_at(db: &PgPool, application_id: &str, instant: DateTime<Utc>) {
    let id = Uuid::parse_str(application_id)
        .unwrap_or_else(|error| panic!("`{application_id}` must be a UUID: {error}"));

    sqlx::query("UPDATE job_applications SET created_at = $2 WHERE id = $1")
        .bind(id)
        .bind(instant)
        .execute(db)
        .await
        .unwrap_or_else(|error| panic!("the created_at fixture must be written: {error}"));
}

// ---------------------------------------------------------------------------
// Stage helpers
// ---------------------------------------------------------------------------

/// Patches an application's stage and returns the raw response.
async fn patch_stage(app: &Router, token: &str, application_id: &str, stage: &str) -> TestResponse {
    patch_application(app, token, application_id, json!({ "status": stage })).await
}

/// Fills every optional field of an application, while it is still a wishlist.
///
/// The self-pair test asserts that a no-op "changes nothing", which is only
/// meaningful if there is something to change: an application whose optional
/// fields are all null could lose one and still pass a careless comparison.
async fn decorate(app: &Router, token: &str, application_id: &str) -> Value {
    let response = patch_application(
        app,
        token,
        application_id,
        json!({
            "source": "referral",
            "applied_on": "2026-10-02",
            "salary_min": "60000.0000",
            "salary_max": "75000.0000",
            "salary_currency": "EUR",
            "job_url": "https://acme.example.test/jobs/42",
            "description": "Platform team, Rust and PostgreSQL.",
        }),
    )
    .await;

    response.expect_status(StatusCode::OK)
}

/// The route a stage is reached by, walking only permitted edges.
fn walk_to(stage: &str) -> &'static [&'static str] {
    match stage {
        "wishlist" => &[],
        "applied" => &["applied"],
        "screening" => &["applied", "screening"],
        "interview" => &["applied", "screening", "interview"],
        "offer" => &["applied", "screening", "interview", "offer"],
        "rejected" => &["rejected"],
        "withdrawn" => &["withdrawn"],
        other => panic!("`{other}` is not one of the seven published stages"),
    }
}

/// Creates an application and walks it to `stage` through the public API.
///
/// The state is built by the operations under test rather than by SQL, so a
/// machine that cannot reach one of its own stages fails here instead of
/// quietly making a transition test vacuous.
async fn application_in_stage(
    app: &Router,
    token: &str,
    company_id: &str,
    role_title: &str,
    stage: &str,
) -> String {
    let created = create_application(app, token, company_id, role_title).await;
    let application_id = id_of(&created);

    decorate(app, token, &application_id).await;

    for next in walk_to(stage) {
        patch_stage(app, token, &application_id, next)
            .await
            .expect_status(StatusCode::OK);
    }

    let stored = get_application(app, token, &application_id)
        .await
        .expect_status(StatusCode::OK);
    assert_eq!(
        stored["status"], stage,
        "the fixture must reach the {stage} stage, but it is {}",
        stored["status"]
    );

    application_id
}

/// The fields of an `ApplicationResponse` that a refused write must not touch.
const APPLICATION_FIELDS: [&str; 14] = [
    "id",
    "company_id",
    "role_title",
    "source",
    "status",
    "applied_on",
    "salary_min",
    "salary_max",
    "salary_currency",
    "job_url",
    "description",
    "next_action_task_id",
    "created_at",
    "updated_at",
];

// ---------------------------------------------------------------------------
// 1 - companies: create, read, change, delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_company_can_be_created_read_changed_and_deleted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-crud").await;

    // create
    let response = post_company(
        &app,
        &user.access_token,
        json!({
            "name": "Acme GmbH",
            "website": "https://acme.example.test",
            "notes": "Referred by Dana",
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);

    let company_id = id_of(&created);
    assert_eq!(created["name"], "Acme GmbH");
    assert_eq!(created["website"], "https://acme.example.test");
    assert_eq!(created["notes"], "Referred by Dana");
    assert!(created["created_at"].is_string());
    assert!(created["updated_at"].is_string());
    assert!(
        created.get("user_id").is_none(),
        "user_id must not be published: {created}"
    );

    // read
    let response = get_company(&app, &user.access_token, &company_id).await;
    let fetched = response.expect_status(StatusCode::OK);
    assert_eq!(fetched["id"], company_id);
    assert_eq!(fetched["name"], "Acme GmbH");

    // update: an omitted field is left untouched, `null` clears the website
    let response = patch_company(
        &app,
        &user.access_token,
        &company_id,
        json!({ "notes": "Second interview scheduled", "website": null }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert_eq!(updated["notes"], "Second interview scheduled");
    assert!(
        updated["website"].is_null(),
        "`website: null` must clear the link: {updated}"
    );
    assert_eq!(
        updated["name"], "Acme GmbH",
        "an omitted name must not be rewritten"
    );

    // the optional fields default rather than being required
    let response = post_company(&app, &user.access_token, json!({ "name": "Globex" })).await;
    let minimal = response.expect_status(StatusCode::CREATED);
    assert!(minimal["website"].is_null());
    assert_eq!(minimal["notes"], "");

    // delete
    delete_company(&app, &user.access_token, &company_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    let response = get_company(&app, &user.access_token, &company_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = list_companies(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(names(&page), vec!["Globex"]);
    assert_pagination(&page, 1, 20, 0);
}

#[tokio::test]
async fn a_company_name_is_unique_per_owner_ignoring_case() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-unique").await;

    create_company(&app, &user.access_token, "Acme").await;

    // Every casing of the same folded name collides, and the stored casing of
    // the first row is what `GET` keeps returning.
    for duplicate in ["Acme", "acme", "ACME", "  acme  "] {
        let response = post_company(&app, &user.access_token, company_body(duplicate)).await;
        let body = response.expect_status(StatusCode::CONFLICT);
        assert_error(&body, "conflict");
    }

    let response = list_companies(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(
        names(&page),
        vec!["Acme"],
        "a refused duplicate must not be stored, and the casing of the first one survives"
    );
    assert_pagination(&page, 1, 20, 0);
}

#[tokio::test]
async fn renaming_a_company_onto_an_existing_name_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-rename").await;

    let acme = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let globex = id_of(&create_company(&app, &user.access_token, "Globex").await);

    // A collision is a conflict whatever the casing.
    for collision in ["Acme", "acme", "ACME"] {
        let response = patch_company(
            &app,
            &user.access_token,
            &globex,
            json!({ "name": collision }),
        )
        .await;
        let body = response.expect_status(StatusCode::CONFLICT);
        assert_error(&body, "conflict");
    }

    // The refused renames wrote nothing.
    let response = get_company(&app, &user.access_token, &globex).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(unchanged["name"], "Globex");

    // Renaming a company onto its own name is not a collision: the unique index
    // is on `(user_id, LOWER(name))`, and this row is the row it already names.
    let response = patch_company(
        &app,
        &user.access_token,
        &globex,
        json!({ "name": "Globex" }),
    )
    .await;
    let same = response.expect_status(StatusCode::OK);
    assert_eq!(same["name"], "Globex");

    // The same rule holds for a case-only rewrite of its own name.
    let response = patch_company(
        &app,
        &user.access_token,
        &globex,
        json!({ "name": "GLOBEX" }),
    )
    .await;
    let recased = response.expect_status(StatusCode::OK);
    assert_eq!(
        recased["name"].as_str().map(str::to_lowercase).as_deref(),
        Some("globex"),
        "a case-only rename of the company's own name must be accepted: {recased}"
    );

    // Acme is untouched by all of it.
    let response = get_company(&app, &user.access_token, &acme).await;
    let kept = response.expect_status(StatusCode::OK);
    assert_eq!(kept["name"], "Acme");
}

#[tokio::test]
async fn two_accounts_may_use_the_same_company_name() {
    let state = test_state().await;
    let app = app(state);
    let first = register_user(&app, "company-share-a").await;
    let second = register_user(&app, "company-share-b").await;

    create_company(&app, &first.access_token, "Acme").await;
    let response = post_company(&app, &second.access_token, company_body("Acme")).await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["name"], "Acme");

    let response = list_companies(&app, &first.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 1, 20, 0);
}

#[tokio::test]
async fn a_company_name_is_trimmed_and_bounded() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-name-shape").await;

    // Trimmed before it is stored and before it is measured.
    let at_bound = "n".repeat(200);
    let response = post_company(
        &app,
        &user.access_token,
        company_body(&format!("   {at_bound}   ")),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(
        created["name"], at_bound,
        "the stored name must be the trimmed value"
    );

    // A blank name is a validation failure, and it is never a 409.
    for blank in ["", "   "] {
        let response = post_company(&app, &user.access_token, company_body(blank)).await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "name");
    }

    // One character past the bound.
    let response = post_company(&app, &user.access_token, company_body(&"n".repeat(201))).await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "name");

    // The same rules on a rename.
    let other = id_of(&create_company(&app, &user.access_token, "Globex").await);
    let response = patch_company(&app, &user.access_token, &other, json!({ "name": "   " })).await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "name");
}

#[tokio::test]
async fn a_company_website_must_be_absolute_http_or_https() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-website").await;

    for website in [
        "acme.example.test",
        "ftp://acme.example.test",
        "data:text/html,hello",
        "//acme.example.test",
    ] {
        let response = post_company(
            &app,
            &user.access_token,
            json!({ "name": format!("Co {website}"), "website": website }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "website");
    }

    let response = post_company(
        &app,
        &user.access_token,
        json!({ "name": "Good Co", "website": "http://acme.example.test" }),
    )
    .await;
    response.expect_status(StatusCode::CREATED);
}

#[tokio::test]
async fn a_company_notes_are_bounded() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-notes").await;

    let response = post_company(
        &app,
        &user.access_token,
        json!({ "name": "Bounded notes", "notes": "d".repeat(5_000) }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["notes"].as_str().map(str::len), Some(5_000));

    let response = post_company(
        &app,
        &user.access_token,
        json!({ "name": "Overlong notes", "notes": "d".repeat(5_001) }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "notes");
}

#[tokio::test]
async fn a_company_with_applications_cannot_be_deleted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-guarded").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Guarded Co").await);
    let application = create_application(&app, &user.access_token, &company_id, "Backend").await;
    let application_id = id_of(&application);

    // 409, not 204 and not a cascade.
    let response = delete_company(&app, &user.access_token, &company_id).await;
    let body = response.expect_status(StatusCode::CONFLICT);
    assert_error(&body, "conflict");

    // The evidence is what follows: nothing was deleted to make the delete
    // succeed. The company is still readable and the application is intact.
    let response = get_company(&app, &user.access_token, &company_id).await;
    let kept = response.expect_status(StatusCode::OK);
    assert_eq!(kept["name"], "Guarded Co");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let kept = response.expect_status(StatusCode::OK);
    assert_eq!(kept["company_id"], company_id);
    assert_eq!(kept["role_title"], "Backend");

    let response = list_applications(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![application_id]);
}

#[tokio::test]
async fn a_company_can_be_deleted_once_its_applications_are_gone() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-deletable").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Doomed Co").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Backend engineer").await);

    delete_application(&app, &user.access_token, &application_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    let response = delete_company(&app, &user.access_token, &company_id).await;
    let body = response.expect_status(StatusCode::NO_CONTENT);
    assert!(
        body.is_null(),
        "a 204 must carry no body; this one carried {body}"
    );

    let response = get_company(&app, &user.access_token, &company_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = list_companies(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

#[tokio::test]
async fn the_companies_listing_is_alphabetical_and_paginated() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-order").await;

    // Inserted out of order on purpose: the order must come from the published
    // `lower(name), name, id` rule, not from insertion order.
    for name in ["Zeta", "acme", "Beta"] {
        create_company(&app, &user.access_token, name).await;
    }

    let response = list_companies(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(
        names(&page),
        vec!["acme", "Beta", "Zeta"],
        "the listing must be ordered by lower(name)"
    );
    assert_pagination(&page, 3, 20, 0);

    let response = list_companies(&app, &user.access_token, "?limit=1&offset=1").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(names(&page), vec!["Beta"]);
    assert_pagination(&page, 3, 1, 1);
    assert_eq!(page["pagination"]["has_more"], true);
}

#[tokio::test]
async fn the_companies_listing_searches_by_name() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "company-search").await;

    create_company(&app, &user.access_token, "Acme GmbH").await;
    create_company(&app, &user.access_token, "Globex").await;

    for term in ["acm", "ACME", "Acme"] {
        let response = list_companies(&app, &user.access_token, &format!("?search={term}")).await;
        let page = response.expect_status(StatusCode::OK);
        assert_eq!(
            names(&page),
            vec!["Acme GmbH"],
            "the search must be a case-insensitive substring of the name, for `{term}`"
        );
        assert_pagination(&page, 1, 20, 0);
    }

    // No match is an empty page, not a 404.
    let response = list_companies(&app, &user.access_token, "?search=nothing").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);

    // An empty term means "no filter".
    let response = list_companies(&app, &user.access_token, "?search=").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 2, 20, 0);

    // Over the published bound of 200 characters.
    let term = "x".repeat(201);
    let response = list_companies(&app, &user.access_token, &format!("?search={term}")).await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "search");
}

// ---------------------------------------------------------------------------
// 2 - applications: create, read, change, delete, list
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_application_can_be_created_read_changed_and_deleted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-crud").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    // create: every published field, in one body
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "  Backend engineer  ",
            "source": "referral",
            "status": "wishlist",
            "applied_on": "2026-10-02",
            "salary_min": "60000.0000",
            "salary_max": "75000.0000",
            "salary_currency": "EUR",
            "job_url": "https://acme.example.test/jobs/42",
            "description": "Platform team, Rust and PostgreSQL.",
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);

    let application_id = id_of(&created);
    assert_eq!(created["company_id"], company_id);
    assert_eq!(
        created["role_title"], "Backend engineer",
        "a role title is trimmed before it is stored"
    );
    assert_eq!(created["status"], "wishlist");
    assert_eq!(created["source"], "referral");
    assert_eq!(created["applied_on"], "2026-10-02");
    assert_eq!(
        assert_money(&created["salary_min"], "salary_min"),
        "60000.0000"
    );
    assert_eq!(
        assert_money(&created["salary_max"], "salary_max"),
        "75000.0000"
    );
    assert_eq!(created["salary_currency"], "EUR");
    assert_eq!(created["job_url"], "https://acme.example.test/jobs/42");
    assert_eq!(
        created["description"],
        "Platform team, Rust and PostgreSQL."
    );
    assert!(created["next_action_task_id"].is_null());
    assert!(created["created_at"].is_string());
    assert!(created["updated_at"].is_string());
    assert!(
        created.get("user_id").is_none(),
        "user_id must not be published: {created}"
    );

    // the defaults
    let response = post_application(
        &app,
        &user.access_token,
        json!({ "company_id": company_id, "role_title": "Minimal" }),
    )
    .await;
    let minimal = response.expect_status(StatusCode::CREATED);
    assert_eq!(
        minimal["status"], "wishlist",
        "an omitted stage defaults to wishlist"
    );
    assert!(minimal["source"].is_null());
    assert!(minimal["applied_on"].is_null());
    assert!(minimal["salary_min"].is_null());
    assert!(minimal["salary_max"].is_null());
    assert!(minimal["salary_currency"].is_null());
    assert!(minimal["job_url"].is_null());
    assert_eq!(minimal["description"], "");

    // read
    let response = get_application(&app, &user.access_token, &application_id).await;
    let fetched = response.expect_status(StatusCode::OK);
    assert_eq!(fetched["id"], application_id);
    assert_eq!(fetched["role_title"], "Backend engineer");

    // update: an omitted field is untouched, `null` clears the nullable ones
    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "source": null, "applied_on": null, "job_url": null }),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert!(updated["source"].is_null());
    assert!(updated["applied_on"].is_null());
    assert!(updated["job_url"].is_null());
    assert_eq!(
        updated["salary_min"], "60000.0000",
        "clearing another field must not touch the salary"
    );
    assert_eq!(updated["role_title"], "Backend engineer");
    assert_eq!(
        updated["description"],
        "Platform team, Rust and PostgreSQL."
    );

    // delete
    delete_application(&app, &user.access_token, &application_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    let response = get_application(&app, &user.access_token, &application_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // the company survives its application
    let response = get_company(&app, &user.access_token, &company_id).await;
    response.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn a_create_with_a_stage_other_than_wishlist_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-birth").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    // The machine cannot be bypassed at birth: six of the seven stages are
    // refused, so `wishlist` cannot become `offer` in one request.
    for stage in [
        "applied",
        "screening",
        "interview",
        "offer",
        "rejected",
        "withdrawn",
    ] {
        let response = post_application(
            &app,
            &user.access_token,
            json!({
                "company_id": company_id,
                "role_title": format!("born {stage}"),
                "status": stage,
            }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "status");
    }

    // `wishlist` is accepted, and so is an omitted stage.
    let response = post_application(
        &app,
        &user.access_token,
        json!({ "company_id": company_id, "role_title": "explicit", "status": "wishlist" }),
    )
    .await;
    let explicit = response.expect_status(StatusCode::CREATED);
    assert_eq!(explicit["status"], "wishlist");

    let response = post_application(
        &app,
        &user.access_token,
        json!({ "company_id": company_id, "role_title": "omitted" }),
    )
    .await;
    let omitted = response.expect_status(StatusCode::CREATED);
    assert_eq!(omitted["status"], "wishlist");

    // Only the two accepted creates exist: every refusal wrote nothing.
    let response = list_applications(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 2, 20, 0);
    let mut returned = titles(&page);
    returned.sort();
    assert_eq!(returned, vec!["explicit", "omitted"]);
}

#[tokio::test]
async fn a_role_title_is_trimmed_and_bounded() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-title").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    for blank in ["", "   "] {
        let response = post_application(
            &app,
            &user.access_token,
            json!({ "company_id": company_id, "role_title": blank }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "role_title");
    }

    let response = post_application(
        &app,
        &user.access_token,
        json!({ "company_id": company_id, "role_title": "x".repeat(201) }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "role_title");

    // 200 characters, surrounded by spaces, is accepted and stored trimmed.
    let at_bound = "x".repeat(200);
    let response = post_application(
        &app,
        &user.access_token,
        json!({ "company_id": company_id, "role_title": format!("   {at_bound}   ") }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["role_title"], at_bound);

    // The same rule on PATCH.
    let application_id = id_of(&created);
    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "role_title": "   " }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "role_title");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(
        unchanged["role_title"], at_bound,
        "a refused PATCH must not have changed the title"
    );
}

#[tokio::test]
async fn an_application_date_must_be_a_date() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-date").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    for value in ["2026-02-30", "2026-13-01", "02-10-2026", "tomorrow"] {
        let response = post_application(
            &app,
            &user.access_token,
            json!({
                "company_id": company_id,
                "role_title": "Dated",
                "applied_on": value,
            }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);

        // The violation names `body`, not `applied_on`: none of these values
        // ever becomes a date, so the failure is the published
        // "well-formed JSON that does not match the contract" case
        // ([`docs/api/error-contract.md`] §4), which carries one violation for
        // the field `body`. The status is what the endpoint table pins; the
        // field name follows the error contract's own rule.
        assert_error(&body, "validation_failed");
        assert!(
            body["error"]["details"]
                .as_array()
                .is_some_and(|details| !details.is_empty()),
            "a refused date must name the field that failed: {body}"
        );
    }
}

#[tokio::test]
async fn a_job_url_must_be_an_http_url() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-url").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    for value in [
        "data:text/html,hi",
        "acme.example.test/jobs/1",
        "ftp://x.example.test",
    ] {
        let response = post_application(
            &app,
            &user.access_token,
            json!({
                "company_id": company_id,
                "role_title": "Linked",
                "job_url": value,
            }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "job_url");
    }
}

#[tokio::test]
async fn an_unknown_stage_in_a_body_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-stage-shape").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Backend").await);

    // An unknown value is a business rule broken by a well-formed body: 422,
    // never 409 and never 400.
    let response = patch_stage(&app, &user.access_token, &application_id, "hired").await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(unchanged["status"], "wishlist");
}

#[tokio::test]
async fn the_applications_listing_is_newest_first() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "application-order").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    // Four applications, two of them sharing a creation instant, so the
    // identifier is the only tie-breaker the published order allows.
    let base = at(2026, 5, 1, 0, 0, 0);

    let oldest = id_of(&create_application(&app, &user.access_token, &company_id, "oldest").await);
    let tie_low = id_of(&create_application(&app, &user.access_token, &company_id, "tie").await);
    let tie_high = id_of(&create_application(&app, &user.access_token, &company_id, "tie").await);
    let newest = id_of(&create_application(&app, &user.access_token, &company_id, "newest").await);

    pin_created_at(&db, &oldest, base).await;
    pin_created_at(&db, &tie_low, base + chrono::Duration::seconds(10)).await;
    pin_created_at(&db, &tie_high, base + chrono::Duration::seconds(10)).await;
    pin_created_at(&db, &newest, base + chrono::Duration::seconds(20)).await;

    // The tie is decided by `id DESC`, so the expectation is derived from the
    // identifiers the server assigned rather than guessed.
    let mut tie = [tie_low, tie_high];
    tie.sort();
    tie.reverse();

    let response = list_applications(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);

    assert_eq!(
        ids(&page),
        vec![newest, tie[0].clone(), tie[1].clone(), oldest],
        "the listing must be ordered by created_at DESC, then id DESC"
    );
    assert_pagination(&page, 4, 20, 0);
}

#[tokio::test]
async fn the_applications_listing_filters_by_stage_company_and_dates() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-filter").await;

    let alpha = id_of(&create_company(&app, &user.access_token, "Alpha Ltd").await);
    let beta = id_of(&create_company(&app, &user.access_token, "Beta GmbH").await);

    // Four applications, each distinguished by one filter dimension.
    post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": alpha,
            "role_title": "Backend engineer",
            "applied_on": "2026-10-02",
        }),
    )
    .await
    .expect_status(StatusCode::CREATED);
    let interview =
        id_of(&create_application(&app, &user.access_token, &alpha, "Backend engineer").await);
    for stage in ["applied", "screening", "interview"] {
        patch_stage(&app, &user.access_token, &interview, stage)
            .await
            .expect_status(StatusCode::OK);
    }

    post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": alpha,
            "role_title": "Frontend engineer",
            "applied_on": "2026-10-20",
        }),
    )
    .await
    .expect_status(StatusCode::CREATED);

    // A `null` `applied_on`, which any date bound must exclude.
    create_application(&app, &user.access_token, &beta, "Data engineer").await;

    post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": beta,
            "role_title": "Office manager",
            "applied_on": "2026-09-15",
        }),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let company_filter = format!("?company_id={alpha}");

    let cases: [(&str, &[&str]); 6] = [
        (
            "",
            &[
                "Backend engineer",
                "Backend engineer",
                "Frontend engineer",
                "Data engineer",
                "Office manager",
            ],
        ),
        ("?status=interview", &["Backend engineer"]),
        (
            company_filter.as_str(),
            &["Backend engineer", "Backend engineer", "Frontend engineer"],
        ),
        (
            "?search=engineer",
            &[
                "Backend engineer",
                "Backend engineer",
                "Frontend engineer",
                "Data engineer",
            ],
        ),
        (
            "?applied_from=2026-10-01&applied_to=2026-10-31",
            &["Backend engineer", "Frontend engineer"],
        ),
        ("?applied_to=2026-10-01", &["Office manager"]),
    ];

    for (query, expected) in cases {
        let response = list_applications(&app, &user.access_token, query).await;
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

    // A filter that matches nothing is an empty page, not a 404.
    let response = list_applications(&app, &user.access_token, "?status=withdrawn").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);

    // The stage filter is an enum in the query string: outside it is a 400,
    // because the query string could not be parsed.
    let response = list_applications(&app, &user.access_token, "?status=hired").await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");

    // A `company_id` that is not a UUID is the same parse failure.
    let response = list_applications(&app, &user.access_token, "?company_id=not-a-uuid").await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");

    // A reversed date range is a business rule on a parsed value: 422.
    let response = list_applications(
        &app,
        &user.access_token,
        "?applied_from=2026-10-31&applied_to=2026-10-01",
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "applied_from");
}

// ---------------------------------------------------------------------------
// 3 - the stage machine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_permitted_stage_change_is_accepted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "stage-allowed").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Machine Co").await);

    // The table is pinned: 12 edges + 7 self-pairs + 30 refusals = 49 pairs.
    assert_eq!(ALLOWED_EDGES.len(), 12);
    assert_eq!(REFUSED_EDGES.len(), 30);
    assert_eq!(
        ALLOWED_EDGES.len() + REFUSED_EDGES.len() + ALL_STAGES.len(),
        ALL_STAGES.len() * ALL_STAGES.len(),
        "the three tables must partition every ordered pair of stages"
    );

    for (index, (from, to)) in ALLOWED_EDGES.iter().enumerate() {
        assert_ne!(from, to, "an edge is not a self-pair");

        let application_id = application_in_stage(
            &app,
            &user.access_token,
            &company_id,
            &format!("edge {index}"),
            from,
        )
        .await;

        let response = patch_stage(&app, &user.access_token, &application_id, to).await;
        let moved = response.expect_status(StatusCode::OK);

        assert_eq!(
            moved["status"], *to,
            "the {from} -> {to} edge must be accepted, but the response says {}",
            moved["status"]
        );
        assert_eq!(
            moved["id"], application_id,
            "the move must answer with the application it moved"
        );

        // The stored row agrees with the response.
        let stored = get_application(&app, &user.access_token, &application_id)
            .await
            .expect_status(StatusCode::OK);
        assert_eq!(
            stored["status"], *to,
            "the {from} -> {to} edge must be stored, not only answered"
        );
    }
}

#[tokio::test]
async fn sending_the_current_stage_is_accepted_and_changes_nothing() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "stage-self").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Machine Co").await);

    // The seven self-pairs are no-ops, not refusals. An implementation that
    // treats "the stage changed" as "the pair is one of the twelve edges" answers
    // `409` here, and this is the only test that can see it: the twelve-edge
    // test alone passes for that implementation.
    for (index, stage) in ALL_STAGES.iter().enumerate() {
        let application_id = application_in_stage(
            &app,
            &user.access_token,
            &company_id,
            &format!("self {index}"),
            stage,
        )
        .await;

        let before = get_application(&app, &user.access_token, &application_id)
            .await
            .expect_status(StatusCode::OK);
        assert_eq!(before["status"], *stage);

        let response = patch_stage(&app, &user.access_token, &application_id, stage).await;
        let after = response.expect_status(StatusCode::OK);

        assert_eq!(
            after["status"], *stage,
            "re-sending the current stage ({stage}) must be a 200 no-op, not a transition"
        );

        // Every field is untouched, `updated_at` included: the no-op is
        // accepted and changes nothing, which is the contract's wording for the
        // seven self-pairs.
        for field in APPLICATION_FIELDS {
            assert_eq!(
                after[field], before[field],
                "re-sending the current stage ({stage}) must not change `{field}`"
            );
        }

        // The stored row is the same no-op.
        let stored = get_application(&app, &user.access_token, &application_id)
            .await
            .expect_status(StatusCode::OK);
        for field in APPLICATION_FIELDS {
            assert_eq!(
                stored[field], before[field],
                "the stored `{field}` must be unchanged after re-sending {stage}"
            );
        }
    }
}

#[tokio::test]
async fn every_forbidden_stage_change_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "stage-refused").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Machine Co").await);

    // One application per stage: a refusal cannot move a stage, so each fixture
    // is reused for every refused pair that starts at its stage.
    let mut fixtures: BTreeMap<&str, String> = BTreeMap::new();
    for stage in ALL_STAGES {
        let application_id = application_in_stage(
            &app,
            &user.access_token,
            &company_id,
            &format!("from {stage}"),
            stage,
        )
        .await;
        fixtures.insert(stage, application_id);
    }

    for (from, to) in REFUSED_EDGES {
        let application_id = fixtures
            .get(from)
            .unwrap_or_else(|| panic!("no fixture application for the {from} stage"));

        let before = get_application(&app, &user.access_token, application_id)
            .await
            .expect_status(StatusCode::OK);
        assert_eq!(
            before["status"], from,
            "the fixture for {from} -> {to} must start in {from}"
        );

        let response = patch_stage(&app, &user.access_token, application_id, to).await;
        let body = response.expect_status(StatusCode::CONFLICT);
        assert_error(&body, "conflict");

        // The refusal is real: the stage did not move, and no timestamp moved
        // either, because the row was never written.
        let after = get_application(&app, &user.access_token, application_id)
            .await
            .expect_status(StatusCode::OK);

        assert_eq!(
            after["status"], from,
            "{from} -> {to} must be refused, but the stage became {}",
            after["status"]
        );
        assert_eq!(
            after["created_at"], before["created_at"],
            "a refused {from} -> {to} move must not rewrite created_at"
        );
        assert_eq!(
            after["updated_at"], before["updated_at"],
            "a refused {from} -> {to} move must not stamp updated_at"
        );
    }
}

#[tokio::test]
async fn offer_rejected_and_withdrawn_are_terminal() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "stage-terminal").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Machine Co").await);

    for (index, terminal) in TERMINAL_STAGES.iter().enumerate() {
        let application_id = application_in_stage(
            &app,
            &user.access_token,
            &company_id,
            &format!("terminal {index}"),
            terminal,
        )
        .await;

        for target in ALL_STAGES {
            let before = get_application(&app, &user.access_token, &application_id)
                .await
                .expect_status(StatusCode::OK);
            assert_eq!(before["status"], *terminal);

            let response = patch_stage(&app, &user.access_token, &application_id, target).await;

            if target == *terminal {
                // Re-sending the terminal stage itself is still a no-op, not an
                // exit and not a refusal.
                let body = response.expect_status(StatusCode::OK);
                assert_eq!(
                    body["status"], *terminal,
                    "re-sending the terminal stage {terminal} must be a no-op"
                );
            } else {
                let body = response.expect_status(StatusCode::CONFLICT);
                assert_error(&body, "conflict");
            }

            let after = get_application(&app, &user.access_token, &application_id)
                .await
                .expect_status(StatusCode::OK);
            assert_eq!(
                after["status"], *terminal,
                "{terminal} is terminal: `{target}` must not move it, but it became {}",
                after["status"]
            );
        }
    }
}

#[tokio::test]
async fn a_refused_stage_change_does_not_write_the_other_fields() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "stage-transaction").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Machine Co").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Original title").await);

    let before = get_application(&app, &user.access_token, &application_id)
        .await
        .expect_status(StatusCode::OK);

    // The move is refused; the other field in the same body must not land,
    // because the request is one transaction.
    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "status": "interview", "role_title": "Changed title" }),
    )
    .await;
    let body = response.expect_status(StatusCode::CONFLICT);
    assert_error(&body, "conflict");

    let after = get_application(&app, &user.access_token, &application_id)
        .await
        .expect_status(StatusCode::OK);

    assert_eq!(
        after["role_title"], before["role_title"],
        "a refused stage move must not write the other fields of the same request"
    );
    assert_eq!(after["status"], "wishlist");
    assert_eq!(after["updated_at"], before["updated_at"]);
}

// ---------------------------------------------------------------------------
// 4 - the cross-links to Phase 1
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_next_action_task_must_belong_to_the_caller() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-task").await;
    let other = register_user(&app, "application-task-other").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    let own_project = create_project(&app, &user, "Own project").await;
    let own_task = create_task(&app, &user, own_project, "Send the CV").await;

    let other_project = create_project(&app, &other, "Other project").await;
    let other_task = create_task(&app, &other, other_project, "Not the caller's").await;

    // A foreign task on create is 404, and nothing is created.
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "Intruder",
            "next_action_task_id": other_task,
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = list_applications(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);

    // The caller's own task is accepted.
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "Own task",
            "next_action_task_id": own_task,
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["next_action_task_id"], own_task.to_string());
    let application_id = id_of(&created);

    // A foreign task on PATCH is the same answer, and the link does not move.
    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "next_action_task_id": other_task }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(
        unchanged["next_action_task_id"],
        own_task.to_string(),
        "a refused link must not clear or replace the existing one"
    );

    // A task that does not exist at all is the same answer.
    let missing = Uuid::new_v4();
    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "next_action_task_id": missing }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");
}

#[tokio::test]
async fn deleting_the_task_clears_the_next_action() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-task-delete").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let project_id = create_project(&app, &user, "Own project").await;
    let task_id = create_task(&app, &user, project_id, "Send the CV").await;

    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "Linked",
            "next_action_task_id": task_id,
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["next_action_task_id"], task_id.to_string());
    let application_id = id_of(&created);

    // Deleting the task is Phase 1's operation; the application must survive it
    // with the link cleared, because the reminder is not the record.
    send(
        &app,
        request(
            Method::DELETE,
            &format!("/api/v1/tasks/{task_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NO_CONTENT);

    let response = get_application(&app, &user.access_token, &application_id).await;
    let after = response.expect_status(StatusCode::OK);

    assert!(
        after["next_action_task_id"].is_null(),
        "deleting the task must clear the link: {after}"
    );
    assert_eq!(
        after["role_title"], "Linked",
        "the application must survive its task"
    );
    assert_eq!(after["status"], "wishlist");
}

#[tokio::test]
async fn an_application_can_be_moved_to_another_of_the_callers_companies() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-company-move").await;

    let first = id_of(&create_company(&app, &user.access_token, "First Co").await);
    let second = id_of(&create_company(&app, &user.access_token, "Second Co").await);

    let created = create_application(&app, &user.access_token, &first, "Backend").await;
    let application_id = id_of(&created);
    assert_eq!(created["company_id"], first);

    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "company_id": second }),
    )
    .await;
    let moved = response.expect_status(StatusCode::OK);
    assert_eq!(moved["company_id"], second);

    let response = get_application(&app, &user.access_token, &application_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["company_id"], second);

    // The old company is now free to delete, and the new one is not.
    let response = delete_company(&app, &user.access_token, &first).await;
    response.expect_status(StatusCode::NO_CONTENT);

    let response = delete_company(&app, &user.access_token, &second).await;
    let body = response.expect_status(StatusCode::CONFLICT);
    assert_error(&body, "conflict");
}

// ---------------------------------------------------------------------------
// 5 - events
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_event_can_be_added_changed_and_deleted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "event-crud").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Backend").await);

    // create
    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        json!({
            "kind": "interview",
            "occurred_at": "2026-10-14T13:00:00Z",
            "summary": "  Technical interview  ",
            "details": "Two exercises, Rust and SQL.",
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);

    let event_id = id_of(&created);
    assert_eq!(created["application_id"], application_id);
    assert_eq!(created["kind"], "interview");
    assert_eq!(
        created["summary"], "Technical interview",
        "a summary is trimmed before it is stored"
    );
    assert_eq!(created["details"], "Two exercises, Rust and SQL.");
    assert_eq!(
        instant(&created, "occurred_at"),
        at(2026, 10, 14, 13, 0, 0),
        "the event must carry the instant that was sent"
    );
    assert!(created["created_at"].is_string());
    assert!(
        created.get("updated_at").is_none(),
        "an event has no updated_at (A-D1-1): {created}"
    );
    assert!(
        created.get("user_id").is_none(),
        "user_id must not be published: {created}"
    );

    // The optional field defaults rather than being required.
    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        event_body("note", "2026-10-15T09:00:00Z", "Follow up"),
    )
    .await;
    let minimal = response.expect_status(StatusCode::CREATED);
    assert_eq!(minimal["details"], "");

    // update: one field, then the rest
    let response = patch_event(
        &app,
        &user.access_token,
        &application_id,
        &event_id,
        json!({ "summary": "Technical interview — rescheduled" }),
    )
    .await;
    let patched = response.expect_status(StatusCode::OK);
    assert_eq!(patched["summary"], "Technical interview — rescheduled");
    assert_eq!(
        patched["kind"], "interview",
        "an omitted kind must not be rewritten"
    );
    assert_eq!(
        patched["occurred_at"], created["occurred_at"],
        "an omitted occurred_at must not be rewritten"
    );
    assert_eq!(patched["details"], "Two exercises, Rust and SQL.");
    assert!(
        patched.get("updated_at").is_none(),
        "a corrected event still carries no updated_at: {patched}"
    );

    let response = patch_event(
        &app,
        &user.access_token,
        &application_id,
        &event_id,
        json!({
            "kind": "call",
            "occurred_at": "2026-10-15T09:00:00+02:00",
            "details": "Rescheduled by phone.",
        }),
    )
    .await;
    let moved = response.expect_status(StatusCode::OK);
    assert_eq!(moved["kind"], "call");
    assert_eq!(
        instant(&moved, "occurred_at"),
        at(2026, 10, 15, 7, 0, 0),
        "an offset is normalised to UTC, and the instant is preserved"
    );
    assert_eq!(moved["details"], "Rescheduled by phone.");
    assert_eq!(
        moved["summary"], "Technical interview — rescheduled",
        "an omitted summary must not be cleared"
    );
    assert!(moved.get("updated_at").is_none());

    // The correction is stored, and the creation instant is not rewritten.
    let response = list_events(&app, &user.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 2, 20, 0);
    let stored = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == event_id)
        .unwrap_or_else(|| panic!("the corrected event must be listed: {page}"));
    assert_eq!(stored["kind"], "call");
    assert_eq!(stored["created_at"], created["created_at"]);

    // delete
    let body = delete_event(&app, &user.access_token, &application_id, &event_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);
    assert!(
        body.is_null(),
        "a 204 must carry no body; this one carried {body}"
    );

    let response = list_events(&app, &user.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![id_of(&minimal)]);
    assert_pagination(&page, 1, 20, 0);
}

#[tokio::test]
async fn events_are_ordered_by_occurred_at() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "event-order").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Backend").await);

    // Posted out of order on purpose: the response order must come from the
    // published rule, never from insertion order.
    let last = id_of(
        &create_event(
            &app,
            &user.access_token,
            &application_id,
            "note",
            "2026-05-03T10:00:00Z",
            "last",
        )
        .await,
    );
    let first = id_of(
        &create_event(
            &app,
            &user.access_token,
            &application_id,
            "note",
            "2026-05-01T10:00:00Z",
            "first",
        )
        .await,
    );
    let tie_a = create_event(
        &app,
        &user.access_token,
        &application_id,
        "note",
        "2026-05-02T10:00:00Z",
        "tie",
    )
    .await;
    let tie_b = create_event(
        &app,
        &user.access_token,
        &application_id,
        "note",
        "2026-05-02T10:00:00Z",
        "tie",
    )
    .await;

    // Two events at the same instant are ordered by `id DESC`; the expectation
    // is derived from the identifiers the server assigned.
    let mut tie = [id_of(&tie_a), id_of(&tie_b)];
    tie.sort();
    tie.reverse();

    let response = list_events(&app, &user.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);

    assert_eq!(
        summaries(&page),
        vec!["last", "tie", "tie", "first"],
        "the events must be ordered by occurred_at DESC"
    );
    assert_eq!(
        ids(&page),
        vec![last, tie[0].clone(), tie[1].clone(), first],
        "two events at the same instant must be ordered by id DESC"
    );
    assert_pagination(&page, 4, 20, 0);

    // The page bounds are the shared ones.
    let response = list_events(&app, &user.access_token, &application_id, "?limit=2").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(page["pagination"]["has_more"], true);
    assert_eq!(summaries(&page), vec!["last", "tie"]);
}

#[tokio::test]
async fn an_event_with_a_future_occurred_at_is_accepted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "event-future").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Backend").await);

    // "occurred" means "belongs at this point on the timeline": an interview is
    // scheduled before it happens, so there is no past-only rule.
    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        event_body("interview", "2099-01-05T09:00:00Z", "Scheduled interview"),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(
        instant(&created, "occurred_at"),
        at(2099, 1, 5, 9, 0, 0),
        "a future instant is stored as it was sent"
    );
}

#[tokio::test]
async fn an_event_kind_is_one_of_six() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "event-kind").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Backend").await);

    for kind in ["note", "email", "call", "interview", "offer", "rejection"] {
        let response = post_event(
            &app,
            &user.access_token,
            &application_id,
            event_body(kind, "2026-10-14T13:00:00Z", &format!("a {kind}")),
        )
        .await;
        let created = response.expect_status(StatusCode::CREATED);
        assert_eq!(created["kind"], kind);
    }

    // A kind outside the vocabulary is a well-formed body breaking the
    // contract: 422, never 400.
    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        event_body("meeting", "2026-10-14T13:00:00Z", "not a kind"),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    // An application stage is not an event kind, even where the words overlap.
    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        event_body("screening", "2026-10-14T13:00:00Z", "not a kind"),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    let response = list_events(&app, &user.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 6, 20, 0);
}

#[tokio::test]
async fn an_event_summary_is_required_and_bounded() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "event-summary").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Backend").await);

    for blank in ["", "   "] {
        let response = post_event(
            &app,
            &user.access_token,
            &application_id,
            event_body("note", "2026-10-14T13:00:00Z", blank),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "summary");
    }

    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        event_body("note", "2026-10-14T13:00:00Z", &"s".repeat(201)),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "summary");

    // The details are bounded too.
    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        json!({
            "kind": "note",
            "occurred_at": "2026-10-14T13:00:00Z",
            "summary": "Long details",
            "details": "d".repeat(10_001),
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "details");
}

#[tokio::test]
async fn an_event_occurred_at_requires_a_timezone() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "event-timezone").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Backend").await);

    // A local time with no offset is not an instant, so the body does not match
    // the contract.
    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        event_body("call", "2026-10-14T13:00:00", "No offset"),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    let response = post_event(
        &app,
        &user.access_token,
        &application_id,
        event_body("call", "yesterday", "Not a timestamp"),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    let response = list_events(&app, &user.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

#[tokio::test]
async fn an_event_id_cannot_be_used_under_another_application() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "event-wrong-parent").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let first = id_of(&create_application(&app, &user.access_token, &company_id, "First").await);
    let second = id_of(&create_application(&app, &user.access_token, &company_id, "Second").await);

    let event_id = id_of(
        &create_event(
            &app,
            &user.access_token,
            &first,
            "note",
            "2026-10-14T13:00:00Z",
            "Belongs to the first",
        )
        .await,
    );

    // The event exists and is the caller's, but not under this parent: 404, and
    // it is neither moved nor returned.
    let response = patch_event(
        &app,
        &user.access_token,
        &second,
        &event_id,
        json!({ "summary": "hijacked" }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = delete_event(&app, &user.access_token, &second, &event_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The event is untouched and still under the application it belongs to.
    let response = list_events(&app, &user.access_token, &first, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![event_id]);
    assert_eq!(summaries(&page), vec!["Belongs to the first"]);

    let response = list_events(&app, &user.access_token, &second, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

#[tokio::test]
async fn deleting_an_application_removes_its_events() {
    let state = test_state().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "event-cascade").await;
    let other = register_user(&app, "event-cascade-other").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Doomed").await);

    for index in 0..2 {
        create_event(
            &app,
            &user.access_token,
            &application_id,
            "note",
            "2026-10-14T13:00:00Z",
            &format!("entry {index}"),
        )
        .await;
    }

    // A second account's application with its own event, so the cascade is
    // shown to be scoped and not a table-wide delete.
    let other_company = id_of(&create_company(&app, &other.access_token, "Other Co").await);
    let kept_application =
        id_of(&create_application(&app, &other.access_token, &other_company, "Kept").await);
    let kept_event = id_of(
        &create_event(
            &app,
            &other.access_token,
            &kept_application,
            "note",
            "2026-10-14T13:00:00Z",
            "kept",
        )
        .await,
    );

    delete_application(&app, &user.access_token, &application_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    // The events endpoint answers 404 because the parent is gone...
    let response = list_events(&app, &user.access_token, &application_id, "").await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // ...and the rows really are gone, which the 404 alone would not prove.
    let application_uuid = Uuid::parse_str(&application_id)
        .unwrap_or_else(|error| panic!("`{application_id}` must be a UUID: {error}"));
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM application_events WHERE application_id = $1")
            .bind(application_uuid)
            .fetch_one(&db)
            .await
            .expect("the event count must be readable");
    assert_eq!(
        remaining, 0,
        "deleting an application must delete its events, but {remaining} survived"
    );

    // The other account's timeline is untouched.
    let response = list_events(&app, &other.access_token, &kept_application, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![kept_event]);
}

// ---------------------------------------------------------------------------
// 6 - salary money
// ---------------------------------------------------------------------------

#[tokio::test]
async fn salary_money_round_trips_with_four_decimals() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "application-money").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    for (minimum, maximum) in [
        ("60000.0000", "75000.0000"),
        ("0.0000", "0.0000"),
        ("999999999999999.9999", "999999999999999.9999"),
    ] {
        let response = post_application(
            &app,
            &user.access_token,
            json!({
                "company_id": company_id,
                "role_title": format!("range {minimum}"),
                "salary_min": minimum,
                "salary_max": maximum,
                "salary_currency": "EUR",
            }),
        )
        .await;
        let created = response.expect_status(StatusCode::CREATED);

        // Byte-identical on the way out, through a different operation.
        let application_id = id_of(&created);
        let response = get_application(&app, &user.access_token, &application_id).await;
        let stored = response.expect_status(StatusCode::OK);

        assert_eq!(
            assert_money(&stored["salary_min"], "salary_min"),
            minimum,
            "the stored minimum must be the value that was sent, at scale four"
        );
        assert_eq!(
            assert_money(&stored["salary_max"], "salary_max"),
            maximum,
            "the stored maximum must be the value that was sent, at scale four"
        );
        assert_eq!(stored["salary_currency"], "EUR");
    }

    // A bound may stand alone, with a currency.
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "Open ended",
            "salary_min": "60000.0000",
            "salary_currency": "EUR",
        }),
    )
    .await;
    let one_sided = response.expect_status(StatusCode::CREATED);
    assert_eq!(
        assert_money(&one_sided["salary_min"], "salary_min"),
        "60000.0000"
    );
    assert!(
        one_sided["salary_max"].is_null(),
        "an absent maximum must stay null: {one_sided}"
    );

    // Clearing both bounds clears the currency, because a currency with no
    // bound is refused.
    let application_id = id_of(&one_sided);
    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "salary_min": null }),
    )
    .await;
    let cleared = response.expect_status(StatusCode::OK);
    assert!(cleared["salary_min"].is_null());
    assert!(
        cleared["salary_currency"].is_null(),
        "clearing the last bound must clear the currency: {cleared}"
    );
}

#[tokio::test]
async fn a_salary_bound_without_a_currency_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "salary-currency-missing").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    for body in [
        json!({ "company_id": company_id, "role_title": "min only", "salary_min": "60000.0000" }),
        json!({ "company_id": company_id, "role_title": "max only", "salary_max": "75000.0000" }),
        json!({
            "company_id": company_id,
            "role_title": "both",
            "salary_min": "60000.0000",
            "salary_max": "75000.0000",
        }),
    ] {
        let response = post_application(&app, &user.access_token, body).await;
        // The client must see a 422 rather than a `23514` surfacing as a 500:
        // the schema holds the same rule, and both halves have to refuse.
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_error(&body, "validation_failed");
    }

    let response = list_applications(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

#[tokio::test]
async fn a_salary_currency_without_a_bound_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "salary-currency-lonely").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    // A currency whose number is missing says nothing about the number.
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "currency only",
            "salary_currency": "EUR",
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    // The same on PATCH, and the stored row keeps its bounds.
    let application = create_application(&app, &user.access_token, &company_id, "Bounded").await;
    let application_id = id_of(&application);

    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({
            "salary_min": "60000.0000",
            "salary_max": "75000.0000",
            "salary_currency": "EUR",
        }),
    )
    .await;
    response.expect_status(StatusCode::OK);

    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "salary_currency": "USD", "salary_min": null, "salary_max": null }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(unchanged["salary_currency"], "EUR");
    assert_eq!(
        assert_money(&unchanged["salary_min"], "salary_min"),
        "60000.0000",
        "a refused salary patch must not clear the bounds"
    );
}

#[tokio::test]
async fn an_inverted_salary_range_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "salary-range").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "inverted",
            "salary_min": "80000.0000",
            "salary_max": "75000.0000",
            "salary_currency": "EUR",
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    // Equal bounds are a single value, not an inverted range.
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "equal",
            "salary_min": "75000.0000",
            "salary_max": "75000.0000",
            "salary_currency": "EUR",
        }),
    )
    .await;
    response.expect_status(StatusCode::CREATED);

    // Inverting them with a PATCH is the same refusal, and the pair stays put.
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "pair").await);
    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({
            "salary_min": "60000.0000",
            "salary_max": "75000.0000",
            "salary_currency": "EUR",
        }),
    )
    .await;
    response.expect_status(StatusCode::OK);

    let response = patch_application(
        &app,
        &user.access_token,
        &application_id,
        json!({ "salary_min": "90000.0000" }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(
        assert_money(&unchanged["salary_min"], "salary_min"),
        "60000.0000"
    );
    assert_eq!(
        assert_money(&unchanged["salary_max"], "salary_max"),
        "75000.0000"
    );
}

#[tokio::test]
async fn a_salary_currency_is_three_upper_case_letters() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "salary-currency-shape").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    for currency in ["EU", "EURO", "E1R", "EU R"] {
        let response = post_application(
            &app,
            &user.access_token,
            json!({
                "company_id": company_id,
                "role_title": format!("bad {currency}"),
                "salary_min": "1.0000",
                "salary_currency": currency,
            }),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_error(&body, "validation_failed");
    }

    // The schema stores the code upper case; a lower-case code is accepted and
    // normalised, so the client never sees a code it did not send back.
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "lower case",
            "salary_min": "1.0000",
            "salary_currency": "eur",
        }),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["salary_currency"], "EUR");
}

#[tokio::test]
async fn a_json_number_where_a_salary_is_expected_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "salary-number").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);

    // The wire form is a string at scale four (ADR-0007). A number in that
    // position does not match the contract, so the body is refused.
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "numeric salary",
            "salary_min": 60000,
            "salary_currency": "EUR",
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    // Two decimals are a different shape of the same mistake: the server
    // refuses rather than rounding.
    let response = post_application(
        &app,
        &user.access_token,
        json!({
            "company_id": company_id,
            "role_title": "short scale",
            "salary_min": "60000.00",
            "salary_currency": "EUR",
        }),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    let response = list_applications(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

// ---------------------------------------------------------------------------
// 7 - ownership: the second account gets 404, never 403 and never 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_company_of_another_user_is_invisible() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "company-owner").await;
    let other = register_user(&app, "company-other").await;

    let company_id = id_of(&create_company(&app, &owner.access_token, "Private Co").await);
    let application_id =
        id_of(&create_application(&app, &owner.access_token, &company_id, "Private").await);

    // READ
    let response = get_company(&app, &other.access_token, &company_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // UPDATE
    let response = patch_company(
        &app,
        &other.access_token,
        &company_id,
        json!({ "name": "hijacked" }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // DELETE: refused because the company is not the caller's, not because it
    // still has an application. The order matters: `409` here would leak that
    // the company exists.
    let response = delete_company(&app, &other.access_token, &company_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The owner's company and application are untouched.
    let response = get_company(&app, &owner.access_token, &company_id).await;
    let kept = response.expect_status(StatusCode::OK);
    assert_eq!(kept["name"], "Private Co");

    let response = get_application(&app, &owner.access_token, &application_id).await;
    response.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn an_application_of_another_user_is_invisible() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "application-owner").await;
    let other = register_user(&app, "application-other").await;

    let company_id = id_of(&create_company(&app, &owner.access_token, "Private Co").await);
    let application_id =
        id_of(&create_application(&app, &owner.access_token, &company_id, "Private").await);
    let event_id = id_of(
        &create_event(
            &app,
            &owner.access_token,
            &application_id,
            "note",
            "2026-10-14T13:00:00Z",
            "Private entry",
        )
        .await,
    );

    // READ
    let response = get_application(&app, &other.access_token, &application_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // UPDATE: including a legal transition, which must not move another
    // account's stage.
    let response = patch_stage(&app, &other.access_token, &application_id, "applied").await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // DELETE
    let response = delete_application(&app, &other.access_token, &application_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // EVENTS: every one of the four
    let response = list_events(&app, &other.access_token, &application_id, "").await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = post_event(
        &app,
        &other.access_token,
        &application_id,
        event_body("note", "2026-10-15T13:00:00Z", "intruder"),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = patch_event(
        &app,
        &other.access_token,
        &application_id,
        &event_id,
        json!({ "summary": "hijacked" }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = delete_event(&app, &other.access_token, &application_id, &event_id).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The owner's application still holds its one event, in its own stage.
    let response = get_application(&app, &owner.access_token, &application_id).await;
    let kept = response.expect_status(StatusCode::OK);
    assert_eq!(kept["status"], "wishlist");
    assert_eq!(kept["role_title"], "Private");

    let response = list_events(&app, &owner.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![event_id]);
}

#[tokio::test]
async fn an_application_cannot_target_another_users_company() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "application-target-owner").await;
    let other = register_user(&app, "application-target-other").await;

    let owner_company = id_of(&create_company(&app, &owner.access_token, "Owner Co").await);
    let other_company = id_of(&create_company(&app, &other.access_token, "Other Co").await);

    // CREATE with a foreign company: 404, not 403 and not 422.
    let response = post_application(
        &app,
        &other.access_token,
        json!({ "company_id": owner_company, "role_title": "Intruder" }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // PATCH with a foreign company: also 404, and the link does not move.
    let created = create_application(&app, &other.access_token, &other_company, "Own").await;
    let application_id = id_of(&created);

    let response = patch_application(
        &app,
        &other.access_token,
        &application_id,
        json!({ "company_id": owner_company }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // A company that does not exist at all is the same answer.
    let missing = Uuid::new_v4();
    let response = patch_application(
        &app,
        &other.access_token,
        &application_id,
        json!({ "company_id": missing }),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    let response = get_application(&app, &other.access_token, &application_id).await;
    let unchanged = response.expect_status(StatusCode::OK);
    assert_eq!(
        unchanged["company_id"], other_company,
        "a refused company change must not have been written: {unchanged}"
    );
    assert_eq!(unchanged["role_title"], "Own");
    assert_eq!(unchanged["status"], "wishlist");
}

#[tokio::test]
async fn the_listings_only_contain_the_callers_rows() {
    let state = test_state().await;
    let app = app(state);
    let first = register_user(&app, "listing-a").await;
    let second = register_user(&app, "listing-b").await;

    let first_company = id_of(&create_company(&app, &first.access_token, "First Co").await);
    let second_company = id_of(&create_company(&app, &second.access_token, "Second Co").await);

    for title in ["first alpha", "first beta"] {
        create_application(&app, &first.access_token, &first_company, title).await;
    }
    let second_application = id_of(
        &create_application(&app, &second.access_token, &second_company, "second gamma").await,
    );

    // Companies
    let response = list_companies(&app, &first.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(names(&page), vec!["First Co"]);
    assert_pagination(&page, 1, 20, 0);

    let response = list_companies(&app, &second.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(names(&page), vec!["Second Co"]);

    // Applications
    let response = list_applications(&app, &first.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 2, 20, 0);
    assert!(
        !ids(&page).contains(&second_application),
        "another account's application leaked into the listing: {page}"
    );

    let response = list_applications(&app, &second.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![second_application]);

    // A company filter naming another account's company matches nothing rather
    // than leaking its applications.
    let response = list_applications(
        &app,
        &second.access_token,
        &format!("?company_id={first_company}"),
    )
    .await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);
}

// ---------------------------------------------------------------------------
// 8 - the shared status codes: 400, 401, 404, 413 and 422 on all fourteen
// ---------------------------------------------------------------------------

/// The fourteen operations, each addressed at a real resource of `user`.
///
/// The list is built once so that the credential sweep, the ownership sweep and
/// the malformed-id sweep cannot drift from one another: an operation added in
/// one place and forgotten in another is the failure mode a hand-written sweep
/// has, and a single construction is the guard against it.
fn operations(
    company_id: &str,
    application_id: &str,
    event_id: &str,
    task_id: Uuid,
) -> Vec<(Method, String, Option<Value>)> {
    vec![
        // --- companies ---
        (Method::GET, COMPANIES.to_string(), None),
        (
            Method::POST,
            COMPANIES.to_string(),
            Some(json!({ "name": "Intruder Co" })),
        ),
        (Method::GET, format!("{COMPANIES}/{company_id}"), None),
        (
            Method::PATCH,
            format!("{COMPANIES}/{company_id}"),
            Some(json!({ "name": "hijacked" })),
        ),
        (Method::DELETE, format!("{COMPANIES}/{company_id}"), None),
        // --- applications ---
        (Method::GET, APPLICATIONS.to_string(), None),
        (
            Method::POST,
            APPLICATIONS.to_string(),
            Some(json!({
                "company_id": company_id,
                "role_title": "Intruder",
                "next_action_task_id": task_id,
            })),
        ),
        (
            Method::GET,
            format!("{APPLICATIONS}/{application_id}"),
            None,
        ),
        (
            Method::PATCH,
            format!("{APPLICATIONS}/{application_id}"),
            Some(json!({ "role_title": "hijacked" })),
        ),
        (
            Method::DELETE,
            format!("{APPLICATIONS}/{application_id}"),
            None,
        ),
        // --- events ---
        (Method::GET, events_path(application_id), None),
        (
            Method::POST,
            events_path(application_id),
            Some(event_body("note", "2026-10-15T13:00:00Z", "intruder")),
        ),
        (
            Method::PATCH,
            event_path(application_id, event_id),
            Some(json!({ "summary": "hijacked" })),
        ),
        (Method::DELETE, event_path(application_id, event_id), None),
    ]
}

#[tokio::test]
async fn every_job_application_operation_requires_a_credential() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "jobs-credential").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Guarded Co").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Guarded role").await);
    let event_id = id_of(
        &create_event(
            &app,
            &user.access_token,
            &application_id,
            "note",
            "2026-10-14T13:00:00Z",
            "Guarded entry",
        )
        .await,
    );

    let project_id = create_project(&app, &user, "Guarded project").await;
    let task_id = create_task(&app, &user, project_id, "Guarded task").await;

    let operations = operations(&company_id, &application_id, &event_id, task_id);
    assert_eq!(
        operations.len(),
        14,
        "the feature publishes fourteen operations"
    );

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

    // Nothing the unauthenticated calls sent landed.
    let response = get_company(&app, &user.access_token, &company_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["name"], "Guarded Co");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["role_title"], "Guarded role");
    assert_eq!(stored["status"], "wishlist");

    let response = list_events(&app, &user.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(
        ids(&page),
        vec![event_id],
        "an unauthenticated call must not add an event"
    );

    let response = list_companies(&app, &user.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(names(&page), vec!["Guarded Co"]);
}

#[tokio::test]
async fn every_job_application_operation_is_not_found_for_another_account() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "jobs-foreign-owner").await;
    let other = register_user(&app, "jobs-foreign-other").await;

    let project_id = create_project(&app, &owner, "Owner project").await;
    let task_id = create_task(&app, &owner, project_id, "Owner task").await;

    let company_id = id_of(&create_company(&app, &owner.access_token, "Owner Co").await);
    let response = post_application(
        &app,
        &owner.access_token,
        json!({
            "company_id": company_id,
            "role_title": "Owner role",
            "next_action_task_id": task_id,
        }),
    )
    .await;
    let application_id = id_of(&response.expect_status(StatusCode::CREATED));
    let event_id = id_of(
        &create_event(
            &app,
            &owner.access_token,
            &application_id,
            "note",
            "2026-10-14T13:00:00Z",
            "Owner entry",
        )
        .await,
    );

    // The eleven operations that address a row. The three collection
    // operations cannot answer 404 - a foreign collection is an empty page -
    // and are asserted separately below.
    let addressed = [
        (Method::GET, format!("{COMPANIES}/{company_id}"), None),
        (
            Method::PATCH,
            format!("{COMPANIES}/{company_id}"),
            Some(json!({ "name": "hijacked" })),
        ),
        (Method::DELETE, format!("{COMPANIES}/{company_id}"), None),
        (
            Method::GET,
            format!("{APPLICATIONS}/{application_id}"),
            None,
        ),
        (
            Method::PATCH,
            format!("{APPLICATIONS}/{application_id}"),
            Some(json!({ "role_title": "hijacked" })),
        ),
        (
            Method::DELETE,
            format!("{APPLICATIONS}/{application_id}"),
            None,
        ),
        (Method::GET, events_path(&application_id), None),
        (
            Method::POST,
            events_path(&application_id),
            Some(event_body("note", "2026-10-15T13:00:00Z", "intruder")),
        ),
        (
            Method::PATCH,
            event_path(&application_id, &event_id),
            Some(json!({ "summary": "hijacked" })),
        ),
        (Method::DELETE, event_path(&application_id, &event_id), None),
        (
            Method::POST,
            APPLICATIONS.to_string(),
            Some(json!({ "company_id": company_id, "role_title": "Intruder" })),
        ),
    ];

    assert_eq!(
        addressed.len(),
        11,
        "eleven of the fourteen operations address an owned row"
    );

    for (method, uri, body) in addressed {
        let response = send(
            &app,
            request(method.clone(), &uri, Some(&other.access_token), body),
        )
        .await;
        let body = response.expect_status(StatusCode::NOT_FOUND);

        assert_error(&body, "not_found");
        assert!(
            body["error"].get("details").is_none(),
            "a 404 carries no field details: {body}"
        );
    }

    // The two list operations answer `200` with nothing of the owner's in it.
    let response = list_companies(&app, &other.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);

    let response = list_applications(&app, &other.access_token, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 0, 20, 0);

    // The fourteenth: a create is the caller's own company, and a name the
    // owner already uses is not a collision across accounts.
    let response = post_company(&app, &other.access_token, company_body("Owner Co")).await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["name"], "Owner Co");

    // Nothing the other account attempted landed on the owner's rows.
    let response = get_company(&app, &owner.access_token, &company_id).await;
    let kept = response.expect_status(StatusCode::OK);
    assert_eq!(kept["name"], "Owner Co");

    let response = get_application(&app, &owner.access_token, &application_id).await;
    let kept = response.expect_status(StatusCode::OK);
    assert_eq!(kept["role_title"], "Owner role");
    assert_eq!(kept["status"], "wishlist");
    assert_eq!(kept["next_action_task_id"], task_id.to_string());

    let response = list_events(&app, &owner.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_eq!(ids(&page), vec![event_id]);
}

#[tokio::test]
async fn every_path_id_must_be_a_uuid() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "jobs-bad-id").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Role").await);
    let event_id = id_of(
        &create_event(
            &app,
            &user.access_token,
            &application_id,
            "note",
            "2026-10-14T13:00:00Z",
            "entry",
        )
        .await,
    );

    const BAD: &str = "not-a-uuid";

    // Every position a UUID is read from, including both positions of the
    // event route: a path that parses one id and not the other must fail.
    let cases: [(Method, String, Option<Value>); 12] = [
        (Method::GET, format!("{COMPANIES}/{BAD}"), None),
        (
            Method::PATCH,
            format!("{COMPANIES}/{BAD}"),
            Some(json!({ "name": "valid body" })),
        ),
        (Method::DELETE, format!("{COMPANIES}/{BAD}"), None),
        (Method::GET, format!("{APPLICATIONS}/{BAD}"), None),
        (
            Method::PATCH,
            format!("{APPLICATIONS}/{BAD}"),
            Some(json!({ "role_title": "valid body" })),
        ),
        (Method::DELETE, format!("{APPLICATIONS}/{BAD}"), None),
        (Method::GET, events_path(BAD), None),
        (
            Method::POST,
            events_path(BAD),
            Some(event_body("note", "2026-10-14T13:00:00Z", "valid body")),
        ),
        (
            Method::PATCH,
            event_path(BAD, &event_id),
            Some(json!({ "summary": "valid body" })),
        ),
        (
            Method::PATCH,
            event_path(&application_id, BAD),
            Some(json!({ "summary": "valid body" })),
        ),
        (Method::DELETE, event_path(BAD, &event_id), None),
        (Method::DELETE, event_path(&application_id, BAD), None),
    ];

    for (method, uri, body) in cases {
        let response = send(
            &app,
            request(method.clone(), &uri, Some(&user.access_token), body),
        )
        .await;
        let body = response.expect_status(StatusCode::BAD_REQUEST);
        assert_error(&body, "bad_request");
    }
}

#[tokio::test]
async fn every_write_operation_refuses_a_malformed_body() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "jobs-bad-body").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Role").await);
    let event_id = id_of(
        &create_event(
            &app,
            &user.access_token,
            &application_id,
            "note",
            "2026-10-14T13:00:00Z",
            "entry",
        )
        .await,
    );

    let writes: [(Method, String); 6] = [
        (Method::POST, COMPANIES.to_string()),
        (Method::PATCH, format!("{COMPANIES}/{company_id}")),
        (Method::POST, APPLICATIONS.to_string()),
        (Method::PATCH, format!("{APPLICATIONS}/{application_id}")),
        (Method::POST, events_path(&application_id)),
        (Method::PATCH, event_path(&application_id, &event_id)),
    ];

    for (method, uri) in writes {
        // A body that is not JSON at all is a malformed request: 400.
        let response = send_raw(&app, method.clone(), &uri, &user.access_token, "{not json").await;
        let body = response.expect_status(StatusCode::BAD_REQUEST);
        assert_error(&body, "bad_request");

        // An empty body is the same failure, not a 422.
        let response = send_raw(&app, method, &uri, &user.access_token, "").await;
        let body = response.expect_status(StatusCode::BAD_REQUEST);
        assert_error(&body, "bad_request");
    }

    // A body that parses as JSON but does not match the contract is a business
    // rule failure: 422, and the violation names the body.
    for (method, uri) in [
        (Method::POST, COMPANIES.to_string()),
        (Method::PATCH, format!("{COMPANIES}/{company_id}")),
        (Method::POST, APPLICATIONS.to_string()),
        (Method::PATCH, format!("{APPLICATIONS}/{application_id}")),
    ] {
        let response = send_raw(&app, method, &uri, &user.access_token, "[]").await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "body");
    }

    // Nothing the malformed calls sent landed.
    let response = get_company(&app, &user.access_token, &company_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["name"], "Acme");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["role_title"], "Role");
}

#[tokio::test]
async fn every_operation_with_a_documented_validation_failure_is_a_422() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "jobs-422").await;

    let company_id = id_of(&create_company(&app, &user.access_token, "Acme").await);
    let application_id =
        id_of(&create_application(&app, &user.access_token, &company_id, "Role").await);
    let event_id = id_of(
        &create_event(
            &app,
            &user.access_token,
            &application_id,
            "note",
            "2026-10-14T13:00:00Z",
            "entry",
        )
        .await,
    );

    let over_bound = "x".repeat(201);

    let cases: [(&str, Method, String, Value); 8] = [
        (
            "GET companies with a search over the bound",
            Method::GET,
            format!("{COMPANIES}?search={over_bound}"),
            Value::Null,
        ),
        (
            "POST companies with a blank name",
            Method::POST,
            COMPANIES.to_string(),
            json!({ "name": "   " }),
        ),
        (
            "PATCH companies with a blank notes-free over-long name",
            Method::PATCH,
            format!("{COMPANIES}/{company_id}"),
            json!({ "name": "x".repeat(201) }),
        ),
        (
            "GET applications with a search over the bound",
            Method::GET,
            format!("{APPLICATIONS}?search={over_bound}"),
            Value::Null,
        ),
        (
            "POST applications with a stage other than wishlist",
            Method::POST,
            APPLICATIONS.to_string(),
            json!({ "company_id": company_id, "role_title": "born applied", "status": "applied" }),
        ),
        (
            "PATCH applications with a role title over the bound",
            Method::PATCH,
            format!("{APPLICATIONS}/{application_id}"),
            json!({ "role_title": "x".repeat(201) }),
        ),
        (
            "POST events with a blank summary",
            Method::POST,
            events_path(&application_id),
            event_body("note", "2026-10-14T13:00:00Z", "   "),
        ),
        (
            "PATCH events with a kind outside the vocabulary",
            Method::PATCH,
            event_path(&application_id, &event_id),
            json!({ "kind": "meeting" }),
        ),
    ];

    for (what, method, uri, body) in cases {
        let body = if body.is_null() { None } else { Some(body) };

        let response = send(&app, request(method, &uri, Some(&user.access_token), body)).await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_error(&body, "validation_failed");
        assert!(
            body["error"]["details"]
                .as_array()
                .is_some_and(|details| !details.is_empty()),
            "{what} must name the field that failed: {body}"
        );
    }

    // Nothing refused was written.
    let response = get_company(&app, &user.access_token, &company_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["name"], "Acme");

    let response = get_application(&app, &user.access_token, &application_id).await;
    let stored = response.expect_status(StatusCode::OK);
    assert_eq!(stored["role_title"], "Role");
    assert_eq!(stored["status"], "wishlist");

    let response = list_events(&app, &user.access_token, &application_id, "").await;
    let page = response.expect_status(StatusCode::OK);
    assert_pagination(&page, 1, 20, 0);
}

#[tokio::test]
async fn a_job_applications_body_over_the_limit_is_refused() {
    // A body limit far below a real application, so the refusal is about size
    // and not about validation.
    let state = test_state_with_body_limit(256).await;
    let app = app(state);
    let user = register_user(&app, "jobs-body-limit").await;

    // A real employer, so the oversized body cannot be refused for naming a
    // company that does not exist.
    let company = create_company(&app, &user.access_token, "Acme").await;
    let company_id = company["id"]
        .as_str()
        .unwrap_or_else(|| panic!("a company carries an id: {company}"))
        .to_string();

    let oversized = json!({
        "company_id": company_id,
        "role_title": "Backend engineer",
        "description": "d".repeat(400),
    });
    let response = post_application(&app, &user.access_token, oversized).await;
    let body = response.expect_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert_error(&body, "payload_too_large");

    // The control: the same route, the same caller and a body inside the limit
    // is accepted, so the `413` above is the transport bound and not a route
    // that refuses this shape.
    let response = post_application(
        &app,
        &user.access_token,
        json!({ "company_id": company_id, "role_title": "Backend engineer" }),
    )
    .await;
    response.expect_status(StatusCode::CREATED);
}

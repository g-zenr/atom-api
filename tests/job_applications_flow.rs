//! The whole job application feature in one journey, and the ownership sweep.
//!
//! `C-J2`-`C-J4` prove one operation at a time: `tests/job_applications_api.rs`
//! asserts each of the fourteen operations, and `tests/jobs_schema.rs` the
//! tables underneath them. Two properties are not visible from any single
//! operation, and they are what this file exists for:
//!
//! * **The journey.** An application that is created as a lead, walked to
//!   `offer`, linked to a next-action task and given a timeline must still carry
//!   the right stage, the right money, the same `created_at` and the right
//!   timeline at every step. A per-operation test can pass while the state the
//!   *next* step depends on is lost between two calls: the row the stage machine
//!   reads back, the cross-link a PATCH writes, and the fact that `offer` is
//!   terminal for **every** move and not only for the one a per-operation test
//!   fires.
//! * **The sweep.** Ownership must be enforced by every one of the fourteen
//!   operations, and it must be enforced as `404` even where the row's own state
//!   would otherwise answer `409`: deleting a company that still has
//!   applications is a `409` for its owner, and must stay a `404` for everyone
//!   else, or the error code itself leaks the existence of a foreign row. A test
//!   that fires one foreign read proves one of fourteen.
//!
//! The rules this file follows, in the spirit of `AGENTS.md` §4:
//!
//! * every assertion is on the status code and the observable body - never on a
//!   log line, a timing value or a random value;
//! * where a value is **stored** - a stage, a link, a timeline - the assertion
//!   is on a fresh read, never only on the response of the call that wrote it;
//! * the field set is asserted exactly at every step, so a field that appears,
//!   disappears or is renamed fails even when its value happened to be right;
//! * the shared database is never truncated and no count is global: every
//!   fixture belongs to an account this file registered, so every count is
//!   restricted to that account by construction;
//! * nothing waits on a timer. There is no `sleep` here, and none is needed:
//!   every step is a synchronous request whose expected status is the condition.
//!
//! The one direct SQL statement is a count, used to prove that deleting an
//! application really deleted its event **rows** rather than merely making the
//! parent route answer `404`. It writes nothing, and the assertion it feeds is
//! on the database, which is the only place that fact is observable.

pub mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use chrono::{DateTime, Utc};
use common::{
    TestResponse, app, assert_pagination, create_project, create_task, login_user, register_user,
    request, send, test_state,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// The company collection path.
const COMPANIES: &str = "/api/v1/companies";

/// The application collection path.
const APPLICATIONS: &str = "/api/v1/job-applications";

/// The authenticated-account path, used to read back who registered and who
/// logged in.
const ME: &str = "/api/v1/auth/me";

/// The five stages of the forward path, in order.
///
/// The first is the stage a new application is created in; the other four are
/// the permitted forward edges the journey walks. The machine's twelve edges
/// include `rejected` and `withdrawn` from every non-terminal stage, which this
/// journey deliberately does not take: it is the lead-to-offer path.
const STAGE_PATH: [&str; 5] = ["wishlist", "applied", "screening", "interview", "offer"];

/// Every stage the machine refuses to leave `offer` for.
///
/// The six other stages, `offer` itself excluded: re-sending `offer` is the
/// published accepted no-op, not a refusal, and is asserted separately.
const REFUSED_FROM_OFFER: [&str; 6] = [
    "wishlist",
    "applied",
    "screening",
    "interview",
    "rejected",
    "withdrawn",
];

/// The fields of `UserResponse`, sorted.
const USER_FIELDS: [&str; 5] = ["created_at", "display_name", "email", "id", "is_active"];

/// The fields of `ProjectResponse`, sorted.
const PROJECT_FIELDS: [&str; 5] = ["created_at", "description", "id", "name", "updated_at"];

/// The fields of `TaskResponse`, sorted.
const TASK_FIELDS: [&str; 10] = [
    "completed_at",
    "created_at",
    "description",
    "id",
    "position",
    "priority",
    "project_id",
    "status",
    "title",
    "updated_at",
];

/// The fields of `CompanyResponse`, sorted.
const COMPANY_FIELDS: [&str; 6] = ["created_at", "id", "name", "notes", "updated_at", "website"];

/// The fields of `ApplicationResponse`, sorted.
const APPLICATION_FIELDS: [&str; 14] = [
    "applied_on",
    "company_id",
    "created_at",
    "description",
    "id",
    "job_url",
    "next_action_task_id",
    "role_title",
    "salary_currency",
    "salary_max",
    "salary_min",
    "source",
    "status",
    "updated_at",
];

/// The fields of `EventResponse`, sorted.
///
/// `updated_at` is deliberately absent: a timeline entry records what the owner
/// says happened, not when the row was last written (`A-D1-1`).
const EVENT_FIELDS: [&str; 7] = [
    "application_id",
    "created_at",
    "details",
    "id",
    "kind",
    "occurred_at",
    "summary",
];

/// The fields of the list envelope, sorted.
const PAGE_FIELDS: [&str; 2] = ["items", "pagination"];

/// The fields of the pagination block, sorted.
const PAGINATION_FIELDS: [&str; 4] = ["has_more", "limit", "offset", "total"];

/// One timeline entry of the journey, exactly as the client sends it.
struct JourneyEvent {
    kind: &'static str,
    occurred_at: &'static str,
    /// Not trimmed: the service trims it, and the assertion is on what comes
    /// back.
    summary: &'static str,
    details: &'static str,
}

/// The three events the journey adds.
///
/// Three properties are deliberate:
///
/// * the first two share one instant, so the secondary `id DESC` tiebreaker is
///   asserted rather than assumed - and the expected order is derived from the
///   identifiers the server assigned, never from the posting order;
/// * the third is in the future, which the contract accepts because an
///   interview is scheduled before it happens;
/// * the summaries are posted padded, so the trimming the contract promises is
///   asserted on the stored value.
const JOURNEY_EVENTS: [JourneyEvent; 3] = [
    JourneyEvent {
        kind: "note",
        occurred_at: "2026-10-03T09:15:00Z",
        summary: "  Applied through the portal  ",
        details: "Sourced from the careers page.",
    },
    JourneyEvent {
        kind: "call",
        occurred_at: "2026-10-03T09:15:00Z",
        summary: "Recruiter screen",
        details: "",
    },
    JourneyEvent {
        kind: "interview",
        occurred_at: "2099-01-05T09:00:00Z",
        summary: "  Scheduled interview  ",
        details: "Two exercises, Rust and SQL.",
    },
];

/// The index in [`JOURNEY_EVENTS`] of the entry that is scheduled in the future.
const FUTURE_EVENT: usize = 2;

/// The order the three events are posted in: deliberately neither the
/// chronological order nor the reverse of it.
const POSTING_ORDER: [usize; 3] = [1, 2, 0];

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

/// The path of one company.
fn company_path(company_id: &str) -> String {
    format!("{COMPANIES}/{company_id}")
}

/// The path of one application.
fn application_path(application_id: &str) -> String {
    format!("{APPLICATIONS}/{application_id}")
}

/// The path of one application's event collection.
fn events_path(application_id: &str) -> String {
    format!("{APPLICATIONS}/{application_id}/events")
}

/// The path of one event inside its application.
fn event_path(application_id: &str, event_id: &str) -> String {
    format!("{APPLICATIONS}/{application_id}/events/{event_id}")
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

/// The body of a fully populated company create request.
///
/// Every optional field is present, so the field-by-field assertion after the
/// create has something to assert.
fn company_body(name: &str) -> Value {
    json!({
        "name": name,
        "website": "https://acme.example.test",
        "notes": "Referred by Dana",
    })
}

/// The body of a fully populated application create request.
///
/// `role_title` and `source` are padded: both are trimmed by the service, so the
/// assertion is on the stored value. `next_action_task_id` is deliberately
/// absent - the journey attaches the task later, through a `PATCH`, which is
/// what proves the cross-link survives a stage change.
fn application_body(company_id: &str) -> Value {
    json!({
        "company_id": company_id,
        "role_title": "  Senior backend engineer  ",
        "source": "  referral  ",
        "applied_on": "2026-10-02",
        "salary_min": "60000.0000",
        "salary_max": "75000.0000",
        "salary_currency": "EUR",
        "job_url": "https://acme.example.test/jobs/42",
        "description": "Platform team, Rust and PostgreSQL.",
    })
}

/// The body of an event create request.
fn event_body(kind: &str, occurred_at: &str, summary: &str, details: &str) -> Value {
    json!({
        "kind": kind,
        "occurred_at": occurred_at,
        "summary": summary,
        "details": details,
    })
}

/// Creates a company through the API and returns the `201` body.
async fn create_company(app: &Router, token: &str, name: &str) -> Value {
    let response = call(
        app,
        Method::POST,
        COMPANIES,
        token,
        Some(company_body(name)),
    )
    .await;

    response.expect_status(StatusCode::CREATED)
}

/// Reads one company and returns the `200` body.
async fn read_company(app: &Router, token: &str, company_id: &str) -> Value {
    let response = call(app, Method::GET, &company_path(company_id), token, None).await;

    response.expect_status(StatusCode::OK)
}

/// Lists companies with the given query string, including its leading `?`.
async fn list_companies(app: &Router, token: &str, query: &str) -> TestResponse {
    call(
        app,
        Method::GET,
        &format!("{COMPANIES}{query}"),
        token,
        None,
    )
    .await
}

/// Creates an application through the API and returns the `201` body.
async fn create_application(app: &Router, token: &str, body: Value) -> Value {
    let response = call(app, Method::POST, APPLICATIONS, token, Some(body)).await;

    response.expect_status(StatusCode::CREATED)
}

/// Reads one application and returns the `200` body.
async fn read_application(app: &Router, token: &str, application_id: &str) -> Value {
    let response = call(
        app,
        Method::GET,
        &application_path(application_id),
        token,
        None,
    )
    .await;

    response.expect_status(StatusCode::OK)
}

/// Patches one application and returns the raw response.
async fn patch_application(
    app: &Router,
    token: &str,
    application_id: &str,
    body: Value,
) -> TestResponse {
    call(
        app,
        Method::PATCH,
        &application_path(application_id),
        token,
        Some(body),
    )
    .await
}

/// Moves one application to `stage`, which the caller expects to be accepted.
async fn patch_stage(app: &Router, token: &str, application_id: &str, stage: &str) -> Value {
    let response = patch_application(app, token, application_id, json!({ "status": stage })).await;

    response.expect_status(StatusCode::OK)
}

/// Lists applications with the given query string, including its leading `?`.
async fn list_applications(app: &Router, token: &str, query: &str) -> TestResponse {
    call(
        app,
        Method::GET,
        &format!("{APPLICATIONS}{query}"),
        token,
        None,
    )
    .await
}

/// Adds an event through the API and returns the `201` body.
async fn create_event(app: &Router, token: &str, application_id: &str, body: Value) -> Value {
    let response = call(
        app,
        Method::POST,
        &events_path(application_id),
        token,
        Some(body),
    )
    .await;

    response.expect_status(StatusCode::CREATED)
}

/// Lists one application's events with the given query string.
async fn list_events(app: &Router, token: &str, application_id: &str, query: &str) -> TestResponse {
    call(
        app,
        Method::GET,
        &format!("{}{query}", events_path(application_id)),
        token,
        None,
    )
    .await
}

/// Reads the authenticated account and returns the `200` body.
async fn read_account(app: &Router, token: &str) -> Value {
    let response = call(app, Method::GET, ME, token, None).await;

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
/// The type check is the point: a JSON number that happened to render `60000`
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

/// Parses an RFC 3339 literal the test itself wrote.
fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap_or_else(|error| panic!("{text:?} must be an RFC 3339 literal: {error}"))
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

/// The identifier of a resource response, as the wire spells it.
fn id_of(value: &Value) -> String {
    uuid_of(value, "id").to_string()
}

/// The identifiers of a list response, in the order the server returned them.
fn ids_of(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the listing must be an array envelope: {page}"))
        .iter()
        .map(id_of)
        .collect()
}

/// Asserts the standard error envelope and its stable code.
fn assert_error(body: &Value, code: &str) {
    assert_eq!(
        body["error"]["code"], code,
        "unexpected error code; the envelope was {body}"
    );
    assert_eq!(
        fields_of(&body["error"]),
        ["code", "message"],
        "every error uses the standard envelope, with no extra field: {body}"
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "every error envelope carries a non-empty message: {body}"
    );
}

/// Asserts the standard error envelope of a `404`, and that it is not a `403`.
fn assert_not_found(body: &Value, operation: &str) {
    assert_eq!(
        body["error"]["code"], "not_found",
        "{operation} must answer `404 not_found` for a foreign row, never `403`: {body}"
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

/// Asserts that `after` carries every named field of `before` unchanged, except
/// the ones in `except`.
///
/// The comparison is on the body the API returned, so a field that was silently
/// re-defaulted, re-trimmed or re-stamped is caught even when the status code is
/// the one the test expected.
fn assert_fields_unchanged(
    before: &Value,
    after: &Value,
    fields: &[&str],
    except: &[&str],
    what: &str,
) {
    for field in fields {
        if except.contains(field) {
            continue;
        }

        assert_eq!(
            after[*field], before[*field],
            "{what}: `{field}` must not change"
        );
    }
}

// ---------------------------------------------------------------------------
// Task 1 - the whole journey
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_whole_job_application_journey_from_lead_to_offer() {
    let state = test_state().await;
    // The pool is used exactly once, by the cascade count at the end.
    let db = state.db.clone();
    let app = app(state);

    // -----------------------------------------------------------------
    // 1. register
    // -----------------------------------------------------------------
    let owner = register_user(&app, "journey-applicant").await;
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
    // 3. a project and the next-action task the application will point at
    // -----------------------------------------------------------------
    let project_id = create_project(&app, &owner, "Job search 2026").await;

    let response = call(
        &app,
        Method::GET,
        &format!("/api/v1/projects/{project_id}"),
        &token,
        None,
    )
    .await;
    let project = response.expect_status(StatusCode::OK);

    assert_eq!(
        fields_of(&project),
        PROJECT_FIELDS,
        "a project must publish exactly the documented fields: {project}"
    );
    assert_eq!(project["id"], project_id.to_string());
    assert_eq!(project["name"], "Job search 2026");
    instant(&project, "created_at");
    instant(&project, "updated_at");

    let task_id = create_task(&app, &owner, project_id, "Prepare for the on-site").await;

    let response = call(
        &app,
        Method::GET,
        &format!("/api/v1/tasks/{task_id}"),
        &token,
        None,
    )
    .await;
    let task = response.expect_status(StatusCode::OK);

    assert_eq!(
        fields_of(&task),
        TASK_FIELDS,
        "a task must publish exactly the documented fields: {task}"
    );
    assert_eq!(task["id"], task_id.to_string());
    assert_eq!(task["project_id"], project_id.to_string());
    assert_eq!(task["title"], "Prepare for the on-site");
    assert_eq!(task["description"], "");
    assert_eq!(task["status"], "open");
    assert_eq!(task["priority"], "medium");
    assert!(
        task["completed_at"].is_null(),
        "a new task is not completed: {task}"
    );

    // -----------------------------------------------------------------
    // 4. a company
    // -----------------------------------------------------------------
    let company = create_company(&app, &token, "Acme GmbH").await;

    assert_eq!(
        fields_of(&company),
        COMPANY_FIELDS,
        "a company must publish exactly the documented fields: {company}"
    );
    assert!(
        company.get("user_id").is_none(),
        "user_id must not be published: {company}"
    );
    assert_eq!(company["name"], "Acme GmbH");
    assert_eq!(company["website"], "https://acme.example.test");
    assert_eq!(company["notes"], "Referred by Dana");
    let company_created_at = instant(&company, "created_at");
    assert!(
        instant(&company, "updated_at") >= company_created_at,
        "a company cannot be modified before it existed"
    );

    let company_id = id_of(&company);

    // -----------------------------------------------------------------
    // 5. the lead: a wishlist application, fully populated
    // -----------------------------------------------------------------
    let created = create_application(&app, &token, application_body(&company_id)).await;

    assert_eq!(
        fields_of(&created),
        APPLICATION_FIELDS,
        "an application must publish exactly the documented fields: {created}"
    );
    assert_eq!(created["company_id"], company_id.as_str());
    assert_eq!(
        created["status"], STAGE_PATH[0],
        "an application is created in the first stage of the machine"
    );
    assert_eq!(
        created["role_title"], "Senior backend engineer",
        "a role title is stored trimmed"
    );
    assert_eq!(
        created["source"], "referral",
        "a source label is stored trimmed"
    );
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
    assert!(
        created["next_action_task_id"].is_null(),
        "the task is attached later, through a PATCH: {created}"
    );

    let application_id = id_of(&created);
    let application_created_at = instant(&created, "created_at");
    let mut last_updated_at = instant(&created, "updated_at");
    assert!(
        last_updated_at >= application_created_at,
        "an application cannot be modified before it existed"
    );

    // -----------------------------------------------------------------
    // 6. the stage path: the four forward edges, one at a time
    //
    // The path is the five stages in [`STAGE_PATH`]. Stage zero was the create;
    // each of the next four is a `PATCH` whose answer must name the stage it was
    // asked for, and must not disturb the fields the stage machine has no
    // business touching.
    // -----------------------------------------------------------------
    let mut previous = created.clone();

    for expected in &STAGE_PATH[1..] {
        let moved = patch_stage(&app, &token, &application_id, expected).await;

        assert_eq!(
            fields_of(&moved),
            APPLICATION_FIELDS,
            "a stage change must publish exactly the documented fields: {moved}"
        );
        assert_eq!(
            moved["status"], *expected,
            "the stage change must answer with the stage it was asked for"
        );
        assert_fields_unchanged(
            &previous,
            &moved,
            &APPLICATION_FIELDS,
            &["status", "updated_at"],
            &format!("the move to {expected}"),
        );
        assert_eq!(
            moved["created_at"], created["created_at"],
            "created_at is stamped once and never changes"
        );

        let updated_at = instant(&moved, "updated_at");
        assert!(
            updated_at >= last_updated_at,
            "a stage change must not move updated_at backwards ({updated_at} < {last_updated_at})"
        );

        last_updated_at = updated_at;
        previous = moved;
    }

    let offered = read_application(&app, &token, &application_id).await;
    assert_eq!(
        offered["status"], "offer",
        "the journey must have reached the offer stage"
    );
    assert_fields_unchanged(
        &created,
        &offered,
        &APPLICATION_FIELDS,
        &["status", "updated_at"],
        "the walk to offer",
    );

    // -----------------------------------------------------------------
    // 7. attach the next-action task
    //
    // A `PATCH` on a terminal stage: the stage machine governs `status` only, so
    // the cross-link is still writable. The assertion is on a read, not on the
    // PATCH's own answer.
    // -----------------------------------------------------------------
    let response = patch_application(
        &app,
        &token,
        &application_id,
        json!({ "next_action_task_id": task_id }),
    )
    .await;
    let linked = response.expect_status(StatusCode::OK);

    assert_eq!(fields_of(&linked), APPLICATION_FIELDS);
    assert_eq!(
        linked["next_action_task_id"],
        task_id.to_string(),
        "the task must be attached"
    );
    assert_eq!(
        linked["status"], "offer",
        "attaching a task is not a stage change"
    );
    assert_fields_unchanged(
        &offered,
        &linked,
        &APPLICATION_FIELDS,
        &["next_action_task_id", "updated_at"],
        "attaching the next-action task",
    );

    let stored_link = read_application(&app, &token, &application_id).await;
    assert_eq!(
        stored_link["next_action_task_id"],
        task_id.to_string(),
        "the link must be stored, not merely echoed"
    );

    // -----------------------------------------------------------------
    // 8. three timeline entries, posted out of order
    // -----------------------------------------------------------------
    let mut posted: Vec<Option<String>> = vec![None; JOURNEY_EVENTS.len()];

    for index in POSTING_ORDER {
        let spec = &JOURNEY_EVENTS[index];

        let event = create_event(
            &app,
            &token,
            &application_id,
            event_body(spec.kind, spec.occurred_at, spec.summary, spec.details),
        )
        .await;

        assert_eq!(
            fields_of(&event),
            EVENT_FIELDS,
            "an event must publish exactly the documented fields: {event}"
        );
        assert!(
            event.get("updated_at").is_none(),
            "an event has no updated_at (A-D1-1): {event}"
        );
        assert_eq!(event["application_id"], application_id.as_str());
        assert_eq!(event["kind"], spec.kind);
        assert_eq!(
            instant(&event, "occurred_at"),
            at(spec.occurred_at),
            "an event instant is stored as it was sent, including a future one"
        );
        assert_eq!(
            event["summary"],
            spec.summary.trim(),
            "an event summary is stored trimmed"
        );
        assert_eq!(
            event["details"], spec.details,
            "event details are kept verbatim"
        );
        instant(&event, "created_at");

        posted[index] = Some(id_of(&event));
    }

    let posted: Vec<String> = posted
        .into_iter()
        .map(|id| id.unwrap_or_else(|| panic!("every journey event must have been posted")))
        .collect();

    // -----------------------------------------------------------------
    // 9. read the timeline back
    //
    // The order is `occurred_at DESC, id DESC`, although the events were posted
    // in neither that order nor its reverse. Two of them share one instant, so
    // the tiebreaker is exercised; its expected order is derived from the
    // identifiers the server assigned.
    // -----------------------------------------------------------------
    let page = list_events(&app, &token, &application_id, "").await;
    let page = page.expect_status(StatusCode::OK);

    assert_eq!(
        fields_of(&page),
        PAGE_FIELDS,
        "the timeline must answer the standard envelope: {page}"
    );
    assert_eq!(
        fields_of(&page["pagination"]),
        PAGINATION_FIELDS,
        "the pagination block must publish exactly its four fields: {page}"
    );
    assert_pagination(&page, 3, 20, 0);

    let mut tied = [posted[0].clone(), posted[1].clone()];
    tied.sort();
    tied.reverse();

    let ordered = vec![
        posted[FUTURE_EVENT].clone(),
        tied[0].clone(),
        tied[1].clone(),
    ];

    assert_eq!(
        ids_of(&page),
        ordered,
        "the timeline must be ordered by occurred_at DESC, then id DESC: {page}"
    );

    let listed = page["items"].as_array().expect("an items array");
    for (position, event) in listed.iter().enumerate() {
        assert_eq!(
            fields_of(event),
            EVENT_FIELDS,
            "items[{position}] must publish exactly the documented fields, with no updated_at: {event}"
        );
        assert_eq!(event["id"], ordered[position].as_str());

        let spec = posted
            .iter()
            .position(|id| id == &ordered[position])
            .map(|index| &JOURNEY_EVENTS[index])
            .unwrap_or_else(|| panic!("event {position} was not posted by this test"));

        assert_eq!(event["kind"], spec.kind);
        assert_eq!(instant(event, "occurred_at"), at(spec.occurred_at));
        assert_eq!(event["summary"], spec.summary.trim());
        assert_eq!(event["details"], spec.details);
        assert_eq!(event["application_id"], application_id.as_str());
        instant(event, "created_at");
    }

    let mut stored_instants: Vec<DateTime<Utc>> = listed
        .iter()
        .map(|event| instant(event, "occurred_at"))
        .collect();
    let descending = stored_instants.clone();
    stored_instants.sort_by(|left, right| right.cmp(left));
    assert_eq!(
        stored_instants, descending,
        "the timeline must be newest first: {page}"
    );

    // The future entry really is in the future, which is what the first
    // position of the order rests on.
    assert!(
        instant(&listed[0], "occurred_at") > instant(&listed[1], "occurred_at"),
        "the scheduled event belongs at the head of the timeline: {page}"
    );

    // -----------------------------------------------------------------
    // 10. read the application and assert the whole object
    // -----------------------------------------------------------------
    let stored = read_application(&app, &token, &application_id).await;

    assert_eq!(
        fields_of(&stored),
        APPLICATION_FIELDS,
        "the application must publish exactly the documented fields: {stored}"
    );
    assert_eq!(stored["id"], application_id.as_str());
    assert_eq!(stored["company_id"], company_id.as_str());
    assert_eq!(stored["status"], "offer");
    assert_eq!(stored["role_title"], "Senior backend engineer");
    assert_eq!(stored["source"], "referral");
    assert_eq!(stored["applied_on"], "2026-10-02");
    assert_eq!(
        assert_money(&stored["salary_min"], "salary_min"),
        "60000.0000"
    );
    assert_eq!(
        assert_money(&stored["salary_max"], "salary_max"),
        "75000.0000"
    );
    assert_eq!(stored["salary_currency"], "EUR");
    assert_eq!(stored["job_url"], "https://acme.example.test/jobs/42");
    assert_eq!(stored["description"], "Platform team, Rust and PostgreSQL.");
    assert_eq!(
        stored["next_action_task_id"],
        task_id.to_string(),
        "the next-action task must survive the whole journey"
    );
    assert_eq!(
        stored["created_at"], created["created_at"],
        "created_at is stamped once and never changes"
    );
    assert!(
        instant(&stored, "updated_at") >= last_updated_at,
        "the stored updated_at must not predate the last write"
    );

    // The company is untouched by the journey through its application.
    let stored_company = read_company(&app, &token, &company_id).await;
    assert_eq!(
        stored_company, company,
        "an application must not change the company it references"
    );

    // -----------------------------------------------------------------
    // 11. offer is terminal
    //
    // Every other stage is refused, and no refused move changes a byte of the
    // row: the comparison is on the whole body, so a stage that stayed while a
    // stamp moved or a link was lost is caught.
    // -----------------------------------------------------------------
    for refused in REFUSED_FROM_OFFER {
        let response =
            patch_application(&app, &token, &application_id, json!({ "status": refused })).await;
        let body = response.expect_status(StatusCode::CONFLICT);

        assert_error(&body, "conflict");

        let after = read_application(&app, &token, &application_id).await;
        assert_eq!(
            after, stored,
            "the refused move to `{refused}` must not change the offered application"
        );
    }

    // Re-sending `offer` is not a transition: it is the published accepted
    // no-op, and it commits nothing either.
    let response =
        patch_application(&app, &token, &application_id, json!({ "status": "offer" })).await;
    let repeated = response.expect_status(StatusCode::OK);

    assert_eq!(
        repeated, stored,
        "re-sending the current stage must answer with the unchanged row"
    );

    let after_no_op = read_application(&app, &token, &application_id).await;
    assert_eq!(
        after_no_op, stored,
        "re-sending the current stage must write nothing"
    );

    // -----------------------------------------------------------------
    // 12. the company cannot be deleted while the application exists
    // -----------------------------------------------------------------
    let response = call(
        &app,
        Method::DELETE,
        &company_path(&company_id),
        &token,
        None,
    )
    .await;
    let body = response.expect_status(StatusCode::CONFLICT);

    assert_error(&body, "conflict");
    assert_eq!(
        read_company(&app, &token, &company_id).await,
        stored_company,
        "a refused company delete must not touch the company"
    );
    assert_eq!(
        read_application(&app, &token, &application_id).await,
        stored,
        "a refused company delete must not touch its application"
    );

    // -----------------------------------------------------------------
    // 13. deleting the application removes its events, and the company then
    //     becomes deletable
    // -----------------------------------------------------------------
    let body = call(
        &app,
        Method::DELETE,
        &application_path(&application_id),
        &token,
        None,
    )
    .await
    .expect_status(StatusCode::NO_CONTENT);

    assert!(
        body.is_null(),
        "a 204 must carry no body; this one carried {body}"
    );

    let response = call(
        &app,
        Method::GET,
        &application_path(&application_id),
        &token,
        None,
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_not_found(&body, "reading the deleted application");

    // The timeline route is a `404` because the parent is gone...
    let response = list_events(&app, &token, &application_id, "").await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_not_found(&body, "reading the timeline of the deleted application");

    // ...and the rows really are gone, which the `404` alone would not prove.
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

    let body = call(
        &app,
        Method::DELETE,
        &company_path(&company_id),
        &token,
        None,
    )
    .await
    .expect_status(StatusCode::NO_CONTENT);
    assert!(body.is_null(), "a 204 must carry no body: {body}");

    let response = call(&app, Method::GET, &company_path(&company_id), &token, None).await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_not_found(&body, "reading the deleted company");

    // -----------------------------------------------------------------
    // 14. the account's two listings are empty, and the task it created
    //     survived the application
    // -----------------------------------------------------------------
    let page = list_companies(&app, &token, "").await;
    let page = page.expect_status(StatusCode::OK);
    assert_eq!(fields_of(&page), PAGE_FIELDS);
    assert_pagination(&page, 0, 20, 0);
    assert!(
        page["items"].as_array().is_some_and(Vec::is_empty),
        "the journey deleted its only company: {page}"
    );

    let page = list_applications(&app, &token, "").await;
    let page = page.expect_status(StatusCode::OK);
    assert_eq!(fields_of(&page), PAGE_FIELDS);
    assert_pagination(&page, 0, 20, 0);
    assert!(
        page["items"].as_array().is_some_and(Vec::is_empty),
        "the journey deleted its only application: {page}"
    );

    let response = call(
        &app,
        Method::GET,
        &format!("/api/v1/tasks/{task_id}"),
        &token,
        None,
    )
    .await;
    let kept_task = response.expect_status(StatusCode::OK);
    assert_eq!(
        kept_task["id"],
        task_id.to_string(),
        "deleting the application must not delete the task it pointed at"
    );
}

// ---------------------------------------------------------------------------
// Task 2 - the ownership sweep on its own
// ---------------------------------------------------------------------------

/// One operation a second account may aim at the first account's data.
struct Attempt {
    /// What the operation is, for a failure message that names it.
    label: &'static str,
    method: Method,
    uri: String,
    body: Option<Value>,
}

/// The operations that address one of the owner's rows, every one of them
/// pointed at the owner's company, application, event and task.
///
/// The three collection operations are not here: they cannot answer `404`,
/// because the contract publishes `200` with the caller's own set. They are
/// asserted separately, by the property that does hold - the second account's
/// page is empty and its `total` counts only its own rows.
fn foreign_operations(
    company_id: &str,
    application_id: &str,
    event_id: &str,
    other_company_id: &str,
) -> Vec<Attempt> {
    vec![
        Attempt {
            label: "read the company",
            method: Method::GET,
            uri: company_path(company_id),
            body: None,
        },
        Attempt {
            label: "change the company",
            method: Method::PATCH,
            uri: company_path(company_id),
            body: Some(json!({ "name": "Hijacked Co" })),
        },
        // The company still has the owner's application, so its owner would get
        // `409 conflict`. A second account must get `404`: a `409` here would
        // answer the question "does this id exist?" for a row that is not the
        // caller's.
        Attempt {
            label: "delete the company",
            method: Method::DELETE,
            uri: company_path(company_id),
            body: None,
        },
        Attempt {
            label: "read the application",
            method: Method::GET,
            uri: application_path(application_id),
            body: None,
        },
        Attempt {
            label: "change the application",
            method: Method::PATCH,
            uri: application_path(application_id),
            body: Some(json!({ "role_title": "Hijacked role" })),
        },
        // Two bodies, one property. The first names the owner's company, which
        // is foreign to the second account as well; the second names the second
        // account's **own** company, which leaves the application's ownership as
        // the only reason to refuse. A predicate that only checked the company
        // would answer `200` to the second one and quietly reassign someone
        // else's application.
        Attempt {
            label: "move the application onto the owner's company",
            method: Method::PATCH,
            uri: application_path(application_id),
            body: Some(json!({ "company_id": company_id })),
        },
        Attempt {
            label: "move the application onto the second account's own company",
            method: Method::PATCH,
            uri: application_path(application_id),
            body: Some(json!({ "company_id": other_company_id })),
        },
        Attempt {
            label: "delete the application",
            method: Method::DELETE,
            uri: application_path(application_id),
            body: None,
        },
        Attempt {
            label: "read the timeline",
            method: Method::GET,
            uri: events_path(application_id),
            body: None,
        },
        Attempt {
            label: "add an event",
            method: Method::POST,
            uri: events_path(application_id),
            body: Some(event_body(
                "note",
                "2026-10-15T13:00:00Z",
                "Intruder entry",
                "",
            )),
        },
        Attempt {
            label: "correct the event",
            method: Method::PATCH,
            uri: event_path(application_id, event_id),
            body: Some(json!({ "summary": "Hijacked entry" })),
        },
        Attempt {
            label: "delete the event",
            method: Method::DELETE,
            uri: event_path(application_id, event_id),
            body: None,
        },
        Attempt {
            label: "create an application for the owner's company",
            method: Method::POST,
            uri: APPLICATIONS.to_string(),
            body: Some(json!({
                "company_id": company_id,
                "role_title": "Intruder role",
            })),
        },
    ]
}

#[tokio::test]
async fn the_second_account_sees_not_found_on_every_job_application_operation() {
    let state = test_state().await;
    let app = app(state);

    let owner = register_user(&app, "sweep-applicant").await;
    let other = register_user(&app, "sweep-intruder").await;

    // The owner's fixtures: a project and a task for the cross-link, a company, an
    // application that has already moved once, and one event.
    let project_id = create_project(&app, &owner, "Application pipeline").await;
    let task_id = create_task(&app, &owner, project_id, "Reply to the recruiter").await;

    let company_id = id_of(&create_company(&app, &owner.access_token, "Owner Co").await);

    let mut body = application_body(&company_id);
    body["next_action_task_id"] = json!(task_id);
    body["role_title"] = json!("Backend engineer");
    let created = create_application(&app, &owner.access_token, body).await;
    let application_id = id_of(&created);

    // Mid-machine on purpose: an owner can still move it, so a foreign write
    // that was refused for the wrong reason (`409`) would be visible.
    let applied = patch_stage(&app, &owner.access_token, &application_id, "applied").await;
    assert_eq!(applied["status"], "applied");

    let event_id = id_of(
        &create_event(
            &app,
            &owner.access_token,
            &application_id,
            event_body(
                "email",
                "2026-10-04T08:30:00Z",
                "Recruiter replied",
                "They asked for availability.",
            ),
        )
        .await,
    );

    // The second account's own company. It exists only to make the
    // company-crossing `PATCH` unambiguous - the body names a row the caller
    // *does* own - and it is removed before the empty-page assertion at the end.
    let other_company_id = id_of(&create_company(&app, &other.access_token, "Second Co").await);

    // The owner's rows, before the sweep.
    let before_company = read_company(&app, &owner.access_token, &company_id).await;
    let before_application = read_application(&app, &owner.access_token, &application_id).await;
    let before_events = list_events(&app, &owner.access_token, &application_id, "")
        .await
        .expect_status(StatusCode::OK);

    assert_eq!(
        before_events["items"].as_array().map(Vec::len),
        Some(1),
        "the fixture must carry its one event: {before_events}"
    );

    // The second account attempts every addressed operation.
    let attempts = foreign_operations(&company_id, &application_id, &event_id, &other_company_id);
    assert_eq!(
        attempts.len(),
        13,
        "thirteen addressed operations are swept; the one collection operation is asserted below"
    );

    for attempt in attempts {
        let response = call(
            &app,
            attempt.method.clone(),
            &attempt.uri,
            &other.access_token,
            attempt.body,
        )
        .await;

        // The status assertion is the "never 403" assertion: any `403` fails it.
        let body = response.expect_status(StatusCode::NOT_FOUND);
        assert_not_found(&body, attempt.label);

        // The delete-the-company case is the one a state check could answer
        // `409` from, leaking the existence of a foreign row.
        assert_ne!(
            body["error"]["code"], "conflict",
            "`{}` must not leak a conflict for a row the caller does not own: {body}",
            attempt.label
        );
    }

    // The owner's rows are byte-identical: same name, same stage, same link,
    // same stamps, same timeline, same identifiers.
    assert_eq!(
        read_company(&app, &owner.access_token, &company_id).await,
        before_company,
        "a second account must not be able to change the company"
    );
    assert_eq!(
        read_application(&app, &owner.access_token, &application_id).await,
        before_application,
        "a second account must not be able to change the application"
    );
    assert_eq!(
        list_events(&app, &owner.access_token, &application_id, "")
            .await
            .expect_status(StatusCode::OK),
        before_events,
        "a second account must not be able to change the timeline"
    );

    // The company-crossing probes did not move the application anywhere, and in
    // particular not onto the second account's own company.
    let stored = read_application(&app, &owner.access_token, &application_id).await;
    assert_eq!(
        stored["company_id"],
        company_id.as_str(),
        "the application must still belong to the owner's company"
    );
    assert_ne!(
        stored["company_id"],
        other_company_id.as_str(),
        "the application must never move to a company of another account"
    );
    assert_eq!(
        stored["next_action_task_id"],
        task_id.to_string(),
        "the next-action link must survive the sweep"
    );

    // The filtered listing is not an oracle either: the filter is applied to
    // the caller's own rows, so it is an empty page, never a `404`.
    let page = list_applications(
        &app,
        &other.access_token,
        &format!("?company_id={company_id}"),
    )
    .await;
    let page = page.expect_status(StatusCode::OK);
    assert_eq!(fields_of(&page), PAGE_FIELDS);
    assert_pagination(&page, 0, 20, 0);
    assert!(
        page["items"].as_array().is_some_and(Vec::is_empty),
        "filtering by another account's company must return nothing: {page}"
    );

    // The owner still sees everything.
    let page = list_companies(&app, &owner.access_token, "").await;
    let page = page.expect_status(StatusCode::OK);
    assert_pagination(&page, 1, 20, 0);

    let page = list_applications(&app, &owner.access_token, "").await;
    let page = page.expect_status(StatusCode::OK);
    assert_pagination(&page, 1, 20, 0);

    // The second account removes its own company, which proves the sweep left it
    // able to, and makes the empty-page assertion below exact rather than a
    // statement about a listing that was never allowed to hold anything.
    let body = call(
        &app,
        Method::DELETE,
        &company_path(&other_company_id),
        &other.access_token,
        None,
    )
    .await
    .expect_status(StatusCode::NO_CONTENT);
    assert!(body.is_null(), "a 204 must carry no body: {body}");

    // The two listings the second account owns are empty, with `total: 0`.
    let page = list_companies(&app, &other.access_token, "").await;
    let page = page.expect_status(StatusCode::OK);
    assert_eq!(
        fields_of(&page),
        PAGE_FIELDS,
        "the listing must answer the standard envelope: {page}"
    );
    assert_eq!(fields_of(&page["pagination"]), PAGINATION_FIELDS);
    assert_pagination(&page, 0, 20, 0);
    assert!(
        page["items"].as_array().is_some_and(Vec::is_empty),
        "the second account must not see the owner's companies: {page}"
    );

    let page = list_applications(&app, &other.access_token, "").await;
    let page = page.expect_status(StatusCode::OK);
    assert_eq!(fields_of(&page), PAGE_FIELDS);
    assert_eq!(fields_of(&page["pagination"]), PAGINATION_FIELDS);
    assert_pagination(&page, 0, 20, 0);
    assert!(
        page["items"].as_array().is_some_and(Vec::is_empty),
        "the second account must not see the owner's applications: {page}"
    );
}

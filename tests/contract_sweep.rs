//! The whole-surface contract and ownership sweeps (`D5.H1` / `D5.H2`).
//!
//! `tests/openapi_contract.rs` proves that the route table, the router and the
//! generated document agree about *which* operations exist. This file asks the
//! two questions that comparison cannot answer:
//!
//! * **H1, the whole surface.** Every operation of both route tables is driven
//!   against the real router. An operation the document publishes with a bearer
//!   requirement must answer `401 unauthorized` to an unauthenticated call, an
//!   operation published without one must answer `400 bad_request` to the empty
//!   probe body, and the two probes outside the version prefix must answer
//!   `200`. Nothing may answer `404` or `405`: those are the router's answers
//!   for a path it does not mount and for a method a path does not declare.
//! * **H2, the ownership sweep.** One of every resource is created by account
//!   A, then account B addresses each one by id under its own token. Every
//!   id-taking operation the contract publishes must answer `404 not_found`,
//!   never `403`, and A's rows must read back unchanged.
//!
//! Both tests are driven by `ROUTE_TABLE` rather than a hand-written list, so an
//! operation added later is swept automatically. `tests/claims_api.rs`,
//! `tests/job_applications_api.rs`, `tests/ml_api.rs` and `tests/api_keys_api.rs`
//! sweep their own features in depth; this file is the cross-feature net that
//! catches a route added *without* an ownership predicate.
//!
//! The two tests share nothing but the harness: each registers its own accounts,
//! counts only the ids it created, and never truncates a table.

pub mod common;

use std::collections::BTreeSet;

use axum::Router;
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use utoipa::openapi::OpenApi;
use utoipa::openapi::path::{Operation, PathItem};
use uuid::Uuid;

use atom_api::api::routes::{API_PREFIX, ROUTE_TABLE, UNVERSIONED_ROUTE_TABLE, openapi_document};

/// The number of operations the locked contract publishes.
///
/// It mirrors the private constant of `tests/openapi_contract.rs`: 74 operations
/// under [`API_PREFIX`] - the cross-feature groups of `C-D0` to `D5`, the three
/// API-key operations `C-K1` adds (create, list and revoke) and the
/// twenty-two price-monitoring operations `ADR-0012` adds - plus the
/// two unversioned probes. The duplication is deliberate and the two constants
/// change together: that test pins the surface so it cannot shrink, this one
/// pins the sweep so it cannot drive less of it.
const CONTRACT_OPERATIONS: usize = 76;

/// The three operations that are public by contract.
///
/// `docs/features/authentication/api.md` publishes registration, login and the
/// refresh as the three operations a client calls **without** a credential, so
/// an unauthenticated probe cannot expect a `401` from them: the empty body is
/// rejected first, with `400 bad_request`.
///
/// The set is pinned and compared with the set derived from the generated
/// document, so both directions are covered: a protected operation that stops
/// declaring its bearer requirement is caught, and so is a public operation that
/// starts requiring one.
const PUBLIC_OPERATIONS: [&str; 3] = [
    "POST /api/v1/auth/register",
    "POST /api/v1/auth/login",
    "POST /api/v1/auth/refresh",
];

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Parses a verb of the route table.
fn method_of(name: &str) -> Method {
    Method::from_bytes(name.as_bytes())
        .unwrap_or_else(|error| panic!("{name} is not an HTTP method: {error}"))
}

/// Replaces every axum path placeholder with an identifier.
///
/// The replacement is per segment and the placeholder **name** decides the id,
/// so a route that renames a parameter is still swept instead of producing an
/// unparsable URI, and a route that introduces a new resource name fails loudly
/// in [`Fixtures::id_for`] rather than being skipped.
fn fill_placeholders(path: &str, resolve: impl Fn(&str) -> Uuid) -> String {
    path.split('/')
        .map(
            |segment| match segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                Some(name) => resolve(name).to_string(),
                None => segment.to_string(),
            },
        )
        .collect::<Vec<String>>()
        .join("/")
}

/// One path item of the generated document.
fn path_item<'a>(document: &'a OpenApi, path: &str) -> &'a PathItem {
    document.paths.get_path_item(path).unwrap_or_else(|| {
        panic!(
            "the OpenAPI document must describe the path {path}; \
             tests/openapi_contract.rs holds the document and the router together"
        )
    })
}

/// One operation of the generated document.
fn published_operation<'a>(document: &'a OpenApi, method: &str, path: &str) -> &'a Operation {
    let item = path_item(document, path);

    let operation = match method {
        "GET" => item.get.as_ref(),
        "POST" => item.post.as_ref(),
        "PATCH" => item.patch.as_ref(),
        "DELETE" => item.delete.as_ref(),
        "PUT" => item.put.as_ref(),
        other => panic!("{other} is a verb neither route table declares"),
    };

    operation.unwrap_or_else(|| panic!("the OpenAPI document must describe {method} {path}"))
}

/// Whether the document publishes a credential requirement for an operation.
///
/// `utoipa` writes no `security` attribute as `None`, so an operation without the
/// bearer requirement is exactly the public one.
fn requires_a_credential(operation: &Operation) -> bool {
    operation
        .security
        .as_ref()
        .is_some_and(|security| !security.is_empty())
}

// ---------------------------------------------------------------------------
// H1 — the whole surface
// ---------------------------------------------------------------------------

/// Every published operation is served, and answers what the document publishes.
///
/// The route table is the input, not a copy of it, so a new operation is swept
/// automatically. The two answers the router gives for a surface that does not
/// exist - `404` for an unmounted path and `405` for a method a path does not
/// declare - are asserted against explicitly first, so the failure message says
/// *that*, and not merely that the status was not the expected one.
#[tokio::test]
async fn every_published_operation_is_served_with_its_documented_status() {
    let app = common::app(common::test_state().await);
    let document = openapi_document(&common::test_config());

    assert!(
        !ROUTE_TABLE.is_empty(),
        "the route table must declare at least one operation, or this sweep asserts nothing"
    );

    let mut swept_public: BTreeSet<String> = BTreeSet::new();
    let mut swept_versioned = 0usize;

    for &(method, path) in ROUTE_TABLE {
        let published_path = format!("{API_PREFIX}{path}");
        let uri = fill_placeholders(&published_path, |_| Uuid::new_v4());

        assert!(
            !uri.contains('{') && !uri.contains('}'),
            "{method} {published_path} still carries a path placeholder the probe cannot fill: \
             {uri}"
        );

        let public =
            !requires_a_credential(published_operation(&document, method, &published_path));
        if public {
            swept_public.insert(format!("{method} {published_path}"));
        }

        let response =
            common::send(&app, common::request(method_of(method), &uri, None, None)).await;

        assert_ne!(
            response.status,
            StatusCode::NOT_FOUND,
            "{method} {uri} is published in the contract, but the router answered 404: the path \
             is not mounted, so a client generated from the document calls an operation that does \
             not exist"
        );
        assert_ne!(
            response.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri} is published in the contract, but the router answered 405: the path \
             exists and does not declare this method"
        );

        if public {
            assert_eq!(
                response.status,
                StatusCode::BAD_REQUEST,
                "{method} {uri} is published without a bearer requirement, so the empty probe body \
                 must be rejected with 400, not {}: {}",
                response.status,
                response.body
            );
            assert_eq!(
                response.body["error"]["code"], "bad_request",
                "{method} {uri} must answer the documented bad_request code: {}",
                response.body
            );
        } else {
            assert_eq!(
                response.status,
                StatusCode::UNAUTHORIZED,
                "{method} {uri} publishes a bearer requirement, so an unauthenticated call must \
                 answer 401 before the body is read, not {}: {}",
                response.status,
                response.body
            );
            assert_eq!(
                response.body["error"]["code"], "unauthorized",
                "{method} {uri} must answer the documented unauthorized code: {}",
                response.body
            );
        }

        swept_versioned += 1;
    }

    let mut swept_probes = 0usize;

    for &(method, path) in UNVERSIONED_ROUTE_TABLE {
        let uri = fill_placeholders(path, |_| Uuid::new_v4());

        let response =
            common::send(&app, common::request(method_of(method), &uri, None, None)).await;

        assert_ne!(
            response.status,
            StatusCode::NOT_FOUND,
            "{method} {uri} is published in the contract, but the router answered 404"
        );
        assert_ne!(
            response.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri} is published in the contract, but the router answered 405"
        );
        assert_eq!(
            response.status,
            StatusCode::OK,
            "{method} {uri} is a probe outside the version prefix and needs no credential, so it \
             must answer 200, not {}: {}",
            response.status,
            response.body
        );

        swept_probes += 1;
    }

    let expected_versioned = CONTRACT_OPERATIONS - UNVERSIONED_ROUTE_TABLE.len();

    assert_eq!(
        swept_versioned,
        expected_versioned,
        "the sweep drove {swept_versioned} versioned operations, but the locked contract publishes \
         {CONTRACT_OPERATIONS} operations, {} of which are the unversioned probes, so exactly \
         {expected_versioned} versioned operations must be swept",
        UNVERSIONED_ROUTE_TABLE.len()
    );
    assert_eq!(
        swept_versioned + swept_probes,
        CONTRACT_OPERATIONS,
        "the sweep drove {} operations in total ({swept_versioned} versioned and {swept_probes} \
         probes), but the locked contract publishes {CONTRACT_OPERATIONS}",
        swept_versioned + swept_probes
    );

    let expected_public: BTreeSet<String> = PUBLIC_OPERATIONS
        .iter()
        .map(|operation| (*operation).to_string())
        .collect();

    assert_eq!(
        swept_public, expected_public,
        "the operations the document publishes without a bearer requirement must stay the three \
         public authentication operations"
    );
}

// ---------------------------------------------------------------------------
// H2 — the ownership sweep
// ---------------------------------------------------------------------------

/// One of every resource, owned by the first account.
///
/// The field names are the path placeholders the contract uses, so
/// [`Fixtures::id_for`] is the single place that decides which row a swept
/// operation addresses.
struct Fixtures {
    note: Uuid,
    project: Uuid,
    task: Uuid,
    claim: Uuid,
    item: Uuid,
    company: Uuid,
    application: Uuid,
    event: Uuid,
    api_key: Uuid,
    product: Uuid,
    listing: Uuid,
    rule: Uuid,
    run: Uuid,
    alert: Uuid,
}

impl Fixtures {
    /// The id a path placeholder addresses.
    ///
    /// A placeholder this sweep does not know is a **failure**, not a skip: the
    /// whole point is that a resource added later cannot be left unaddressed by
    /// forgetting to extend a hand-written list.
    fn id_for(&self, placeholder: &str) -> Uuid {
        match placeholder {
            "note_id" => self.note,
            "project_id" => self.project,
            "task_id" => self.task,
            "claim_id" => self.claim,
            "item_id" => self.item,
            "company_id" => self.company,
            "application_id" => self.application,
            "event_id" => self.event,
            "key_id" => self.api_key,
            "product_id" => self.product,
            "listing_id" => self.listing,
            "rule_id" => self.rule,
            "run_id" => self.run,
            "alert_id" => self.alert,
            other => panic!(
                "the ownership sweep has no fixture for the path placeholder `{{{other}}}`: create \
                 one in `no_operation_reaches_another_accounts_row` and add the field to \
                 `Fixtures`, so the new operation is swept rather than silently skipped"
            ),
        }
    }
}

/// The bodies the id-taking write operations need.
///
/// The body exists so that the request fails on **ownership** and not on
/// validation: a `POST` with no body would be rejected by the JSON extractor
/// with `400` before the handler ever looked at the row. An empty string means
/// the verb takes no body at all, which is true of the four claim actions.
///
/// The sweep asserts that every entry here was used, so this table cannot rot
/// when a route is renamed or removed.
const OPERATION_BODIES: &[(&str, &str, &str)] = &[
    (
        "PATCH",
        "/notes/{note_id}",
        r#"{"title":"swept by the second account"}"#,
    ),
    (
        "PATCH",
        "/projects/{project_id}",
        r#"{"name":"swept by the second account"}"#,
    ),
    (
        "POST",
        "/projects/{project_id}/tasks",
        r#"{"title":"swept by the second account"}"#,
    ),
    (
        "PATCH",
        "/tasks/{task_id}",
        r#"{"title":"swept by the second account"}"#,
    ),
    (
        "PATCH",
        "/claims/{claim_id}",
        r#"{"description":"swept by the second account"}"#,
    ),
    (
        "POST",
        "/claims/{claim_id}/items",
        r#"{"description":"swept by the second account","amount":"1.0000","incurred_on":"2026-10-20"}"#,
    ),
    (
        "PATCH",
        "/claims/{claim_id}/items/{item_id}",
        r#"{"description":"swept by the second account"}"#,
    ),
    ("POST", "/claims/{claim_id}/submit", ""),
    ("POST", "/claims/{claim_id}/approve", ""),
    ("POST", "/claims/{claim_id}/reject", ""),
    ("POST", "/claims/{claim_id}/pay", ""),
    (
        "PATCH",
        "/companies/{company_id}",
        r#"{"name":"swept by the second account"}"#,
    ),
    (
        "PATCH",
        "/job-applications/{application_id}",
        r#"{"role_title":"swept by the second account"}"#,
    ),
    (
        "POST",
        "/job-applications/{application_id}/events",
        r#"{"kind":"interview","occurred_at":"2026-10-14T13:00:00Z","summary":"swept by the second account"}"#,
    ),
    (
        "PATCH",
        "/job-applications/{application_id}/events/{event_id}",
        r#"{"summary":"swept by the second account"}"#,
    ),
    ("POST", "/job-applications/{application_id}/match", "{}"),
    (
        "PATCH",
        "/products/{product_id}",
        r#"{"brand":"swept by the second account"}"#,
    ),
    (
        "POST",
        "/products/{product_id}/listings",
        r#"{"retailer":"swept by the second account","url":"https://swept.example.test/p/2","currency":"EUR"}"#,
    ),
    (
        "PATCH",
        "/products/{product_id}/listings/{listing_id}",
        r#"{"availability":"unavailable"}"#,
    ),
    (
        "POST",
        "/listings/{listing_id}/observations",
        r#"{"amount":"1.0000","currency":"EUR","observed_at":"2026-10-30T09:00:00Z","fingerprint":"sweep-probe-observation"}"#,
    ),
    (
        "POST",
        "/products/{product_id}/rules",
        r#"{"target_amount":"500.0000","target_currency":"EUR"}"#,
    ),
    (
        "PATCH",
        "/products/{product_id}/rules/{rule_id}",
        r#"{"enabled":false}"#,
    ),
    (
        "PATCH",
        "/automation/runs/{run_id}",
        r#"{"outcome":"failed"}"#,
    ),
    ("PATCH", "/alerts/{alert_id}", r#"{"read":true}"#),
];

/// The body the probe for one operation sends, if any.
///
/// A verb that carries no body returns `None` without an entry. A write verb
/// with no entry panics: sending it bodyless would test the extractor instead of
/// the ownership predicate. The key is recorded so the caller can prove that no
/// entry went unused.
fn probe_body(method: &str, path: &str, used: &mut BTreeSet<(String, String)>) -> Option<Value> {
    if !matches!(method, "POST" | "PATCH" | "PUT") {
        return None;
    }

    let (_, _, body) = OPERATION_BODIES
        .iter()
        .find(|(declared_method, declared_path, _)| {
            *declared_method == method && *declared_path == path
        })
        .unwrap_or_else(|| {
            panic!(
                "the ownership sweep drives {method} {path} with no declared probe body: add one to \
                 OPERATION_BODIES, or the request fails on validation instead of on ownership"
            )
        });

    used.insert((method.to_string(), path.to_string()));

    if body.is_empty() {
        return None;
    }

    Some(
        serde_json::from_str(body).unwrap_or_else(|error| {
            panic!("the probe body for {method} {path} is not JSON: {error}")
        }),
    )
}

/// Sends a create request and asserts the documented `201`.
async fn created(app: &Router, token: &str, uri: &str, body: Value) -> Value {
    let response = common::send(
        app,
        common::request(Method::POST, uri, Some(token), Some(body)),
    )
    .await;

    response.expect_status(StatusCode::CREATED)
}

/// Reads the created identifier out of a response body.
fn id_of(body: &Value) -> Uuid {
    let id = body["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the create response must carry a string id: {body}"));

    Uuid::parse_str(id)
        .unwrap_or_else(|error| panic!("the created id `{id}` is not a UUID: {error}"))
}

/// The identifiers a key listing carries.
///
/// `C-K1` publishes the page type without fixing whether the keys sit under an
/// `items` field or are the body itself, so both published spellings are read.
/// An empty page is a valid answer; a body under neither spelling is not.
fn api_key_ids(page: &Value) -> Vec<Uuid> {
    let items = page
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| page.as_array())
        .unwrap_or_else(|| panic!("the api-key listing must carry this account's keys: {page}"));

    items.iter().map(id_of).collect()
}

/// One operation reads another account's row, and the row survives.
///
/// Account A creates one of every resource the contract publishes a collection
/// for. Account B then addresses each one by id under its own token, using the
/// verbs the resource publishes. Every answer must be `404` with the standard
/// `not_found` envelope - never `403`, which would confirm that the row exists -
/// and A's resources must read back byte-identical, so the sweep also proves
/// that no write leaked through.
///
/// The operations come from the route table: every entry whose path carries a
/// placeholder is swept, including the nested item and event routes. A new
/// id-taking operation is therefore covered the day it is published, and a new
/// resource name fails loudly in [`Fixtures::id_for`].
#[tokio::test]
async fn no_operation_reaches_another_accounts_row() {
    let app = common::app(common::test_state().await);

    let owner = common::register_user(&app, "sweep-owner").await;
    let intruder = common::register_user(&app, "sweep-intruder").await;

    let owner_token = owner.access_token.as_str();
    let intruder_token = intruder.access_token.as_str();

    // ------------------------------------------------------------- fixtures
    let note = id_of(
        &created(
            &app,
            owner_token,
            "/api/v1/notes",
            json!({ "title": "swept note", "content": "owned by the first account" }),
        )
        .await,
    );

    let project = id_of(
        &created(
            &app,
            owner_token,
            "/api/v1/projects",
            json!({ "name": "swept project" }),
        )
        .await,
    );

    let task = id_of(
        &created(
            &app,
            owner_token,
            &format!("/api/v1/projects/{project}/tasks"),
            json!({ "title": "swept task" }),
        )
        .await,
    );

    let claim = id_of(
        &created(
            &app,
            owner_token,
            "/api/v1/claims",
            json!({ "title": "swept claim", "description": "", "currency": "EUR" }),
        )
        .await,
    );

    let item = id_of(
        &created(
            &app,
            owner_token,
            &format!("/api/v1/claims/{claim}/items"),
            json!({
                "description": "swept item",
                "amount": "1.0000",
                "incurred_on": "2026-10-20",
            }),
        )
        .await,
    );

    let company = id_of(
        &created(
            &app,
            owner_token,
            "/api/v1/companies",
            json!({ "name": "swept company" }),
        )
        .await,
    );

    let application = id_of(
        &created(
            &app,
            owner_token,
            "/api/v1/job-applications",
            json!({ "company_id": company, "role_title": "swept role" }),
        )
        .await,
    );

    let event = id_of(
        &created(
            &app,
            owner_token,
            &format!("/api/v1/job-applications/{application}/events"),
            json!({
                "kind": "interview",
                "occurred_at": "2026-10-14T13:00:00Z",
                "summary": "swept event",
            }),
        )
        .await,
    );

    // An API key is a credential, so the sweep needs its secret as well as its
    // id: the id is what account B addresses, and the secret is what proves
    // afterwards that B's delete did not revoke it.
    let api_key_body = created(
        &app,
        owner_token,
        "/api/v1/api-keys",
        json!({ "name": "swept api key" }),
    )
    .await;
    let api_key = id_of(&api_key_body);
    let api_key_secret = api_key_body["key"]
        .as_str()
        .unwrap_or_else(|| panic!("the create response must carry the secret: {api_key_body}"))
        .to_string();

    // The monitoring chain: a product, one of its listings, the product's one
    // rule, a run, and the two observations that trip the rule. The alert the
    // second observation raised is the fixture its placeholder addresses; the
    // sweep then proves account B can neither read it nor mark it.
    let product = id_of(
        &created(
            &app,
            owner_token,
            "/api/v1/products",
            json!({
                "brand": "Swept",
                "model": "Model X",
                "specs": "swept configuration",
            }),
        )
        .await,
    );

    let listing = id_of(
        &created(
            &app,
            owner_token,
            &format!("/api/v1/products/{product}/listings"),
            json!({
                "retailer": "Swept Retailer",
                "url": "https://swept.example.test/p/1",
                "currency": "EUR",
            }),
        )
        .await,
    );

    let rule = id_of(
        &created(
            &app,
            owner_token,
            &format!("/api/v1/products/{product}/rules"),
            json!({
                "target_amount": "500.0000",
                "target_currency": "EUR",
            }),
        )
        .await,
    );

    let run = id_of(&created(&app, owner_token, "/api/v1/automation/runs", json!({})).await);

    // The first observation is above the target and raises nothing; the second
    // reaches it and is the one the alert names.
    for (amount, fingerprint) in [
        ("999.0000", "sweep-observation-1"),
        ("499.0000", "sweep-observation-2"),
    ] {
        let recorded = common::send(
            &app,
            common::request(
                Method::POST,
                &format!("/api/v1/listings/{listing}/observations"),
                Some(owner_token),
                Some(json!({
                    "amount": amount,
                    "currency": "EUR",
                    "observed_at": "2026-10-30T09:00:00Z",
                    "fingerprint": fingerprint,
                })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);

        let _ = recorded;
    }

    let alerts_page = common::send(
        &app,
        common::request(Method::GET, "/api/v1/alerts", Some(owner_token), None),
    )
    .await
    .expect_status(StatusCode::OK);

    let alert = alerts_page["items"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|alert| alert["id"].as_str())
        .map(|id| {
            Uuid::parse_str(id)
                .unwrap_or_else(|error| panic!("the alert id `{id}` is not a UUID: {error}"))
        })
        .unwrap_or_else(|| {
            panic!(
                "the second observation must have tripped the rule and raised one alert: \
                 {alerts_page}"
            )
        });

    let fixtures = Fixtures {
        note,
        project,
        task,
        claim,
        item,
        company,
        application,
        event,
        api_key,
        product,
        listing,
        rule,
        run,
        alert,
    };

    // ------------------------------------------------------- state before
    //
    // The read is the evidence: a later comparison of the whole body catches a
    // write that succeeded, including one this account cannot see through its
    // own lists.
    let snapshots = [
        ("note", format!("/api/v1/notes/{note}")),
        ("project", format!("/api/v1/projects/{project}")),
        ("task", format!("/api/v1/tasks/{task}")),
        ("claim with its item", format!("/api/v1/claims/{claim}")),
        ("company", format!("/api/v1/companies/{company}")),
        (
            "job application",
            format!("/api/v1/job-applications/{application}"),
        ),
        (
            "event timeline",
            format!("/api/v1/job-applications/{application}/events"),
        ),
        ("tracked product", format!("/api/v1/products/{product}")),
        (
            "product listings",
            format!("/api/v1/products/{product}/listings"),
        ),
        (
            "price history",
            format!("/api/v1/listings/{listing}/observations"),
        ),
        ("alerts", "/api/v1/alerts".to_string()),
    ];

    let mut before: Vec<(&str, String, Value)> = Vec::new();
    for (name, path) in snapshots {
        let body = common::send(
            &app,
            common::request(Method::GET, &path, Some(owner_token), None),
        )
        .await
        .expect_status(StatusCode::OK);

        before.push((name, path, body));
    }

    // ------------------------------------------------------------- sweep
    let mut used_bodies: BTreeSet<(String, String)> = BTreeSet::new();
    let mut swept_operations: BTreeSet<String> = BTreeSet::new();

    for &(method, path) in ROUTE_TABLE {
        if !path.contains('{') {
            continue;
        }

        let uri = fill_placeholders(&format!("{API_PREFIX}{path}"), |placeholder| {
            fixtures.id_for(placeholder)
        });

        let body = probe_body(method, path, &mut used_bodies);

        let response = common::send(
            &app,
            common::request(method_of(method), &uri, Some(intruder_token), body),
        )
        .await;

        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "{method} {uri} addressed account A's row with account B's token and answered {} \
             instead of 404; a 403 confirms the row exists, and anything else means the operation \
             reached a row it does not own. The error code was {:?}.",
            response.status,
            response.body["error"]["code"]
        );
        assert_eq!(
            response.body["error"]["code"], "not_found",
            "{method} {uri} must answer the standard not_found envelope, because the caller may \
             not learn that the row exists: {}",
            response.body
        );

        swept_operations.insert(format!("{method} {API_PREFIX}{path}"));
    }

    // Every id-taking entry of the table was driven, and the api-key revoke is
    // named explicitly: a route that quietly disappeared from the table would
    // otherwise leave this sweep passing with one operation fewer.
    let id_taking = ROUTE_TABLE
        .iter()
        .filter(|(_, path)| path.contains('{'))
        .count();

    assert_eq!(
        swept_operations.len(),
        id_taking,
        "the sweep drove {} id-taking operations, but the route table declares {id_taking}: a \
         route was skipped rather than probed",
        swept_operations.len()
    );
    assert!(
        swept_operations.contains("DELETE /api/v1/api-keys/{key_id}"),
        "the sweep must have addressed account A's api key with account B's token; it drove \
         {swept_operations:?}"
    );

    assert!(
        !swept_operations.is_empty(),
        "the route table publishes no id-taking operation, so this sweep asserted nothing"
    );

    for &(method, path, _) in OPERATION_BODIES {
        assert!(
            used_bodies.contains(&(method.to_string(), path.to_string())),
            "OPERATION_BODIES declares a probe body for {method} {path}, but no route in the table \
             took an id on that path: the operation was renamed or removed, so the entry must be \
             updated or deleted"
        );
    }

    // -------------------------------------------------------- state after
    for (name, path, expected) in before {
        let body = common::send(
            &app,
            common::request(Method::GET, &path, Some(owner_token), None),
        )
        .await
        .expect_status(StatusCode::OK);

        assert_eq!(
            body, expected,
            "the ownership sweep changed account A's {name} through {path}: the row read `{body}` \
             after account B addressed it, but `{expected}` before"
        );
    }

    let collections = [
        ("notes", "/api/v1/notes".to_string()),
        ("projects", "/api/v1/projects".to_string()),
        (
            "tasks of the swept project",
            format!("/api/v1/projects/{project}/tasks"),
        ),
        ("claims", "/api/v1/claims".to_string()),
        ("companies", "/api/v1/companies".to_string()),
        ("job applications", "/api/v1/job-applications".to_string()),
        ("tracked products", "/api/v1/products".to_string()),
        ("automation runs", "/api/v1/automation/runs".to_string()),
        ("alerts", "/api/v1/alerts".to_string()),
    ];

    for (name, path) in collections {
        let body = common::send(
            &app,
            common::request(Method::GET, &path, Some(owner_token), None),
        )
        .await
        .expect_status(StatusCode::OK);

        assert_eq!(
            body["pagination"]["total"], 1,
            "account A must still hold exactly the one {name} this sweep created; the sweep \
             created or removed a row through another account's token: {body}"
        );
    }

    for path in [
        "/api/v1/notes",
        "/api/v1/projects",
        "/api/v1/claims",
        "/api/v1/companies",
        "/api/v1/job-applications",
        "/api/v1/products",
        "/api/v1/automation/runs",
        "/api/v1/alerts",
    ] {
        let body = common::send(
            &app,
            common::request(Method::GET, path, Some(intruder_token), None),
        )
        .await
        .expect_status(StatusCode::OK);

        assert_eq!(
            body["pagination"]["total"], 0,
            "account B must see none of account A's rows through {path}: {body}"
        );
    }

    // --------------------------------------------------------- api keys
    //
    // The key collection is asserted directly rather than through the loop
    // above: `C-K1` leaves the page unpaginated in the contract, so
    // `pagination.total` is not published for it, while "A holds its key and B
    // does not see it" is.
    let keys = common::send(
        &app,
        common::request(Method::GET, "/api/v1/api-keys", Some(owner_token), None),
    )
    .await
    .expect_status(StatusCode::OK);

    assert!(
        api_key_ids(&keys).contains(&api_key),
        "account A must still hold the api key this sweep created: {keys}"
    );

    let keys = common::send(
        &app,
        common::request(Method::GET, "/api/v1/api-keys", Some(intruder_token), None),
    )
    .await
    .expect_status(StatusCode::OK);

    assert!(
        !api_key_ids(&keys).contains(&api_key),
        "account B must see none of account A's api keys: {keys}"
    );

    // The key account B tried to revoke still authenticates as account A, so
    // the ownership predicate refused the write rather than half-applying it.
    let profile = common::send(
        &app,
        common::request(
            Method::GET,
            "/api/v1/auth/me",
            Some(api_key_secret.as_str()),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::OK);

    assert_eq!(
        profile["id"],
        owner.id.to_string(),
        "account B's delete must not have revoked account A's key: {profile}"
    );
}

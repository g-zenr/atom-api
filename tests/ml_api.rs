//! HTTP-level tests for the local machine-learning endpoints.
//!
//! `POST /api/v1/notes/search`, `POST /api/v1/job-applications/{id}/match` and
//! `POST /api/v1/ml/reindex` are the three operations of
//! [`docs/features/ml/api.md`]. This file is the integration suite the build
//! contract in `docs/features/ml/architecture.md` §12.7 names: every test here
//! carries exactly that name, because the contract is the source of the list.
//!
//! Four rules shape the file.
//!
//! * **No model file, no network.** Every scored mode runs under
//!   `ml.backend = deterministic`, the implementation `ADR-0008` §7.1 reserves
//!   for the test suite. The two degradation triggers are produced by the two
//!   configuration states that need no artifact at all: `ml.enabled = false`,
//!   and `ml.enabled = true` with `flex` and no weights. Nothing here loads
//!   tensors or opens a socket.
//! * **Behaviour, not implementation.** Every assertion is on a status code or
//!   on the observable body. The invented helper [`plant_vector`] writes unit
//!   vectors straight into `embeddings` so a test can state the score it
//!   expects; that is a fixture, and the assertion stays on the response.
//! * **Independence.** Each test registers its own account and restricts every
//!   count to that account's id. No table is truncated and no test depends on
//!   the order another one ran in.
//! * **The four degradation fields are asserted together** (§12.6). Any one of
//!   them alone can pass by accident.
//!
//! The file is compiled only with the `ml` feature, so
//! `cargo test --no-default-features` still builds the base API
//! (`ADR-0008` §7.3).

#![cfg(feature = "ml")]

pub mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestResponse, TestUser, app, assert_pagination, register_user, request, send, test_state,
    test_state_with_a_missing_model, test_state_with_candidate_limit,
    test_state_with_deterministic_ml, test_state_with_ml_disabled, test_state_with_pagination,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use utoipa::openapi::OpenApi;
use uuid::Uuid;

use atom_api::api::routes::{API_PREFIX, openapi_document};
use atom_api::ml::{Deterministic, Embedder};

/// The search operation.
const SEARCH: &str = "/api/v1/notes/search";

/// The reindex operation.
const REINDEX: &str = "/api/v1/ml/reindex";

/// The notes collection, for the lexical comparison.
const NOTES: &str = "/api/v1/notes";

/// The companies collection, needed to create an application.
const COMPANIES: &str = "/api/v1/companies";

/// The applications collection.
const APPLICATIONS: &str = "/api/v1/job-applications";

/// The identifier the deterministic embedder reports and every planted row
/// carries (`docs/features/ml/architecture.md` §12.2).
const DETERMINISTIC_MODEL_ID: &str = "deterministic-sha256-v1-384";

/// The vector length of that embedder.
const DIMENSION: usize = 384;

/// The tolerance of a unit-norm comparison.
///
/// `embeddings.vector` is `REAL[]`, so every one of the 384 components is
/// rounded to `f32` on the way in and widened again on the way out. The
/// rounding error of one component is about `6e-8` relative, and the norm sums
/// 384 squared errors, so an exact `1.0` is arithmetically unreachable and a
/// comparison without a tolerance would fail on correct data. `1e-5` is the
/// bound the contract states (`architecture.md` §2 invariant 2 and §12.1
/// `embed`), which makes it the tolerance a test may hold the product to.
const UNIT_NORM_TOLERANCE: f64 = 1e-5;

/// The tolerance of a score comparison.
///
/// The score is published rounded to four decimals, and the value that reached
/// the response travelled through `f32`, so it is compared against its own
/// four-decimal value rather than bit for bit.
const SCORE_TOLERANCE: f64 = 1e-4;

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

/// Posts a search request and returns the raw response.
async fn search(app: &Router, token: &str, query: &str, body: Value) -> TestResponse {
    send(
        app,
        request(
            Method::POST,
            &format!("{SEARCH}{query}"),
            Some(token),
            Some(body),
        ),
    )
    .await
}

/// Posts a search request with the given mode and returns the raw response.
async fn search_mode(app: &Router, token: &str, query_text: &str, mode: &str) -> TestResponse {
    search(app, token, "", json!({ "query": query_text, "mode": mode })).await
}

/// Lists the caller's notes the way `lexical` mode must agree with.
async fn list_notes(app: &Router, token: &str, query: &str) -> TestResponse {
    send(
        app,
        request(Method::GET, &format!("{NOTES}{query}"), Some(token), None),
    )
    .await
}

/// Creates a note through the published API and returns its id.
async fn create_note(app: &Router, user: &TestUser, title: &str, content: &str) -> Uuid {
    let response = send(
        app,
        request(
            Method::POST,
            NOTES,
            Some(&user.access_token),
            Some(json!({ "title": title, "content": content })),
        ),
    )
    .await;

    Uuid::parse_str(&id_of(&response.expect_status(StatusCode::CREATED)))
        .expect("the note id is a UUID")
}

/// Creates a company and returns its id.
async fn create_company(app: &Router, user: &TestUser, name: &str) -> Uuid {
    let response = send(
        app,
        request(
            Method::POST,
            COMPANIES,
            Some(&user.access_token),
            Some(json!({ "name": name })),
        ),
    )
    .await;

    Uuid::parse_str(&id_of(&response.expect_status(StatusCode::CREATED)))
        .expect("the company id is a UUID")
}

/// Creates a job application and returns its id.
async fn create_application(
    app: &Router,
    user: &TestUser,
    company_id: Uuid,
    role_title: &str,
    description: &str,
) -> Uuid {
    let response = send(
        app,
        request(
            Method::POST,
            APPLICATIONS,
            Some(&user.access_token),
            Some(json!({
                "company_id": company_id,
                "role_title": role_title,
                "description": description,
            })),
        ),
    )
    .await;

    Uuid::parse_str(&id_of(&response.expect_status(StatusCode::CREATED)))
        .expect("the application id is a UUID")
}

/// A match request that applies the published defaults.
///
/// The body is optional **as a set of fields**, not as a document: the handler
/// extracts a JSON body, so an absent one is the `400` that
/// `a_match_request_with_no_body_is_refused` asserts on its own. Every other
/// match case therefore sends `{}`.
fn default_match_body() -> Option<Value> {
    Some(json!({}))
}

/// Posts a match request and returns the raw response.
async fn matching(
    app: &Router,
    token: &str,
    application_id: &str,
    body: Option<Value>,
) -> TestResponse {
    send(
        app,
        request(
            Method::POST,
            &format!("{APPLICATIONS}/{application_id}/match"),
            Some(token),
            body,
        ),
    )
    .await
}

/// Posts a reindex request and returns the raw response.
async fn reindex(app: &Router, token: &str) -> TestResponse {
    send(app, request(Method::POST, REINDEX, Some(token), None)).await
}

/// Deletes one note and returns the raw response.
async fn delete_note(app: &Router, token: &str, note_id: Uuid) -> TestResponse {
    send(
        app,
        request(
            Method::DELETE,
            &format!("{NOTES}/{note_id}"),
            Some(token),
            None,
        ),
    )
    .await
}

/// Deletes one application and returns the raw response.
async fn delete_application(app: &Router, token: &str, application_id: Uuid) -> TestResponse {
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

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

/// Parses the `id` of a single-resource response.
fn id_of(body: &Value) -> String {
    body["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the response must carry an id: {body}"))
        .to_string()
}

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

/// Asserts the stable code and that the detail names `field`.
fn assert_field_error(body: &Value, code: &str, field: &str) {
    assert_error(body, code);

    let details = body["error"]["details"]
        .as_array()
        .unwrap_or_else(|| panic!("the error must carry field details: {body}"));

    assert!(
        details.iter().any(|detail| detail["field"] == field),
        "the validation failure must name `{field}`: {body}"
    );
}

/// The `note.id` of every hit, in the order the server returned them.
fn hit_ids(body: &Value) -> Vec<String> {
    hits(body)
        .iter()
        .map(|hit| hit["note"]["id"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The hits of a search response.
fn hits(body: &Value) -> Vec<&Value> {
    body["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the search response must carry an items array: {body}"))
        .iter()
        .collect()
}

/// The score of every hit, as `f64`, or `None` where it is `null`.
fn scores(body: &Value) -> Vec<Option<f64>> {
    hits(body).iter().map(|hit| hit["score"].as_f64()).collect()
}

/// Asserts that a response is the published `SearchResponse` envelope.
///
/// The four degradation fields are always read together, because any one of
/// them alone can be right by accident (`architecture.md` §12.6).
fn assert_search_response(
    body: &Value,
    requested_mode: &str,
    applied_mode: &str,
    degraded: bool,
    reason: Option<&str>,
) {
    assert_eq!(
        body["requested_mode"], requested_mode,
        "the response must name the mode the client asked for: {body}"
    );
    assert_eq!(
        body["mode"], applied_mode,
        "the response must name the mode that was applied: {body}"
    );
    assert_eq!(
        body["degraded"], degraded,
        "the response must say whether it degraded: {body}"
    );
    assert_eq!(
        body["degraded_reason"],
        reason.map_or(Value::Null, |reason| Value::String(reason.to_string())),
        "`degraded_reason` is `null` exactly when `degraded` is `false`: {body}"
    );
}

/// Asserts that a paged response carries a complete, well-formed envelope.
fn assert_complete_page(body: &Value) {
    assert!(
        body["items"].is_array(),
        "the response must carry an items array: {body}"
    );
    for field in ["limit", "offset", "total", "has_more"] {
        assert!(
            body["pagination"].get(field).is_some(),
            "the pagination block must carry `{field}`: {body}"
        );
    }

    for hit in hits(body) {
        let note = &hit["note"];
        for field in [
            "id",
            "title",
            "content",
            "is_archived",
            "created_at",
            "updated_at",
        ] {
            assert!(
                note.get(field).is_some(),
                "a degraded response is still a complete response, but the note has no \
                 `{field}`: {hit}"
            );
        }
        assert!(
            hit.get("score").is_some(),
            "every hit carries a score field, `null` in lexical mode: {hit}"
        );
    }
}

/// Rounds a value to the four decimals the contract publishes.
fn round_four(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// The Euclidean norm, in `f64` so the sum does not lose precision.
fn l2(values: &[f32]) -> f64 {
    values
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt()
}

/// The dot product, in `f64`. Two unit vectors, so this is the cosine.
fn dot(left: &[f32], right: &[f32]) -> f64 {
    assert_eq!(
        left.len(),
        right.len(),
        "the two vectors of a cosine must have the same length"
    );

    left.iter()
        .zip(right)
        .map(|(left, right)| f64::from(*left) * f64::from(*right))
        .sum()
}

/// A unit vector perpendicular to `vector`.
///
/// It is built from the axis the input is *least* aligned with, so the
/// subtraction below never cancels: any other axis could be close enough to the
/// input for the result to lose most of its length.
fn perpendicular_to(vector: &[f32]) -> Vec<f32> {
    let axis = (0..vector.len())
        .min_by(|left, right| {
            vector[*left]
                .abs()
                .partial_cmp(&vector[*right].abs())
                .expect("a component is finite")
        })
        .expect("a vector has at least one component");

    let mut perpendicular = vec![0.0_f32; vector.len()];
    perpendicular[axis] = 1.0;

    let projection = f64::from(vector[axis]);
    for (index, value) in perpendicular.iter_mut().enumerate() {
        *value -= (projection * f64::from(vector[index])) as f32;
    }

    let norm = l2(&perpendicular);
    perpendicular
        .iter()
        .map(|value| (f64::from(*value) / norm) as f32)
        .collect()
}

/// A unit combination `weight * basis + remainder * perpendicular`.
///
/// The two inputs are orthonormal to within `f32`, so the result is unit length
/// and its dot product with `basis` is `weight`.
fn mix(basis: &[f32], perpendicular: &[f32], weight: f64) -> Vec<f32> {
    let remainder = (1.0 - weight * weight).sqrt();

    let mixed: Vec<f32> = basis
        .iter()
        .zip(perpendicular)
        .map(|(basis, perpendicular)| {
            (weight * f64::from(*basis) + remainder * f64::from(*perpendicular)) as f32
        })
        .collect();

    let norm = l2(&mixed);
    mixed
        .iter()
        .map(|value| (f64::from(*value) / norm) as f32)
        .collect()
}

// ---------------------------------------------------------------------------
// SQL fixtures
//
// The API writes embeddings only through `POST /ml/reindex`. A test that needs
// a *known* pair of vectors plants them, exactly as `tests/claims_api.rs` writes
// a claim state the claim API cannot produce. Every planted vector is unit
// length, because `ADR-0008` §9 forbids the product from storing anything else
// and a test must not exercise a shape the product cannot produce.
// ---------------------------------------------------------------------------

/// Writes one embedding row for a source owned by `owner`.
async fn plant_vector(
    db: &PgPool,
    owner: Uuid,
    source_type: &str,
    source_id: Uuid,
    vector: &[f32],
) {
    assert!(
        (1.0 - l2(vector)).abs() <= UNIT_NORM_TOLERANCE,
        "a planted vector must be unit length, like every stored vector"
    );

    sqlx::query(
        "INSERT INTO embeddings
            (id, user_id, source_type, source_id, model_id, dim, vector)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (user_id, source_type, source_id, model_id)
         DO UPDATE SET vector = EXCLUDED.vector, dim = EXCLUDED.dim, updated_at = NOW()",
    )
    .bind(Uuid::new_v4())
    .bind(owner)
    .bind(source_type)
    .bind(source_id)
    .bind(DETERMINISTIC_MODEL_ID)
    .bind(i32::try_from(vector.len()).expect("the dimension fits an i32"))
    .bind(vector.to_vec())
    .execute(db)
    .await
    .unwrap_or_else(|error| panic!("the fixture embedding must be inserted: {error}"));
}

/// One stored embedding row: source type, source id, model id, dim, vector.
type EmbeddingRow = (String, Uuid, String, i32, Vec<f32>);

/// Every embedding row of one account.
async fn embeddings_of(db: &PgPool, owner: Uuid) -> Vec<EmbeddingRow> {
    sqlx::query_as::<_, EmbeddingRow>(
        "SELECT source_type, source_id, model_id, dim, vector
         FROM embeddings
         WHERE user_id = $1
         ORDER BY source_type, source_id",
    )
    .bind(owner)
    .fetch_all(db)
    .await
    .expect("the embeddings of the account must be readable")
}

/// The number of embedding rows of one account.
async fn embedding_count(db: &PgPool, owner: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM embeddings WHERE user_id = $1")
        .bind(owner)
        .fetch_one(db)
        .await
        .expect("the embeddings of the account must be countable")
}

/// The vector the deterministic embedder produces for `text`.
///
/// The service embeds the trimmed query, so the fixture embeds the same string.
fn query_vector(text: &str) -> Vec<f32> {
    Deterministic::new(DIMENSION)
        .embed(text)
        .expect("the deterministic embedder never fails")
}

// ---------------------------------------------------------------------------
// 12.7 - the search modes
// ---------------------------------------------------------------------------

/// `ML-R05`: `lexical` is the Phase 1 full-text search, not a second one.
///
/// The same query is sent to `POST /notes/search` and to `GET /notes?search=`,
/// and the two answers must be the same ids in the same order with the same
/// pagination. A second implementation of the text search would let the two
/// disagree, which is exactly what the contract forbids.
#[tokio::test]
async fn lexical_mode_matches_the_notes_search() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "ml-lexical-matches").await;

    create_note(
        &app,
        &user,
        "Kubernetes rollout",
        "blue-green with a canary",
    )
    .await;
    create_note(
        &app,
        &user,
        "Interview notes",
        "the kubernetes round went well",
    )
    .await;
    create_note(&app, &user, "Groceries", "milk, bread, coffee").await;

    let response = search(
        &app,
        &user.access_token,
        "?limit=20",
        json!({ "query": "kubernetes", "mode": "lexical" }),
    )
    .await;
    let searched = response.expect_status(StatusCode::OK);

    let response = list_notes(&app, &user.access_token, "?search=kubernetes&limit=20").await;
    let listed = response.expect_status(StatusCode::OK);

    let listed_ids: Vec<String> = listed["items"]
        .as_array()
        .expect("the notes listing is an array envelope")
        .iter()
        .map(|note| note["id"].as_str().unwrap_or_default().to_string())
        .collect();

    assert_eq!(
        hit_ids(&searched),
        listed_ids,
        "lexical mode must return exactly the notes the notes search returns, in its order"
    );
    assert_eq!(
        searched["pagination"], listed["pagination"],
        "the two must agree about the page as well as the items"
    );
    assert!(
        !listed_ids.is_empty(),
        "the fixture must produce lexical hits, otherwise the equality is vacuous"
    );
}

#[tokio::test]
async fn lexical_mode_returns_a_null_score() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "ml-lexical-score").await;

    create_note(&app, &user, "Kubernetes rollout", "a canary release").await;
    create_note(&app, &user, "Kubernetes upgrade", "the control plane").await;

    let response = search_mode(&app, &user.access_token, "kubernetes", "lexical").await;
    let body = response.expect_status(StatusCode::OK);

    assert_search_response(&body, "lexical", "lexical", false, None);
    assert_complete_page(&body);
    assert_eq!(scores(&body).len(), 2, "both notes must match: {body}");
    for score in scores(&body) {
        assert_eq!(
            score, None,
            "lexical mode ranks nothing, so every score is null: {body}"
        );
    }
}

#[tokio::test]
async fn hybrid_returns_a_score_for_every_hit() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-hybrid-score").await;

    let query = "kubernetes";
    for title in ["Kubernetes rollout", "Kubernetes upgrade"] {
        let note_id = create_note(&app, &user, title, "a canary release").await;
        plant_vector(&db, user.id, "note", note_id, &query_vector(title)).await;
    }

    let response = search_mode(&app, &user.access_token, query, "hybrid").await;
    let body = response.expect_status(StatusCode::OK);

    assert_search_response(&body, "hybrid", "hybrid", false, None);
    assert_eq!(
        hits(&body).len(),
        2,
        "both candidates are embedded, so both are scored: {body}"
    );
    for score in scores(&body) {
        let score = score.unwrap_or_else(|| panic!("hybrid mode scores every hit: {body}"));
        assert!(
            (0.0..=1.0).contains(&score),
            "a published score is in [0,1]: {score}"
        );
    }
}

/// The sort is total: `score DESC`, then `note_id ASC`.
///
/// One note scores above the other two, and the other two are planted with the
/// **same** vector, so their scores are equal and only the identifier can order
/// them. The expected order is therefore derived from the ids the API returned
/// rather than guessed.
#[tokio::test]
async fn hybrid_orders_by_score_then_note_id() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-hybrid-order").await;

    let query = "kubernetes";
    let basis = query_vector(query);
    let perpendicular = perpendicular_to(&basis);

    let top = create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    plant_vector(&db, user.id, "note", top, &basis).await;

    let first_tie = create_note(&app, &user, "Kubernetes upgrade", "the control plane").await;
    let second_tie = create_note(&app, &user, "Kubernetes migration", "the api server").await;
    let tied = mix(&basis, &perpendicular, 0.8);
    plant_vector(&db, user.id, "note", first_tie, &tied).await;
    plant_vector(&db, user.id, "note", second_tie, &tied).await;

    let response = search_mode(&app, &user.access_token, query, "hybrid").await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(hits(&body).len(), 3, "every candidate is scored: {body}");

    let ids = hit_ids(&body);
    assert_eq!(
        ids[0],
        top.to_string(),
        "the highest score must come first: {body}"
    );

    let mut tied_expected = vec![first_tie.to_string(), second_tie.to_string()];
    tied_expected.sort();

    assert_eq!(
        &ids[1..],
        tied_expected.as_slice(),
        "equal scores must be ordered by note id ascending: {body}"
    );

    let returned = scores(&body);
    let values: Vec<f64> = returned.iter().flatten().copied().collect();
    assert_eq!(values.len(), 3, "all three hits carry a score: {body}");
    assert!(
        values[0] > values[1] && values[1] == values[2],
        "the first score must be the largest and the tie must be exact: {values:?}"
    );
}

/// `ML-R05`: the hybrid re-rank is bounded by `ml.candidate_limit`.
///
/// Six notes match lexically and all six are embedded, so a response that
/// returned all of them would overflow the bound. The page bound is set to the
/// same value as the candidate bound, because the configuration refuses a
/// candidate limit below the maximum page size.
#[tokio::test]
async fn hybrid_never_returns_more_than_the_candidate_limit() {
    const CANDIDATES: i64 = 3;

    let state = test_state_with_candidate_limit(CANDIDATES, CANDIDATES).await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-hybrid-bound").await;

    for index in 0..6 {
        let note_id = create_note(
            &app,
            &user,
            &format!("Kubernetes rollouts {index}"),
            "a canary release behind a feature flag",
        )
        .await;
        plant_vector(
            &db,
            user.id,
            "note",
            note_id,
            &query_vector(&format!("note-{index}")),
        )
        .await;
    }

    let response = search(
        &app,
        &user.access_token,
        "?limit=3",
        json!({ "query": "kubernetes", "mode": "hybrid" }),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    let total = body["pagination"]["total"]
        .as_i64()
        .unwrap_or_else(|| panic!("the pagination block must carry a total: {body}"));

    assert!(
        hits(&body).len() as i64 <= CANDIDATES,
        "the page may never exceed the candidate limit of {CANDIDATES}: {body}"
    );
    assert!(
        total <= CANDIDATES,
        "the hybrid total is the size of the bounded candidate set, so {total} > {CANDIDATES} \
         means the re-rank read the whole corpus: {body}"
    );
    assert_eq!(
        total, CANDIDATES,
        "six notes match lexically, so the candidate set must be full: {body}"
    );
    assert_eq!(hit_ids(&body).len(), 3, "the page is full as well: {body}");
}

/// A candidate the model has never seen is **not** returned by hybrid.
///
/// The alternative — a zero score — would bury the note, and a `null` score in
/// a mode defined by its scores would make the hit two-shaped
/// (`architecture.md` §4.2).
#[tokio::test]
async fn a_note_without_an_embedding_is_not_returned_by_hybrid() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-hybrid-unembedded").await;

    let first = create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    let second = create_note(&app, &user, "Kubernetes upgrade", "the control plane").await;
    let unembedded = create_note(&app, &user, "Kubernetes notes", "never embedded").await;

    plant_vector(&db, user.id, "note", first, &query_vector("first")).await;
    plant_vector(&db, user.id, "note", second, &query_vector("second")).await;

    let response = search_mode(&app, &user.access_token, "kubernetes", "hybrid").await;
    let body = response.expect_status(StatusCode::OK);

    let ids = hit_ids(&body);
    assert!(
        !ids.contains(&unembedded.to_string()),
        "a note with no embedding must not be returned by hybrid: {body}"
    );
    assert_eq!(
        ids.len(),
        2,
        "the two embedded candidates are returned: {body}"
    );
    assert_eq!(
        body["pagination"]["total"], 2,
        "the total counts the scored candidates, so the number and the items agree: {body}"
    );
}

#[tokio::test]
async fn semantic_search_returns_the_callers_notes() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-semantic").await;

    let first = create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    let second = create_note(&app, &user, "Groceries", "milk, bread, coffee").await;
    create_note(&app, &user, "Unembedded", "no vector at all").await;

    plant_vector(&db, user.id, "note", first, &query_vector("first")).await;
    plant_vector(&db, user.id, "note", second, &query_vector("second")).await;

    // A query that matches neither note lexically: semantic mode is a scan of
    // the caller's embeddings, not a text search.
    let response = search_mode(&app, &user.access_token, "zeppelin", "semantic").await;
    let body = response.expect_status(StatusCode::OK);

    assert_search_response(&body, "semantic", "semantic", false, None);

    let mut ids = hit_ids(&body);
    ids.sort();
    let mut expected = vec![first.to_string(), second.to_string()];
    expected.sort();

    assert_eq!(
        ids, expected,
        "semantic mode must return every embedded note of the caller, and only those: {body}"
    );
    assert_eq!(
        body["pagination"]["total"], 2,
        "the semantic total is the number of the caller's embedded notes: {body}"
    );
}

#[tokio::test]
async fn the_search_pagination_bounds_are_applied() {
    let state = test_state_with_pagination(3, 5).await;
    let app = app(state);
    let user = register_user(&app, "ml-pagination").await;

    for index in 0..7 {
        create_note(
            &app,
            &user,
            &format!("Gamma note {index}"),
            "gamma radiation",
        )
        .await;
    }

    // No limit: the configured default, not the shipped one.
    let response = search_mode(&app, &user.access_token, "gamma", "lexical").await;
    let body = response.expect_status(StatusCode::OK);
    assert_pagination(&body, 7, 3, 0);
    assert_eq!(hits(&body).len(), 3);

    // A limit inside the range is honoured.
    let response = search(
        &app,
        &user.access_token,
        "?limit=2",
        json!({ "query": "gamma", "mode": "lexical" }),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_pagination(&body, 7, 2, 0);
    assert_eq!(hits(&body).len(), 2);

    // A limit above the maximum is clamped to it.
    let response = search(
        &app,
        &user.access_token,
        "?limit=1000",
        json!({ "query": "gamma", "mode": "lexical" }),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_pagination(&body, 7, 5, 0);
    assert_eq!(hits(&body).len(), 5);

    // A non-positive limit falls back to the default; a negative offset is zero.
    let response = search(
        &app,
        &user.access_token,
        "?limit=0&offset=-5",
        json!({ "query": "gamma", "mode": "lexical" }),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_pagination(&body, 7, 3, 0);

    // A page past the end is empty with the right total, not an error.
    let response = search(
        &app,
        &user.access_token,
        "?limit=5&offset=6",
        json!({ "query": "gamma", "mode": "lexical" }),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_pagination(&body, 7, 5, 6);
    assert_eq!(hits(&body).len(), 1, "the last page holds the seventh note");
}

#[tokio::test]
async fn an_empty_query_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "ml-empty-query").await;

    for blank in ["", "   "] {
        let response = search_mode(&app, &user.access_token, blank, "lexical").await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "query");
    }
}

#[tokio::test]
async fn a_query_over_the_bound_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "ml-query-bound").await;

    // 200 characters are the bound and are accepted.
    let at_bound = "q".repeat(200);
    let response = search_mode(&app, &user.access_token, &at_bound, "lexical").await;
    response.expect_status(StatusCode::OK);

    // 201 are refused, and the field is named.
    let over = "q".repeat(201);
    let response = search_mode(&app, &user.access_token, &over, "lexical").await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "query");

    // The bound counts characters, not bytes: 200 two-byte characters are 400
    // bytes and must still be accepted.
    let wide = "é".repeat(200);
    let response = search_mode(&app, &user.access_token, &wide, "lexical").await;
    response.expect_status(StatusCode::OK);

    let over_wide = "é".repeat(201);
    let response = search_mode(&app, &user.access_token, &over_wide, "lexical").await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_field_error(&body, "validation_failed", "query");
}

/// An unknown mode is a `422`, never a silent degradation (§12.6).
#[tokio::test]
async fn an_unknown_search_mode_is_refused() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);
    let user = register_user(&app, "ml-unknown-mode").await;

    for mode in ["fuzzy", "SEMANTIC", "lexical-ish"] {
        let response = search_mode(&app, &user.access_token, "kubernetes", mode).await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_error(&body, "validation_failed");
        assert!(
            body.get("mode").is_none(),
            "a refused mode is an error envelope, never a degraded search response: {body}"
        );
    }
}

// ---------------------------------------------------------------------------
// 12.7 - the degradation rule
// ---------------------------------------------------------------------------

/// `ML-R06`: a disabled model turns a scored search into lexical results, not
/// an error.
#[tokio::test]
async fn search_degrades_to_lexical_when_ml_is_disabled() {
    let state = test_state_with_ml_disabled().await;
    let app = app(state);
    let user = register_user(&app, "ml-degrade-disabled").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    create_note(&app, &user, "Kubernetes upgrade", "the control plane").await;
    create_note(&app, &user, "Groceries", "milk, bread").await;

    for requested in ["hybrid", "semantic"] {
        let response = search_mode(&app, &user.access_token, "kubernetes", requested).await;
        let body = response.expect_status(StatusCode::OK);

        assert_search_response(&body, requested, "lexical", true, Some("ml_disabled"));
        assert_complete_page(&body);
        assert_eq!(
            hit_ids(&body).len(),
            2,
            "a degraded response carries the lexical results, not an empty page: {body}"
        );
        for score in scores(&body) {
            assert_eq!(
                score, None,
                "the applied mode is lexical, so no hit carries a score: {body}"
            );
        }
        assert_eq!(
            body["pagination"]["total"], 2,
            "the degraded total is the lexical total: {body}"
        );
    }
}

/// `ML-R06`: a missing model artifact is a configuration state, not an outage.
#[tokio::test]
async fn search_degrades_to_lexical_when_the_model_is_missing() {
    let state = test_state_with_a_missing_model().await;
    let app = app(state);
    let user = register_user(&app, "ml-degrade-missing").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    create_note(&app, &user, "Groceries", "milk, bread").await;

    for requested in ["hybrid", "semantic"] {
        let response = search_mode(&app, &user.access_token, "kubernetes", requested).await;
        let body = response.expect_status(StatusCode::OK);

        assert_search_response(&body, requested, "lexical", true, Some("model_missing"));
        assert_complete_page(&body);
        assert_eq!(
            hit_ids(&body).len(),
            1,
            "the lexical hit must still be returned when the model is absent: {body}"
        );
    }
}

/// A degraded answer differs from a lexical one in the four fields and in
/// nothing else.
#[tokio::test]
async fn a_degraded_response_is_a_complete_response() {
    let state = test_state_with_ml_disabled().await;
    let app = app(state);
    let user = register_user(&app, "ml-degraded-complete").await;

    for index in 0..3 {
        create_note(
            &app,
            &user,
            &format!("Kubernetes rollout {index}"),
            "the canary release",
        )
        .await;
    }
    create_note(&app, &user, "Groceries", "milk, bread").await;

    let response = search_mode(&app, &user.access_token, "kubernetes", "lexical").await;
    let lexical = response.expect_status(StatusCode::OK);

    let response = search(
        &app,
        &user.access_token,
        "?limit=2",
        json!({ "query": "kubernetes", "mode": "hybrid" }),
    )
    .await;
    let degraded = response.expect_status(StatusCode::OK);

    assert_search_response(&degraded, "hybrid", "lexical", true, Some("ml_disabled"));
    assert_complete_page(&degraded);

    // Same envelope, same page size, same order as the lexical answer.
    assert_eq!(
        degraded["pagination"],
        json!({ "limit": 2, "offset": 0, "total": 3, "has_more": true }),
        "the pagination block is the shared envelope, not a degraded variant: {degraded}"
    );
    assert_eq!(
        hit_ids(&degraded),
        hit_ids(&lexical)[..2],
        "the degraded page is the first page of the lexical answer: {degraded}"
    );

    // Every hit has the full published shape.
    for hit in hits(&degraded) {
        assert!(
            hit["note"].is_object(),
            "a hit is `{{ note, score }}`, never a bare note: {hit}"
        );
        assert!(
            hit["score"].is_null(),
            "the applied mode is lexical, so the score is null: {hit}"
        );
        assert!(
            hit["note"]["title"].is_string() && hit["note"]["is_archived"].is_boolean(),
            "the note is the full `NoteResponse` shape: {hit}"
        );
    }
    assert_eq!(hit_ids(&degraded).len(), 2);
}

/// The property `ML-R06` is really about: a missing model never becomes a
/// server error.
#[tokio::test]
async fn a_degraded_request_never_returns_500_or_503() {
    for state in [
        test_state_with_ml_disabled().await,
        test_state_with_a_missing_model().await,
    ] {
        let app = app(state);
        let user = register_user(&app, "ml-never-500").await;

        create_note(&app, &user, "Kubernetes rollout", "the canary release").await;

        for body in [
            json!({ "query": "kubernetes", "mode": "hybrid" }),
            json!({ "query": "kubernetes", "mode": "semantic" }),
            // The default mode is `hybrid`, so an omitted `mode` degrades too.
            json!({ "query": "kubernetes" }),
        ] {
            let response = search(&app, &user.access_token, "", body.clone()).await;

            assert_ne!(
                response.status,
                StatusCode::INTERNAL_SERVER_ERROR,
                "a missing model must never be a 500; body {body} answered {}: {}",
                response.status,
                response.body
            );
            assert_ne!(
                response.status,
                StatusCode::SERVICE_UNAVAILABLE,
                "a missing model must never be a 503; body {body} answered {}: {}",
                response.status,
                response.body
            );
            assert_eq!(
                response.status,
                StatusCode::OK,
                "a model that is absent is a configuration state, so the answer is 200; body \
                 {body} answered {}: {}",
                response.status,
                response.body
            );
        }
    }
}

#[tokio::test]
async fn lexical_mode_is_not_degraded() {
    let state = test_state_with_ml_disabled().await;
    let app = app(state);
    let user = register_user(&app, "ml-lexical-not-degraded").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;

    let response = search_mode(&app, &user.access_token, "kubernetes", "lexical").await;
    let body = response.expect_status(StatusCode::OK);

    assert_search_response(&body, "lexical", "lexical", false, None);
    assert_eq!(
        body["requested_mode"], body["mode"],
        "an applied mode equal to the requested one is the whole signal: {body}"
    );
    assert!(
        hits(&body)[0]["score"].is_null(),
        "a non-degraded lexical answer still carries no score: {body}"
    );
}

// ---------------------------------------------------------------------------
// 12.7 - ownership
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_never_returns_another_users_note() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let owner = register_user(&app, "ml-search-owner").await;
    let other = register_user(&app, "ml-search-other").await;

    let note_id = create_note(&app, &owner, "Kubernetes rollout", "the canary release").await;
    plant_vector(&db, owner.id, "note", note_id, &query_vector("kubernetes")).await;

    // The owner sees the note, so the isolation assertions are not vacuous.
    let response = search_mode(&app, &owner.access_token, "kubernetes", "hybrid").await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        hit_ids(&body).len(),
        1,
        "the owner must see the note: {body}"
    );

    // Every mode is empty for the other account, and none of them leaks a score
    // computed against the owner's vector.
    for requested in ["lexical", "hybrid", "semantic"] {
        let response = search_mode(&app, &other.access_token, "kubernetes", requested).await;
        let body = response.expect_status(StatusCode::OK);

        assert!(
            hits(&body).is_empty(),
            "{requested} returned another account's note: {body}"
        );
        assert_eq!(
            body["pagination"]["total"], 0,
            "{requested} counted another account's row: {body}"
        );
    }
}

#[tokio::test]
async fn reindex_is_owner_scoped() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let owner = register_user(&app, "ml-reindex-owner").await;
    let other = register_user(&app, "ml-reindex-other").await;

    create_note(&app, &owner, "Kubernetes rollout", "the canary release").await;
    create_note(&app, &owner, "Groceries", "milk, bread").await;

    let response = reindex(&app, &owner.access_token).await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["indexed"], 2, "the owner has two sources: {body}");

    let before = embedding_count(&db, owner.id).await;
    assert_eq!(before, 2);

    // The other account owns nothing, so its reindex writes nothing and reads
    // nothing of the owner's.
    let response = reindex(&app, &other.access_token).await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(
        body["indexed"], 0,
        "the other account has no sources: {body}"
    );
    assert_eq!(body["skipped"], 0, "the other account has no rows: {body}");
    assert_eq!(
        body["deleted"], 0,
        "the sweep may not delete another account's row: {body}"
    );

    assert_eq!(
        embedding_count(&db, owner.id).await,
        before,
        "a reindex of another account must not touch these rows"
    );
    assert_eq!(
        embedding_count(&db, other.id).await,
        0,
        "the other account must own no embeddings"
    );
}

#[tokio::test]
async fn matching_a_foreign_application_is_not_found() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);
    let owner = register_user(&app, "ml-match-owner").await;
    let other = register_user(&app, "ml-match-other").await;

    let company = create_company(&app, &owner, "Acme GmbH").await;
    let application = create_application(
        &app,
        &owner,
        company,
        "Backend engineer",
        "Rust and PostgreSQL",
    )
    .await;

    let response = matching(
        &app,
        &other.access_token,
        &application.to_string(),
        default_match_body(),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // A well-formed identifier that names nothing is the same answer.
    let response = matching(
        &app,
        &owner.access_token,
        &Uuid::new_v4().to_string(),
        default_match_body(),
    )
    .await;
    let body = response.expect_status(StatusCode::NOT_FOUND);
    assert_error(&body, "not_found");

    // The owner's own application is matchable, so the `404`s are the ownership
    // rule and not "match never works".
    let response = matching(
        &app,
        &owner.access_token,
        &application.to_string(),
        default_match_body(),
    )
    .await;
    response.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn the_ml_routes_require_a_credential() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);

    let application_id = Uuid::new_v4().to_string();

    let probes = [
        (
            "POST",
            SEARCH.to_string(),
            Some(json!({ "query": "kubernetes", "mode": "hybrid" })),
        ),
        (
            "POST",
            format!("{APPLICATIONS}/{application_id}/match"),
            Some(json!({ "mode": "hybrid" })),
        ),
        ("POST", REINDEX.to_string(), None),
    ];

    for (method, uri, body) in probes {
        let method = Method::from_bytes(method.as_bytes()).expect("a known method");

        let response = send(&app, request(method.clone(), &uri, None, body.clone())).await;
        let unauthorized = response.expect_status(StatusCode::UNAUTHORIZED);
        assert_error(&unauthorized, "unauthorized");

        // An unusable credential is refused the same way.
        let response = send(&app, request(method, &uri, Some("not-a-token"), body)).await;
        let unauthorized = response.expect_status(StatusCode::UNAUTHORIZED);
        assert_error(&unauthorized, "unauthorized");
    }
}

// ---------------------------------------------------------------------------
// 12.7 - matching
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_match_ranks_the_callers_notes() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-match-rank").await;

    let company = create_company(&app, &user, "Acme GmbH").await;
    let application = create_application(
        &app,
        &user,
        company,
        "Backend engineer",
        "Rust, PostgreSQL and a local model",
    )
    .await;

    let mut embedded = Vec::new();
    for index in 0..3 {
        let note_id = create_note(
            &app,
            &user,
            &format!("Application note {index}"),
            "Rust and PostgreSQL experience",
        )
        .await;
        plant_vector(
            &db,
            user.id,
            "note",
            note_id,
            &query_vector(&format!("note-{index}")),
        )
        .await;
        embedded.push(note_id.to_string());
    }
    let unembedded = create_note(&app, &user, "Draft", "never embedded").await;

    let response = matching(
        &app,
        &user.access_token,
        &application.to_string(),
        Some(json!({ "mode": "hybrid", "limit": 10 })),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(
        body["application_id"],
        application.to_string(),
        "the response names the application it ranked: {body}"
    );
    assert_eq!(
        body["mode"], "hybrid",
        "the applied mode is reported: {body}"
    );

    let matches = body["matches"]
        .as_array()
        .unwrap_or_else(|| panic!("a match response carries a matches array: {body}"));
    assert_eq!(
        matches.len(),
        3,
        "the three embedded notes are ranked, the unembedded one is not: {body}"
    );

    let mut previous = f64::INFINITY;
    for hit in matches {
        let note_id = hit["note_id"]
            .as_str()
            .unwrap_or_else(|| panic!("a match hit names the note: {hit}"))
            .to_string();
        assert!(
            embedded.contains(&note_id),
            "a match must rank the caller's embedded notes: {hit}"
        );
        assert_ne!(
            note_id,
            unembedded.to_string(),
            "a note with no embedding cannot be ranked: {hit}"
        );
        assert!(
            hit["title"].is_string(),
            "a match hit carries the note title: {hit}"
        );

        let score = hit["score"]
            .as_f64()
            .unwrap_or_else(|| panic!("a match hit carries a score: {hit}"));
        assert!(
            (0.0..=1.0).contains(&score),
            "a published score is in [0,1]: {score}"
        );
        assert!(
            score <= previous,
            "matches are ordered by score descending: {body}"
        );
        previous = score;
    }
}

#[tokio::test]
async fn a_match_returns_an_empty_list_when_nothing_is_embedded() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);
    let user = register_user(&app, "ml-match-empty").await;

    let company = create_company(&app, &user, "Acme GmbH").await;
    let application = create_application(&app, &user, company, "Backend engineer", "Rust").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;

    let response = matching(
        &app,
        &user.access_token,
        &application.to_string(),
        default_match_body(),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(
        body["matches"].as_array().map(Vec::len),
        Some(0),
        "with no stored vector there is nothing to rank, and an empty list is the honest answer: \
         {body}"
    );
}

/// `ML-R06`: the one place where the absence of a model **is** an error.
///
/// A match has no lexical fallback, so a degraded ranking would be a fiction.
#[tokio::test]
async fn matching_returns_conflict_when_ml_is_disabled() {
    let state = test_state_with_ml_disabled().await;
    let app = app(state);
    let user = register_user(&app, "ml-match-disabled").await;

    let company = create_company(&app, &user, "Acme GmbH").await;
    let application = create_application(&app, &user, company, "Backend engineer", "Rust").await;

    let response = matching(
        &app,
        &user.access_token,
        &application.to_string(),
        default_match_body(),
    )
    .await;
    let body = response.expect_status(StatusCode::CONFLICT);
    assert_error(&body, "conflict");

    // The same application answers `200` once a model is selected, so the
    // `409` is the model's absence and not the request.
    let enabled = test_state_with_deterministic_ml().await;
    let enabled_app = common::app(enabled);
    let user = register_user(&enabled_app, "ml-match-enabled").await;
    let company = create_company(&enabled_app, &user, "Acme GmbH").await;
    let application =
        create_application(&enabled_app, &user, company, "Backend engineer", "Rust").await;
    let response = matching(
        &enabled_app,
        &user.access_token,
        &application.to_string(),
        default_match_body(),
    )
    .await;
    response.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn a_match_limit_out_of_range_is_refused() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);
    let user = register_user(&app, "ml-match-limit").await;

    let company = create_company(&app, &user, "Acme GmbH").await;
    let application = create_application(&app, &user, company, "Backend engineer", "Rust").await;

    for limit in [0, -1, 51] {
        let response = matching(
            &app,
            &user.access_token,
            &application.to_string(),
            Some(json!({ "limit": limit })),
        )
        .await;
        let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_field_error(&body, "validation_failed", "limit");
    }

    // The bounds themselves are accepted.
    for limit in [1, 50] {
        let response = matching(
            &app,
            &user.access_token,
            &application.to_string(),
            Some(json!({ "limit": limit })),
        )
        .await;
        response.expect_status(StatusCode::OK);
    }
}

#[tokio::test]
async fn lexical_mode_is_refused_by_match() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);
    let user = register_user(&app, "ml-match-lexical").await;

    let company = create_company(&app, &user, "Acme GmbH").await;
    let application = create_application(&app, &user, company, "Backend engineer", "Rust").await;

    let response = matching(
        &app,
        &user.access_token,
        &application.to_string(),
        Some(json!({ "mode": "lexical" })),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");

    let response = matching(
        &app,
        &user.access_token,
        &application.to_string(),
        Some(json!({ "mode": "fuzzy" })),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&body, "validation_failed");
}

/// A match reads a JSON body, so an absent one is a malformed request: `400`,
/// the shared body extractor's mapping, not a `422` and not a default body.
#[tokio::test]
async fn a_match_request_with_no_body_is_refused() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);
    let user = register_user(&app, "ml-match-nobody").await;

    let company = create_company(&app, &user, "Acme GmbH").await;
    let application = create_application(&app, &user, company, "Backend engineer", "Rust").await;

    let response = matching(&app, &user.access_token, &application.to_string(), None).await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_error(&body, "bad_request");
}

// ---------------------------------------------------------------------------
// 12.7 - reindex
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reindexing_twice_leaves_the_same_rows() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-reindex-twice").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    create_note(&app, &user, "Groceries", "milk, bread").await;
    let company = create_company(&app, &user, "Acme GmbH").await;
    create_application(&app, &user, company, "Backend engineer", "Rust").await;

    let response = reindex(&app, &user.access_token).await;
    let first = response.expect_status(StatusCode::OK);
    let before = embeddings_of(&db, user.id).await;
    assert_eq!(before.len(), 3, "every source is embedded once: {first}");

    let response = reindex(&app, &user.access_token).await;
    let second = response.expect_status(StatusCode::OK);

    assert_eq!(
        second["indexed"], 0,
        "a second run writes nothing: {second}"
    );
    assert_eq!(second["skipped"], 3, "every source is skipped: {second}");
    assert_eq!(second["deleted"], 0, "nothing is orphaned: {second}");

    let after = embeddings_of(&db, user.id).await;
    assert_eq!(
        before, after,
        "a second run must leave the identical rows: same sources, same model, same vectors"
    );
}

/// `ML-R07`: the operation cannot do what it says without a model, so it is
/// refused rather than silently doing nothing.
#[tokio::test]
async fn reindex_is_refused_when_ml_is_disabled() {
    let state = test_state_with_ml_disabled().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-reindex-disabled").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;

    let response = reindex(&app, &user.access_token).await;
    let body = response.expect_status(StatusCode::CONFLICT);
    assert_error(&body, "conflict");

    assert_eq!(
        embedding_count(&db, user.id).await,
        0,
        "a refused reindex must write no row"
    );

    // The same account answers `200` once a model is selected.
    let enabled = test_state_with_deterministic_ml().await;
    let enabled_app = common::app(enabled);
    let user = register_user(&enabled_app, "ml-reindex-enabled").await;
    create_note(
        &enabled_app,
        &user,
        "Kubernetes rollout",
        "the canary release",
    )
    .await;
    let response = reindex(&enabled_app, &user.access_token).await;
    response.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn reindex_writes_one_row_per_source() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-reindex-sources").await;

    let first = create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    let second = create_note(&app, &user, "Groceries", "milk, bread").await;
    let third = create_note(&app, &user, "PostgreSQL tuning", "indexes").await;
    let company = create_company(&app, &user, "Acme GmbH").await;
    let application = create_application(&app, &user, company, "Backend engineer", "Rust").await;

    let response = reindex(&app, &user.access_token).await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        body["indexed"], 4,
        "three notes and one application, one row each: {body}"
    );

    let rows = embeddings_of(&db, user.id).await;
    assert_eq!(rows.len(), 4, "one row per source: {rows:?}");

    let mut found: Vec<(String, Uuid)> = rows
        .iter()
        .map(|(source_type, source_id, _, _, _)| (source_type.clone(), *source_id))
        .collect();
    found.sort();

    let mut expected = vec![
        ("application".to_string(), application),
        ("note".to_string(), first),
        ("note".to_string(), second),
        ("note".to_string(), third),
    ];
    expected.sort();

    assert_eq!(
        found, expected,
        "the row set is exactly the caller's notes and applications"
    );

    for (source_type, _, model_id, dim, vector) in &rows {
        assert_eq!(
            model_id, DETERMINISTIC_MODEL_ID,
            "the row records the embedder that produced it"
        );
        assert_eq!(
            *dim,
            i32::try_from(DIMENSION).expect("the dimension fits"),
            "the row records the vector length"
        );
        assert_eq!(vector.len(), DIMENSION, "the vector has that many elements");
        assert!(
            source_type == "note" || source_type == "application",
            "the two source types are the whole contract"
        );
    }
}

#[tokio::test]
async fn reindex_deletes_an_orphan() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-reindex-orphan").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;

    let response = reindex(&app, &user.access_token).await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["deleted"], 0, "nothing is orphaned yet: {body}");

    // A row whose source does not exist: the shape a delete outside the service
    // leaves behind. The polymorphic `source_id` has no foreign key, so the
    // database cannot prevent it, and the sweep is the backstop.
    let orphan = Uuid::new_v4();
    plant_vector(&db, user.id, "note", orphan, &query_vector("orphan")).await;
    assert_eq!(embedding_count(&db, user.id).await, 2);

    let response = reindex(&app, &user.access_token).await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["deleted"], 1, "the sweep removes the orphan: {body}");
    assert_eq!(
        body["indexed"], 0,
        "the real source is already embedded: {body}"
    );
    assert_eq!(body["skipped"], 1, "the real source is skipped: {body}");

    let rows = embeddings_of(&db, user.id).await;
    assert_eq!(rows.len(), 1, "only the real source keeps a row: {rows:?}");
    assert!(
        !rows
            .iter()
            .any(|(_, source_id, _, _, _)| *source_id == orphan),
        "the orphan row must be gone"
    );
}

#[tokio::test]
async fn reindex_reports_its_counts() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);
    let user = register_user(&app, "ml-reindex-counts").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    create_note(&app, &user, "Groceries", "milk, bread").await;
    let company = create_company(&app, &user, "Acme GmbH").await;
    create_application(&app, &user, company, "Backend engineer", "Rust").await;

    let response = reindex(&app, &user.access_token).await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["model_id"], DETERMINISTIC_MODEL_ID);
    assert_eq!(
        body["dimension"],
        i64::try_from(DIMENSION).expect("the dimension fits an i64"),
        "the reported dimension is the embedder's"
    );
    assert_eq!(body["indexed"], 3, "two notes and one application: {body}");
    assert_eq!(body["skipped"], 0, "a first run skips nothing: {body}");
    assert_eq!(body["deleted"], 0, "a first run deletes nothing: {body}");

    for field in ["indexed", "skipped", "deleted"] {
        assert!(
            body[field].is_i64(),
            "`{field}` is a JSON number, never a string: {body}"
        );
    }
}

#[tokio::test]
async fn deleting_a_note_removes_its_embeddings() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-delete-note").await;

    let note_id = create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    let kept = create_note(&app, &user, "Groceries", "milk, bread").await;

    let response = reindex(&app, &user.access_token).await;
    response.expect_status(StatusCode::OK);
    assert_eq!(embedding_count(&db, user.id).await, 2);

    delete_note(&app, &user.access_token, note_id)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    let rows = embeddings_of(&db, user.id).await;
    assert_eq!(
        rows.len(),
        1,
        "deleting a note must delete its embedding in the same statement: {rows:?}"
    );
    assert_eq!(
        rows[0].1, kept,
        "the surviving row belongs to the note that was not deleted"
    );

    // The delete did not merely make the row unreachable: a reindex has nothing
    // to sweep.
    let response = reindex(&app, &user.access_token).await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        body["deleted"], 0,
        "a correct delete leaves no orphan for the sweep: {body}"
    );
}

#[tokio::test]
async fn deleting_an_application_removes_its_embeddings() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-delete-application").await;

    let company = create_company(&app, &user, "Acme GmbH").await;
    let application = create_application(&app, &user, company, "Backend engineer", "Rust").await;
    let note = create_note(&app, &user, "Kubernetes rollout", "the canary release").await;

    let response = reindex(&app, &user.access_token).await;
    response.expect_status(StatusCode::OK);
    assert_eq!(embedding_count(&db, user.id).await, 2);

    delete_application(&app, &user.access_token, application)
        .await
        .expect_status(StatusCode::NO_CONTENT);

    let rows = embeddings_of(&db, user.id).await;
    assert_eq!(
        rows.len(),
        1,
        "deleting an application must delete its embedding in the same statement: {rows:?}"
    );
    assert_eq!(
        rows[0].1, note,
        "the surviving row belongs to the note, not the deleted application"
    );

    let response = reindex(&app, &user.access_token).await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        body["deleted"], 0,
        "a correct delete leaves no orphan for the sweep: {body}"
    );
}

// ---------------------------------------------------------------------------
// 12.7 - the published contract
// ---------------------------------------------------------------------------

/// One published component schema, as JSON.
fn component(document: &OpenApi, name: &str) -> Value {
    let components = document
        .components
        .as_ref()
        .unwrap_or_else(|| panic!("the document must publish a components section"));

    let schema = components.schemas.get(name).unwrap_or_else(|| {
        panic!(
            "the document must publish the `{name}` schema; it publishes {:?}",
            components.schemas.keys().collect::<Vec<&String>>()
        )
    });

    serde_json::to_value(schema).unwrap_or_else(|error| panic!("`{name}` must serialize: {error}"))
}

/// Whether a serialized schema node declares `type: <name>`.
fn has_type(map: &serde_json::Map<String, Value>, name: &str) -> bool {
    match map.get("type") {
        Some(Value::String(published)) => published == name,
        Some(Value::Array(published)) => published
            .iter()
            .any(|published| published.as_str() == Some(name)),
        _ => false,
    }
}

/// Whether a serialized schema carries a `number` branch anywhere.
fn carries_number(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            has_type(map, "number")
                || map
                    .get("format")
                    .is_some_and(|format| format == "double" || format == "float")
                || map.values().any(carries_number)
        }
        Value::Array(items) => items.iter().any(carries_number),
        _ => false,
    }
}

/// The three operations and the seven schemas are in the document.
#[test]
fn the_ml_routes_are_in_the_openapi_document() {
    let document = openapi_document(&common::test_config());

    let published = [
        ("POST".to_string(), format!("{API_PREFIX}/notes/search")),
        (
            "POST".to_string(),
            format!("{API_PREFIX}/job-applications/{{application_id}}/match"),
        ),
        ("POST".to_string(), format!("{API_PREFIX}/ml/reindex")),
    ];

    for (method, path) in &published {
        let item = document
            .paths
            .get_path_item(path)
            .unwrap_or_else(|| panic!("the document must describe the path {path}"));

        let operation = match method.as_str() {
            "POST" => item.post.as_ref(),
            other => panic!("{other} is not one of the methods this contract uses"),
        }
        .unwrap_or_else(|| panic!("the document must describe {method} {path}"));

        let statuses: Vec<&String> = operation.responses.responses.keys().collect();
        assert!(
            statuses.iter().any(|status| status.as_str() == "200"),
            "{method} {path} must publish the 200 success response; it publishes {statuses:?}"
        );
    }

    for schema in [
        "SearchRequest",
        "SearchResponse",
        "SearchHit",
        "MatchRequest",
        "MatchResponse",
        "MatchHit",
        "ReindexResponse",
    ] {
        component(&document, schema);
    }

    // The published surface really is the three new operations and nothing
    // else under the two collections they extend.
    let search = document
        .paths
        .get_path_item(format!("{API_PREFIX}/notes/search"))
        .expect("the search path must be described");
    assert!(
        search.get.is_none(),
        "only POST is published for the search path; a GET would be the note read"
    );
}

/// The search operation must not be shadowed by the note read.
///
/// `/notes/{note_id}` is a dynamic segment, so registering `POST /notes/search`
/// puts a static path beside it. Axum gives the static path precedence and
/// answers a verb it does not declare with `405`, so a `GET /notes/search` is
/// **not** answered by the note read: it reaches the search path, which owns
/// only `POST`.
///
/// **This is one place where the published contract and the router disagree.**
/// [`docs/features/ml/api.md`](../../docs/features/ml/api.md) §1 states that a
/// `GET` is matched by the note read and answers `400`; the implementation
/// answers `405`, because the static path wins the match before the dynamic one
/// is considered. The observable fact the test exists for — the note read does
/// not shadow the search operation — holds either way, and `405` proves it more
/// directly than `400` would. The sentence in `api.md` is the thing that needs
/// correcting; it is recorded here so the disagreement is not silent.
#[tokio::test]
async fn post_notes_search_is_not_shadowed_by_the_note_read() {
    let state = test_state_with_deterministic_ml().await;
    let app = app(state);
    let user = register_user(&app, "ml-not-shadowed").await;

    // GET /notes/search reaches the search path, which declares only POST. It is
    // therefore 405 - not the note read's 400, and not the router's 404.
    let response = send(
        &app,
        request(Method::GET, SEARCH, Some(&user.access_token), None),
    )
    .await;
    assert_eq!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /notes/search must reach the search path, which declares only POST: {}",
        response.body
    );

    // POST /notes/search without a credential is `401`, which proves the
    // operation is mounted: an unmounted path would be `404`, and a method the
    // path does not declare would be `405`.
    let response = send(
        &app,
        request(
            Method::POST,
            SEARCH,
            None,
            Some(json!({ "query": "kubernetes" })),
        ),
    )
    .await;
    assert_eq!(
        response.status,
        StatusCode::UNAUTHORIZED,
        "POST /notes/search must be served by the search operation: {}",
        response.body
    );

    // And it does its work for a credentialed caller.
    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    let response = search_mode(&app, &user.access_token, "kubernetes", "hybrid").await;
    response.expect_status(StatusCode::OK);
}

/// The pagination parameters are published once, on the query string, and the
/// body carries only `query` and `mode` (`api.md` §6).
#[test]
fn the_search_endpoint_publishes_pagination_once() {
    let document = openapi_document(&common::test_config());
    let path = format!("{API_PREFIX}/notes/search");

    let item = document
        .paths
        .get_path_item(&path)
        .unwrap_or_else(|| panic!("the document must describe {path}"));
    let operation = item
        .post
        .as_ref()
        .unwrap_or_else(|| panic!("the document must describe POST {path}"));

    let parameters: Vec<(String, String)> = item
        .parameters
        .iter()
        .flatten()
        .chain(operation.parameters.iter().flatten())
        .map(|parameter| {
            (
                parameter.name.clone(),
                match parameter.parameter_in {
                    utoipa::openapi::path::ParameterIn::Query => "query".to_string(),
                    utoipa::openapi::path::ParameterIn::Path => "path".to_string(),
                    utoipa::openapi::path::ParameterIn::Header => "header".to_string(),
                    utoipa::openapi::path::ParameterIn::Cookie => "cookie".to_string(),
                },
            )
        })
        .collect();

    let mut query: Vec<&str> = parameters
        .iter()
        .filter(|(_, place)| place == "query")
        .map(|(name, _)| name.as_str())
        .collect();
    query.sort_unstable();

    assert_eq!(
        query,
        vec!["limit", "offset"],
        "the search publishes exactly the shared pagination pair, once each: {parameters:?}"
    );

    // The body schema carries no pagination parameter.
    let schema = component(&document, "SearchRequest");
    let properties = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("SearchRequest must publish its properties: {schema}"))
        .keys()
        .cloned()
        .collect::<Vec<String>>();

    let mut properties = properties;
    properties.sort();
    assert_eq!(
        properties,
        vec!["mode".to_string(), "query".to_string()],
        "the body carries only `query` and `mode`, so the page bound has one home: {schema}"
    );

    let required = schema["required"]
        .as_array()
        .map(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    assert!(
        required.iter().any(|field| field == "query"),
        "`query` is required: {schema}"
    );
}

/// A score is a JSON **number**, never a string, and `null` in lexical mode.
#[tokio::test]
async fn the_score_is_a_number() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-score-type").await;

    let note_id = create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    plant_vector(&db, user.id, "note", note_id, &query_vector("kubernetes")).await;

    let response = search_mode(&app, &user.access_token, "kubernetes", "hybrid").await;
    let body = response.expect_status(StatusCode::OK);

    let score = &hits(&body)[0]["score"];
    assert!(
        score.is_number(),
        "a score is a similarity, so it is a JSON number: {body}"
    );
    assert!(!score.is_string(), "a score is never a string: {body}");

    let response = search_mode(&app, &user.access_token, "kubernetes", "lexical").await;
    let body = response.expect_status(StatusCode::OK);
    assert!(
        hits(&body)[0]["score"].is_null(),
        "lexical mode publishes a null score: {body}"
    );

    // The document publishes the same shape: a number, with a null branch.
    let document = openapi_document(&common::test_config());
    let hit = component(&document, "SearchHit");
    let score = hit["properties"]
        .get("score")
        .cloned()
        .unwrap_or_else(|| panic!("SearchHit must publish `score`: {hit}"));

    assert!(
        carries_number(&score),
        "SearchHit.score must be published as a number: {score}"
    );
}

// ---------------------------------------------------------------------------
// 12.7 - the two integration tests this contract adds
// ---------------------------------------------------------------------------

/// Every stored vector is unit length, within the contract's tolerance.
///
/// The comparison needs a tolerance because the column is `REAL[]`: each of the
/// 384 components is rounded to `f32` when it is stored and widened when it is
/// read, so the reconstructed norm differs from `1.0` by an amount that depends
/// on the rounding of every component. `1e-5` is the bound `ADR-0008` §9 and
/// `architecture.md` §2 state; an exact comparison would fail on correct data,
/// and a looser one would stop detecting an unnormalised writer (the spike's
/// measured norm was `0.059342`).
#[tokio::test]
async fn reindex_writes_unit_vectors() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-unit-vectors").await;

    create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    create_note(&app, &user, "PostgreSQL tuning", "indexes and statistics").await;
    let company = create_company(&app, &user, "Acme GmbH").await;
    create_application(
        &app,
        &user,
        company,
        "Backend engineer",
        "Rust and PostgreSQL",
    )
    .await;

    let response = reindex(&app, &user.access_token).await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["indexed"], 3, "three sources must be embedded: {body}");

    let rows = embeddings_of(&db, user.id).await;
    assert_eq!(rows.len(), 3, "one row per source: {rows:?}");

    for (source_type, source_id, model_id, dim, vector) in &rows {
        let length = vector.len();
        assert_eq!(
            length,
            usize::try_from(*dim).expect("the dimension is positive"),
            "a row's declared dim is its vector length ({source_type} {source_id})"
        );
        assert_eq!(length, DIMENSION, "the embedder's dimension is 384");
        assert_eq!(
            model_id, DETERMINISTIC_MODEL_ID,
            "the row records the embedder that wrote it"
        );

        let norm = l2(vector);
        assert!(
            (1.0 - norm).abs() <= UNIT_NORM_TOLERANCE,
            "every stored vector is unit length: {source_type} {source_id} has norm {norm}, \
             which is outside 1.0 +/- {UNIT_NORM_TOLERANCE}"
        );
    }
}

/// The published score is the cosine of the planted vectors.
///
/// Both planted vectors are unit length, so the cosine is the dot product
/// (`ADR-0008` §9). Each is a known combination of the query vector and a
/// perpendicular one, so the expected scores are exactly `0.8` and `0.6` and a
/// re-ranking that used something other than the dot product would fail.
#[tokio::test]
async fn a_hybrid_score_is_the_cosine_of_the_planted_vectors() {
    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-cosine").await;

    let query = "kubernetes";
    let basis = query_vector(query);
    let perpendicular = perpendicular_to(&basis);

    let high = create_note(&app, &user, "Kubernetes rollout", "the canary release").await;
    let low = create_note(&app, &user, "Kubernetes upgrade", "the control plane").await;

    let high_vector = mix(&basis, &perpendicular, 0.8);
    let low_vector = mix(&basis, &perpendicular, 0.6);

    // The fixture states the dot product it expects, so a wrong expectation
    // cannot hide behind a wrong vector.
    assert!(
        (dot(&basis, &high_vector) - 0.8).abs() <= 1e-6,
        "the first planted vector must have a dot product of 0.8 with the query"
    );
    assert!(
        (dot(&basis, &low_vector) - 0.6).abs() <= 1e-6,
        "the second planted vector must have a dot product of 0.6 with the query"
    );

    plant_vector(&db, user.id, "note", high, &high_vector).await;
    plant_vector(&db, user.id, "note", low, &low_vector).await;

    let response = search_mode(&app, &user.access_token, query, "hybrid").await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(
        hit_ids(&body),
        vec![high.to_string(), low.to_string()],
        "the higher cosine must rank first: {body}"
    );

    for (index, expected) in [(0, 0.8_f64), (1, 0.6_f64)] {
        let score = hits(&body)[index]["score"]
            .as_f64()
            .unwrap_or_else(|| panic!("every hybrid hit carries a score: {body}"));

        assert!(
            (score - round_four(expected)).abs() <= SCORE_TOLERANCE,
            "the score must be the cosine of the planted vector, which is {expected}: got {score}"
        );
        assert!(
            (score - round_four(score)).abs() <= SCORE_TOLERANCE,
            "the score is published rounded to four decimals: {score}"
        );
    }
}

/// `ML-R09`: the fixture set proves the plumbing and nothing about quality.
///
/// The deterministic embedder hashes its input, so a note is "about" a query
/// only because the fixture planted the query's own vector for it. What the
/// fifteen cases establish is the pipeline: the query is embedded, the stored
/// vectors are compared, the sort runs, and the expected note is in the top
/// results.
#[tokio::test]
async fn the_fixture_queries_find_the_expected_notes() {
    const LIMIT: usize = 5;

    // Fifteen queries, each with the note that must be found for it.
    let fixtures = [
        ("kubernetes rollout", "Kubernetes rollout"),
        ("postgres index tuning", "PostgreSQL index tuning"),
        ("rust ownership rules", "Rust ownership rules"),
        ("interview preparation", "Interview preparation"),
        ("expense report deadline", "Expense report deadline"),
        ("canary release checklist", "Canary release checklist"),
        ("on-call rotation", "On-call rotation"),
        ("incident postmortem", "Incident postmortem"),
        ("salary negotiation", "Salary negotiation"),
        ("remote work setup", "Remote work setup"),
        ("database migration plan", "Database migration plan"),
        ("api rate limiting", "API rate limiting"),
        ("observability dashboard", "Observability dashboard"),
        ("technical debt backlog", "Technical debt backlog"),
        ("promotion case", "Promotion case"),
    ];

    let state = test_state_with_deterministic_ml().await;
    let db = state.db.clone();
    let app = app(state);
    let user = register_user(&app, "ml-fixtures").await;

    for (query, title) in fixtures {
        let note_id = create_note(&app, &user, title, "a fixture note").await;
        plant_vector(&db, user.id, "note", note_id, &query_vector(query)).await;
    }

    assert_eq!(
        embedding_count(&db, user.id).await,
        i64::try_from(fixtures.len()).expect("the fixture count fits"),
        "every fixture note must have a vector, or the case proves nothing"
    );

    for (query, title) in fixtures {
        let response = search(
            &app,
            &user.access_token,
            &format!("?limit={LIMIT}"),
            json!({ "query": query, "mode": "semantic" }),
        )
        .await;
        let body = response.expect_status(StatusCode::OK);

        assert_search_response(&body, "semantic", "semantic", false, None);
        assert_eq!(
            body["pagination"]["total"],
            i64::try_from(fixtures.len()).expect("the fixture count fits"),
            "the semantic total counts every embedded note: {body}"
        );

        let ids = hit_ids(&body);
        assert!(
            !ids.is_empty(),
            "the query {query:?} must return a page: {body}"
        );

        let page = hits(&body);
        let hit = page
            .iter()
            .find(|hit| hit["note"]["title"] == title)
            .unwrap_or_else(|| {
                panic!("the query {query:?} must find the note {title:?}; the page was {body}")
            });

        let score = hit["score"]
            .as_f64()
            .unwrap_or_else(|| panic!("a semantic hit carries a score: {hit}"));
        assert!(
            score > 0.999,
            "the fixture plants the query's own vector, so {title:?} must score at the top for \
             {query:?}: got {score}"
        );
    }
}

//! Integration tests for the notes endpoints.

pub mod common;

use axum::http::{Method, StatusCode};
use common::{
    app, assert_pagination, register_user, request, send, test_state, test_state_with_pagination,
};
use serde_json::json;

#[tokio::test]
async fn notes_can_be_created_read_updated_and_deleted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-crud").await;

    // create
    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&user.access_token),
            Some(json!({ "title": "Release checklist", "content": "cut the tag" })),
        ),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let note_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["title"], "Release checklist");
    assert!(!created["is_archived"].as_bool().unwrap());
    assert!(created["created_at"].is_string());

    // read
    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/notes/{note_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let fetched = response.expect_status(StatusCode::OK);
    assert_eq!(fetched["id"], note_id);
    assert_eq!(fetched["content"], "cut the tag");

    // update
    let response = send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/notes/{note_id}"),
            Some(&user.access_token),
            Some(json!({ "title": "Release checklist v2" })),
        ),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert_eq!(updated["title"], "Release checklist v2");
    assert_eq!(
        updated["content"], "cut the tag",
        "an omitted field must not be overwritten"
    );

    // delete
    send(
        &app,
        request(
            Method::DELETE,
            &format!("/api/v1/notes/{note_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NO_CONTENT);

    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/notes/{note_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_note_belonging_to_another_user_is_invisible() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "notes-owner").await;
    let other = register_user(&app, "notes-other").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&owner.access_token),
            Some(json!({ "title": "Private", "content": "secret" })),
        ),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let note_id = created["id"].as_str().unwrap().to_string();

    // read
    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/notes/{note_id}"),
            Some(&other.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);

    // update
    send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/notes/{note_id}"),
            Some(&other.access_token),
            Some(json!({ "title": "hijacked" })),
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);

    // delete
    send(
        &app,
        request(
            Method::DELETE,
            &format!("/api/v1/notes/{note_id}"),
            Some(&other.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);

    // the note must still exist for its owner
    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/notes/{note_id}"),
            Some(&owner.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::OK);
}

#[tokio::test]
async fn the_listing_only_contains_the_callers_notes() {
    let state = test_state().await;
    let app = app(state);
    let first = register_user(&app, "notes-list-a").await;
    let second = register_user(&app, "notes-list-b").await;

    for title in ["alpha note", "beta note"] {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/notes",
                Some(&first.access_token),
                Some(json!({ "title": title })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&second.access_token),
            Some(json!({ "title": "gamma note" })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes",
            Some(&first.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_pagination(&body, 2, 20, 0);
    for item in body["items"].as_array().unwrap() {
        assert!(
            item["title"].as_str().unwrap().contains("note"),
            "unexpected note leaked into the listing"
        );
    }
}

#[tokio::test]
async fn search_matches_title_and_content() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-search").await;

    send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&user.access_token),
            Some(json!({ "title": "kubernetes rollout", "content": "unrelated" })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&user.access_token),
            Some(json!({ "title": "groceries", "content": "buy kubernetes secrets" })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?search=kubernetes",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["items"].as_array().unwrap().len(), 2);

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?search=groceries",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_search_term_over_the_bound_is_refused() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-search-bound").await;

    // One character more than the bound of 200.
    let term = "x".repeat(201);

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/notes?search={term}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);

    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["details"][0]["field"], "search");
    assert_eq!(
        body["error"]["details"][0]["message"],
        "must be at most 200 characters long"
    );
}

#[tokio::test]
async fn archived_notes_are_hidden_by_default_and_can_be_requested() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-archive").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&user.access_token),
            Some(json!({ "title": "archived" })),
        ),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let note_id = created["id"].as_str().unwrap().to_string();

    send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/notes/{note_id}"),
            Some(&user.access_token),
            Some(json!({ "is_archived": true })),
        ),
    )
    .await
    .expect_status(StatusCode::OK);

    let response = send(
        &app,
        request(Method::GET, "/api/v1/notes", Some(&user.access_token), None),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0);

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?archived=true",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn an_empty_title_is_rejected() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-invalid").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&user.access_token),
            Some(json!({ "title": "   " })),
        ),
    )
    .await;

    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn pagination_is_applied_and_reported() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-page").await;

    for index in 0..5 {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/notes",
                Some(&user.access_token),
                Some(json!({ "title": format!("note {index}") })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?limit=2&offset=0",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_pagination(&body, 5, 2, 0);
}

#[tokio::test]
async fn the_note_id_must_be_a_uuid() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-badid").await;

    send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes/not-a-uuid",
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_reported_total_is_the_size_of_the_filtered_set() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-total").await;

    for (title, content) in [
        ("alpha one", "body one"),
        ("alpha two", "body two"),
        ("beta three", "body three"),
        ("gamma four", "body four"),
    ] {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/notes",
                Some(&user.access_token),
                Some(json!({ "title": title, "content": content })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    // The page query and the count query must filter identically, otherwise
    // the total silently drifts away from the rows that were returned.
    for (query, expected) in [
        ("", 4),
        ("?search=alpha", 2),
        ("?search=beta", 1),
        ("?search=nothing-matches-this", 0),
        ("?archived=true", 0),
    ] {
        let response = send(
            &app,
            request(
                Method::GET,
                &format!("/api/v1/notes{query}"),
                Some(&user.access_token),
                None,
            ),
        )
        .await;
        let body = response.expect_status(StatusCode::OK);

        assert_eq!(
            body["pagination"]["total"], expected,
            "unexpected total for '{query}'"
        );
        assert_eq!(
            body["items"].as_array().unwrap().len(),
            expected,
            "the page and the count must describe the same set for '{query}'"
        );
    }
}

#[tokio::test]
async fn an_offset_past_the_end_still_reports_the_whole_filtered_set() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "notes-offset").await;

    for index in 0..5 {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/notes",
                Some(&user.access_token),
                Some(json!({ "title": format!("note {index}") })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?limit=2&offset=100",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_pagination(&body, 5, 2, 100);
}

#[tokio::test]
async fn the_configured_page_bounds_are_applied_to_the_note_listing() {
    // Bounds that are neither the shipped defaults nor any plausible
    // compiled-in constant, so a fallback to a literal cannot pass this test.
    let state = test_state_with_pagination(7, 13).await;
    let bounds = state.config.api.pagination;
    let app = app(state);
    let user = register_user(&app, "notes-bounds").await;

    for index in 0..3 {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/notes",
                Some(&user.access_token),
                Some(json!({ "title": format!("bounded {index}") })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    // A limit far above the configured maximum is answered with the maximum,
    // not with the requested value.
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?limit=100000",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        body["pagination"]["limit"], bounds.max_limit,
        "the request must be capped at the configured maximum"
    );
    assert_eq!(
        body["pagination"]["offset"], 0,
        "the offset must be the one that was applied"
    );

    // No limit falls back to the configured default.
    let response = send(
        &app,
        request(Method::GET, "/api/v1/notes", Some(&user.access_token), None),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        body["pagination"]["limit"], bounds.default_limit,
        "the default must come from the configuration"
    );

    // A non-positive limit also falls back to the configured default.
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?limit=0",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        body["pagination"]["limit"], bounds.default_limit,
        "a non-positive limit must not bypass the default"
    );
}

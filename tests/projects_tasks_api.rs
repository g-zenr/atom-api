//! Integration tests for the projects and tasks endpoints.

pub mod common;

use axum::http::{Method, StatusCode};
use common::{
    app, assert_pagination, create_project, create_task, register_user, request, send, test_state,
    test_state_with_pagination,
};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn projects_can_be_created_read_updated_and_deleted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "project-crud").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/projects",
            Some(&user.access_token),
            Some(json!({ "name": "ATOM Phase 1", "description": "foundation" })),
        ),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let project_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["name"], "ATOM Phase 1");

    let response = send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/projects/{project_id}"),
            Some(&user.access_token),
            Some(json!({ "name": "ATOM Phase 2" })),
        ),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert_eq!(updated["name"], "ATOM Phase 2");
    assert_eq!(updated["description"], "foundation");

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

    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_project_of_another_user_is_invisible() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "project-owner").await;
    let other = register_user(&app, "project-other").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/projects",
            Some(&owner.access_token),
            Some(json!({ "name": "Private project" })),
        ),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let project_id = created["id"].as_str().unwrap().to_string();

    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}"),
            Some(&other.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);

    send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/projects/{project_id}"),
            Some(&other.access_token),
            Some(json!({ "name": "hijacked" })),
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tasks_can_be_created_and_listed_inside_a_project() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-create").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    let response = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            Some(json!({
                "title": "Draft the ADR",
                "description": "record the decision",
                "priority": "high",
            })),
        ),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["title"], "Draft the ADR");
    assert_eq!(created["status"], "open");
    assert_eq!(created["priority"], "high");
    assert!(created["completed_at"].is_null());

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_pagination(&body, 1, 20, 0);
}

#[tokio::test]
async fn completing_a_task_stamps_the_completion_timestamp() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-complete").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    let task_id = create_task(&app, &user, project_id, "Ship it").await;

    let response = send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/tasks/{task_id}"),
            Some(&user.access_token),
            Some(json!({ "status": "done" })),
        ),
    )
    .await;
    let updated = response.expect_status(StatusCode::OK);
    assert_eq!(updated["status"], "done");
    assert!(
        updated["completed_at"].is_string(),
        "a done task must carry a completion timestamp"
    );

    // Reopening clears the timestamp again.
    let response = send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/tasks/{task_id}"),
            Some(&user.access_token),
            Some(json!({ "status": "open" })),
        ),
    )
    .await;
    let reopened = response.expect_status(StatusCode::OK);
    assert_eq!(reopened["status"], "open");
    assert!(reopened["completed_at"].is_null());
}

#[tokio::test]
async fn tasks_can_be_filtered_by_status_and_priority() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-filter").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    for (title, priority) in [("one", "low"), ("two", "urgent")] {
        send(
            &app,
            request(
                Method::POST,
                &format!("/api/v1/projects/{project_id}/tasks"),
                Some(&user.access_token),
                Some(json!({ "title": title, "priority": priority })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks?priority=urgent"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["title"], "two");

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks?status=open"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_task_cannot_be_reached_through_a_project_of_another_user() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "task-owner").await;
    let other = register_user(&app, "task-other").await;

    let project_id = create_project(&app, &owner, "Private").await;

    let task_id = create_task(&app, &owner, project_id, "secret work").await;

    // direct access
    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/tasks/{task_id}"),
            Some(&other.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);

    // access through the project collection
    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&other.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);

    // mutation
    send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/tasks/{task_id}"),
            Some(&other.access_token),
            Some(json!({ "title": "hijacked" })),
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_task_cannot_be_created_in_a_project_of_another_user() {
    let state = test_state().await;
    let app = app(state);
    let owner = register_user(&app, "task-create-owner").await;
    let other = register_user(&app, "task-create-other").await;

    let project_id = create_project(&app, &owner, "Private").await;

    send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&other.access_token),
            Some(json!({ "title": "intruder" })),
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_project_removes_its_tasks() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-cascade").await;
    let project_id = create_project(&app, &user, "Doomed").await;

    let task_id = create_task(&app, &user, project_id, "will be cascaded").await;

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

    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/tasks/{task_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_unknown_task_status_is_rejected() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-badstatus").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            Some(json!({ "title": "invalid", "status": "archived" })),
        ),
    )
    .await
    .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_task_can_be_read_and_deleted() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-read-delete").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    let response = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            Some(json!({ "title": "Readable", "description": "with a body" })),
        ),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let task_id = created["id"].as_str().unwrap().to_string();

    // read
    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/tasks/{task_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let fetched = response.expect_status(StatusCode::OK);
    assert_eq!(fetched["id"], task_id);
    assert_eq!(fetched["title"], "Readable");
    assert_eq!(fetched["description"], "with a body");
    assert_eq!(fetched["project_id"], project_id.to_string());
    assert_eq!(fetched["status"], "open");
    assert_eq!(fetched["priority"], "medium");

    // delete
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

    // a deleted task is gone
    send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/tasks/{task_id}"),
            Some(&user.access_token),
            None,
        ),
    )
    .await
    .expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tasks_are_returned_in_manual_order() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-order").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    // Created without a position: the service appends to the end.
    create_task(&app, &user, project_id, "last").await;

    // Created with an explicit position: it sorts before the appended task.
    send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            Some(json!({ "title": "first", "position": 1.0 })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    let titles: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["title"].as_str().unwrap())
        .collect();

    assert_eq!(titles, vec!["first", "last"]);
}

#[tokio::test]
async fn task_listing_is_paginated() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-page").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    for index in 0..5 {
        create_task(&app, &user, project_id, &format!("task {index}")).await;
    }

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks?limit=2&offset=0"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_pagination(&body, 5, 2, 0);

    // The second page continues where the first stopped.
    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks?limit=2&offset=2"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_pagination(&body, 5, 2, 2);
}

#[tokio::test]
async fn projects_can_be_searched() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "project-search").await;

    for (name, description) in [
        ("kernel rewrite", "the storage engine"),
        ("deployment guide", "how to ship it"),
        ("kernel notes", "a summary"),
        ("unrelated", "mentions the storage engine"),
    ] {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/projects",
                Some(&user.access_token),
                Some(json!({ "name": name, "description": description })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    // A term in the name matches.
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects?search=kernel",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_pagination(&body, 2, 20, 0);

    // A term in the description matches too.
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects?search=storage",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_pagination(&body, 2, 20, 0);

    // A term that matches nothing returns an empty page, not an error.
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects?search=nothing-matches-this",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_pagination(&body, 0, 20, 0);
}

#[tokio::test]
async fn project_listing_is_paginated() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "project-page").await;

    for index in 0..5 {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/projects",
                Some(&user.access_token),
                Some(json!({ "name": format!("project {index}") })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects?limit=2&offset=0",
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
async fn the_project_listing_only_contains_the_callers_projects() {
    let state = test_state().await;
    let app = app(state);
    let first = register_user(&app, "project-list-a").await;
    let second = register_user(&app, "project-list-b").await;

    for name in ["alpha", "beta"] {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/projects",
                Some(&first.access_token),
                Some(json!({ "name": name })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    send(
        &app,
        request(
            Method::POST,
            "/api/v1/projects",
            Some(&second.access_token),
            Some(json!({ "name": "gamma" })),
        ),
    )
    .await
    .expect_status(StatusCode::CREATED);

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects",
            Some(&first.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_pagination(&body, 2, 20, 0);
    for item in body["items"].as_array().unwrap() {
        let name = item["name"].as_str().unwrap();
        assert!(
            name == "alpha" || name == "beta",
            "another user's project leaked into the listing: {name}"
        );
    }
}

#[tokio::test]
async fn a_blank_project_name_is_rejected() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "project-blank").await;

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/projects",
            Some(&user.access_token),
            Some(json!({ "name": "   " })),
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn a_non_uuid_project_id_is_rejected() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "project-badid").await;

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects/not-a-uuid",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn a_slow_request_is_abandoned_with_the_error_envelope() {
    use std::time::Duration;

    // A router with a very short timeout and a deliberately slow handler.
    //
    // The timeout travels in the request context, exactly as it does in the
    // real router.
    let mut config = common::test_config();
    config.app.limits.request_timeout = Duration::from_millis(50);
    let context = atom_api::api::RequestContext::new(&config);

    let app = axum::Router::new()
        .route(
            "/slow",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "too late"
            }),
        )
        .layer(axum::middleware::from_fn(atom_api::api::timeout_middleware))
        .layer(axum::Extension(context))
        .layer(tower_http::request_id::PropagateRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
        ))
        .layer(tower_http::request_id::SetRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
            tower_http::request_id::MakeRequestUuid,
        ));

    let raw = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/slow")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(raw.status(), StatusCode::REQUEST_TIMEOUT);
    assert!(
        raw.headers().get("x-request-id").is_some(),
        "a timed-out response must still carry its correlation id"
    );
}

#[tokio::test]
async fn a_task_created_as_done_carries_a_completion_timestamp() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-done-at-birth").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    let response = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            Some(json!({ "title": "already done", "status": "done" })),
        ),
    )
    .await;

    let created = response.expect_status(StatusCode::CREATED);
    assert_eq!(created["status"], "done");
    assert!(
        created["completed_at"].is_string(),
        "a task created as done must carry a completion timestamp"
    );
}

#[tokio::test]
async fn a_project_without_tasks_is_an_empty_page_and_not_a_not_found() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-empty-project").await;
    let project_id = create_project(&app, &user, "Empty").await;

    // The project exists, so the listing is an empty page with a total of
    // zero, not a 404. The distinction is observable.
    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_pagination(&body, 0, 20, 0);
}

#[tokio::test]
async fn an_offset_past_the_end_still_reports_the_whole_task_set() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-offset").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    for index in 0..5 {
        create_task(&app, &user, project_id, &format!("task {index}")).await;
    }

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks?limit=2&offset=100"),
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
async fn the_reported_task_total_is_the_size_of_the_filtered_set() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "task-total").await;
    let project_id = create_project(&app, &user, "Roadmap").await;

    for (title, status) in [("one", "open"), ("two", "open"), ("three", "done")] {
        send(
            &app,
            request(
                Method::POST,
                &format!("/api/v1/projects/{project_id}/tasks"),
                Some(&user.access_token),
                Some(json!({ "title": title, "status": status })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    // The page query and the count query must filter identically, otherwise
    // the total silently drifts away from the rows that were returned.
    for (query, expected) in [("", 3), ("?status=open", 2), ("?status=done", 1)] {
        let response = send(
            &app,
            request(
                Method::GET,
                &format!("/api/v1/projects/{project_id}/tasks{query}"),
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
async fn the_reported_project_total_is_the_size_of_the_filtered_set() {
    let state = test_state().await;
    let app = app(state);
    let user = register_user(&app, "project-total").await;

    for name in ["kernel one", "kernel two", "storage three"] {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/projects",
                Some(&user.access_token),
                Some(json!({ "name": name })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    // The page query and the count query must filter identically, otherwise
    // the total silently drifts away from the rows that were returned.
    for (query, expected) in [("", 3), ("?search=kernel", 2), ("?search=storage", 1)] {
        let response = send(
            &app,
            request(
                Method::GET,
                &format!("/api/v1/projects{query}"),
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
async fn the_configured_page_bounds_are_applied_to_the_project_listing() {
    // Bounds that are neither the shipped defaults nor any plausible
    // compiled-in constant, so a fallback to a literal cannot pass this test.
    let state = test_state_with_pagination(7, 13).await;
    let bounds = state.config.api.pagination;
    let app = app(state);
    let user = register_user(&app, "project-bounds").await;

    for index in 0..3 {
        send(
            &app,
            request(
                Method::POST,
                "/api/v1/projects",
                Some(&user.access_token),
                Some(json!({ "name": format!("bounded {index}") })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects?limit=100000",
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

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        body["pagination"]["limit"], bounds.default_limit,
        "the default must come from the configuration"
    );
}

#[tokio::test]
async fn the_configured_page_bounds_are_applied_to_the_task_listing() {
    // Bounds that are neither the shipped defaults nor any plausible
    // compiled-in constant, so a fallback to a literal cannot pass this test.
    let state = test_state_with_pagination(7, 13).await;
    let bounds = state.config.api.pagination;
    let app = app(state);
    let user = register_user(&app, "task-bounds").await;
    let project_id = create_project(&app, &user, "Bounded").await;

    for index in 0..3 {
        create_task(&app, &user, project_id, &format!("bounded {index}")).await;
    }

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks?limit=100000"),
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

    let response = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/projects/{project_id}/tasks"),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);
    assert_eq!(
        body["pagination"]["limit"], bounds.default_limit,
        "the default must come from the configuration"
    );
}

//! The list endpoints parse filters and pagination from a single query string.
//!
//! Each list handler reads the whole query string through one extractor, splits
//! it into the feature filter and the pagination pair, and applies both. These
//! tests drive the real router: they send one query string carrying a filter
//! *and* pagination, and assert that both halves are honoured. A filter that is
//! applied while the pagination is dropped, or a page size applied to the whole
//! set instead of the filtered one, fails here.
//!
//! Every assertion is deterministic. The note and project listings are ordered
//! by creation time, so the tests never assert which row is on which page; they
//! assert that the two pages are disjoint and that their union is the filtered
//! set, which is exactly what an applied offset means.

pub mod common;

use axum::http::{Method, StatusCode};
use common::{app, assert_pagination, create_project, register_user, request, send, test_state};
use serde_json::{Value, json};

/// The items of a list response.
fn items(body: &Value) -> &[Value] {
    body["items"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("the response must carry an items array: {body}"))
}

/// The ids of the items of a list response.
fn ids(body: &Value) -> Vec<&str> {
    items(body)
        .iter()
        .map(|item| {
            item["id"]
                .as_str()
                .unwrap_or_else(|| panic!("every listed item must carry an id: {item}"))
        })
        .collect()
}

#[tokio::test]
async fn a_note_list_parses_filters_and_pagination_from_one_query_string() {
    let app = app(test_state().await);
    let user = register_user(&app, "list-query-notes").await;

    // Four notes: three match the search term, and one of those is archived.
    for (title, content) in [
        ("alpha one", "body one"),
        ("alpha two", "body two"),
        ("beta three", "body three"),
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

    let response = send(
        &app,
        request(
            Method::POST,
            "/api/v1/notes",
            Some(&user.access_token),
            Some(json!({ "title": "alpha archived", "content": "body four" })),
        ),
    )
    .await;
    let created = response.expect_status(StatusCode::CREATED);
    let archived_note = created["id"].as_str().unwrap().to_string();

    send(
        &app,
        request(
            Method::PATCH,
            &format!("/api/v1/notes/{archived_note}"),
            Some(&user.access_token),
            Some(json!({ "is_archived": true })),
        ),
    )
    .await
    .expect_status(StatusCode::OK);

    // One query string: the second page of one row, of the active notes that
    // match "alpha".
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?limit=1&offset=1&search=alpha&archived=false",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    // The total is the size of the filtered set: four notes, three matching the
    // term, one of those archived. A filter parsed in isolation from the page
    // would report a different number.
    assert_pagination(&body, 2, 1, 1);

    let second_page = items(&body);
    assert_eq!(second_page.len(), 1, "the request asked for a page of one");
    assert_eq!(
        second_page[0]["is_archived"], false,
        "`archived=false` must hide the archived note"
    );
    assert!(
        second_page[0]["title"]
            .as_str()
            .unwrap_or_default()
            .contains("alpha"),
        "a note that does not match the search term was returned: {}",
        second_page[0]
    );

    // The first page of the same filter. Two distinct rows, one per page, is
    // what an applied offset means; an offset that is parsed but ignored would
    // return the same row twice.
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?limit=1&offset=0&search=alpha&archived=false",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let first_page_body = response.expect_status(StatusCode::OK);
    assert_pagination(&first_page_body, 2, 1, 0);

    let first_page = ids(&first_page_body);
    assert_eq!(first_page.len(), 1);
    assert_ne!(
        first_page[0],
        ids(&body)[0],
        "offset=1 must skip the row that offset=0 returns"
    );

    // The same term with `archived=true` selects the archived note instead,
    // which proves the flag is read from the same query string as the rest.
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/notes?limit=10&offset=0&search=alpha&archived=true",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let archived_page_body = response.expect_status(StatusCode::OK);
    assert_pagination(&archived_page_body, 1, 10, 0);

    let archived_page = items(&archived_page_body);
    assert_eq!(archived_page.len(), 1);
    assert_eq!(
        archived_page[0]["id"],
        Value::String(archived_note),
        "`archived=true` must return the archived note and nothing else"
    );
}

#[tokio::test]
async fn a_project_list_parses_filters_and_pagination_from_one_query_string() {
    let app = app(test_state().await);
    let user = register_user(&app, "list-query-projects").await;

    for name in ["alpha one", "alpha two", "beta three"] {
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

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects?limit=1&offset=1&search=alpha",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    // Two of the three projects match, and the second page of one row was
    // asked for in the same string.
    assert_pagination(&body, 2, 1, 1);

    let second_page = items(&body);
    assert_eq!(second_page.len(), 1, "the request asked for a page of one");
    assert!(
        second_page[0]["name"]
            .as_str()
            .unwrap_or_default()
            .contains("alpha"),
        "a project that does not match the search term was returned: {}",
        second_page[0]
    );

    let response = send(
        &app,
        request(
            Method::GET,
            "/api/v1/projects?limit=1&offset=0&search=alpha",
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let first_page_body = response.expect_status(StatusCode::OK);
    assert_pagination(&first_page_body, 2, 1, 0);

    let first_page = ids(&first_page_body);
    assert_eq!(first_page.len(), 1);
    assert_ne!(
        first_page[0],
        ids(&body)[0],
        "offset=1 must skip the row that offset=0 returns"
    );
}

#[tokio::test]
async fn a_task_list_parses_filters_and_pagination_from_one_query_string() {
    let app = app(test_state().await);
    let user = register_user(&app, "list-query-tasks").await;
    let project_id = create_project(&app, &user, "Query project").await;

    // The status values are the ones the API actually accepts, read from
    // `src/tasks/models.rs`: `open`, `in_progress`, `done`.
    for (title, status, priority) in [
        ("alpha one", "open", "high"),
        ("alpha two", "open", "high"),
        ("beta three", "open", "low"),
        ("gamma four", "in_progress", "high"),
        ("delta five", "done", "high"),
    ] {
        send(
            &app,
            request(
                Method::POST,
                &format!("/api/v1/projects/{project_id}/tasks"),
                Some(&user.access_token),
                Some(json!({ "title": title, "status": status, "priority": priority })),
            ),
        )
        .await
        .expect_status(StatusCode::CREATED);
    }

    let response = send(
        &app,
        request(
            Method::GET,
            &format!(
                "/api/v1/projects/{project_id}/tasks?limit=1&offset=1&status=open&priority=high"
            ),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let body = response.expect_status(StatusCode::OK);

    // Both filters apply: `status=open` alone matches three tasks,
    // `priority=high` alone matches four, and the conjunction two. The page
    // size is the one requested alongside them.
    assert_pagination(&body, 2, 1, 1);

    let second_page = items(&body);
    assert_eq!(second_page.len(), 1, "the request asked for a page of one");
    assert_eq!(second_page[0]["status"], "open");
    assert_eq!(second_page[0]["priority"], "high");

    let response = send(
        &app,
        request(
            Method::GET,
            &format!(
                "/api/v1/projects/{project_id}/tasks?limit=1&offset=0&status=open&priority=high"
            ),
            Some(&user.access_token),
            None,
        ),
    )
    .await;
    let first_page_body = response.expect_status(StatusCode::OK);
    assert_pagination(&first_page_body, 2, 1, 0);

    let first_page = items(&first_page_body);
    assert_eq!(first_page.len(), 1);

    // Tasks are listed in manual order, which is creation order here, so the
    // two pages together are exactly the two tasks that match both filters.
    let mut titles: Vec<&str> = first_page
        .iter()
        .chain(second_page.iter())
        .map(|item| item["title"].as_str().unwrap_or_default())
        .collect();
    titles.sort_unstable();
    assert_eq!(
        titles,
        vec!["alpha one", "alpha two"],
        "the two pages must together be the filtered set, with no row repeated"
    );
}

#[tokio::test]
async fn a_malformed_list_parameter_is_a_bad_request() {
    let app = app(test_state().await);
    let user = register_user(&app, "list-query-bad").await;
    let project_id = create_project(&app, &user, "Malformed query").await;

    let queries = [
        "/api/v1/notes?limit=not-a-number".to_string(),
        "/api/v1/projects?limit=not-a-number".to_string(),
        format!("/api/v1/projects/{project_id}/tasks?limit=not-a-number"),
    ];

    for uri in queries {
        let response = send(
            &app,
            request(Method::GET, &uri, Some(&user.access_token), None),
        )
        .await;
        let body = response.expect_status(StatusCode::BAD_REQUEST);

        assert_eq!(
            body["error"]["code"], "bad_request",
            "a malformed query parameter must use the standard error code for {uri}"
        );
        assert!(
            body["error"]["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty()),
            "the error envelope must carry a message for {uri}: {body}"
        );
    }
}

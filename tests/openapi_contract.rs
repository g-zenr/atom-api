//! Contract tests for the published HTTP surface.
//!
//! The OpenAPI document is derived at compile time by `utoipa`, and the router
//! is declared by hand. Neither can be generated from the other, so the two are
//! held together by assertion instead: the route table and the document must
//! describe the same operations, in both directions, and every operation must
//! be mounted on the real router.
//!
//! The version prefix itself is pinned as well. It is the one string the router
//! and the explorer both build from, and typing it twice is what lets the two
//! halves drift apart.
//!
//! The published query parameter sets are pinned exactly. They are part of the
//! client contract: adding, removing or duplicating a parameter is a breaking
//! change and must fail here before it reaches a client.

pub mod common;

use std::collections::BTreeSet;
use std::path::Path;

use axum::http::{Method, StatusCode};
use utoipa::openapi::path::{Operation, ParameterIn, PathItem};

use atom_api::api::routes::{API_PREFIX, ROUTE_TABLE, UNVERSIONED_ROUTE_TABLE, openapi_document};

/// Every operation declared on a path item, as `(METHOD, operation)`.
///
/// `utoipa` stores one optional field per verb, so the list is written out. All
/// eight verbs are named on purpose: a verb this helper silently ignored would
/// make the document look smaller than it is, and the agreement test would fail
/// for the wrong reason.
fn operations(item: &PathItem) -> Vec<(&'static str, &Operation)> {
    let mut operations = Vec::new();

    for (method, operation) in [
        ("GET", &item.get),
        ("PUT", &item.put),
        ("POST", &item.post),
        ("DELETE", &item.delete),
        ("OPTIONS", &item.options),
        ("HEAD", &item.head),
        ("PATCH", &item.patch),
        ("TRACE", &item.trace),
    ] {
        if let Some(operation) = operation {
            operations.push((method, operation));
        }
    }

    operations
}

/// Replaces every axum path placeholder with a fixed identifier.
///
/// The replacement is per segment, so it does not depend on the placeholder
/// being named `note_id` or `project_id`: a route that adds a parameter under a
/// new name is still probed instead of producing an unparsable URI.
fn fill_placeholders(path: &str) -> String {
    const ID: &str = "00000000-0000-4000-8000-000000000000";

    path.split('/')
        .map(|segment| {
            if segment.starts_with('{') && segment.ends_with('}') {
                ID
            } else {
                segment
            }
        })
        .collect::<Vec<&str>>()
        .join("/")
}

/// Reads a source file of the crate.
fn source(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// The location of a published parameter, as the document spells it.
fn location(parameter_in: &ParameterIn) -> &'static str {
    match parameter_in {
        ParameterIn::Query => "query",
        ParameterIn::Path => "path",
        ParameterIn::Header => "header",
        ParameterIn::Cookie => "cookie",
    }
}

/// Every parameter published for one operation.
///
/// OpenAPI allows a parameter to be declared once on the path item and
/// overridden per operation, so both levels are read. Declaring the same
/// parameter at both levels is a duplicate and is caught by the caller.
fn published_parameters(item: &PathItem, operation: &Operation) -> Vec<(String, &'static str)> {
    item.parameters
        .iter()
        .flatten()
        .chain(operation.parameters.iter().flatten())
        .map(|parameter| (parameter.name.clone(), location(&parameter.parameter_in)))
        .collect()
}

/// The route table and the document describe the same operations.
///
/// A route the router serves but the document does not describe leaves the
/// published surface incomplete. An operation the document describes but the
/// router does not serve is worse: a client generated from the document calls a
/// path that answers `404`.
///
/// Both sides are derived - the table from the constants, the document from the
/// crate - so the test cannot agree with itself by both being stale. The
/// explorer paths (`/api/v1/docs`, `/api/v1/openapi.json`) are router mounts,
/// not operations, and are therefore not part of either side.
#[test]
fn the_route_table_and_the_document_describe_the_same_operations() {
    let document = openapi_document(&common::test_config());

    let mut served: BTreeSet<String> = BTreeSet::new();
    for (method, path) in ROUTE_TABLE {
        served.insert(format!("{method} {API_PREFIX}{path}"));
    }
    for (method, path) in UNVERSIONED_ROUTE_TABLE {
        served.insert(format!("{method} {path}"));
    }

    let mut described: BTreeSet<String> = BTreeSet::new();
    for (path, item) in document.paths.paths.iter() {
        for (method, _) in operations(item) {
            described.insert(format!("{method} {path}"));
        }
    }

    assert!(
        !served.is_empty(),
        "the route tables must declare at least one operation; an empty table would make this \
         test pass without comparing anything"
    );

    let served_but_not_described: Vec<&String> = served.difference(&described).collect();
    let described_but_not_served: Vec<&String> = described.difference(&served).collect();

    assert!(
        served_but_not_described.is_empty(),
        "the router serves operations the OpenAPI document does not describe: \
         {served_but_not_described:?}"
    );
    assert!(
        described_but_not_served.is_empty(),
        "the OpenAPI document describes operations the router does not serve: \
         {described_but_not_served:?}"
    );
}

/// Every entry of both route tables is mounted on the real router.
///
/// The probe carries no credentials, so the expected answer is a rejection -
/// except `404`, which is the router's fallback for a path that is not mounted.
/// `401`, `400`, `405`, `415`, `422` and `200` all prove that the route exists.
/// The test therefore proves mounting, not authorisation, and deliberately does
/// not assert a success status. The method comes from the table, not from a
/// hard-coded `GET`.
#[tokio::test]
async fn every_route_in_the_table_is_served_by_the_router() {
    let app = common::app(common::test_state().await);

    let mut probes: Vec<(String, String)> = Vec::new();
    for (method, path) in ROUTE_TABLE {
        probes.push(((*method).to_string(), format!("{API_PREFIX}{path}")));
    }
    for (method, path) in UNVERSIONED_ROUTE_TABLE {
        probes.push(((*method).to_string(), (*path).to_string()));
    }

    assert!(
        !probes.is_empty(),
        "the route tables must declare at least one operation"
    );

    for (method, path) in probes {
        let uri = fill_placeholders(&path);

        assert!(
            !uri.contains('{') && !uri.contains('}'),
            "{path} still carries a path placeholder the probe cannot fill: {uri}"
        );

        let method = Method::from_bytes(method.as_bytes())
            .unwrap_or_else(|error| panic!("{method} is not an HTTP method: {error}"));

        let response = common::send(&app, common::request(method, &uri, None, None)).await;

        assert_ne!(
            response.status,
            StatusCode::NOT_FOUND,
            "{uri} is declared in the route table but the router does not serve it: the request \
             answered 404, which is the fallback for an unmounted path"
        );
    }
}

/// `/api/v1` is typed once, in the declaration that mounts it.
///
/// The check is textual on purpose and scoped to the mounting layer: the
/// `utoipa` attributes on the handlers must spell absolute paths, because the
/// derive runs at compile time and cannot read a constant. What must not happen
/// is a second declaration or a second mount, so the exact literal - quotes
/// included - is counted across `src/api/routes.rs` and `src/api/mod.rs`.
///
/// The operator-facing prose in `src/api/mod.rs` mentions `/api/v1/docs` and
/// `/api/v1/openapi.json` without quotes; prose is not a literal and must not
/// match. The assertion is not loosened to accommodate it.
#[test]
fn the_version_prefix_is_declared_once() {
    const LITERAL: &str = "\"/api/v1\"";

    let files = [
        ("src/api/routes.rs", source("src/api/routes.rs")),
        ("src/api/mod.rs", source("src/api/mod.rs")),
    ];

    let mut occurrences: Vec<(&str, usize, &str)> = Vec::new();
    let mut total = 0usize;

    for (name, contents) in &files {
        for (index, line) in contents.lines().enumerate() {
            let count = line.matches(LITERAL).count();
            if count > 0 {
                total += count;
                occurrences.push((name, index + 1, line.trim()));
            }
        }
    }

    let found = occurrences
        .iter()
        .map(|(name, number, line)| format!("{name}:{number}: {line}"))
        .collect::<Vec<String>>()
        .join("\n  ");

    assert_eq!(
        total, 1,
        "the literal {LITERAL} must appear exactly once in the mounting layer, but it appears \
         {total} time(s):\n  {found}"
    );

    let (name, number, line) = occurrences[0];
    assert!(
        line.contains("pub const API_PREFIX"),
        "the single declaration of {LITERAL} must be the API_PREFIX constant, but it is at \
         {name}:{number}: {line}"
    );

    let module = &files[1].1;
    assert!(
        module.contains(".nest(routes::API_PREFIX"),
        "src/api/mod.rs must mount the prefix it declares, as .nest(routes::API_PREFIX, ...); \
         the router must not type the prefix a second time"
    );
}

/// Each list handler extracts the query string once.
///
/// This is the invariant the behaviour tests cannot see: a handler with two
/// `Query<...>` extractors over the same URI produces exactly the same response
/// as one extractor over the combined type, because `serde_urlencoded` ignores
/// the keys a type does not name. The difference is structural - the string is
/// parsed twice, and the two halves are free to disagree about which parameter
/// is which - so the assertion is structural too, on the source of each list
/// handler, which is the layer the split belongs to.
#[test]
fn each_list_handler_extracts_the_query_string_once() {
    let modules = [
        ("src/notes/handlers.rs", "NotesListQuery"),
        ("src/projects/handlers.rs", "ProjectsListQuery"),
        ("src/tasks/handlers.rs", "TasksListQuery"),
    ];

    for (module, expected) in modules {
        let contents = source(module);

        let extractors: Vec<(usize, &str)> = contents
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains("Query<"))
            .map(|(index, line)| (index + 1, line.trim()))
            .collect();

        let found = extractors
            .iter()
            .map(|(number, line)| format!("{module}:{number}: {line}"))
            .collect::<Vec<String>>()
            .join("\n  ");

        assert_eq!(
            extractors.len(),
            1,
            "{module} must extract the query string exactly once, but it declares {} query \
             extractor(s):\n  {found}",
            extractors.len()
        );

        assert!(
            extractors[0].1.contains(expected),
            "{module}:{} must extract the whole query string as Query<{expected}>, but it \
             extracts: {}",
            extractors[0].0,
            extractors[0].1
        );
    }
}

/// Each list endpoint publishes its filter and pagination parameters once.
///
/// The published set is the client contract: a duplicated parameter confuses a
/// generated client, a missing one hides a feature, and an extra one promises a
/// filter the server does not implement. The vectors keep duplicates, so a
/// parameter published twice fails the equality as well.
#[test]
fn each_list_endpoint_publishes_its_filter_and_pagination_parameters_once() {
    let document = openapi_document(&common::test_config());

    // path, expected query parameters, expected path parameters.
    let endpoints: [(&str, &[&str], &[&str]); 3] = [
        (
            "/api/v1/notes",
            &["archived", "limit", "offset", "search"],
            &[],
        ),
        ("/api/v1/projects", &["limit", "offset", "search"], &[]),
        (
            "/api/v1/projects/{project_id}/tasks",
            &["limit", "offset", "priority", "status"],
            &["project_id"],
        ),
    ];

    for (path, expected_query, expected_path) in endpoints {
        let item = document
            .paths
            .get_path_item(path)
            .unwrap_or_else(|| panic!("the document must describe the path {path}"));
        let operation = item
            .get
            .as_ref()
            .unwrap_or_else(|| panic!("the document must describe GET {path}"));

        let parameters = published_parameters(item, operation);

        let mut query: Vec<&str> = parameters
            .iter()
            .filter(|(_, place)| *place == "query")
            .map(|(name, _)| name.as_str())
            .collect();
        query.sort_unstable();

        let mut in_path: Vec<&str> = parameters
            .iter()
            .filter(|(_, place)| *place == "path")
            .map(|(name, _)| name.as_str())
            .collect();
        in_path.sort_unstable();

        let duplicates: Vec<&str> = query
            .windows(2)
            .filter(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
            .collect();
        assert!(
            duplicates.is_empty(),
            "GET {path} publishes these query parameters more than once: {duplicates:?}"
        );

        let mut expected_query = expected_query.to_vec();
        expected_query.sort_unstable();
        assert_eq!(
            query, expected_query,
            "GET {path} must publish exactly the contract's query parameters, once each.\n  \
             published: {query:?}\n  expected:  {expected_query:?}"
        );

        let mut expected_path = expected_path.to_vec();
        expected_path.sort_unstable();
        assert_eq!(
            in_path, expected_path,
            "GET {path} must publish exactly the contract's path parameters, once each.\n  \
             published: {in_path:?}\n  expected:  {expected_path:?}"
        );
    }
}

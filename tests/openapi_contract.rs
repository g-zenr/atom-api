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
use serde_json::Value;
use utoipa::openapi::OpenApi;
use utoipa::openapi::path::{Operation, ParameterIn, PathItem};

use atom_api::api::routes::{API_PREFIX, ROUTE_TABLE, UNVERSIONED_ROUTE_TABLE, openapi_document};

/// The number of operations the locked contract publishes.
///
/// Thirty-two versioned operations - five authentication, five notes, five
/// projects, five tasks and twelve claims (five claim routes, three item writes
/// and four lifecycle actions) - plus the two unversioned probes. The two-way
/// comparison below cannot see an operation that was dropped from the table,
/// the router and the document together, so the size of the agreed surface is
/// pinned as well: the contract is that all 34 operations of
/// `/api/v1/openapi.json` still exist.
const CONTRACT_OPERATIONS: usize = 34;

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

/// Every Rust source file of the crate, as `(relative path, contents)`.
///
/// The paths are relative to the crate root and always use `/`, so that an
/// assertion naming a file reads the same on every platform.
fn rust_sources() -> Vec<(String, String)> {
    fn walk(root: &Path, directory: &Path, found: &mut Vec<(String, String)>) {
        let entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()));

        for entry in entries {
            let path = entry
                .unwrap_or_else(|error| {
                    panic!("cannot read an entry of {}: {error}", directory.display())
                })
                .path();

            if path.is_dir() {
                walk(root, &path, found);
                continue;
            }

            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }

            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");

            let contents = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));

            found.push((relative, contents));
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    walk(root, &root.join("src"), &mut found);
    found.sort_by(|left, right| left.0.cmp(&right.0));

    assert!(
        !found.is_empty(),
        "the scan found no Rust source under {}",
        root.join("src").display()
    );

    found
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
///
/// The size of the agreed surface is pinned as well, so that an operation
/// dropped from the table, the router and the document together - which the
/// two-way comparison cannot see - is still a failure. Every operation must
/// declare a success response, so one re-badied to an error-only entry is a
/// failure too.
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
        for (method, operation) in operations(item) {
            assert!(
                operation
                    .responses
                    .responses
                    .keys()
                    .any(|status| status.starts_with('2')),
                "{method} {path} declares no success response"
            );
            described.insert(format!("{method} {path}"));
        }
    }

    assert!(
        !served.is_empty() && !described.is_empty(),
        "both sides must declare at least one operation: an empty table or an empty document \
         would make this comparison pass without asserting anything\n  served: {served:#?}\n  \
         described: {described:#?}"
    );

    assert_eq!(
        served.len(),
        CONTRACT_OPERATIONS,
        "the published surface must stay the {CONTRACT_OPERATIONS} operations of the locked \
         contract, but the route tables now declare {}:\n{served:#?}",
        served.len()
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
/// except `404`, which is the router's fallback for a path that is not mounted,
/// and `405`, which is what an existing path answers for a method it does not
/// declare. Every other status - `401`, `400`, `415`, `422` or `200` - proves
/// that this method on this path exists. The versioned routes are therefore
/// asserted only to be mounted, not to succeed; the two probes outside the
/// version prefix need no credentials, so those two are asserted to answer
/// `200`. The method comes from the table, not from a hard-coded `GET`.
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

        // The probes outside the version prefix need no credentials, so they are
        // the two operations whose answer is a success rather than a rejection.
        let unversioned = UNVERSIONED_ROUTE_TABLE
            .iter()
            .any(|(_, health)| *health == path.as_str());

        let method = Method::from_bytes(method.as_bytes())
            .unwrap_or_else(|error| panic!("{method} is not an HTTP method: {error}"));

        let response = common::send(&app, common::request(method, &uri, None, None)).await;

        assert_ne!(
            response.status,
            StatusCode::NOT_FOUND,
            "{uri} is declared in the route table but the router does not serve it: the request \
             answered 404, which is the fallback for an unmounted path"
        );
        assert_ne!(
            response.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{uri} is declared in the route table, but the router does not accept that method on \
             it: the request answered 405"
        );

        if unversioned {
            assert_eq!(
                response.status,
                StatusCode::OK,
                "{uri} is a probe that needs no credentials, so it must answer 200, not {}",
                response.status
            );
        }
    }
}

/// `/api/v1` is typed once, in the declaration that mounts it.
///
/// Every Rust source file of the crate is scanned, and the versioned literal -
/// quotes included - must appear exactly once in all of them: in `API_PREFIX`, in
/// `src/api/routes.rs`. A second constant, a second mount, or a handler that
/// spells the prefix by hand is what lets the router and the document drift
/// apart, so any occurrence outside that one declaration fails.
///
/// The check is textual because the prefix has to be usable at compile time:
/// `utoipa` derives the document at compile time and cannot read a constant, so
/// the attributes on the handlers spell absolute paths. Those spell
/// `/api/v1/notes`, not `/api/v1`, and therefore do not match the literal. The
/// explorer builds its two paths from `API_PREFIX`, so it does not type the
/// prefix either.
#[test]
fn the_version_prefix_is_declared_once() {
    const LITERAL: &str = "\"/api/v1\"";
    const DECLARATION: &str = "pub const API_PREFIX";

    // The prefix is part of the published contract, so its value is pinned as
    // well as its single declaration: a rename that kept one declaration would
    // otherwise be invisible to every generated client.
    assert_eq!(
        API_PREFIX, "/api/v1",
        "the version prefix is part of the published contract"
    );

    let sources = rust_sources();

    // The scan must really be recursive: the one legitimate occurrence is a
    // nested file, and a walk that stopped at `src/` would silently scan almost
    // nothing. Two different nested directories have to be present.
    for required in ["src/api/routes.rs", "src/notes/dto.rs"] {
        assert!(
            sources.iter().any(|(name, _)| name == required),
            "the source scan missed {required}; it found {:?}",
            sources
                .iter()
                .map(|(name, _)| name)
                .collect::<Vec<&String>>()
        );
    }

    let mut occurrences: Vec<(String, usize, String)> = Vec::new();
    let mut total = 0usize;

    for (name, contents) in sources {
        for (index, line) in contents.lines().enumerate() {
            let count = line.matches(LITERAL).count();
            if count == 0 {
                continue;
            }

            total += count;
            occurrences.push((name.clone(), index + 1, line.trim().to_string()));
        }
    }

    let found = occurrences
        .iter()
        .map(|(name, number, line)| format!("{name}:{number}: {line}"))
        .collect::<Vec<String>>()
        .join("\n  ");

    assert_eq!(
        total, 1,
        "the literal {LITERAL} must be typed exactly once in the crate, but it appears {total} \
         time(s):\n  {found}"
    );

    let (file, number, line) = &occurrences[0];
    assert_eq!(
        file.as_str(),
        "src/api/routes.rs",
        "the single declaration of {LITERAL} must be in src/api/routes.rs, but it is at \
         {file}:{number}: {line}"
    );
    assert!(
        line.contains(DECLARATION),
        "the single occurrence of {LITERAL} must be the API_PREFIX declaration, but it is at \
         {file}:{number}: {line}"
    );

    assert!(
        source("src/api/mod.rs").contains(".nest(routes::API_PREFIX"),
        "src/api/mod.rs must mount the prefix it declares, as .nest(routes::API_PREFIX, ...); \
         the router must not type the prefix a second time"
    );

    // A table entry is relative to the prefix: `API_PREFIX` is the only place
    // the version appears, so an entry that spells it again would mount the
    // version twice and drift from the document.
    for (method, path) in ROUTE_TABLE {
        assert!(
            path.starts_with('/') && !path.contains("/api/"),
            "{method} {path} must be relative to {API_PREFIX}; the version prefix may not be \
             typed inside an entry"
        );
    }

    for (method, path) in UNVERSIONED_ROUTE_TABLE {
        assert!(
            path.starts_with('/') && !path.contains("/api/"),
            "{method} {path} is declared unversioned, so it may not carry the version prefix"
        );
    }
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
    let endpoints: [(&str, &[&str], &[&str]); 4] = [
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
        (
            "/api/v1/claims",
            &[
                "currency",
                "limit",
                "offset",
                "search",
                "status",
                "submitted_from",
                "submitted_to",
            ],
            &[],
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

/// One published component schema, as JSON.
///
/// The document is read through its serialized form because the shape `utoipa`
/// produces for an optional field - a `oneOf` with a `null` branch, a
/// `nullable` flag, or something else again - is a rendering detail of the
/// library, while "the field is published and can be `null`" is the contract.
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

    serde_json::to_value(schema)
        .unwrap_or_else(|error| panic!("the `{name}` schema must serialize: {error}"))
}

/// One published property of one component, as JSON.
fn property(document: &OpenApi, component_name: &str, property_name: &str) -> Value {
    let schema = component(document, component_name);

    schema
        .get("properties")
        .and_then(|properties| properties.get(property_name))
        .cloned()
        .unwrap_or_else(|| {
            let published: Vec<String> = schema
                .get("properties")
                .and_then(Value::as_object)
                .map(|properties| properties.keys().cloned().collect())
                .unwrap_or_default();
            panic!("`{component_name}` must publish `{property_name}`; it publishes {published:?}")
        })
}

/// Whether a published schema node declares `type: <name>`.
///
/// A node may carry a single type, or the JSON Schema array form - `utoipa`
/// renders an optional value as `["string", "null"]` - so both are read.
fn has_type(map: &serde_json::Map<String, Value>, name: &str) -> bool {
    match map.get("type") {
        Some(Value::String(published)) => published == name,
        Some(Value::Array(published)) => published
            .iter()
            .any(|published| published.as_str() == Some(name)),
        _ => false,
    }
}

/// Whether a serialized schema carries a `null` branch anywhere.
fn carries_null(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            has_type(map, "null")
                || map.get("nullable") == Some(&Value::Bool(true))
                || map.values().any(carries_null)
        }
        Value::Array(items) => items.iter().any(carries_null),
        _ => false,
    }
}

/// Whether a serialized schema carries a UUID-string branch anywhere.
fn carries_uuid(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            (has_type(map, "string")
                && map.get("format") == Some(&Value::String("uuid".to_string())))
                || map.values().any(carries_uuid)
        }
        Value::Array(items) => items.iter().any(carries_uuid),
        _ => false,
    }
}

/// Every money field is published as a four-decimal string.
///
/// `ADR-0007` makes a money value a JSON **string** at scale four. A generated
/// client reads the document, not this repository, so a `total_amount`
/// published as `number` - or as a bare `string` with no scale - is the
/// contract already broken for that client. The pattern is asserted because
/// "always four decimals" is the part a type alone cannot say.
#[test]
fn the_claim_money_fields_are_strings_in_the_document() {
    let document = openapi_document(&common::test_config());

    let money_fields = [
        ("ClaimResponse", "total_amount"),
        ("ItemResponse", "amount"),
    ];

    for (component_name, field) in money_fields {
        let schema = property(&document, component_name, field);

        assert_ne!(
            schema["type"], "number",
            "{component_name}.{field} must never be published as a number: {schema}"
        );
        assert_eq!(
            schema["type"], "string",
            "{component_name}.{field} must be published as a JSON string, because the wire form \
             of money is a string (ADR-0007): {schema}"
        );

        let pattern = schema["pattern"].as_str().unwrap_or_else(|| {
            panic!(
                "{component_name}.{field} must publish the four-decimal pattern, so that a \
                 generated client knows the scale: {schema}"
            )
        });

        assert!(
            pattern.starts_with('^') && pattern.ends_with('$'),
            "{component_name}.{field} must publish an anchored pattern: {pattern:?}"
        );
        assert!(
            pattern.contains("{4}"),
            "{component_name}.{field} must publish exactly four decimal places: {pattern:?}"
        );
        assert!(
            pattern.contains("\\."),
            "{component_name}.{field} must publish a mandatory decimal point: {pattern:?}"
        );
    }
}

/// `UpdateClaimRequest.project_id` is published, optional, and nullable.
///
/// The tri-state of `C-R2` collapses in the document if the field is missing or
/// required: a generated client then cannot send `null` to clear the link, and
/// "absent" and "cleared" become one instruction. Both halves are asserted:
/// the field is not in `required`, and its schema has a `null` branch and a
/// UUID-string branch.
#[test]
fn the_update_claim_request_publishes_a_nullable_project_id() {
    let document = openapi_document(&common::test_config());

    let schema = component(&document, "UpdateClaimRequest");

    let project_id = property(&document, "UpdateClaimRequest", "project_id");

    let required: Vec<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    assert!(
        !required.iter().any(|name| name == "project_id"),
        "UpdateClaimRequest.project_id must be optional, so that omitting it leaves the link \
         untouched; the required fields are {required:?}"
    );

    assert!(
        carries_null(&project_id),
        "UpdateClaimRequest.project_id must publish a `null` branch, so that a client can clear \
         the link: {project_id}"
    );
    assert!(
        carries_uuid(&project_id),
        "UpdateClaimRequest.project_id must publish the UUID form it accepts: {project_id}"
    );
}

/// The item and lifecycle operations publish their documented success status.
///
/// A generated client picks its return type from the success response, so the
/// status is part of the contract and not a rendering detail: an item write
/// that published `200` where the API answers `201`, or a delete that published
/// no `204`, is broken for that client even though the router serves the path.
/// The seven operations and their statuses are the table `C-R3`/`C-R4` locks and
/// [`docs/features/reimbursements/api.md`] §1 publishes.
#[test]
fn the_item_and_lifecycle_operations_publish_their_success_status() {
    let document = openapi_document(&common::test_config());

    let published: [(&str, &str, &str); 7] = [
        ("POST", "/api/v1/claims/{claim_id}/items", "201"),
        ("PATCH", "/api/v1/claims/{claim_id}/items/{item_id}", "200"),
        ("DELETE", "/api/v1/claims/{claim_id}/items/{item_id}", "204"),
        ("POST", "/api/v1/claims/{claim_id}/submit", "200"),
        ("POST", "/api/v1/claims/{claim_id}/approve", "200"),
        ("POST", "/api/v1/claims/{claim_id}/reject", "200"),
        ("POST", "/api/v1/claims/{claim_id}/pay", "200"),
    ];

    for (method, path, expected) in published {
        let item = document
            .paths
            .get_path_item(path)
            .unwrap_or_else(|| panic!("the document must describe the path {path}"));

        let operation = match method {
            "POST" => item.post.as_ref(),
            "PATCH" => item.patch.as_ref(),
            "DELETE" => item.delete.as_ref(),
            other => panic!("{other} is not one of the methods these operations use"),
        }
        .unwrap_or_else(|| panic!("the document must describe {method} {path}"));

        let statuses: Vec<&String> = operation.responses.responses.keys().collect();

        assert!(
            statuses.iter().any(|status| status.as_str() == expected),
            "{method} {path} must publish the {expected} success response; it publishes \
             {statuses:?}"
        );
    }
}

# ATOM API

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent

This directory holds the HTTP contract of the ATOM API. It describes the
contract as a whole; the per-feature endpoint references live with the
features themselves.

| Document | Contents |
| -------- | -------- |
| [`conventions.md`](conventions.md) | Versioning, resource naming, HTTP methods and status codes, the pagination convention, timestamps, identifiers, PATCH semantics, content types, the body size limit, CORS, and the health endpoints. |
| [`error-contract.md`](error-contract.md) | The single error envelope, every error code with its HTTP status, the field-level validation detail format, and the rule that internal details are never exposed. |

The per-feature endpoint references are:

| Feature | Document | Operations documented |
| ------- | -------- | -------------------- |
| Authentication | [`../features/authentication/api.md`](../features/authentication/api.md) | 5 |
| Notes | [`../features/notes/api.md`](../features/notes/api.md) | 5 |
| Projects | [`../features/projects/api.md`](../features/projects/api.md) | 5 |
| Tasks | [`../features/tasks/api.md`](../features/tasks/api.md) | 5 |
| Reimbursements | [`../features/reimbursements/api.md`](../features/reimbursements/api.md) | 5 of the 12 it specifies — the item and lifecycle operations are not built |

**The document describes 27 operations**: 25 under `/api/v1` and the two health
probes outside it. The reimbursement feature contributes five of them — list,
create, read, patch and delete a claim. Its other seven operations (three item
endpoints and four lifecycle actions) are specified in its `api.md` and are
**not** in the document, because no handler exists. A reader who counts the
inventory below must not count them.

## 1. Where the OpenAPI document lives

The OpenAPI document is **generated from the handlers**, so it cannot drift
silently away from the code.

- The document is built by the `utoipa` derive on `ApiDoc` in
  `src/api/routes.rs`. Every operation, request body and response schema is
  registered there.
- It is served at **`GET /api/v1/openapi.json`**.
- The interactive explorer is served at **`GET /api/v1/docs/`** by
  `utoipa-swagger-ui`, which is vendored into the binary, so the explorer
  works without network access.

The document describes the API as it is built. Its `info.version` is the crate
version from `Cargo.toml` (`0.1.0`), and its `info.title` is the configured
`api.title` (`ATOM API` by default).

### The route table and the document agree by construction

The document's content is asserted, not merely generated. `tests/openapi_contract.rs`
compares the route table in `src/api/routes.rs` with the operations in the
generated document **in both directions**, and separately asserts that every
table entry is mounted, so a declared operation and the document cannot drift
apart without a failing test:

| Test | What it proves |
| ---- | -------------- |
| `the_route_table_and_the_document_describe_the_same_operations` | Every entry in `ROUTE_TABLE` and `UNVERSIONED_ROUTE_TABLE` is an operation in the document, and every operation in the document is an entry in one of the tables. Either direction failing fails the test. |
| `every_route_in_the_table_is_served_by_the_router` | Every entry in the tables is actually mounted, so the tables cannot describe a route the API does not serve. It distinguishes mounted from unmounted by status — an unauthenticated probe must not answer the `404` fallback — so it does **not** assert a successful response, and a wrong verb would answer `405` and pass. |
| `the_version_prefix_is_declared_once` | `/api/v1` is typed exactly once in the mounting layer, on the `pub const API_PREFIX` line, and `src/api/mod.rs` mounts that constant. It does not check the explorer paths; those are asserted by inspection of the code and by `tests/docs_route.rs`. |
| `each_list_endpoint_publishes_its_filter_and_pagination_parameters_once` | Each list operation publishes its filter and pagination parameters exactly once: no duplicate, no missing parameter, no extra. |
| `each_list_handler_extracts_the_query_string_once` | Each list handler module declares exactly one `Query<…>` extractor, and it is the combined list query type. A second extractor over the same query string produces an identical response, so only a structural assertion can hold this rule. |

The matching test for the parsed side of the list endpoints is in
`tests/list_query.rs`; the rules themselves are in
[`conventions.md`](conventions.md) §1 and §5.

The same file also asserts the two schema details the reimbursement contract
depends on: `the_claim_money_fields_are_strings_in_the_document` (a money field
is `type: string` with four decimals, never `type: number`) and
`the_update_claim_request_publishes_a_nullable_project_id` (the tri-state is a
nullable field in the document, not a missing one).

Two consequences for a contributor: adding an operation means adding it to the
table **and** to the `#[utoipa::path(...)]` annotations, and moving the API to
another version prefix means changing `API_PREFIX` and nothing else in the
mounting layer. Both are held by a test, so neither is a matter of remembering.

**One case no test covers.** Axum exposes no route introspection, so a route
added to `api_routes()` whose handler is **also** absent from `ApiDoc`'s
`paths(...)` list is in neither side of the comparison, and the test has
nothing to fail on. If the handler is registered in `ApiDoc` instead, the
document gains the operation and the agreement test fails; it is the pair of
omissions that is invisible. That case is caught by review, not by the suite: a
new endpoint lands with its table entry in the same change.

## 2. How to regenerate and serve it

There is **no separate generation step**. The document is a Rust value
compiled into the binary:

```bash
# Start the API locally.
cargo run

# Then read the document.
curl -s http://127.0.0.1:3000/api/v1/openapi.json

# Or open the explorer.
# http://127.0.0.1:3000/api/v1/docs/
```

To change the document, change the `#[utoipa::path(...)]` annotations on the
handlers in `src/auth/handlers.rs`, `src/notes/handlers.rs`,
`src/projects/handlers.rs`, `src/tasks/handlers.rs`,
`src/reimbursements/handlers.rs` and `src/health.rs`, and add any new schema to
the `components(schemas(...))` list in `src/api/routes.rs`.

## 3. Endpoint inventory

Every path is relative to the API root. `Auth` states whether a bearer token
is required.

| Method | Path | Auth | Success | Feature |
| ------ | ---- | ---- | ------- | ------- |
| `GET` | `/health/live` | — | `200` | health |
| `GET` | `/health/ready` | — | `200` / `503` | health |
| `POST` | `/api/v1/auth/register` | — | `201` | authentication |
| `POST` | `/api/v1/auth/login` | — | `200` | authentication |
| `POST` | `/api/v1/auth/refresh` | — | `200` | authentication |
| `POST` | `/api/v1/auth/logout` | bearer | `204` | authentication |
| `GET` | `/api/v1/auth/me` | bearer | `200` | authentication |
| `GET` | `/api/v1/notes` | bearer | `200` | notes |
| `POST` | `/api/v1/notes` | bearer | `201` | notes |
| `GET` | `/api/v1/notes/{note_id}` | bearer | `200` | notes |
| `PATCH` | `/api/v1/notes/{note_id}` | bearer | `200` | notes |
| `DELETE` | `/api/v1/notes/{note_id}` | bearer | `204` | notes |
| `GET` | `/api/v1/projects` | bearer | `200` | projects |
| `POST` | `/api/v1/projects` | bearer | `201` | projects |
| `GET` | `/api/v1/projects/{project_id}` | bearer | `200` | projects |
| `PATCH` | `/api/v1/projects/{project_id}` | bearer | `200` | projects |
| `DELETE` | `/api/v1/projects/{project_id}` | bearer | `204` | projects |
| `GET` | `/api/v1/projects/{project_id}/tasks` | bearer | `200` | tasks |
| `POST` | `/api/v1/projects/{project_id}/tasks` | bearer | `201` | tasks |
| `GET` | `/api/v1/tasks/{task_id}` | bearer | `200` | tasks |
| `PATCH` | `/api/v1/tasks/{task_id}` | bearer | `200` | tasks |
| `DELETE` | `/api/v1/tasks/{task_id}` | bearer | `204` | tasks |
| `GET` | `/api/v1/claims` | bearer | `200` | reimbursements |
| `POST` | `/api/v1/claims` | bearer | `201` | reimbursements |
| `GET` | `/api/v1/claims/{claim_id}` | bearer | `200` | reimbursements |
| `PATCH` | `/api/v1/claims/{claim_id}` | bearer | `200` | reimbursements |
| `DELETE` | `/api/v1/claims/{claim_id}` | bearer | `204` | reimbursements |
| `GET` | `/api/v1/openapi.json` | — | `200` | the document itself |
| `GET` | `/api/v1/docs/` | — | `200` | the explorer |

The first 27 rows are the operations the document describes and the two route
tables declare, and `tests/openapi_contract.rs` holds them together in both
directions. The last two rows are the explorer and the document itself: they are
served, but they are mounted by `swagger_ui` and are **not** operations in the
document, so neither table lists them and no contract test compares them.

The health endpoints, the OpenAPI routes and the three token endpoints below do
not require a credential. `POST /api/v1/auth/register`, `POST /api/v1/auth/login`
and `POST /api/v1/auth/refresh` are unauthenticated by design and are rate
limited by client IP.

The five claim operations are the whole of the reimbursement surface that
answers today. `GET /api/v1/claims/{claim_id}` returns an `items` array, which is
always empty for a claim this API created: the three item endpoints and the four
lifecycle actions are specified in
[`../features/reimbursements/api.md`](../features/reimbursements/api.md) and are
not built, so a claim stays in `draft` with a total of `"0.0000"`.

## 4. The request pipeline

Every request passes through the same layers, in this order:

```text
request
  -> cors             an explicit origin allowlist, when one is configured
  -> trace            structured request logging
  -> body limit       a body over APP_MAX_BODY_BYTES is refused (413)
  -> request id       an x-request-id correlation id on the request and the response
  -> timeout          the server stops processing after the configured timeout (408)
  -> panic guard      a panic becomes a 500, never a dropped connection
  -> route            handler -> service -> repository -> PostgreSQL
```

An unmatched route returns `404` with the standard error envelope. The
pipeline is described in full in
[`request-lifecycle.md`](../architecture/request-lifecycle.md).

## 5. Where to look for what

| Question | Document |
| -------- | -------- |
| Which status code does this situation return? | [`error-contract.md`](error-contract.md) |
| How do I page through a list? | [`conventions.md`](conventions.md) §5 |
| What does this endpoint take and return? | The feature's `api.md` |
| Why does the API answer `404` instead of `403`? | [`error-contract.md`](error-contract.md) §6 |
| How is the schema defined? | [`../database/schema.md`](../database/schema.md) |
| What settings does the API read? | [`../../config/README.md`](../../config/README.md) |
| What are the agreed product requirements? | [`../project-truth.md`](../project-truth.md) |

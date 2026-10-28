# ATOM API

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `docs` agent

This directory holds the HTTP contract of the ATOM API. It describes the
contract as a whole; the per-feature endpoint references live with the
features themselves.

| Document | Contents |
| -------- | -------- |
| [`conventions.md`](conventions.md) | Versioning, resource naming, HTTP methods and status codes, the pagination convention, timestamps, identifiers, PATCH semantics, content types, the body size limit, CORS, and the health endpoints. |
| [`error-contract.md`](error-contract.md) | The single error envelope, every error code with its HTTP status, the field-level validation detail format, and the rule that internal details are never exposed. |
| [`frontend-handover.md`](frontend-handover.md) | The client-facing guide for a frontend engineer: the auth flow and what to do on a `401`, the error envelope and the `400`/`422` split, pagination, money, the per-feature list filters, the two stage machines, ML degradation, versioning and a generated client, and the five behaviours that surprise a new client. |

The per-feature endpoint references are:

| Feature | Document | Operations documented |
| ------- | -------- | -------------------- |
| Authentication | [`../features/authentication/api.md`](../features/authentication/api.md) | 5 |
| Notes | [`../features/notes/api.md`](../features/notes/api.md) | 5 |
| Projects | [`../features/projects/api.md`](../features/projects/api.md) | 5 |
| Tasks | [`../features/tasks/api.md`](../features/tasks/api.md) | 5 |
| Reimbursements | [`../features/reimbursements/api.md`](../features/reimbursements/api.md) | all 12 it specifies |
| Job applications | [`../features/job-applications/api.md`](../features/job-applications/api.md) | 14 |
| Machine learning | [`../features/ml/api.md`](../features/ml/api.md) | 3 specified, all 3 published |

**The document describes 54 operations**: 52 under `/api/v1` and the two health
probes outside it. The reimbursement feature contributes twelve of them - five
claim operations, three item operations and four lifecycle actions - the
job-application feature fourteen - five company, five application and four event
operations - the machine-learning feature three, and the API-key feature the
three that create, list and revoke a machine credential. A reader who counts the
inventory below must agree with those numbers; the contract test
`the_route_table_and_the_document_describe_the_same_operations` fails if they
disagree in either direction.

**The three API-key operations have no per-feature reference document.** Their
single card is [`../features/api-keys/status.md`](../features/api-keys/status.md).
The decision, the credential format, the storage rule and the failure modes are
recorded in [`../decisions/ADR-0011-api-keys.md`](../decisions/ADR-0011-api-keys.md),
and the boundary they sit on is described in
[`../architecture/security-architecture.md`](../architecture/security-architecture.md)
§2. They are authenticated like every other private operation: the `Auth` column
below means an `Authorization: Bearer` credential, which is either a user access
token or an API key. The three are mounted in `api_routes()` and published in
the document (`src/api_keys/handlers.rs`). **A run of `tests/api_keys_api.rs` is
recorded**: its 11 cases pass in the measured run of 2026-10-27 (536 passed,
0 failed, with the six gates green), and the three routes are asserted by
`tests/contract_sweep.rs` and `tests/openapi_contract.rs`. The quality-gate
verdict is PASS (`ATOM-REVIEW-20261027-001`) and the final independent read of
the repaired tree is PASS (`ATOM-REVIEW-20261029-001`), so the feature is
`Verified`.

**The three machine-learning operations are published.** They answer, and they
degrade as their contract says: a scored search returns lexical results with
`200` when `ml.enabled` is `false` (the default), a match and a reindex return
`409`. The suite ran green: 41 cases in `tests/ml_api.rs` pass in the measured
run of 2026-10-27 (536 passed, 0 failed, with the six gates green). The feature
is `Implemented` and not `Verified`, because no reviewer has read the encoder span
and the re-gate of `ATOM-REVIEW-20261024-011` returned FAIL on documentation. The missing half is the
review, not `ML-R02`: `embedder::select` returns a `BurnEmbedder` when both
artifacts load. See [`../features/ml/status.md`](../features/ml/status.md)
§1.3 and [`../features/ml/testing.md`](../features/ml/testing.md).

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
handlers in `src/auth/handlers.rs`, `src/api_keys/handlers.rs`,
`src/notes/handlers.rs`, `src/projects/handlers.rs`, `src/tasks/handlers.rs`,
`src/reimbursements/handlers.rs`, `src/job_applications/handlers.rs`,
`src/ml/handlers.rs` and `src/health.rs`, and add any new schema to
the `components(schemas(...))` list in `src/api/routes.rs`.

## 3. Endpoint inventory

Every path is relative to the API root. `Auth` states whether a bearer
credential — a user access token or an API key — is required.

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
| `POST` | `/api/v1/claims/{claim_id}/items` | bearer | `201` | reimbursements |
| `PATCH` | `/api/v1/claims/{claim_id}/items/{item_id}` | bearer | `200` | reimbursements |
| `DELETE` | `/api/v1/claims/{claim_id}/items/{item_id}` | bearer | `204` | reimbursements |
| `POST` | `/api/v1/claims/{claim_id}/submit` | bearer | `200` | reimbursements |
| `POST` | `/api/v1/claims/{claim_id}/approve` | bearer | `200` | reimbursements |
| `POST` | `/api/v1/claims/{claim_id}/reject` | bearer | `200` | reimbursements |
| `POST` | `/api/v1/claims/{claim_id}/pay` | bearer | `200` | reimbursements |
| `GET` | `/api/v1/companies` | bearer | `200` | job applications |
| `POST` | `/api/v1/companies` | bearer | `201` | job applications |
| `GET` | `/api/v1/companies/{company_id}` | bearer | `200` | job applications |
| `PATCH` | `/api/v1/companies/{company_id}` | bearer | `200` | job applications |
| `DELETE` | `/api/v1/companies/{company_id}` | bearer | `204` | job applications |
| `GET` | `/api/v1/job-applications` | bearer | `200` | job applications |
| `POST` | `/api/v1/job-applications` | bearer | `201` | job applications |
| `GET` | `/api/v1/job-applications/{application_id}` | bearer | `200` | job applications |
| `PATCH` | `/api/v1/job-applications/{application_id}` | bearer | `200` | job applications |
| `DELETE` | `/api/v1/job-applications/{application_id}` | bearer | `204` | job applications |
| `GET` | `/api/v1/job-applications/{application_id}/events` | bearer | `200` | job applications |
| `POST` | `/api/v1/job-applications/{application_id}/events` | bearer | `201` | job applications |
| `PATCH` | `/api/v1/job-applications/{application_id}/events/{event_id}` | bearer | `200` | job applications |
| `DELETE` | `/api/v1/job-applications/{application_id}/events/{event_id}` | bearer | `204` | job applications |
| `POST` | `/api/v1/notes/search` | bearer | `200` | machine learning |
| `POST` | `/api/v1/job-applications/{application_id}/match` | bearer | `200` | machine learning |
| `POST` | `/api/v1/ml/reindex` | bearer | `200` | machine learning |
| `POST` | `/api/v1/api-keys` | bearer | `201` | API keys |
| `GET` | `/api/v1/api-keys` | bearer | `200` | API keys |
| `DELETE` | `/api/v1/api-keys/{key_id}` | bearer | `204` | API keys |
| `GET` | `/api/v1/openapi.json` | — | `200` | the document itself |
| `GET` | `/api/v1/docs/` | — | `200` | the explorer |

The first 54 rows are the operations the document describes and the two route
tables declare, and `tests/openapi_contract.rs` holds them together in both
directions. The last two rows are the explorer and the document itself: they are
served, but they are mounted by `swagger_ui` and are **not** operations in the
document, so neither table lists them and no contract test compares them.

The health endpoints, the OpenAPI routes and the three token endpoints below do
not require a credential. `POST /api/v1/auth/register`, `POST /api/v1/auth/login`
and `POST /api/v1/auth/refresh` are unauthenticated by design and are rate
limited by client IP.

All twelve reimbursement operations answer. `GET /api/v1/claims/{claim_id}`
returns the claim with its `items` array, and the three item endpoints and four
lifecycle actions are served and published, so a claim can move from `draft` to
`paid`. The fourteen job-application operations also answer: five company, five
application and four event operations. The three machine-learning operations
answer too, and are described in
[`../features/ml/api.md`](../features/ml/api.md): a scored search returns lexical
results with `200` while `ml.enabled` is `false`, which is the shipped default, a
match returns `409`, and a reindex returns `409`. See
[`../features/reimbursements/api.md`](../features/reimbursements/api.md) and
[`../features/job-applications/api.md`](../features/job-applications/api.md).

The three API-key operations are published too: they create, list and revoke a
machine credential for the caller. Only the `201` response of
`POST /api/v1/api-keys` carries the secret; the listing returns the `prefix`, so
a key can be recognised after the one time it is shown. The credential format,
the storage rule and the failure modes are in
[`../decisions/ADR-0011-api-keys.md`](../decisions/ADR-0011-api-keys.md).

`GET /api/v1/notes/search` is **not** the search operation: the dynamic
`{note_id}` segment matches it first and answers `400`, because `search` is not a
UUID. The search operation is the `POST` on that path.

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

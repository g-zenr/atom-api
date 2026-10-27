# API conventions

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `docs` agent

The rules every endpoint of the ATOM API follows. A client that knows these
rules can use any endpoint without reading its documentation.

The error rules are in [`error-contract.md`](error-contract.md).

## 1. Versioning

- Every product endpoint lives under **`/api/v1`**. The version is part of
  the path, at the root, so that a future version can be served side by side
  with the current one.
- The version is the **API** version. It changes when the contract changes
  incompatibly. Adding an optional field, adding an endpoint or adding a
  new error detail does not change it.
- **The health endpoints are outside the versioned prefix.** `/health/live`
  and `/health/ready` are infrastructure probes, not product endpoints, and
  they are not versioned.
- **The OpenAPI document and the explorer are inside the versioned prefix**:
  `/api/v1/openapi.json` and `/api/v1/docs/`.
- A client must send the version it was written against. There is no
  negotiation through a header.

### The prefix is declared once

The string `/api/v1` is declared exactly **one** time **as a constant**, in the
mounting layer: `API_PREFIX` in `src/api/routes.rs`. The router mounts the
versioned routes under it in `src/api/mod.rs`, and the explorer and the document
paths are built from it with `format!`, so the prefix is not typed a second time
in the layer that mounts anything. The `#[utoipa::path(path = "/api/v1/…")]`
attributes on the handlers do spell it out, and the next paragraph explains why
that cannot be removed.

The `#[utoipa::path(path = "/api/v1/…")]` attributes on the handlers stay
**absolute**. `utoipa`'s derive runs at compile time and cannot read a `const`,
so an attribute cannot be derived from `API_PREFIX`; writing the prefix by hand
there would only move the duplication, not remove it. The drift that leaves
open is closed by the agreement described in the next subsection, not by a
textual search.

### The route table and the document are checked against each other

`src/api/routes.rs` publishes the routes it serves as two tables:

| Table | Contents |
| ----- | -------- |
| `ROUTE_TABLE` | One `(method, path)` pair per versioned operation, the path **relative** to `API_PREFIX` and in Axum syntax, in the order the router declares them. |
| `UNVERSIONED_ROUTE_TABLE` | The health probes, `GET /health/live` and `GET /health/ready`, which are outside the prefix. |

The tables and the generated document are compared **in both directions** by
the contract test: every entry in the tables must be an operation in the
document, and every operation in the document must be an entry in one of the
tables. The prefix is prepended to a table path when the two are compared, so
the comparison itself carries no second copy of `/api/v1`. The test is named
and described in [`README.md`](README.md) §1.

A route that is served but not described, and an operation that is described
but not served, are both failures. The explorer routes are outside the
comparison because they are not part of the described API: they serve the
document, they are not in it.

A new endpoint is therefore added in **one change**: the handler, its
`#[utoipa::path(...)]` attribute and its table entry together, with the
contract test as the gate. The case the gate cannot see is a handler mounted in
`api_routes()` that is **also** missing from `ApiDoc`'s `paths(...)` list —
Axum exposes no route introspection, so an unregistered handler appears in
neither the tables nor the document. Registering it in `ApiDoc` without adding
a table entry does fail the test; it is the pair of omissions that is a review
responsibility.

## 2. Resource naming

- Paths use **plural nouns**: `/notes`, `/projects`, `/tasks`,
  `/auth/login`.
- A resource identifier is a **path parameter in braces**:
  `{note_id}`, `{project_id}`, `{task_id}`. The parameter name always ends
  in `_id`.
- A **sub-resource** is nested under its parent:
  `/projects/{project_id}/tasks`. A task belongs to a project, and the path
  says so.
- A task is also addressable on its own, `/tasks/{task_id}`, because a
  client that already knows the id should not have to know the project id.
- The names in the path are the names of the **concept**, not of the table.
  The `projects` table and the `/projects` path use the same word.
- Verbs do not appear in a path. The method carries the action.

## 3. HTTP methods and status codes

| Method | Meaning in ATOM | Used for |
| ------ | --------------- | -------- |
| `GET` | Read. Never changes anything. | Listing, fetching. |
| `POST` | Create, or perform an action that is not an update. | Creating a resource, and the three token endpoints. |
| `PATCH` | Partially update a resource. | Changing some fields of a note, a project or a task. |
| `DELETE` | Remove a resource. | Deleting a note, a project or a task. |
| `PUT` | Not used. | A full replacement is not offered; PATCH covers the need. |

| Status | When the API returns it |
| ------ | ----------------------- |
| `200 OK` | A successful read or update. The body carries the resource. |
| `201 Created` | A resource was created. The body carries the new resource. |
| `204 No Content` | A successful delete. There is no body. |
| `400 Bad Request` | The request is malformed: invalid JSON, an unparsable path parameter, an unparsable query parameter. |
| `401 Unauthorized` | The credential is missing, malformed, expired or invalid. |
| `403 Forbidden` | The credential is valid, but the action is not permitted — for example disabling an account, or revoking a session that belongs to somebody else. |
| `404 Not Found` | The resource does not exist, or the caller may not know that it does. |
| `409 Conflict` | The request conflicts with the current state, for example an email address that is already registered. |
| `408 Request Timeout` | The server abandoned the request after the configured timeout. |
| `413 Payload Too Large` | The request body exceeds the accepted size. |
| `422 Unprocessable Entity` | The request is well formed but breaks a business rule. |
| `429 Too Many Requests` | The client exceeded the rate limit. A `retry-after` header is present. |
| `500 Internal Server Error` | An unexpected failure. The details are logged, never returned. |
| `503 Service Unavailable` | Only from `/health/ready`, when a dependency is unreachable. |

The mapping from the API's error variants to these statuses is in
[`error-contract.md`](error-contract.md).

## 4. Identifiers

- Every public identifier is a **UUID**, generated by the application as
  version 4. No sequential identifier is ever exposed, so an identifier
  cannot be guessed or enumerated.
- Identifiers are rendered in the canonical lower-case hyphenated form.
- A path parameter that is not a UUID is a **malformed request** and
  returns `400`, not `404`.
- The identifier of a resource is stable and never reused.

## 5. The pagination convention

Every list endpoint takes the same two query parameters and returns the same
envelope. In the **OpenAPI document** the two parameters are declared
**once**, in the shared `PaginationQuery`; a list endpoint declares only its
own filters on top of them. Declaring them twice would publish them twice in
the generated OpenAPI document.

### The shared query parameters

| Parameter | Type | Default | Rules |
| --------- | ---- | ------- | ----- |
| `limit` | integer | `20` | The page size. A value above the maximum page size is clamped to it. A value of zero or less falls back to the default page size. |
| `offset` | integer | `0` | The number of items to skip. A negative value falls back to `0`. |

Both bounds are **configured values**, resolved once at start-up, not
compile-time constants:

| Setting | Default | What it bounds |
| ------- | ------- | -------------- |
| `api.pagination.default_limit` | `20` | The page size used when the caller does not ask for one. |
| `api.pagination.max_limit` | `100` | The largest page a caller may ask for. |

They live in `config/api.toml` and are described in
[`../architecture/deployment.md`](../architecture/deployment.md) §1.3. The
defaults in the table above are the committed ones; a deployment that
overrides them changes both.

**Changing either of them is a breaking change for a client.** The same
request then returns a different page, and a client written against the
previous bound pages differently. They are therefore part of the published
client convention, not a tuning knob.

A `limit` or `offset` that is not an integer is a malformed request and
returns `400`.

### The envelope

```json
{
  "items": [ { "...": "the resources on this page" } ],
  "pagination": {
    "limit": 20,
    "offset": 0,
    "total": 42,
    "has_more": true
  }
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `items` | array | The resources on this page. May be empty. |
| `pagination.limit` | integer | The limit that was **applied**, after clamping. |
| `pagination.offset` | integer | The offset that was **applied**, after clamping. |
| `pagination.total` | integer | The number of resources matching the filter, ignoring pagination. |
| `pagination.has_more` | boolean | Whether more items exist after this page. It is `offset + limit < total`. |

The `total` counts the resources that match the **filters**, not the whole
collection. A client that pages with `offset += limit` until `has_more` is
false sees every matching resource exactly once.

The envelope is the same for notes, projects and tasks. Only the item type
differs, which is why each feature declares its own named OpenAPI schema
(`NotePage`, `ProjectPage`, `TaskPage`) with the same shape.

### Filters

A list endpoint may add filters of its own, as query parameters. A filter is
declared by the feature that owns it and is listed here once:

| Endpoint | Filter | Effect |
| -------- | ------ | ------ |
| `GET /notes` | `search` | Full-text match over the title and the body. At most 200 characters. |
| `GET /notes` | `archived` | `true` returns only archived notes; `false` or omitted returns only active notes. |
| `GET /projects` | `search` | Case-insensitive substring match over the name and the description. At most 200 characters. |
| `GET /projects/{project_id}/tasks` | `status` | Only tasks with that status. |
| `GET /projects/{project_id}/tasks` | `priority` | Only tasks with that priority. |

The 200-character bound on `search` is a rule of the service layer, not of the
query string: the shared validator `validate_search` in
`src/helpers/validation.rs` rejects a longer term with a `422` validation
failure that names the `search` field, and the wording comes from the message
catalogue in `config/messages.toml`. A term is also trimmed, and an empty
term is treated as no search at all.

### The full parameter set

Every list endpoint accepts the two shared parameters above plus the filters
that belong to it. A parameter is never listed twice for the same endpoint:
the pagination parameters are declared once for the whole API in
`PaginationQuery`, and a feature declares only its own filters, so the
generated OpenAPI document publishes each parameter exactly once. The
contract test `each_list_endpoint_publishes_its_filter_and_pagination_parameters_once`
asserts the published set of each list operation exactly — no duplicate, no
missing parameter, no extra:

| Endpoint | Published parameters | Path parameters |
| -------- | -------------------- | --------------- |
| `GET /api/v1/notes` | `limit`, `offset`, `search`, `archived` | — |
| `GET /api/v1/projects` | `limit`, `offset`, `search` | — |
| `GET /api/v1/projects/{project_id}/tasks` | `limit`, `offset`, `status`, `priority` | `project_id` |

### One deserialization of the query string

The query string of a list request is parsed **once**. Each list endpoint has a
single query type that covers its own filters and the shared pagination
parameters together:

| Endpoint | Query type | Feature filter type |
| -------- | ---------- | ------------------- |
| `GET /api/v1/notes` | `NotesListQuery` | `NoteQuery` |
| `GET /api/v1/projects` | `ProjectsListQuery` | `ProjectQuery` |
| `GET /api/v1/projects/{project_id}/tasks` | `TasksListQuery` | `TaskQuery` |

The handler takes **one** `Query<…>` extractor over that type, and the type
exposes the pair the service layer already works with — the feature filter type
above and `PaginationQuery` — so what is parsed and what is published are the
same set of parameters, read in one pass. Two extractors over one query string
would parse it twice and leave the two halves free to disagree about which
parameter is which.

`#[serde(flatten)]` is deliberately **not** used to compose the two. The
extractor deserializes with `serde_urlencoded`, whose map access yields string
values, so a flattened `Option<i64>` would be asked to deserialize itself from
a string and would fail at run time, on a real request, instead of at compile
time. The fields are written out explicitly in each query type; that
duplication is the price of parsing the string once.

The parsing is covered by `tests/list_query.rs`, one case per list endpoint
plus a malformed-parameter case.

## 6. Timestamps

- Every timestamp is **UTC** and is rendered as **RFC 3339**, for example
  `2026-10-21T09:15:00.000000Z`.
- The format is produced by `chrono`'s default serialisation of
  `DateTime<Utc>`; it always carries the `Z` designator.
- A timestamp that is absent is `null`, never omitted and never an empty
  string. `completed_at` is the only such field today.
- Timestamps are generated by the **database**, not by the client. The
  application never sets a timestamp from a request body.

## 7. PATCH semantics

**PATCH is partial. An omitted field is left untouched.**

- Every field of a PATCH body is optional.
- A field that is absent from the body keeps its stored value. It is never
  written as `null` and never reset to a default.
- A field that is present with a value is validated exactly as it would be
  on creation, and then written.
- A field that is present and explicitly wants an empty value sends an empty
  string, for example `{"content": ""}`. That is a change, not an omission.
- A PATCH on a body with no field at all is accepted and changes nothing
  except `updated_at`.
- The implementation enforces this in SQL with `COALESCE($n, column)`, so a
  new field cannot accidentally be nulled by a handler that forgets it.

The resources that support PATCH are notes, projects and tasks.

## 8. Request and response content types

- **Requests** with a body use `content-type: application/json`. The body
  must be a JSON object.
- **Responses** with a body use `application/json`.
- **A `204 No Content` response has no body** and no content type.
- The API speaks **HTTP/1.1** with JSON bodies, so any HTTP client can use
  it.
- A response body is never HTML. Even the error responses are JSON, and the
  `404` for an unmatched route uses the standard envelope.

## 9. The body size limit

- A body is refused when it exceeds **either** of two limits:
  - the **configured** limit, `app.limits.max_body_bytes`, which the
    environment variable `APP_MAX_BODY_BYTES` overrides and which is
    **1 MiB** (`1048576` bytes) by default. The router installs it as an Axum
    `DefaultBodyLimit`, and the JSON extractor in `src/api/extract.rs` reads
    it through `req.into_limited_body()`, so the configured value is what is
    enforced for every request that passes through the router;
  - the **4 MiB backstop**, the constant `MAX_BODY_BYTES_BACKSTOP`
    (`4 * 1024 * 1024` bytes) in `src/config/validation.rs`, which is passed to
    `axum::body::to_bytes`. It bounds the extractor even when the layer is
    not installed.
- The backstop is a constant and not a setting, because it is a safety ceiling
  of the parser rather than a deployment policy. The configuration validation
  **refuses** a `max_body_bytes` above it, so a deployment that raises
  `APP_MAX_BODY_BYTES` above 4 MiB does not start at all; the operator sees
  the bound named in the start-up error.
- A body over the limit is rejected with `413` and the code
  `payload_too_large` in the standard envelope.
- The limit applies to the raw body, before it is parsed.

## 10. CORS

- CORS is **disabled** when the resolved allowlist is empty. That is the
  correct configuration for an API that is not reachable from a browser. The
  allowlist is `api.cors.allowed_origins`, which is empty in
  `config/api.toml` and which the environment variable
  `APP_CORS_ALLOWED_ORIGINS` overrides. The variable therefore wins over the
  committed default: a `.env` that carries a non-empty value enables CORS
  even though the file says "off". See
  [`../architecture/deployment.md`](../architecture/deployment.md) §5.2.
- When it is configured, the allowlist is **explicit**. A wildcard `*` is
  rejected at start-up: the process refuses to start rather than shipping a
  foot-gun, because a wildcard origin combined with credentials is rejected
  by browsers anyway.
- The allowed methods are `GET`, `POST`, `PATCH`, `PUT`, `DELETE` and
  `OPTIONS`.
- The allowed request headers are `Authorization`, `Content-Type` and
  `Accept`.
- The preflight cache lifetime is 600 seconds.
- The API never enables "allow credentials". Tokens travel in the
  `Authorization` header, not in a cookie.

## 11. The health endpoints

| Endpoint | Meaning | Response |
| -------- | ------- | -------- |
| `GET /health/live` | The process is running and can serve requests. | `200` `{"status":"live"}` |
| `GET /health/ready` | The process is running **and** its dependencies are reachable. | `200` `{"status":"ready","checks":{"database":"ok"}}` or `503` `{"status":"degraded","checks":{"database":"unavailable"}}` |

- Both are outside the versioned prefix and outside the authentication
  boundary. An orchestrator must be able to probe the process without
  credentials.
- The readiness probe runs `SELECT 1` against the database with a **one
  second timeout**, so a stalled database cannot hang the probe.
- A degraded readiness answer is the only place where the API returns `503`.

```bash
curl -s http://127.0.0.1:3000/health/live
curl -s http://127.0.0.1:3000/health/ready
```

## 12. Authentication

- A private endpoint accepts **two bearer forms**, both in the `Authorization`
  header:
  - `Authorization: Bearer <access_token>` — a user JWT, short-lived and issued
    by the token endpoints (§1 of [`frontend-handover.md`](frontend-handover.md));
  - `Authorization: Bearer <api_key>` — a long-lived key an owner issues for a
    machine client, `atom_` followed by 64 lower-case hex characters.
- The scheme is matched case-insensitively.
- The two forms are told apart by **the token's prefix, and nothing else**. An
  API key always begins `atom_`; a JWT is base64url text whose first segment is a
  JSON header (`eyJ…`) and can therefore never begin with it. The decision is one
  branch in `AuthUser::from_request_parts`
  ([`../../src/api/middleware.rs`](../../src/api/middleware.rs)), so a value that
  begins `atom_` is routed to the key check and a garbage key is a `401`, never a
  decode error.
- Both forms resolve to **exactly one user**, and a key's authority is its
  owner's: there is no scope model and no reduced principal, so every private
  endpoint is reachable with either. The identity is derived **only** from the
  credential; nothing in the body, the query string or the path can influence it.
- Ownership is unchanged by the credential kind. A resource that belongs to
  another account is `404`, never `403`.
- An unknown, malformed or revoked key is `401` `unauthorized` with the same
  generic message an invalid access token gets. The key of a disabled account is
  `403` `forbidden`, which is the answer that account's JWT already receives.
- A key is created by `POST /api/v1/api-keys` — that response is the **only** one
  that carries the secret — listed by `GET /api/v1/api-keys` (never the secret)
  and revoked by `DELETE /api/v1/api-keys/{key_id}`, which is idempotent and
  answers `404` for an unknown or foreign id.
- There is no session cookie, no query parameter credential and no custom
  header alternative. The header is the only accepted form.

## 13. Request correlation

- Every request carries an `x-request-id` header. If the client supplies
  one, it is used; otherwise the server generates a UUID.
- The identifier is echoed on the response. It is carried by the header and
  by the tracing span of the request; it is **not** written as an explicit
  field on every log line, so correlating a single log line with a request
  relies on the tracing span context. The flow is in
  [`../architecture/request-lifecycle.md`](../architecture/request-lifecycle.md)
  §8.
- The header is named `x-request-id`, not the more modern `traceparent`; the
  project does not implement W3C trace context.

## 14. Timeouts

- The server stops processing a request after the configured timeout and
  answers `408 Request Timeout` with the standard envelope and the code
  `timeout`.
- The timeout is `APP_REQUEST_TIMEOUT_SECONDS`, **30 seconds** by default. It
  is read once at start-up and passed to the request through the router, so
  a deployment can shorten or lengthen it without a code change.
- The timeout is a server-side bound, not a client-side convention.

## 15. Money

A money value is a **JSON string with exactly four decimal places**. It is
never a JSON number, never a float and never a formatted string with a
currency symbol.

```json
{ "amount": "125.5000", "currency": "EUR" }
```

| Rule | Detail |
| ---- | ------ |
| Representation | `"125.5000"`, `"0.0000"`, `"999999999999999.9999"`. Always four decimals, even when the value is whole. |
| Rejected forms | A JSON number (`125.5`), `"125.5"`, `"125.500"`, `" 125.5000"`, `"1,250.0000"`, `"€125.5000"`. Each is a `422 validation_failed` naming the field. |
| Range | `0` to `"999999999999999.9999"`. A negative amount is refused. |
| Rounding | **None.** The server never rounds a money value; it refuses one that is not already at the scale. |
| Currency | ISO 4217, three ASCII letters, trimmed and upper-cased. A currency is a separate field. |
| One currency per claim | A reimbursement claim has exactly one currency, and its items carry none. No exchange rate and no conversion exist. |
| Computed totals | A total is computed by the server from the parts and is read-only in a request. |
| Storage | `NUMERIC(19,4)` in PostgreSQL, selected as text. |

The decision and its rejected alternatives are in
[`../decisions/ADR-0007-money-representation.md`](../decisions/ADR-0007-money-representation.md).
The affected fields are `total_amount` and `amount` on a claim, and
`salary_min` and `salary_max` on a job application.

A **similarity score** is not money: it is a JSON number in `[0,1]` (§17).

## 16. Filtering and sorting

### 16.1 How a list endpoint is shaped

Every list endpoint takes the two shared pagination parameters (§5) plus the
filters it owns. Each parameter is declared **once** in the OpenAPI document,
and the contract test asserts the published set exactly.

### 16.2 Filter conventions

| Filter kind | Form | Example | Notes |
| ----------- | ---- | ------- | ----- |
| Enum | the exact value, lowercase | `status=submitted` | An unknown value in the **query string** is a `400`, because the query could not be parsed. The same unknown value in a **body** is a `422` (§4 of the [error contract](error-contract.md)). |
| Identifier | a UUID | `company_id=…` | A value that is not a UUID is a `400`. |
| Free text | `search` | `search=acme` | Trimmed, case-insensitive substring, at most 200 characters. An empty value means "no filter"; a longer one is a `422` naming `search`. |
| Date range | `<field>_from` and `<field>_to` | `submitted_from=2026-10-01&submitted_to=2026-10-31` | `YYYY-MM-DD`, UTC, **inclusive of both ends**. `<field>_from` later than `<field>_to` is a `422`. A row whose field is `null` is excluded when either bound is present. |
| Currency | three letters | `currency=eur` | Trimmed and upper-cased; not three letters is a `422`. |

An unknown **parameter name** on a list endpoint is ignored by the query
parser, so a client must not rely on it having an effect. An unknown **value**
of a known enum parameter is a `400`.

### 16.3 Sort conventions

**The order of each list endpoint is fixed and documented. There is no
client-selectable sort in Phase 2.**

- A list of **records** is newest first: `<created_at> DESC, id DESC`. The
  identifier is the tie-breaker, so the order is total and two pages cannot
  overlap or skip a row.
- A list of **companies** is alphabetical: `lower(name)`, then `name`, then
  `id`.
- An **event timeline** is newest first: `occurred_at DESC, id DESC`. A client
  that wants a chronology reverses the page.
- The **items of a claim** are chronological: `incurred_on`, then
  `created_at`, then `id`.
- A search result is ordered by score, then identifier (§17).

A `sort` or `order` parameter is not part of the Phase 2 contract. Adding one
is a contract change: it changes the pagination a client observes.

### 16.4 The Phase 2 list endpoints

| Endpoint | Filters | Order |
| -------- | ------- | ----- |
| `GET /api/v1/claims` | `status`, `currency`, `search`, `submitted_from`, `submitted_to` | `created_at DESC, id DESC` |
| `GET /api/v1/companies` | `search` | `lower(name), name, id` |
| `GET /api/v1/job-applications` | `status`, `company_id`, `search`, `applied_from`, `applied_to` | `created_at DESC, id DESC` |
| `GET /api/v1/job-applications/{id}/events` | — | `occurred_at DESC, id DESC` |

These are in addition to the Phase 1 lists in §5, which keep their published
parameter sets unchanged.

## 17. Search modes

`POST /api/v1/notes/search` accepts three modes. The decision is
[`../decisions/ADR-0008-local-pure-rust-ml.md`](../decisions/ADR-0008-local-pure-rust-ml.md).

| Mode | What it does | Needs a model |
| ---- | ------------ | ------------- |
| `lexical` | PostgreSQL full-text match, the same query as the notes listing. No score. | No |
| `semantic` | Cosine similarity between the query vector and the caller's stored embeddings. | Yes |
| `hybrid` | A bounded lexical candidate set, re-ranked by cosine similarity in Rust. **The default.** | Yes |

### 17.1 The degradation rule

When ML is disabled or the model file is missing, a `semantic` or `hybrid`
request **succeeds with `200`** and returns lexical results. The response says
so:

| Field | Meaning |
| ----- | ------- |
| `requested_mode` | What the client asked for. |
| `mode` | What was applied. `lexical` when degraded. |
| `degraded` | `true` when the applied mode differs from the requested one. |
| `degraded_reason` | `ml_disabled`, `model_missing`, or `inference_timeout`. |

- A missing optional model is a **configuration state**, not a server error.
  It never produces `500` or `503`.
- A `lexical` request is never degraded, even when no model is configured.
- An unknown mode in the body is a `422`; it is never silently treated as
  lexical.
- `POST /api/v1/job-applications/{id}/match` has **no** lexical fallback: it
  answers `409 conflict` when ML is unavailable, because a ranking with no
  model would be a fiction.

### 17.2 Scores and totals

- `score` is a JSON number in `[0,1]`, rounded to four decimals, ordered
  descending, with the identifier as the tie-breaker. It is `null` in lexical
  mode.
- In `lexical` mode, `pagination.total` is the number of matching notes.
- In `hybrid` mode, `pagination.total` is the size of the **candidate set**,
  which is bounded by `ml.candidate_limit`. It is not "every relevant note",
  and a client must not present it as such.
- `semantic` mode scans the caller's embeddings; its `total` is the number of
  notes that have one.

## 18. Related documents

- [`error-contract.md`](error-contract.md) — the error envelope and every
  error code.
- [`../database/schema.md`](../database/schema.md) — the persistence model
  behind these contracts.
- [`../features/README.md`](../features/README.md) — the per-feature
  endpoint references.
- [`../project-truth.md`](../project-truth.md) — the agreed API design
  principles in §11.

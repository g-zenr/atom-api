# ATOM — request lifecycle

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `architect` agent

The path of a single request, from the socket to PostgreSQL and back. Read
this document when you change a middleware layer, an extractor, an error
mapping or the ownership predicate of a query.

---

## 1. The shape of a request

```text
socket
  │
  ├─ 1  CorsLayer               explicit origin allowlist; absent when the list is empty
  ├─ 2  TraceLayer              structured request logging
  ├─ 3  DefaultBodyLimit        the configured body limit
  ├─ 4  SetRequestIdLayer       sets the `x-request-id` header
  ├─ 5  PropagateRequestIdLayer echoes `x-request-id` into the response
  ├─ 6  Extension(RequestContext)  the message catalogue and the request limits
  ├─ 7  timeout_middleware      the configured timeout, answers 408
  ├─ 8  CatchPanicLayer         a panic becomes a 500, never a dropped connection
  │
  ├─ 9  route match
  │      ├─ /health/live, /health/ready        unauthenticated
  │      ├─ /api/v1/...                        the versioned API
  │      ├─ /api/v1/docs/, /api/v1/openapi.json  the API explorer, when it is enabled
  │      └─ anything else                      the 404 fallback
  │
  ├─ 10 extractors              State, AuthUser, Path, Query, HeaderMap, ClientIp, then the body
  │
  ├─ 11 handler                 HTTP semantics, status codes, serialization
  │
  ├─ 12 service                 business rules, validation
  │
  ├─ 13 repository              parameterised SQL with the ownership predicate
  │
  └─ 14 PostgreSQL             the answer comes back up the same path
```

## 2. The middleware stack

The stack is assembled in exactly one place, `api::router` in
`src/api/mod.rs`, so that the security posture of the API can be read in one
file.

### 2.1 The effective order

Axum's `Router::layer` wraps the routes that are already registered, so the
**last** `.layer(...)` call is the **outermost** layer. The effective order,
outermost first, is therefore the reverse of the order of the calls in the
source:

| # | Layer | Source | What it does |
| - | ----- | ------ | ------------ |
| 1 | `CorsLayer` | `api::cors_layer` | Added **last**, so it is the outermost layer. It is only installed when the configured allowlist is not empty. It allows `GET`, `POST`, `PATCH`, `PUT`, `DELETE` and `OPTIONS`, and the `Authorization`, `Content-Type` and `Accept` request headers. The preflight cache is 600 s. Credentials are not enabled. |
| 2 | `TraceLayer` | `tower_http::trace::TraceLayer::new_for_http()` | Emits a structured span and an event per request. |
| 3 | `DefaultBodyLimit` | `axum::extract::DefaultBodyLimit::max(config.app.limits.max_body_bytes)` | The configured body limit, 1 MiB by default. See §6. |
| 4 | `SetRequestIdLayer` | `tower_http::request_id::SetRequestIdLayer` with `MakeRequestUuid` | Sets the `x-request-id` request header to a UUIDv4 when the request does not already carry one. |
| 5 | `PropagateRequestIdLayer` | `tower_http::request_id::PropagateRequestIdLayer::new("x-request-id")` | Copies the `x-request-id` request header into the response header. Because layer 4 is outside it, the header is already set when this layer reads it. |
| 6 | `Extension` | `axum::Extension(RequestContext::new(&config))` | Puts the per-request context into the request extensions. The context carries the **message catalogue** and the **request limits** — the body limit and the request timeout. It is applied after the middleware, so it is the outer of the two and the value is already there when the timeout middleware and the extractors read it. |
| 7 | `timeout_middleware` | `api::timeout_middleware` in `src/api/mod.rs` | Bounds the server-side processing time at the configured request timeout (30 s by default) and answers `408` in the standard error envelope when it expires. The timeout is read from the `RequestContext` extension, not from a separate one. |
| 8 | `CatchPanicLayer` | `tower_http::catch_panic::CatchPanicLayer::custom(api::routes::handle_panic)` | The innermost layer. It catches a panic raised by the handler and turns it into a `500` in the standard error envelope. |

The timeout is **inner** to the request-id pair, so a request that is
abandoned by the timeout still carries its correlation id on the response.

### 2.2 The CORS layer

`api::cors_layer` returns `None` when the allowlist is empty, and the layer is
then not installed at all. That is the correct behaviour for a deployment that
is not reachable from a browser.

When the list is not empty:

- an entry of `*` is refused by the configuration validation, before the
  socket is bound: it is a `ConfigError`, not an assertion, because a
  wildcard origin combined with credentials is a security foot-gun;
- an entry that is not a valid header value is refused by the same rule, for
  the same reason;
- the router therefore has nothing to assert. If an origin still fails to
  parse, the layer falls back to an empty header value rather than panicking:
  a mistake can never take the process down at run time, and the origin is
  simply not matched;
- the allowed methods and headers are fixed in the source; only the origins
  are configurable.

The layer is the outermost one, so a CORS preflight is answered before any
authentication, any body limit and any rate limit.

## 3. The route table

The router is built in `src/api/mod.rs`:

```text
Router::new()
    .route("/health/live",  get(health::live))
    .route("/health/ready", get(health::ready))
    .nest("/api/v1", api_routes())
    .merge(swagger_ui())
    .fallback(not_found)
```

`api_routes()` in `src/api/routes.rs` defines the versioned surface:

| Method and path | Handler | Credential |
| --------------- | ------- | ---------- |
| `POST /api/v1/auth/register` | `auth::handlers::register` | none, rate limited |
| `POST /api/v1/auth/login` | `auth::handlers::login` | none, rate limited |
| `POST /api/v1/auth/refresh` | `auth::handlers::refresh` | none, rate limited |
| `POST /api/v1/auth/logout` | `auth::handlers::logout` | bearer |
| `GET /api/v1/auth/me` | `auth::handlers::me` | bearer |
| `POST /api/v1/api-keys` | `api_keys::handlers::create_api_key` | bearer |
| `GET /api/v1/api-keys` | `api_keys::handlers::list_api_keys` | bearer |
| `DELETE /api/v1/api-keys/{key_id}` | `api_keys::handlers::revoke_api_key` | bearer |
| `GET /api/v1/notes` | `notes::handlers::list_notes` | bearer |
| `POST /api/v1/notes` | `notes::handlers::create_note` | bearer |
| `GET /api/v1/notes/{note_id}` | `notes::handlers::get_note` | bearer |
| `PATCH /api/v1/notes/{note_id}` | `notes::handlers::update_note` | bearer |
| `DELETE /api/v1/notes/{note_id}` | `notes::handlers::delete_note` | bearer |
| `GET /api/v1/projects` | `projects::handlers::list_projects` | bearer |
| `POST /api/v1/projects` | `projects::handlers::create_project` | bearer |
| `GET /api/v1/projects/{project_id}` | `projects::handlers::get_project` | bearer |
| `PATCH /api/v1/projects/{project_id}` | `projects::handlers::update_project` | bearer |
| `DELETE /api/v1/projects/{project_id}` | `projects::handlers::delete_project` | bearer |
| `GET /api/v1/projects/{project_id}/tasks` | `tasks::handlers::list_tasks` | bearer |
| `POST /api/v1/projects/{project_id}/tasks` | `tasks::handlers::create_task` | bearer |
| `GET /api/v1/tasks/{task_id}` | `tasks::handlers::get_task` | bearer |
| `PATCH /api/v1/tasks/{task_id}` | `tasks::handlers::update_task` | bearer |
| `DELETE /api/v1/tasks/{task_id}` | `tasks::handlers::delete_task` | bearer |

The three API-key rows sit where `ROUTE_TABLE` places them — after the
authentication operations and before the notes ones — because that table is the
router's own order. The table above is not the whole surface: it lists the Phase
1 operations and the three API-key operations, and the full ordered list,
including the Phase 2 routes, is `ROUTE_TABLE` in
[`../../src/api/routes.rs`](../../src/api/routes.rs), which the contract test
compares with the generated document in both directions.

The API explorer is merged at the top level, not nested, and serves the UI at
`/api/v1/docs` and the document at `/api/v1/openapi.json`. It is registered
**only** there: registering the document a second time would create two
handlers for the same method and path. It is merged **only** when the
configured `api.docs_enabled` is true; the production overlay turns it off.

## 4. The extractor order inside a handler

Axum runs the `FromRequestParts` extractors first, in the order of the
handler's parameters, and the `FromRequest` extractor — the one that consumes
the body — last.

A typical private handler is declared like this:

```rust
pub async fn create_note(
    State(state): State<AppState>,      // 1. shared state
    user: AuthUser,                     // 2. authentication
    Json(payload): Json<CreateNoteRequest>, // 3. the body
) -> AppResult<(StatusCode, Json<NoteResponse>)>
```

so the order is:

1. `State<AppState>` — the shared dependencies.
2. `AuthUser` — **authentication happens here**, before the body is even read.
   The extractor requires an `Authorization: Bearer <token>` header and takes
   one of two branches, decided by the token's prefix alone:
   - a token that begins `atom_` is an **API key**. It is resolved by
     `ApiKeyService::authenticate`, which refuses a wrong-shaped value before
     any statement, then binds the SHA-256 digest in one statement that joins
     `users` — so an unknown digest, a revoked key and a disabled account are
     all decided without a second round trip;
   - anything else is a **user JWT**. The HS256 token is verified and decoded
     through `TokenService` and the account is loaded by
     `AuthService::authenticate_bearer`. A JWT's first segment is a base64url
     JSON header, so it can never begin with `atom_` and the two branches
     cannot claim the same value.

   Both branches converge on the same `AuthUser`, so an API key is not a
   reduced principal: it reaches every private route its owner reaches, the
   key-management routes included. A missing, malformed, unknown, revoked or
   expired credential of **either** kind is `AppError::Unauthorized`; a
   **disabled account** is `AppError::Forbidden` for both kinds, with the same
   `messages.auth.account_disabled` message, so one account state has one
   answer (amendment `A-C-K1-1`). The branch is in `AuthUser::from_request_parts`
   in [`../../src/api/middleware.rs`](../../src/api/middleware.rs).
3. `Path`, `Query` and `HeaderMap` — the remaining parts of the request. Each
   rejection is an `AppError::BadRequest` carrying the catalogue message
   `malformed_parameter` — "The request path or query string is malformed."
4. `Json<T>` — the body. A body that is not valid JSON is an
   `AppError::BadRequest` carrying the catalogue message `bad_request` — "The
   request body is not valid JSON." A well-formed body that does not match
   the contract is a `422` validation failure naming the field `body` and
   carrying the catalogue message `validation_failed`.

The extractors take their message text from the catalogue the `RequestContext`
extension carries, so no extractor reads the configuration or the environment
itself. The catalogue lives in `config/messages.toml`; the stable error code
comes from the `code` module in `src/error.rs`.

`ClientIp` is an ordinary `FromRequestParts` extractor. It reads the
`ConnectInfo<SocketAddr>` extension and returns `None` when the server was
not started with the connection information. The server in `src/main.rs`
serves the router through `into_make_service_with_connect_info::<SocketAddr>()`,
so the peer address is available in the running service; the test harness
builds the router directly and therefore does not install it. See
[`security-architecture.md`](security-architecture.md) §6.

The extractors in `src/api/extract.rs` exist because Axum's own `Json`, `Path`
and `Query` reject with a `text/plain` body and framework wording, which
would break the documented error envelope.

## 5. From handler to SQL

The layers below the handler are strictly ordered, and each one has a single
job.

```text
handler            HTTP only: extraction, status code, serialization
  │
  ▼
service            business rules, validation, the owner id is passed in
  │
  ▼
repository         SQL, bound parameters, the ownership predicate
  │
  ▼
PostgreSQL
```

A worked example — `GET /api/v1/tasks/{task_id}`:

1. `tasks::handlers::get_task` extracts `State`, `AuthUser` and
   `Path<Uuid>`. The owner is `user.user_id`, taken from the credential.
2. `TaskService::get(owner, id)` calls
   `TaskRepository::find_owned(id, owner)`.
3. The repository runs one statement:

   ```sql
   SELECT t.id, t.project_id, t.title, t.description, t.status, t.priority,
          t.position, t.created_at, t.updated_at, t.completed_at
   FROM tasks t
   JOIN projects p ON p.id = t.project_id
   WHERE t.id = $1 AND p.user_id = $2
   ```

   The task is reached through its project, and the project is reached
   through its owner. The caller cannot name another account's task.
4. A row becomes a `Task`, then a `TaskResponse`, then the JSON body.
   A missing row becomes `AppError::NotFound` and therefore a `404`, never a
   `403`.

### 5.1 Where authorization happens

**In SQL, not in the handler.** The handler passes the caller's `user_id` down
and does not compare it with anything. This is the decision recorded in
[`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md),
and it is why a resource the caller does not own is answered with `404`.

### 5.2 Transaction boundaries

There is **one explicit transaction** in the Phase 1 request path: the
refresh-token rotation in `auth::service::refresh`. The repository's
`rotate` inserts the replacement session and claims the old one inside a
single transaction, so a rotation that does not complete — a lost race, a
failed statement, a process death — is rolled back and leaves no replacement
row behind. Every other repository method is a single statement, so every
other write is atomic on its own. The boundary is analysed in
[`data-architecture.md`](data-architecture.md) §7.

## 6. Bodies, timeouts and memory

- **Timeout.** `api::timeout_middleware` bounds the whole request at the
  configured request timeout (30 s by default) and answers `408`. It is a
  custom middleware, not a `tower_http` layer: it reads the timeout from the
  `RequestContext` extension the router installed, and answers with
  `AppError::Timeout` carrying the catalogue message `timeout`, so the body of
  that response **is** the JSON envelope, with the code `timeout`.
- **Body limit.** The router installs
  `DefaultBodyLimit::max(config.app.limits.max_body_bytes)`, 1 MiB by
  default. Axum documents that layer as applying only to the extractors that
  consult it — Axum's own `Bytes`, `Json`, `String` and `Form`. Every
  endpoint of this API reads its body through the project's own
  `api::extract::Json`, which calls `req.into_limited_body()` and therefore
  **does** consult the configured limit. The extractor also passes an
  independent **4 MiB backstop** — the constant `MAX_BODY_BYTES_BACKSTOP` —
  to `axum::body::to_bytes`, so a body is refused when it exceeds **either**
  limit. A body over the cap is answered with `AppError::PayloadTooLarge`,
  which is a `413` in the standard envelope.
- **No concurrency limit.** There is no per-client cap on the number of
  simultaneous requests, and the timeout does not bound the work a query
  performs inside the database.

## 7. How errors become responses

Every failure inside the request path is an `AppError`, and `AppError`
implements `IntoResponse`. A handler returns `AppResult<T>` and never builds
a response by hand.

| Origin | Variant | Status | Code |
| ------ | ------- | ------ | ---- |
| `extract::Json` — the body is not JSON | `BadRequest` | `400` | `bad_request` |
| `extract::Json` — the body does not match the contract | `Validation` | `422` | `validation_failed` |
| `extract::Path`, `extract::Query` — an unparsable value | `BadRequest` | `400` | `bad_request` |
| `AuthUser` — no, malformed or invalid credential, of either kind | `Unauthorized` | `401` | `unauthorized` |
| `AuthUser` — a disabled account, with either kind | `Forbidden` | `403` | `forbidden` |
| `api::middleware::enforce_rate_limit` | `RateLimited` | `429` | `rate_limited`, plus a `retry-after` header |
| The service — a resource the caller does not own | `NotFound` | `404` | `not_found` |
| The service — a duplicate registration | `Conflict` | `409` | `conflict` |
| The service — a policy violation | `Validation` | `422` | `validation_failed`, with the field names |
| The repository — a database failure | `Database` | `500` | `database_error`, generic message |
| `routes::handle_panic` | `Internal` | `500` | `internal_error`, generic message |
| The timeout middleware | `Timeout` | `408` | `timeout`, in the envelope |
| The body limit — a body over the configured limit | `PayloadTooLarge` | `413` | `payload_too_large` |
| `routes::not_found` — an unmatched route | — | `404` | `not_found`, in the envelope |

Every message text in that table comes from the catalogue in
`config/messages.toml`, resolved at the call site. The two exceptions are
the database failure and the unexpected failure: their text is fixed in the
error type, because it must never be replaced by something that reveals an
internal detail. The **code** is the contract, and it has exactly one
definition each in the `code` module in `src/error.rs`.

A server error is logged at `error` level. A **database** failure is logged
with its SQLSTATE and the error kind only — the driver message is not
logged, because a PostgreSQL message can contain the failing row, which for
the `users` table includes the password hash. A rejection is logged at
`debug` level. Neither the log line nor the response body ever contains a
password, a token, the signing secret or the connection URL. The full
contract is in [`../api/error-contract.md`](../api/error-contract.md).

## 8. The correlation id

```text
incoming request
   │
   ├─ SetRequestIdLayer      the request has no `x-request-id`?
   │                        → a UUIDv4 is generated and set on the request
   │                        the request already has one?
   │                        → the existing value is kept
   ▼
handler                   the id is in the request headers
   ▼
PropagateRequestIdLayer  the request header is copied into the response
```

- A client may supply its own `x-request-id`; the API keeps it. This lets a
  caller correlate its own logs with the API's.
- The response always carries the same value in `x-request-id`. The timeout
  is inner to the pair, so a request that is abandoned by the timeout still
  carries the id.
- The `TraceLayer` span and the error log lines are emitted inside the same
  request, so the trace and the error share the request context.
- The id is **not** read back out of the headers by the application: there
  is no helper that extracts it. The correlation is therefore carried by the
  header and the tracing span, not by an explicit field in a log line.

## 9. Related documents

- [`module-boundaries.md`](module-boundaries.md) — what each layer owns.
- [`security-architecture.md`](security-architecture.md) — the boundaries this
  lifecycle crosses.
- [`../api/conventions.md`](../api/conventions.md) — the public HTTP
  conventions.
- [`../api/error-contract.md`](../api/error-contract.md) — the error envelope.
- [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md)
  — why authorization lives in the repository.

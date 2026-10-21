# ATOM — module boundaries

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `architect` agent

What each module owns, what it may depend on, and which invariant it is
responsible for. The layering rule that this document enforces is fixed by
[`../../AGENTS.md`](../../AGENTS.md) §2:

| Layer | Responsibility |
| ----- | -------------- |
| `handlers` | HTTP extraction, status codes, response serialization. |
| `service` | Business rules, validation, transaction coordination. |
| `repository` | SQL, parameterised queries, ownership predicates. |
| `models` | Persistence shapes. |
| `dto` | Public request and response contracts. |

**A handler never talks to the database.** Every feature module keeps the
same five files, and the modules communicate through types, not through
function calls into another module's internals.

---

## 1. `api` — routing, middleware and shared HTTP concerns

Source: `src/api/mod.rs`, `src/api/routes.rs`, `src/api/middleware.rs`,
`src/api/extract.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Builds the router, installs the middleware stack, defines the route table and the OpenAPI document, and provides the shared extractors. It owns the *shape* of the HTTP surface, never the business rules. |
| Public interface | `api::router(state) -> Router`, `api::routes::api_routes() -> Router<AppState>`, `api::routes::swagger_ui() -> Router<AppState>`, `api::routes::ApiDoc`, `api::routes::not_found`, `api::routes::handle_panic`, `api::timeout_middleware`, `api::middleware::AuthUser`, `api::middleware::ClientIp`, `api::middleware::enforce_rate_limit`, `api::extract::Json`, `api::extract::Path`, `api::extract::Query`. |
| May depend on | `config`, `error`, `state`, `health`, and the *handlers* of every feature module. It must not depend on a feature module's repository or service directly. |
| Data ownership | None. The module reads and writes no table. |
| Invariants | The middleware stack is installed exactly once, in `router`. Health routes stay outside `/api/v1` and outside the authentication boundary. Every extractor rejection is an `AppError`, so every rejection uses the documented error envelope. |
| Error handling | `extract::Json`, `extract::Path` and `extract::Query` map every rejection onto `AppError`. `routes::not_found` answers an unmatched route with the same envelope and the code `not_found`. `routes::handle_panic` turns a panic into a `500` in the same envelope and deliberately does not log the payload. |
| Security boundary | The authentication boundary. `AuthUser` is the only way a handler learns who the caller is, and it derives the identity from the credential alone. `ClientIp` reads the connection information and never trusts `X-Forwarded-For`. The CORS allowlist is built here and rejects `*` with a start-up assertion. |
| Testing approach | Exercised end to end by `tests/auth_api.rs`, `tests/notes_api.rs` and `tests/projects_tasks_api.rs`, which build the router through `api::router` in `tests/common/mod.rs`. There is no dedicated unit test file for the module. |
| Known limitations | The module dependency on the feature modules is **bidirectional**: `api::routes` names the feature handlers, and the feature handlers use `api::extract` and `api::middleware`. The contract between them is the extractor types and the handler signatures, not the module boundary. The correlation id is carried by the `x-request-id` header and the tracing layer; the application never reads it back out of the headers. |

---

## 2. `auth` — authentication and the token lifecycle

Source: `src/auth/handlers.rs`, `src/auth/service.rs`, `src/auth/token.rs`,
`src/auth/password.rs`, `src/auth/repository.rs`, `src/auth/models.rs`,
`src/auth/dto.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Registration, login, the access/refresh token lifecycle, session rotation, reuse detection, session revocation, and Argon2id password hashing. |
| Public interface | `AuthService` with `register`, `login`, `refresh`, `logout`, `revoke_all_sessions`, `prune_expired_sessions`, `current_user`, `authenticate_bearer`; `TokenService` with `issue_access_token`, `decode_access_token`, `access_ttl`; `PasswordHasher` with `hash`, `verify`, `dummy_hash`; `generate_refresh_token`, `hash_refresh_token`; the DTOs `LoginRequest`, `RefreshRequest`, `LogoutRequest`, `TokenPair`, `AuthResponse`; the model `RefreshSession`. |
| May depend on | `error`, `config` (for `Argon2Params`), `users` (for the account and its public shape), `api::extract` and `api::middleware` (from `src/auth/handlers.rs` only). It must not depend on `notes`, `projects` or `tasks`. |
| Data ownership | The `refresh_sessions` table. It reads the `users` table, which `users` owns. |
| Invariants | The access token is short-lived and HS256-signed with issuer, audience, subject and expiry validated. The refresh token is opaque, is stored only as a SHA-256 hash, is rotated on every use and is single-use. The rotation claims the presented session with an atomic update that only matches a session that is still active, so two concurrent requests carrying the same token cannot both succeed. Presenting a rotated token revokes every session of that account. Passwords are hashed with Argon2id and the plain password is never stored, logged or returned. Every **login** failure carries the same generic message, so that an attacker cannot learn whether an address is registered. A disabled account is refused. |
| Error handling | `AppError::Unauthorized` for a missing, malformed, expired or reused credential; `AppError::Forbidden` for a disabled account and for a logout that targets another account's session; `AppError::Conflict` for a duplicate registration. |
| Security boundary | The authentication boundary (TB-5 in [`../security/threat-model.md`](../security/threat-model.md)). The identity of a caller comes only from the credential. The three public endpoints — `register`, `login`, `refresh` — are rate limited per client by `api::middleware::enforce_rate_limit`. |
| Testing approach | `tests/auth_api.rs` covers registration, normalization, the duplicate-registration conflict, the password policy, login, the generic failure message, refresh rotation, reuse detection, logout, the rejection of missing or invalid credentials, `GET /api/v1/auth/me` and the cross-account logout attempt. Unit tests cover the token service (`src/auth/token.rs`), the password hasher (`src/auth/password.rs`) and the email and password policy (`src/users/service.rs`). |
| Known limitations | `AuthService::revoke_all_sessions` is **not reachable through any endpoint**: there is no user-facing session list and no per-session revocation surface. The expired sessions are removed by the background pruning task in `src/main.rs`, which calls `RefreshSessionRepository::delete_expired` every hour with a one-day grace. The refresh token is returned in the response body rather than in an HTTP-only cookie (open question Q2). There is one absolute refresh-token lifetime and no idle timeout (Q3). The timing equalisation for an unknown email address is a dummy Argon2 verification, not a measured constant. |

---

## 3. `users` — the account and the public user shape

Source: `src/users/service.rs`, `src/users/repository.rs`,
`src/users/models.rs`, `src/users/dto.rs`. The module has **no HTTP handlers**:
the authentication endpoints live in `auth`, and the `users` module exposes its
rules through functions that `auth` calls.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | The user record, email normalization, the password policy, the display-name policy and the safe public representation of an account. |
| Public interface | `UserService` with `create_user`, `get_user`, `get_user_response`; the free functions `normalize_email`, `validate_email`, `validate_password`, `validate_display_name`; the DTOs `RegisterRequest`, `UserResponse`; the model `User`. |
| May depend on | `error`. It must not depend on `auth`, `notes`, `projects` or `tasks`: `auth` depends on `users`, not the other way round. |
| Data ownership | The `users` table. |
| Invariants | The stored email is trimmed and lower-cased, and a `CHECK` constraint in the database enforces the same rule, so the two cannot drift. Uniqueness is enforced by the database, never by a check-then-insert in the application. The password is already hashed when it reaches the repository. `UserResponse` contains no password hash and no session material. |
| Error handling | `AppError::validation` with a named field for every policy violation; `AppError::Conflict` when the email is already registered; `AppError::NotFound` when the user does not exist; `AppError::validation_with` when a `CHECK` constraint is violated (SQLSTATE `23514`), so that the database message never reaches the log. |
| Security boundary | Owns the account record and therefore the credential material. `UserRepository::find_by_email` is the one query that reads a private table without an owner predicate, because it looks the account up *by* its identity rather than by its owner. |
| Testing approach | `tests/auth_api.rs` covers registration, normalization and the duplicate-registration conflict. Unit tests in `src/users/service.rs` cover normalization, the email rules, the password policy and the display-name rule. `src/config.rs` tests the secret validation that protects the signing material. |
| Known limitations | `UserRepository::is_active` is public but unused; the active check is made on the loaded record instead. There is no password-change, email-change or account-deletion surface, and `is_active` cannot be changed through the API. |

---

## 4. `notes` — personal notes

Source: `src/notes/handlers.rs`, `src/notes/service.rs`,
`src/notes/repository.rs`, `src/notes/models.rs`, `src/notes/dto.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Notes: create, list, read, partially update, delete, archive and search. |
| Public interface | `NoteService` with `create`, `list`, `get`, `update`, `delete`; the free functions `validate_title`, `validate_content`; `NoteRepository` with `insert`, `find_owned`, `list_owned`, `update_owned`, `delete_owned`; `NoteFilters`; the DTOs `CreateNoteRequest`, `UpdateNoteRequest`, `NoteQuery`, `NoteResponse`, `NotePage`; the model `Note`. |
| May depend on | `error`, `pagination`, `api::extract`, `api::middleware`. It must not depend on `projects` or `tasks`, so that a future "note attached to a task" does not require a schema change to the `notes` table. |
| Data ownership | The `notes` table. |
| Invariants | Every statement carries the caller's `user_id`. A note that the caller does not own is invisible to the repository and is answered with `404`. The listing is ordered by `created_at DESC, id DESC`. The archive filter is tri-state: omitted and `false` both return active notes only, `true` returns archived notes only. A partial update never writes `null` over an existing value: the update uses `COALESCE($n, column)`. |
| Error handling | `AppError::validation` for an empty or oversized title and for oversized content; `AppError::NotFound` when the note does not exist or is not owned by the caller. |
| Security boundary | The caller's notes only. The search predicate is part of the same statement as the ownership predicate, so a search can never reach another account's notes. |
| Testing approach | `tests/notes_api.rs` covers the full CRUD cycle, cross-account invisibility, the listing scope, the search over title and content, the archive filter, the empty-title rejection, the pagination envelope and the rejection of a non-UUID identifier. Unit tests in `src/notes/service.rs` cover the title and content rules. |
| Known limitations | Search is a plain full-text match with no ranking and no highlighting (open question Q4). The `search_vector` column is a generated `tsvector`; the index strategy is described in [`data-architecture.md`](data-architecture.md). |

---

## 5. `projects` — project containers

Source: `src/projects/handlers.rs`, `src/projects/service.rs`,
`src/projects/repository.rs`, `src/projects/models.rs`,
`src/projects/dto.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Projects: create, list, read, partially update, delete. A project is the container that owns tasks. |
| Public interface | `ProjectService` with `create`, `list`, `get`, `update`, `delete`; the free functions `validate_name`, `validate_description`; `ProjectRepository` with `insert`, `find_owned`, `list_owned`, `update_owned`, `delete_owned`; the DTOs `CreateProjectRequest`, `UpdateProjectRequest`, `ProjectQuery`, `ProjectResponse`, `ProjectPage`; the model `Project`. |
| May depend on | `error`, `pagination`, `api::extract`, `api::middleware`. It must not depend on `tasks`: `tasks` depends on `projects`, not the other way round. |
| Data ownership | The `projects` table. It is the parent of `tasks`; deleting a project cascades to its tasks through `ON DELETE CASCADE` in the database. |
| Invariants | Every statement carries the caller's `user_id`. A project that the caller does not own is answered with `404`. The listing is ordered by `created_at DESC, id DESC`. A partial update uses `COALESCE($n, column)` and never nulls an omitted field. The optional search is a case-insensitive substring match over the name and the description. |
| Error handling | `AppError::validation` for an empty or oversized name and for an oversized description; `AppError::NotFound` when the project does not exist or is not owned by the caller. |
| Security boundary | The caller's projects only. Because the tasks of a project inherit the project's owner, a project is the authorization anchor for its tasks. |
| Testing approach | `tests/projects_tasks_api.rs` covers the full project CRUD cycle, cross-account invisibility and the cascade from a deleted project to its tasks. Unit tests in `src/projects/service.rs` cover the name and description rules. |
| Known limitations | Deleting a project **deletes** its tasks. The product question — delete or archive — is open (Q1) and is recorded in [`../features/projects/specification.md`](../features/projects/specification.md). |

---

## 6. `tasks` — units of work inside a project

Source: `src/tasks/handlers.rs`, `src/tasks/service.rs`,
`src/tasks/repository.rs`, `src/tasks/models.rs`, `src/tasks/dto.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Tasks: create, list, read, partially update, delete, with a status, a priority and a manual ordering key. |
| Public interface | `TaskService` with `create`, `list_for_project`, `get`, `update`, `delete`; the free functions `validate_title`, `validate_description`; `TaskRepository` with `insert`, `find_owned`, `list_for_project`, `update_owned`, `delete_owned`; `NewTask`, `TaskPatch`, `TaskFilters`; the DTOs `CreateTaskRequest`, `UpdateTaskRequest`, `TaskQuery`, `TaskResponse`, `TaskPage`; the models `Task`, `TaskStatus`, `TaskPriority`. |
| May depend on | `error`, `pagination`, `api::extract`, `api::middleware`. It must not depend on `notes`. |
| Data ownership | The `tasks` table. It has no `user_id` column: ownership is inherited from the parent project. |
| Invariants | A task is always reached through its project, and the project is always reached through its owner, so every statement joins to `projects` and carries the caller's `user_id`. The insert is a single `INSERT … SELECT … FROM projects p WHERE p.id = $2 AND p.user_id = $3`, so a task can never be attached to a project the caller does not own. `status` and `priority` are database enum types, so an out-of-contract value cannot be persisted. `completed_at` is maintained by the database: it is stamped when the status becomes `done` and cleared when it leaves `done`, and a `CHECK` constraint ties the two together. A partial update uses `COALESCE($n, column)`. The listing is ordered by `position ASC, created_at ASC, id ASC`. |
| Error handling | `AppError::validation` for an empty or oversized title and for an oversized description; `AppError::NotFound` when the task, or the project the task would live in, does not exist or is not owned by the caller. An unknown `status` or `priority` in the request body is rejected by the deserializer as a validation failure. |
| Security boundary | Ownership is inherited through the project join. A task reached through a project the caller does not own is `404`, never `403`. The list endpoint additionally probes the project when the result set is empty, so that "no such project" and "no tasks" are distinguishable without disclosing another account's project. |
| Testing approach | `tests/projects_tasks_api.rs` covers task creation and listing inside a project, the completion timestamp, filtering by status and priority, the refusal to reach a task through another account's project, the refusal to create a task in another account's project, the cascade on project deletion and the rejection of an unknown status. Unit tests in `src/tasks/service.rs` and `src/tasks/models.rs` cover the title and description rules and the wire names of the enum variants. |
| Known limitations | The ordering key is a `DOUBLE PRECISION` fraction and there is **no reordering endpoint**: a client sets `position` explicitly. A new task is placed at `MAX(position) + 1000` when no position is given. The open question Q6 covers the ordering semantics. |

---

## 7. Cross-cutting modules

### 7.1 `config` — typed configuration

Source: `src/config.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Reads every environment variable exactly once, parses it, validates it and produces a typed `Config`. |
| Public interface | `Config::from_env() -> Result<Config, ConfigError>`, `Config`, `DatabaseConfig`, `AuthConfig`, `Argon2Params`, `RateLimitConfig`, `Environment`, `ConfigError`. |
| May depend on | Nothing inside the crate. |
| Data ownership | No data. It owns the *shape* of the runtime configuration. |
| Invariants | `DATABASE_URL` and `APP_JWT_SECRET` are required and must not be empty. The signing secret must be at least 32 bytes, and in `Production` the documented placeholder is refused. The refresh-token lifetime must be longer than the access-token lifetime. The Argon2 memory must be at least 8 MiB. `APP_MAX_BODY_BYTES` must be positive. `APP_ENV` accepts only `development`/`dev`/`local` or `production`/`prod`. |
| Error handling | `ConfigError::Missing` and `ConfigError::Invalid`, both raised **before** the socket is bound. |
| Security boundary | `DatabaseConfig` and `AuthConfig` implement `Debug` manually and print `[REDACTED]` for the connection URL and the signing secret, so a configuration dump cannot leak them. |
| Testing approach | Unit tests in `src/config.rs` cover the CSV parsing, the environment aliases, the short-secret rejection, the placeholder rejection in production only, and the redaction of the secret in the `Debug` output. |
| Known limitations | The configuration is read from the process environment only; there is no file, command-line or remote configuration source. The CORS allowlist is parsed but never validated against a real origin. |

### 7.2 `error` — the single error contract

Source: `src/error.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | One error type for the whole application, its stable code, its HTTP status and its public envelope. |
| Public interface | `AppError`, `AppResult<T>`, `ErrorBody`, `ErrorDetail`, `FieldViolation`, and the constructors `AppError::validation` and `AppError::validation_with`. |
| May depend on | Nothing inside the crate. |
| Data ownership | No data. |
| Invariants | Every variant has exactly one stable code and one status. A database or internal failure never reaches the client with its detail: the body is the fixed message "The request could not be completed." A validation failure carries the offending field names. A rate-limited response carries a `retry-after` header. |
| Error handling | `impl IntoResponse for AppError` renders the envelope, logs a server error at `error` level and a rejection at `debug` level. A database failure is logged with its SQLSTATE and the error kind only, never with the driver message. |
| Security boundary | This module is the main defence against information disclosure. The full contract is in [`../api/error-contract.md`](../api/error-contract.md). |
| Testing approach | Unit tests in `src/error.rs` cover the validation status and field details, the redaction of internal details and of the SQLx driver message, and the distinct codes for `not_found` and `forbidden`. The HTTP-level shape is exercised by the integration suite. |
| Known limitations | One response is produced outside this module and therefore outside the envelope: Axum's own `405 Method Not Allowed` for a wrong method on a known path. The `408` from the request timeout is an `AppError::Timeout` and therefore uses the standard envelope. Everything the application itself rejects uses the envelope. |

### 7.3 `state` — the shared dependencies

Source: `src/state.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Holds the shared dependencies and passes them to the handlers explicitly. There is no global mutable state. |
| Public interface | `AppState` with the public fields `config`, `db`, `rate_limiter`, `tokens`, `passwords`, `auth`, the accessor `auth_service()` and the method `prune_expired_sessions(grace)`. |
| May depend on | `config`, `auth`, `users`, `ratelimit`. |
| Data ownership | The `PgPool`. It shares the pool with every repository, but each repository holds its own clone of the handle. |
| Invariants | `AppState` is cheap to clone: every field is `Arc`-backed or a `PgPool` handle. The services are constructed once, at start-up, and never rebuilt per request. |
| Error handling | None of its own. Construction cannot fail; it takes an already validated `Config` and an already connected pool. |
| Security boundary | Holds the signing secret inside `TokenService` and the Argon2 parameters inside `PasswordHasher`. Neither is exposed in a `Debug` rendering. |
| Testing approach | No dedicated test file. `tests/common/mod.rs` builds the state through `AppState::new` with a test configuration. |
| Known limitations | The services are built once, so the configuration cannot change at run time. There is no way to rotate the signing secret without restarting the process. |

### 7.4 `pagination` — the list convention

Source: `src/pagination.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | One pagination convention and one list envelope for every list endpoint. |
| Public interface | `PaginationQuery` with `effective_limit` and `effective_offset`, `Pagination::new`, `Page<T>::new`, the constants `DEFAULT_LIMIT` (20) and `MAX_LIMIT` (100). |
| May depend on | Nothing inside the crate. |
| Data ownership | No data. |
| Invariants | The effective limit is never above 100 and never below the default; a non-positive limit falls back to 20. A negative offset is treated as zero. `has_more` is `offset + limit < total`. |
| Error handling | None. The clamping happens silently, so an out-of-range limit is not a validation failure. |
| Security boundary | The limit caps how much data one request can pull out of the database. |
| Testing approach | Unit tests in `src/pagination.rs` cover the clamping, the fallback and the `has_more` computation. The HTTP-level envelope is exercised by `tests/notes_api.rs`. |
| Known limitations | The convention is `limit`/`offset`, so a deep page costs the database a large offset. There is no cursor. |

### 7.5 `ratelimit` — the in-process limiter

Source: `src/ratelimit.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | A fixed-window request counter, keyed by client address, for the authentication endpoints. |
| Public interface | `RateLimiter::new`, `RateLimiter::check`, `RateLimiter::reset`, and the `Decision` enum (`Allowed`, `Rejected { retry_after_seconds }`). |
| May depend on | Nothing inside the crate. `api::middleware::enforce_rate_limit` is the only caller. |
| Data ownership | A `Mutex<HashMap<IpAddr, WindowEntry>>` in the process memory. Nothing is persisted. |
| Invariants | Entries whose window has elapsed are dropped on every check, so the map cannot grow without bound. A rejected call reports the remaining seconds of the window. |
| Error handling | None. `enforce_rate_limit` converts a rejection into `AppError::RateLimited`, which becomes a `429` with a `retry-after` header. |
| Security boundary | A defence-in-depth control on the authentication boundary. It is **not** a substitute for edge rate limiting. |
| Testing approach | Unit tests in `src/ratelimit.rs` cover the limit, the independence of clients and the window reset. The HTTP-level behaviour is exercised by `the_authentication_endpoints_are_rate_limited` in [`../../tests/auth_api.rs`](../../tests/auth_api.rs). |
| Known limitations | The counters are per process, so a multi-instance deployment has *n* times the intended budget. The limiter fails open when its mutex is poisoned. The fixed window allows a burst across the boundary. The server installs the connection information, so the limiter is keyed by the peer address; behind a proxy that does not terminate the connection, every request carries the proxy's address and the budget is shared again. See [`../engineering/known-issues.md`](../engineering/known-issues.md) KI-001, KI-003 and KI-008. |

### 7.6 `telemetry` — logging

Source: `src/telemetry.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Installs the global tracing subscriber and selects the formatter. |
| Public interface | `telemetry::init(filter: &str)`. |
| May depend on | Nothing inside the crate. |
| Data ownership | The global subscriber. |
| Invariants | The filter comes from `APP_LOG_FILTER` and falls back to `info`. An invalid filter falls back to `info` rather than failing the start-up. A debug build uses the human-readable formatter; a release build uses the JSON formatter with a flattened event. |
| Error handling | None of its own. It panics if a subscriber is already installed, which can only be a programming error. |
| Security boundary | The log sink is a secret-bearing boundary: the formatter never adds request bodies, and the error renderer redacts internal detail before it can reach a log line. |
| Testing approach | No dedicated test. The format is exercised by running the service. |
| Known limitations | There is no redaction layer over arbitrary log fields, no audit trail and no metric export. The formatter choice is made by build profile, not by configuration. |

### 7.7 `health` — probes

Source: `src/health.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Liveness and readiness for an orchestrator. |
| Public interface | `health::live`, `health::ready`, and the response types `LiveResponse`, `ReadyResponse`, `ReadyChecks`. |
| May depend on | `state`. It must not depend on a feature module. |
| Data ownership | No data. The readiness probe runs `SELECT 1`. |
| Invariants | Both probes live outside `/api/v1` and outside the authentication boundary, so an orchestrator can reach them without credentials. The readiness probe is bounded by a one-second timeout, so a stalled database cannot hang the probe. |
| Error handling | None: the probes answer with `200` and a JSON body, or with `503` and a `degraded` body. |
| Security boundary | Both endpoints are unauthenticated and reveal only whether the process is alive and whether the database answered. They expose no internal detail. |
| Testing approach | No dedicated automated test. The behaviour was exercised by an operator run; see [`../engineering/implementation-status.md`](../engineering/implementation-status.md). |
| Known limitations | The readiness probe checks only the database. There is no dependency on a cache, a broker or an upstream service, because Phase 1 has none. |

## 8. Related documents

- [`system-overview.md`](system-overview.md) — the whole system on one page.
- [`request-lifecycle.md`](request-lifecycle.md) — how the modules are entered.
- [`data-architecture.md`](data-architecture.md) — the tables each module owns.
- [`security-architecture.md`](security-architecture.md) — the boundaries.
- [`../decisions/ADR-0001-modular-monolith.md`](../decisions/ADR-0001-modular-monolith.md)
  — the modular monolith.
- [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md)
  — ownership in SQL.

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
| Public interface | `api::router(state) -> Router`, `api::routes::api_routes() -> Router<AppState>`, `api::routes::swagger_ui() -> Router<AppState>`, `api::routes::ApiDoc`, `api::routes::not_found`, `api::routes::handle_panic`, `api::timeout_middleware`, `api::RequestContext`, `api::middleware::AuthUser`, `api::middleware::ClientIp`, `api::middleware::enforce_rate_limit`, `api::extract::Json`, `api::extract::Path`, `api::extract::Query`. |
| May depend on | `config`, `error`, `state`, `health`, and the *handlers* of every feature module. It must not depend on a feature module's repository or service directly. |
| Data ownership | None. The module reads and writes no table. |
| Invariants | The middleware stack is installed exactly once, in `router`. The router installs a `RequestContext` extension on every request; it carries the client-facing message catalogue and the request limits, so that no layer or extractor has to read the configuration or the environment itself. Health routes stay outside `/api/v1` and outside the authentication boundary. Every extractor rejection is an `AppError`, so every rejection uses the documented error envelope. |
| Error handling | `extract::Json`, `extract::Path` and `extract::Query` map every rejection onto `AppError`, and take their message text from the catalogue the `RequestContext` carries. `routes::not_found` answers an unmatched route with the same envelope and the code `not_found`. `routes::handle_panic` turns a panic into a `500` in the same envelope and deliberately does not log the payload. The timeout middleware answers with the standard envelope and the catalogue message. |
| Security boundary | The authentication boundary. `AuthUser` is the only way a handler learns who the caller is, and it derives the identity from the credential alone. `ClientIp` reads the connection information and never trusts `X-Forwarded-For`. The CORS allowlist is built here from a list that the configuration validation has already refused a wildcard from. |
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
| Public interface | `UserService` with `create_user`, `get_user`, `get_user_response`; the free functions `normalize_email`, `validate_email`, `validate_password`, `validate_display_name`; the DTOs `RegisterRequest`, `UserResponse`; the model `User`. The validation functions take the message catalogue, `validate_password` also takes the configured `PasswordPolicy`, and the email maximum is the named constant `MAX_EMAIL_LENGTH` (254). |
| May depend on | `error`, `config` (for the message catalogue and the password policy) and `utils::text` (for the character count). It must not depend on `auth`, `notes`, `projects` or `tasks`: `auth` depends on `users`, not the other way round. |
| Data ownership | The `users` table. |
| Invariants | The stored email is trimmed and lower-cased, and a `CHECK` constraint in the database enforces the same rule, so the two cannot drift. Uniqueness is enforced by the database, never by a check-then-insert in the application. The password is already hashed when it reaches the repository. The password lengths are the configured `auth.password.min_length` and `auth.password.max_length`, not constants in the code, so a deployment may tighten or relax the policy. `UserResponse` contains no password hash and no session material. |
| Error handling | `AppError::validation` with a named field for every policy violation; `AppError::Conflict` when the email is already registered; `AppError::NotFound` when the user does not exist; `AppError::validation_with` when a `CHECK` constraint is violated (SQLSTATE `23514`), so that the database message never reaches the log. |
| Security boundary | Owns the account record and therefore the credential material. `UserRepository::find_by_email` is the one query that reads a private table without an owner predicate, because it looks the account up *by* its identity rather than by its owner. |
| Testing approach | `tests/auth_api.rs` covers registration, normalization and the duplicate-registration conflict. Unit tests in `src/users/service.rs` cover normalization, the email rules, the password policy and the display-name rule. The secret validation that protects the signing material lives in `src/config/validation.rs`. |
| Known limitations | `UserRepository::is_active` is public but unused; the active check is made on the loaded record instead. There is no password-change, email-change or account-deletion surface, and `is_active` cannot be changed through the API. |

---

## 4. `notes` — personal notes

Source: `src/notes/handlers.rs`, `src/notes/service.rs`,
`src/notes/repository.rs`, `src/notes/models.rs`, `src/notes/dto.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Notes: create, list, read, partially update, delete, archive and search. |
| Public interface | `NoteService` with `create`, `list`, `get`, `update`, `delete`; the free functions `validate_title`, `validate_content`, which delegate to the shared `required_text` and `bounded_text` in `src/helpers/validation.rs`; `NoteRepository` with `insert`, `find_owned`, `list_owned`, `update_owned`, `delete_owned`; `NoteFilters`; the DTOs `CreateNoteRequest`, `UpdateNoteRequest`, `NoteQuery`, `NoteResponse`, `NotePage`; the model `Note`. The search term is bounded by the shared `validate_search`, also in `src/helpers/validation.rs`. |
| May depend on | `error`, `pagination`, `helpers`, `config` (for the message catalogue), `api::extract`, `api::middleware`. It must not depend on `projects` or `tasks`, so that a future "note attached to a task" does not require a schema change to the `notes` table. |
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
| Public interface | `ProjectService` with `create`, `list`, `get`, `update`, `delete`; the free functions `validate_name`, `validate_description`, which delegate to the shared `required_text` and `bounded_text` in `src/helpers/validation.rs`; `ProjectRepository` with `insert`, `find_owned`, `list_owned`, `update_owned`, `delete_owned`; the DTOs `CreateProjectRequest`, `UpdateProjectRequest`, `ProjectQuery`, `ProjectResponse`, `ProjectPage`; the model `Project`. The search term is bounded by the shared `validate_search`, also in `src/helpers/validation.rs`. |
| May depend on | `error`, `pagination`, `helpers`, `config` (for the message catalogue), `api::extract`, `api::middleware`. It must not depend on `tasks`: `tasks` depends on `projects`, not the other way round. |
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
| Public interface | `TaskService` with `create`, `list_for_project`, `get`, `update`, `delete`; the free functions `validate_title`, `validate_description`, which delegate to the shared `required_text` and `bounded_text` in `src/helpers/validation.rs`; `TaskRepository` with `insert`, `find_owned`, `list_for_project`, `update_owned`, `delete_owned`; `NewTask`, `TaskPatch`, `TaskFilters`; the DTOs `CreateTaskRequest`, `UpdateTaskRequest`, `TaskQuery`, `TaskResponse`, `TaskPage`; the models `Task`, `TaskStatus`, `TaskPriority`. |
| May depend on | `error`, `pagination`, `helpers`, `config` (for the message catalogue), `api::extract`, `api::middleware`. It must not depend on `notes`. |
| Data ownership | The `tasks` table. It has no `user_id` column: ownership is inherited from the parent project. |
| Invariants | A task is always reached through its project, and the project is always reached through its owner, so every statement joins to `projects` and carries the caller's `user_id`. The insert is a single `INSERT … SELECT … FROM projects p WHERE p.id = $2 AND p.user_id = $3`, so a task can never be attached to a project the caller does not own. `status` and `priority` are database enum types, so an out-of-contract value cannot be persisted. `completed_at` is maintained by the database: it is stamped when the status becomes `done` and cleared when it leaves `done`, and a `CHECK` constraint ties the two together. A partial update uses `COALESCE($n, column)`. The listing is ordered by `position ASC, created_at ASC, id ASC`. |
| Error handling | `AppError::validation` for an empty or oversized title and for an oversized description; `AppError::NotFound` when the task, or the project the task would live in, does not exist or is not owned by the caller. An unknown `status` or `priority` in the request body is rejected by the deserializer as a validation failure. |
| Security boundary | Ownership is inherited through the project join. A task reached through a project the caller does not own is `404`, never `403`. The list endpoint additionally probes the project when the result set is empty, so that "no such project" and "no tasks" are distinguishable without disclosing another account's project. |
| Testing approach | `tests/projects_tasks_api.rs` covers task creation and listing inside a project, the completion timestamp, filtering by status and priority, the refusal to reach a task through another account's project, the refusal to create a task in another account's project, the cascade on project deletion and the rejection of an unknown status. Unit tests in `src/tasks/service.rs` and `src/tasks/models.rs` cover the title and description rules and the wire names of the enum variants. |
| Known limitations | The ordering key is a `DOUBLE PRECISION` fraction and there is **no reordering endpoint**: a client sets `position` explicitly. A new task is placed at `MAX(position) + 1000` when no position is given. The open question Q6 covers the ordering semantics. |

---

## 7. Cross-cutting modules

### 7.1 `config` — typed configuration and the message catalogue

Source: `src/config/mod.rs`, `src/config/settings.rs`,
`src/config/loader.rs`, `src/config/validation.rs`, `src/config/messages.rs`,
`src/config/tests.rs`, and the committed files in `config/`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Resolves the configuration once, at start-up, from three layers — the base files in `config/`, the environment overlay and the environment variables — validates it, and produces a typed `Config`. It also owns the client-facing message catalogue. |
| Public interface | `Config::load() -> Result<Config, ConfigError>`, `Config::defaults() -> Result<Config, ConfigError>`, `ConfigLoader` with `new` and `with_variables`, `Config`, the settings types `AppSettings`, `ApiSettings`, `AuthSettings`, `DatabaseSettings`, `Argon2Params`, `PasswordPolicy`, `PaginationSettings`, `RateLimitSettings`, `Environment`, the catalogue `Messages` and `ConfigError`. |
| May depend on | `serde` and `toml`, and `axum::http::HeaderValue` for the CORS check. It must not depend on any other module of the crate. |
| Data ownership | No data. It owns the *shape* of the runtime configuration and the text of the client-facing messages. |
| Invariants | The settings are resolved once and never change while the process runs. A later layer wins over an earlier one, key by key, and a value that a layer does not set keeps the value from the layer below. `DATABASE_URL` and `APP_JWT_SECRET` are required and have no file layer. The signing secret must be at least 32 bytes, and in `Production` the documented placeholder is refused. Every settings structure uses `deny_unknown_fields`, so a typo in a file is a start-up error rather than a silently ignored setting. |
| Error handling | `ConfigError::Missing` for an absent or empty required variable, `ConfigError::Invalid` for a value that is not acceptable, `ConfigError::Io` for a file that cannot be read and `ConfigError::Parse` for a file that cannot be parsed. All four are raised **before** the socket is bound. |
| Security boundary | `AuthSettings` and `DatabaseSettings` implement `Debug` manually and print `[REDACTED]` for the signing secret and the connection URL, so a configuration dump cannot leak them. No file in `config/` may contain a secret: the directory is committed and embedded in the binary. |
| Testing approach | The module has its own unit tests in `src/config/tests.rs`. They cover the documented defaults, the precedence between the three layers, the two required secrets, the validation rules, the message catalogue and the redaction of the secret. The loader takes its environment from an injected source, so the tests never mutate the process environment. The integration harness in `tests/common/mod.rs` builds its configuration with `Config::defaults()`, so the committed files and the real parser are exercised by the whole suite as well. |
| Known limitations | There is **no command-line source**, **no remote source** and **no runtime reload**: a changed file or variable needs a restart. The rules that used to live where the value was used now live in `src/config/validation.rs`: the CORS allowlist is validated against a real origin and a wildcard is refused there, rather than by an assertion in the router; the rate limit, the pool size, the acquire and connect timeouts, the Argon2 iteration count and degree of parallelism, the issuer and the audience, the pagination limits and the log filter are checked in the same place. |

The rules the module enforces, in one place:

| Rule | Where |
| ---- | ----- |
| The two secrets are present, and the signing secret is at least 32 bytes. | `validate` and `validate_secret` in `src/config/validation.rs`. |
| In `Production`, the documented placeholder secret is refused. | `validate_secret` in `src/config/validation.rs`. |
| The body limit, the request timeout, the readiness timeout, the prune interval and the prune grace are positive. | `validate_settings` in `src/config/validation.rs`. |
| The prune grace is at least one hour, the password minimum at least twelve characters, the clock-skew allowance at most five minutes, the access-token lifetime at most one day and the refresh-token lifetime at most one year. | `validate_settings` in `src/config/validation.rs`. The bounds are constants beside the rules, not settings. |
| The pagination default is positive and the maximum is at least the default. | `validate_settings` in `src/config/validation.rs`. |
| The rate limit is positive and its window is not zero. | `validate_settings` in `src/config/validation.rs`. |
| The CORS allowlist contains no wildcard and every origin is a valid header value. | `validate_cors` in `src/config/validation.rs`. |
| The pool size is positive, and neither the acquire nor the connect timeout is zero. | `validate_settings` in `src/config/validation.rs`. |
| The access-token lifetime is positive and the refresh-token lifetime is strictly longer. | `validate_settings` in `src/config/validation.rs`. |
| The issuer and the audience are not empty. | `validate_settings` in `src/config/validation.rs`. |
| The Argon2 memory is at least 8 MiB, and the iteration count and the degree of parallelism are at least one. | `validate_settings` in `src/config/validation.rs`. |
| The password policy has a positive minimum and a maximum that is at least the minimum. | `validate_settings` in `src/config/validation.rs`. |
| `APP_ENV` accepts only `development`/`dev`/`local` or `production`/`prod`. | `Environment::parse` in `src/config/settings.rs`. |

### 7.2 `error` — the single error contract

Source: `src/error.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | One error type for the whole application, its stable code, its HTTP status and its public envelope. |
| Public interface | `AppError`, `AppResult<T>`, `ErrorBody`, `ErrorDetail`, `FieldViolation`, and the constructors `AppError::validation` and `AppError::validation_with`. |
| May depend on | Nothing inside the crate. |
| Data ownership | No data. |
| Invariants | Every variant has exactly one stable code and one status. The codes are defined once each, in the `code` module of this file, and the message text is client-facing wording that comes from the catalogue in `config/messages.toml`; a client switches on the code, never on the text. A database or internal failure never reaches the client with its detail: the body is the fixed message "The request could not be completed." A validation failure carries the offending field names. A rate-limited response carries a `retry-after` header. |
| Error handling | `impl IntoResponse for AppError` renders the envelope, logs a server error at `error` level and a rejection at `debug` level. A database failure is logged with its SQLSTATE and the error kind only, never with the driver message. |
| Security boundary | This module is the main defence against information disclosure. The full contract is in [`../api/error-contract.md`](../api/error-contract.md). |
| Testing approach | Unit tests in `src/error.rs` cover the validation status and field details, the redaction of internal details and of the SQLx driver message, and the distinct codes for `not_found` and `forbidden`. The HTTP-level shape is exercised by the integration suite. |
| Known limitations | One response is produced outside this module and therefore outside the envelope: Axum's own `405 Method Not Allowed` for a wrong method on a known path. The `408` from the request timeout is an `AppError::Timeout` and therefore uses the standard envelope. Everything the application itself rejects uses the envelope. |

### 7.3 `state` — the shared dependencies

Source: `src/state.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Holds the shared dependencies and passes them to the handlers explicitly. There is no global mutable state. |
| Public interface | `AppState` with the public fields `config`, `db`, `rate_limiter`, `auth`, `notes`, `projects` and `tasks`. The five services are reached through their field, which is the only idiom. The `tokens` and `passwords` fields, the `sessions()` accessor, the `auth_service()` accessor and the `prune_expired_sessions(grace)` method were removed: nothing outside the module used them, and the authentication service already holds the `TokenService`, the `PasswordHasher` and the session repository. |
| May depend on | `config`, `auth`, `users`, `notes`, `projects`, `tasks`, `ratelimit`. |
| Data ownership | The `PgPool`. It shares the pool with every repository, but each repository holds its own clone of the handle. |
| Invariants | `AppState` is cheap to clone: every field is `Arc`-backed or a `PgPool` handle. The authentication service and the three feature services are constructed **once**, at start-up, and are never rebuilt per request: a handler reaches the use cases through `state.notes`, `state.projects` and `state.tasks`. There is no per-request service constructor. |
| Error handling | None of its own. `AppState::new` takes an already validated `Config` and an already connected pool, and fails only when the password hasher cannot be built from the configured Argon2 parameters. |
| Security boundary | The signing secret and the Argon2 parameters are reachable only through the authentication service, which holds the `TokenService` and the `PasswordHasher`. Neither is exposed in a `Debug` rendering. |
| Testing approach | No dedicated test file. `tests/common/mod.rs` builds the state through `AppState::new` with a test configuration. |
| Known limitations | The services are built once, so the configuration cannot change at run time. There is no way to rotate the signing secret without restarting the process. |

### 7.4 `pagination` — the list convention

Source: `src/pagination.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | One pagination convention and one list envelope for every list endpoint. |
| Public interface | `PaginationQuery` with `effective_limit` and `effective_offset`, `Pagination::new`, `Page<T>::new`, and `PaginationSettings` — the two bounds that `effective_limit` clamps into. The bounds come from the configuration: `api.pagination.default_limit` and `api.pagination.max_limit`. There is no compile-time constant any more, so the published page size has exactly one home. |
| May depend on | Nothing inside the crate. |
| Data ownership | No data. |
| Invariants | The effective limit is never above the configured maximum page size and never below the configured default page size; a non-positive limit falls back to the default. A negative offset is treated as zero. `has_more` is `offset + limit < total`. The two bounds are part of the published client convention, so changing them is a breaking change for a client. |
| Error handling | None. The clamping happens silently, so an out-of-range limit is not a validation failure. |
| Security boundary | The limit caps how much data one request can pull out of the database. |
| Testing approach | Unit tests in `src/pagination.rs` cover the clamping into the configured range, the configured default, the fallback and the `has_more` computation. The HTTP-level envelope is exercised by `tests/notes_api.rs`. |
| Known limitations | The convention is `limit`/`offset`, so a deep page costs the database a large offset. There is no cursor. |

### 7.5 `ratelimit` — the in-process limiter

Source: `src/ratelimit.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | A fixed-window request counter, keyed by client address, for the authentication endpoints. |
| Public interface | `RateLimiter::new`, `RateLimiter::check` and the `Decision` enum (`Allowed`, `Rejected { retry_after_seconds }`). `reset()` was removed: only a test used it. |
| May depend on | Nothing inside the crate. `api::middleware::enforce_rate_limit` is the only caller. |
| Data ownership | A `Mutex<HashMap<IpAddr, WindowEntry>>` in the process memory. Nothing is persisted. |
| Invariants | Entries whose window has elapsed are swept at most once per window, so a check no longer costs one visit per tracked client. Because sweeps are at most one window apart, an entry lives for at most two windows: the map holds the clients of the last two windows and stays bounded. A client's window resets on its own timeline, so its decision never depends on when the sweep last ran. A rejected call reports the remaining seconds of the window. |
| Error handling | None. `enforce_rate_limit` converts a rejection into `AppError::RateLimited`, which becomes a `429` with a `retry-after` header. |
| Security boundary | A defence-in-depth control on the authentication boundary. It is **not** a substitute for edge rate limiting. |
| Testing approach | Unit tests in `src/ratelimit.rs` cover the limit, the independence of clients and the window reset. The HTTP-level behaviour is exercised by `the_authentication_endpoints_are_rate_limited` in [`../../tests/auth_api.rs`](../../tests/auth_api.rs). |
| Known limitations | The counters are per process, so a multi-instance deployment has *n* times the intended budget. The limiter fails open when its mutex is poisoned. The fixed window allows a burst across the boundary. The server installs the connection information, so the limiter is keyed by the peer address; behind a proxy that does not terminate the connection, every request carries the proxy's address and the budget is shared again. See [`../engineering/known-issues.md`](../engineering/known-issues.md) KI-001, KI-003 and KI-008. |

### 7.6 `telemetry` — logging

Source: `src/telemetry.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | Installs the global tracing subscriber and selects the formatter. |
| Public interface | `telemetry::init(filter: &str, environment: Environment) -> Result<(), Box<dyn Error>>`. |
| May depend on | Nothing inside the crate. |
| Data ownership | The global subscriber. |
| Invariants | The filter comes from the configuration: `app.log_filter`, which the environment variable `APP_LOG_FILTER` overrides and the environment overlay can replace. The formatter follows the **environment**, not the build profile: `APP_ENV=production` uses the JSON formatter with a flattened event, and any other environment uses the human-readable one. A release build that runs a development deployment therefore gets readable logs, and a debug build that runs a production deployment gets JSON. |
| Error handling | Returns an error when the filter is not a valid directive, or when a subscriber is already installed. It does **not** fall back to `info`: the filter was validated when the configuration was loaded, so an invalid one is a start-up failure. |
| Security boundary | The log sink is a secret-bearing boundary: the formatter never adds request bodies, and the error renderer redacts internal detail before it can reach a log line. |
| Testing approach | No dedicated test. The format is exercised by running the service. |
| Known limitations | There is no redaction layer over arbitrary log fields, no audit trail and no metric export. The formatter has two choices and no per-deployment override beyond `APP_ENV`. |

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

### 7.8 `helpers` — the shared text validators

Source: `src/helpers/validation.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | The validation rules that more than one feature module needs, defined once: the trimming behaviour, the length bounds and the wording of a rejection. |
| Public interface | `required_text` (trims, rejects an empty value, bounds the length), `bounded_text` (keeps the value verbatim, bounds the length), `validate_search` (trims a search term and bounds it at 200 characters), `field_error`, and the constant `MAX_SEARCH_LENGTH` (200). |
| May depend on | `error`, `config` (for the message catalogue) and `utils::text` (for the character count). It must not depend on a feature module. |
| Data ownership | No data. |
| Invariants | Every field-level rejection is reported the same way: one field violation in the details, and the shared `validation_failed` message from the catalogue. The trimming behaviour is deliberately **not** uniform: a title, a name and a display name are trimmed, while a body and a description are kept verbatim, because the database `CHECK` constraints measure the stored value. A search term is trimmed and an empty term means "no filter" rather than an error. |
| Error handling | None of its own. Every failure is an `AppError::Validation` with a `422`. |
| Security boundary | It is the layer that gives every request input a bound, so that one request cannot make the database tokenise an arbitrarily long string. |
| Testing approach | Unit tests in `src/helpers/validation.rs` cover the trimming, the empty value, the length boundary, the verbatim behaviour and the search bound (`a_search_term_is_trimmed_and_bounded`). The `422` the search bound produces is covered at HTTP level by `a_search_term_over_the_bound_is_refused` in [`../../tests/notes_api.rs`](../../tests/notes_api.rs). |
| Known limitations | The bounds are constants of the module, not settings. The search bound in particular is not configurable, because a deployment has no reason to accept a longer term. |

### 7.9 `utils` — the generic text operations

Source: `src/utils/mod.rs`, `src/utils/text.rs`.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | The generic, domain-independent text operations that more than one module needs: the character count and the blank test. A utility must not know the business rules of notes, projects or tasks, and must not know the error type of a particular feature; a helper that needs either belongs in [`helpers`](#78-helpers--the-shared-text-validators) instead. |
| Public interface | `char_len` (the number of characters in a string) and `is_blank` (whether a string is empty or contains only whitespace), in `utils::text` and re-exported at the root of the module as `utils::{char_len, is_blank}`. |
| May depend on | Nothing inside the crate. |
| Data ownership | No data. |
| Invariants | `char_len` counts **characters**, not bytes: `str::len` counts bytes, which is the wrong measure for a length limit a user reads in characters — a name of 60 emoji is 240 bytes and 60 characters. Every length rule in the API is expressed in characters. `is_blank` trims before it tests, so a value of only whitespace is blank. |
| Error handling | None. Neither function can fail; the caller decides what a blank or an over-long value means. |
| Security boundary | None of its own. It is the counting primitive underneath the shared validators, which are the layer that gives every request input a bound. |
| Testing approach | Unit tests in `src/utils/text.rs` cover the character count against a multi-byte string and a pair of emoji, and the blank test against an empty string, a whitespace-only string and a value with content. |
| Known limitations | The module holds two functions and no string-normalisation helper: trimming and the case rules stay where the business rules live, in `src/helpers/validation.rs` and `src/users/service.rs`. |

---

## 8. Related documents

- [`system-overview.md`](system-overview.md) — the whole system on one page.
- [`request-lifecycle.md`](request-lifecycle.md) — how the modules are entered.
- [`data-architecture.md`](data-architecture.md) — the tables each module owns.
- [`security-architecture.md`](security-architecture.md) — the boundaries.
- [`../decisions/ADR-0001-modular-monolith.md`](../decisions/ADR-0001-modular-monolith.md)
  — the modular monolith.
- [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md)
  — ownership in SQL.
- [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md)
  — the configuration layers and the secrets.

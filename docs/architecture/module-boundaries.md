# ATOM — module boundaries

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `architect` agent

What each module owns, what it may depend on, and which invariant it is
responsible for. §1–§7 describe the modules that **exist**, and so does the
unnumbered `api_keys` section below §2; the last section describes the Phase 2
modules — the two that are built and the one still planned — and marks each.
The layering rule that this document enforces is fixed by
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
| Public interface | `api::router(state) -> Router`, `api::routes::api_routes() -> Router<AppState>`, `api::routes::swagger_ui() -> Router<AppState>` (only with the `docs` Cargo feature), `api::routes::ApiDoc`, `api::routes::not_found`, `api::routes::handle_panic`, `api::timeout_middleware`, `api::RequestContext`, `api::middleware::AuthUser`, `api::middleware::ClientIp`, `api::middleware::enforce_rate_limit`, `api::extract::Json`, `api::extract::Path`, `api::extract::Query`. |
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

## `api_keys` — the long-lived credential for a machine client

Source: `src/api_keys/handlers.rs`, `src/api_keys/service.rs`,
`src/api_keys/repository.rs`, `src/api_keys/models.rs`, `src/api_keys/dto.rs`,
`src/api_keys/secret.rs`. Built by the `K1` chain (`Implemented`).

*The heading is unnumbered on purpose:* §7 and §8 are cited by number — from
[`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md)
§7.1 and from other documents — so inserting a number here would quietly
repoint those references.

| Aspect | Description |
| ------ | ----------- |
| Responsibility | The long-lived bearer credential an unattended client holds: create, list, revoke, and authenticate a presented secret. It owns the shape and the hashing of the secret itself, and it is the **only** place a key secret is ever produced. |
| Internal layering | The five layers in five files — `handlers.rs`, `service.rs`, `repository.rs`, `models.rs`, `dto.rs` — plus `secret.rs`, which is the *inside* of the service layer rather than a layer of its own: `generate`, `hash`, `is_api_key`, `is_well_formed` and the `GeneratedKey` type. `secret.rs` is the one file that knows how a secret is built and how it is hashed. |
| Public interface | `ApiKeyService` with `create`, `list`, `revoke`, `authenticate`; `ApiKeyRepository` with `insert`, `list_owned` (the page and the collection total), `revoke_owned`, `find_identity_by_hash`, `touch_last_used`; the types `KeyPrincipal`, `KeyIdentity` and `ApiKey`; the DTOs `CreateApiKeyRequest`, `ApiKeyCreatedResponse`, `ApiKeyResponse`, `ApiKeyPage`; the constant `MAX_KEY_NAME_LENGTH` (100); the free functions `identity_refusal` and `validate_name`. A handler reaches it as `state.api_keys`, and `api::middleware::AuthUser` calls `authenticate` when the bearer token begins `atom_`. |
| May depend on | `error`, `pagination`, `helpers` (the label is trimmed and bounded by the shared `required_text`), `config` (the message catalogue and the pagination bounds), `api::extract` and `api::middleware` (from `handlers.rs` only), and `sha2`/`rand` for the secret. It reads the `users` table inside the authentication join and writes no table but its own. |
| Must not depend on | `auth`, the `users` service, or any feature module. It holds **no business rule of any other module**: it learns that an account is active because the join returns `users.is_active`, not by re-implementing the account rules or the token lifecycle. It never mints a JWT and never touches `refresh_sessions`. |
| Data ownership | The `api_keys` table (`0007_api_keys.sql`). `user_id` is the owner and the predicate of every statement. The secret is never stored: only its SHA-256 hex digest in `key_hash`. `prefix` (`atom_` + eight hex characters) is a display label, not a lookup key, and is deliberately not unique. |
| Invariants | Every statement carries the caller's `user_id`, so a foreign or unknown key id is `404`, never `403`. `key_hash` is unique, so one secret is one row. `revoked_at` is written once and never cleared, and a second revoke is still `204` while keeping the first instant. `last_used_at` is written at most once per hour per key, decided by the guarded `UPDATE` rather than by the read. The listing is ordered `created_at DESC, id DESC` and cannot carry the secret: `ApiKey` has no `key_hash` field and `ApiKeyResponse` has no `key` field, so there is no shape to select one into. |
| Error handling | `422` naming `name` for an empty or over-long label; `401` `unauthorized` with `messages.auth.access_token_invalid` for an unknown, malformed or revoked key; `403` `forbidden` with `messages.auth.account_disabled` for the key of a disabled account — the same answer that account's JWT already receives (amendment `A-C-K1-1`), decided in one place, `identity_refusal`; `404` for a key id that is absent or foreign. A failure to record `last_used_at` is logged and does not fail the request. |
| Security boundary | The second form of the authentication boundary (TB-5 in [`../security/threat-model.md`](../security/threat-model.md)). The secret leaves the module in exactly one place, the create response, and nothing derived from it is logged — not the secret, not the digest, not the prefix. A key has the **full authority of its owner**: there is no scope model and no reduced principal, which is also the module's largest limitation. |
| Testing approach | `tests/api_keys_api.rs` (11 cases) covers create, list and revoke, the one-time secret, the absence of a secret in the listing, the `401`/`403` split, the idempotent revoke, the foreign `404`, the pagination and the hourly usage stamp. `tests/openapi_contract.rs` asserts the three success statuses, the `key_id` path parameter, and that `ApiKeyCreatedResponse` publishes `key` while `ApiKeyResponse` publishes neither `key`, `key_hash` nor `user_id`. `tests/contract_sweep.rs` proves a second account sees none of the first account's keys and that a foreign revoke leaves the credential working. Unit tests in the module (13: `secret.rs` 5, `service.rs` 4, `models.rs` 3, `dto.rs` 1) pin the secret's shape, the refusal table and the stamp boundary. |
| Known limitations | No scope model, no expiry and no per-key rate limit: a key is a full-authority credential until it is revoked. A key can create and revoke keys, so revoking a leaked key does not by itself end access if the holder already minted another — stated as the honest limitation in [`../security/README.md`](../security/README.md) §3. `last_used_at` is the only usage signal and it is hourly, so a forensic timeline is coarse. There is no hard delete: a revoked key stays in the listing, with its `revoked_at`. |

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

## Phase 2 modules (planned, except reimbursements and job-applications)

> **Mostly planned.** `reimbursements` is **`Implemented`**: `src/reimbursements/`
> holds six modules, and `migrations/0004_expense_claims.sql` creates
> `expense_claim_status`, `expense_claims` and `expense_items` with their
> constraints and indexes. All twelve published operations are served, and
> `tests/claims_schema.rs`, `tests/claims_api.rs` and `tests/claims_flow.rs`
> prove the constraints and the behaviour. `job-applications` is also
> **`Implemented`**: `src/job_applications/` holds six modules, and
> `migrations/0005_job_applications.sql` creates `job_application_status`,
> `application_event_kind`, `companies`, `job_applications` and
> `application_events` with their constraints and indexes. All fourteen
> published operations are served, and `tests/jobs_schema.rs`,
> `tests/job_applications_api.rs` and `tests/job_applications_flow.rs` prove the
> constraints and the behaviour. `migrations/0006_embeddings.sql` creates
> `embeddings`; `src/ml/` serves the three published operations, and `tests/ml_api.rs`
> proves them. `ml` is `Implemented`, not `Verified`: `ML-R02` **is met**, because
> `embedder::select` returns a `BurnEmbedder` when both artifacts load; the
> feature waits on a review of the encoder span and the re-gate, not on code.

**What "every module" means from here.** §1–§7 describe every module that
**exists**. From this section on, the phrase means "every Phase 2 module",
including `ml`; a module that appears in neither place is not covered by this
document.

The heading is unnumbered on purpose: `§7` and `§8` are cited by number from
[`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md)
§7.1 and from other documents, so inserting a number here would quietly repoint
those references.

**Where the detail lives.** The routes, the request and response shapes, the SQL
sketches, the error tables and the limitations are in
[`../features/reimbursements/architecture.md`](../features/reimbursements/architecture.md),
[`../features/job-applications/architecture.md`](../features/job-applications/architecture.md)
and [`../features/ml/architecture.md`](../features/ml/architecture.md); the
tables are in [`../database/schema.md`](../database/schema.md) §11. This section
answers the one question those documents leave to the module map: **where the
three sit in the crate, and what each may depend on.**

```text
users ──┬──► reimbursements ──(optional, SET NULL)──► projects
        │
        ├──► job-applications ──(optional, SET NULL)──► tasks
        │         └──► companies, application events
        │
        └──► ml ──reads──► notes
                 └─reads──► job-applications
```

| Module | Status | Source | Owns in the database | May depend on |
| ------ | ------ | ------ | -------------------- | ------------- |
| `reimbursements` | `Implemented` | `src/reimbursements/` (six modules) | `expense_claims`, `expense_items` | `error` |
| `job-applications` | `Implemented` | `src/job_applications/` (six modules) | `companies`, `job_applications`, `application_events` | `error`, `pagination`, `helpers`, `money`, `config`, `api`, `tasks` (read-only, through `projects`) |
| `ml` | `Implemented` | `src/ml/` (ten modules) | `embeddings` | `error`, `pagination`, `helpers`, `config`, `notes` (repository), `job_applications` (repository), `burn` |

**The arrow into `ml` is one-way.** No module may depend on `ml`: no module
calls `MlService`, holds an `Arc<dyn Embedder>`, imports the embedder files or
constructs a model. The single exception is the repository-level delete described
below, and it is one SQL function wide.

The requirements rest on the provisional assumptions **A1–A6** in
[`../project-truth.md`](../project-truth.md) §18.4, which the owner has not
confirmed (owner decision O1). This section depends on **A2** (a receipt is a
reference), **A4** (inference is local) and **A6** (money is `NUMERIC(19,4)`); a
revision of one changes this plan, not the requirement.

### `reimbursements`

Expense claims: a claim with one currency, a lifecycle from `draft` to `paid`,
and the line items whose amounts sum to its total. It owns the only stored
aggregate in the API — `expense_claims.total_amount` — and it is the first
module that writes money.

| Aspect | Description |
| ------ | ----------- |
| Public interface | `ClaimService` with `create`, `list`, `get`, `update`, `delete`, `add_item`, `update_item`, `delete_item`, `submit`, `decide` and `pay`; `ClaimRepository` and `ItemRepository` with the statements the feature document lists; the lifecycle guard `ensure_transition`; the DTOs `ClaimResponse`, `ClaimDetailResponse`, `ClaimPage`, `ItemResponse`, the create and update requests, the query type and the length limits. A handler reaches it as `state.reimbursements` — the field `AppState` will carry, following the Phase 1 idiom `state.notes`, `state.projects`, `state.tasks`. |
| Internal layering | The five layers, in the five files the feature document names: `handlers.rs`, `service.rs`, `repository.rs`, `models.rs` (`ExpenseClaim`, `ExpenseItem`), `dto.rs`. The handler extracts and serialises; the service validates, runs the lifecycle guard and coordinates the transaction; the repository holds every statement. No handler touches SQL. |
| May depend on | `error`, `pagination`, `helpers`, `money`, `config` (the message catalogue), `api::extract`, `api::middleware`. It reads the `projects` table for one thing only: to prove that the optional `project_id` belongs to the caller before the insert or the update. |
| Must not depend on | `notes`, `tasks`, `job_applications`, `ml`. It writes no table other than its own two, and it must not depend on `projects`' service or handlers — the ownership read is a statement, not a call into another feature. |
| Data ownership | `expense_claims` and `expense_items`. Both carry `user_id`; `expense_items` carries `FOREIGN KEY (claim_id, user_id) REFERENCES expense_claims (id, user_id)`, so an item whose owner differs from its claim's owner cannot be stored. `project_id` is nullable with `ON DELETE SET NULL`. The physical definition is in [`../database/schema.md`](../database/schema.md) §11.1–§11.2. |
| Invariants | Every statement carries the caller's `user_id`, and an item statement carries `claim_id` **and** `user_id`. `total_amount` is recomputed from `SUM(amount)` in the same transaction as every item write. A claim has one currency for its whole life. Item writes are refused unless the claim is `draft`, and every status change goes through `ensure_transition`. Money is a `String` with four decimals and never an `f32`/`f64`. A receipt is stored text that is never fetched. |
| Error handling | `422` with a named field for a broken validation rule, `404` for a claim, item or project that is absent or foreign, `409` for a transition the lifecycle does not allow and for an item write on a non-draft claim — the table is in the feature document §5. |
| Security boundary | The caller's claims only. A claim or item of another account is `404`, never `403`, and an item id is meaningful only inside its claim, because the path resolves the pair. |
| Verification the implementing change must provide | Unit tests for `ensure_transition` (every allowed and every forbidden edge) and for the money validator (`validate_money`, not a type — see the note under *The planned cross-cutting addition: `money`*); integration tests for cross-account `404` on every verb, for the total after each item write, and for the `ON DELETE SET NULL` behaviour when a project is deleted. |
| Limitations already known | The total is a stored column, not a view; the submit precondition is a service rule, because a `CHECK` cannot count another table's rows; deleting a claim is permanent. The feature document §10 lists the rest. |

### `job-applications`

Companies, the applications the owner is pursuing at them, and the dated events
in one application's timeline. A company name is unique per owner; an
application moves through seven stages; a company that still has an application
cannot be deleted.

| Aspect | Description |
| ------ | ----------- |
| Public interface | `JobService` with `create_company`, `list_companies`, `get_company`, `update_company`, `delete_company`, `create_application`, `list_applications`, `get_application`, `update_application`, `delete_application`, `list_events`, `create_event`, `update_event` and `delete_event`; the repository statements the feature document lists, plus `update_company_owned`, `update_application_owned` and `delete_application_owned`, which it does not name and which the implementing change adds in the Phase 1 `*_owned` style; the stage guard `ensure_transition`; the DTOs `CompanyResponse`, `CompanyPage`, `ApplicationResponse`, `ApplicationPage`, `EventResponse`, `EventPage`, the create and update requests and the query types. A handler reaches it as `state.job_applications`. |
| Internal layering | The five layers, in the five files the feature document names: `handlers.rs`, `service.rs`, `repository.rs`, `models.rs` (`Company`, `JobApplication`, `ApplicationEvent`), `dto.rs`. |
| May depend on | `error`, `pagination`, `helpers`, `money`, `config` (the message catalogue), `api::extract`, `api::middleware`. It reads `tasks` to prove that an optional next-action task belongs to the caller, and that read necessarily joins `projects`, because `tasks` carries no `user_id` (see [`data-architecture.md`](data-architecture.md) §2.1). |
| Must not depend on | `notes`, `reimbursements`, `ml`; and it does not write the `tasks` or `projects` tables at all. |
| Data ownership | `companies`, `job_applications`, `application_events`. Every one carries `user_id`, and the parent links are composite: `(company_id, user_id)` to `companies`, `(application_id, user_id)` to `job_applications`. The company name is unique per owner through a functional unique index on `(user_id, lower(name))`. The physical definition is in [`../database/schema.md`](../database/schema.md) §11.3–§11.5. |
| Invariants | Every statement carries `user_id`. A stage change is decided by `ensure_transition` under `FOR UPDATE`, in one transaction with the rest of the `PATCH`. A company with applications is refused by a single `DELETE … AND NOT EXISTS (…)`, so the check is not a race. An event carries no `updated_at`; it is corrected in place, by decision `A-D1-1`. The event order is `occurred_at DESC, id DESC`. |
| Error handling | `422` with a named field, `409` for a duplicate company name, for an illegal stage move and for deleting a company that still has applications, `404` for every absent or foreign row — the table is in the feature document §5. |
| Security boundary | The caller's companies, applications and events only; a foreign id in a body is `404`, not `403`; an event id is meaningful only inside its application. The stage is personal data and is owner-scoped like every other field. |
| Verification the implementing change must provide | Unit tests for the stage machine (every edge, both refusals) and for the company-name rule; integration tests for the duplicate-name `409`, the company-delete `409`, cross-account `404` on every verb, the event ordering and the next-action clearing. |
| Limitations already known | `FOR UPDATE` serialises stage changes on one application; the company search is `ILIKE '%term%'`, which cannot use a B-tree index for a leading wildcard; the timeline does not record a stage change. The feature document §10 lists the rest. |

### `ml` (planned)

Local inference over the owner's own text: an embedding for a note or an
application, three search modes over notes (`lexical`, `semantic`, `hybrid`), a
ranking of the owner's notes against one application, and an owner-scoped,
idempotent reindex. It is the only module permitted to load a model, and the
only one permitted to run CPU-bound work inside a request.

| Aspect | Description |
| ------ | ----------- |
| Public interface | `MlService` with the three use cases the feature document names — a search in the requested mode, a match and a reindex; the `Embedder` trait with `model_id`, `dimension`, `status` and `embed`, and exactly three implementations, `Burn`, `Deterministic` and `Disabled`; `EmbedderStatus`; `Embedding`; `EmbeddingRepository` with `find_for_sources` and the upsert and delete it needs; the DTOs `SearchRequest`, `SearchResponse`, `SearchHit`, `MatchResponse`, `MatchHit` and `ReindexResponse`. `AppState` carries `state.ml`, and the `MlService` it holds is the only holder of the selected `Arc<dyn Embedder>` — selected once, in `AppState::new`. |
| Internal layering | The feature document names `mod.rs`, `embedder.rs`, `burn_embedder.rs`, `deterministic.rs`, `disabled.rs`, `service.rs`, `repository.rs` and `dto.rs` — no `handlers.rs` and no `models.rs`. The five-layer rule still applies: the row shape (the vector and its recorded `dim`) belongs in a `models.rs` rather than in the repository, and the three routes need a `handlers.rs`. **One of those routes is `POST /notes/search`, whose path sits under `/notes`**; its handler lives in `ml::handlers`, because a `notes` handler that called `MlService` would create the very dependency this document forbids. The embedder files are the *inside* of the service layer, not a layer of their own. The trait is justified by its three real implementations (`ML-R03`); it is not an abstraction waiting for a second one. |
| May depend on | `error`, `pagination`, `helpers` (the search term is trimmed and bounded by the shared `validate_search`, so the bound has one home), `config`, `notes::repository` (the lexical candidate query and the notes search, so the search endpoint and the notes listing cannot disagree), `job_applications::repository` (loading the application a match ranks against), and `burn` v0.22 with the `flex` backend. |
| Must not depend on | An HTTP client, a process spawn or a second runtime — there is none in the process, and adding one is a new decision, not a feature flag. It does not depend on `notes::service` or `job_applications::service`, only on their repositories. |
| Data ownership | `embeddings`, and nothing else. It reads note and application rows through the repositories it is allowed to call, and it never writes them. |
| Invariants | Every statement carries `user_id`. `source_id` is polymorphic and therefore has **no foreign key**; the source delete is responsible for its embeddings. `dim = cardinality(vector)` is a database check, and `(user_id, source_type, source_id, model_id)` is unique, so a re-embed is an upsert. A degraded search answers `200` with the applied mode. The embedder is selected once, at start-up; a missing model file selects `Disabled` and never fails the start-up. |
| Error handling | `422` for an unknown mode or a malformed query, `404` for a foreign note or application, `409` for a reindex while the embedder is disabled, `408` for a match whose inference exceeds `ml.timeout_seconds`. A **search** never fails on a model problem: it degrades. |
| Security boundary | An embedding is derived personal data and is protected like its source. No outbound call, no credential for a third party and no model download, which removes a data-processing question from the feature entirely. |
| Verification the implementing change must provide | Unit tests that `Deterministic` is stable and reports its dimension and that `Disabled` reports rather than fails; integration tests for degraded lexical results in `200`, for owner isolation of search, match and reindex, and for idempotence of a reindex. The recorded manual quality run is not a CI gate. |
| Limitations already known | There is no approximate-nearest-neighbour index, so hybrid recall is bounded by the lexical candidate set; the polymorphic `source_id` needs the delete discipline; inference cannot be cancelled once it runs. The feature document §11 lists the rest. |

#### The one edge into `ml`, and why it is a repository and not a service

[`../project-truth.md`](../project-truth.md) §15.1 and
[`../decisions/ADR-0008-local-pure-rust-ml.md`](../decisions/ADR-0008-local-pure-rust-ml.md)
make the **source delete** responsible for removing its embeddings, because
`source_id` cannot carry a foreign key. That obligation is what makes "no module
may depend on `ml`" need one precise exception, and the exception is bounded to
the SQL layer:

- `EmbeddingRepository::delete_for_source(executor, user_id, source_type,
  source_id)` — the name is proposed; the feature document names the table and
  the obligation, not the function — runs one `DELETE` against `embeddings`,
  scoped by `user_id` as well as by the source.
- It takes the caller's **transaction/connection** as an argument, so it joins
  the delete instead of following it, and it returns no rows or an error.
- It never constructs or consults an `Embedder`, and it never calls `MlService`.

`NoteService::delete` and the application delete call it inside their own
transaction. The module graph stays acyclic at the **service** layer, and the arc
that exists between `notes`/`job_applications` and `ml` is exactly one SQL
function wide, in the direction "the owner of the row deletes its derived rows".
A change that replaces this with an injected hook or a shared trait would be a
module-graph change, and it needs its own ADR before it is built.

This is one of the four Phase 2 operations that need an explicit multi-statement
transaction; Phase 1 has none, which [`data-architecture.md`](data-architecture.md)
§7 records as a deliberate absence. The other three, and the transaction shape
they share, are in the *Phase 2 data* section of that document.

### The dependency direction to `tasks`

Phase 2 adds two **optional** links, and both point *at* tables that exist. The
direction is the invariant: neither link turns `tasks` or `projects` into a
consumer of a Phase 2 module.

| Link | Owned by | Direction | Who proves the owner | When the target is deleted |
| ---- | -------- | --------- | -------------------- | ------------------------- |
| `expense_claims.project_id` | `reimbursements` | claim → project | `reimbursements`, one owner-scoped read of `projects` before the write | `ON DELETE SET NULL`: the claim survives with `project_id` null (`REIMB-R09`) |
| `job_applications.next_action_task_id` | `job-applications` | application → task | `job-applications`, through the `tasks → projects` join, because `tasks` has no `user_id` | `ON DELETE SET NULL`: the application survives with the field null (`JOB-R07`) |

Neither is required: a claim without a project is a complete claim, and an
application without a next action is a complete application. Neither is
enforced by the database on the caller's side — the `SET NULL` handles the
target's deletion, and the ownership predicate is a statement the service runs —
so both need a `404` test, not a constraint test. `projects` and `tasks` gain no
column, no route, no service method and no knowledge of Phase 2, which is what
keeps [`../project-truth.md`](../project-truth.md) §15's "`tasks` depends on
`projects`, not the other way round" true after Phase 2.

### The planned cross-cutting addition: `money`

Both Phase 2 features that handle money need one type: a validated decimal that
deserialises **only** from a four-decimal JSON string and never from a number
([`../decisions/ADR-0007-money-representation.md`](../decisions/ADR-0007-money-representation.md)).
[`../features/reimbursements/architecture.md`](../features/reimbursements/architecture.md)
§2.3 places it in that feature's own `dto.rs`. That placement is not viable once
a second feature needs it:

| Option | Why it is not chosen |
| ------ | -------------------- |
| `reimbursements::dto::Money`, imported by `job-applications` | One feature module would depend on another feature module's `dto`. No Phase 1 rule allows that, and it would make the two features impossible to deliver or retire separately. |
| One `Money` newtype per feature | Two parsers for one published contract, so ADR-0007's four decimals would have two homes and could drift. The contract test would then test the wrong half. |
| `helpers::money` | `helpers` is the **text** validators; a typed decimal with a wire format is not a text rule, and its errors are typed rejections rather than field-length violations. |
| A new single-file module, `src/money.rs` | **Chosen.** It has the shape of [`pagination`](#74-pagination--the-list-convention): one convention, one type, no table, no handler, no configuration. It depends on nothing inside the crate except `error`, and it is the only place that knows the four-decimal rule. |

#### `R2` narrows this plan (2026-10-24, design only — not built)

The table above is the plan for a **typed** money value. Reading the claim CRUD
design against it showed that the first chain to need money does not need a type
at all, so the plan is narrowed here rather than built:

- **Claim CRUD (`R2`) handles money in one direction only.** Every money field
  it publishes is a `String` selected with `::text`, and no request body it
  accepts carries money, because `total_amount` is server-owned. There is
  therefore no money input to validate, and `R2` adds no type and no validator.
- **The four-decimal rule, when an input exists, is a text validator in
  `helpers::validation`, not a type.** The first caller is `R3`'s item `amount`.
  The rule is a character-level check that returns the string unchanged and
  reports a `FieldViolation` naming the field — the same shape as
  `required_text` and `validate_search`, in the module that already owns
  field-level text rules. This is **not** the `helpers::money` row the table
  above rejects: that row rejects putting a *typed decimal* in `helpers`, and
  what is proposed here has no type, no parsing and no arithmetic.
- **A `Money` newtype adds nothing here.** The JSON extractor reports a number
  where a string is expected as one violation for the field `body`, so a
  `Deserialize` impl cannot name the field better than `String` does; and money
  has no Rust arithmetic to encapsulate, because the sum is `SUM` in SQL
  ([`../decisions/ADR-0007-money-representation.md`](../decisions/ADR-0007-money-representation.md)).
- **`src/money.rs` is not reversed, it is deferred.** If a later requirement
  needs a *typed* money value — a comparison or an arithmetic in Rust — the
  choice above stands and this subsection is superseded by that change.

The consequence for the two tables in this section: the `money` entries in the
`May depend on` rows of `reimbursements` and `job-applications` are superseded
for the validation half. The rule lives in `helpers`, and a `money` module
exists only if the deferred case above materialises. The full argument, the
rejected alternatives and the input rule are in
[`reimbursements-crud-design.md`](reimbursements-crud-design.md) §4, which is
the design for `R2`; the feature document's §2.3 sentence is corrected in the
same change.

This is a cross-feature structure question, and [`../README.md`](../README.md) §1
gives it to this directory (rank 2), not to `features/` (rank 3, "not
authoritative for cross-feature architecture"). The reimbursements feature
document's §2.3 sentence must therefore be amended when the money type is built,
in the same change as the module.

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
| Public interface | `AppState` with the public fields `config`, `db`, `rate_limiter`, `auth`, `api_keys`, `notes`, `projects`, `tasks`, `reimbursements`, `job_applications` and `ml`, in that order. The eight services are reached through their field, which is the only idiom: the authentication service, the API-key service and the six feature services. The `tokens` and `passwords` fields, the `sessions()` accessor, the `auth_service()` accessor and the `prune_expired_sessions(grace)` method were removed: nothing outside the module used them, and the authentication service already holds the `TokenService`, the `PasswordHasher` and the session repository. |
| May depend on | `config`, `auth`, `api_keys`, `users`, `notes`, `projects`, `tasks`, `reimbursements`, `job_applications`, `ml`, `ratelimit`. |
| Data ownership | The `PgPool`. It shares the pool with every repository, but each repository holds its own clone of the handle. |
| Invariants | `AppState` is cheap to clone: every field is `Arc`-backed or a `PgPool` handle. The authentication service, the API-key service and the six feature services are constructed **once**, at start-up, and are never rebuilt per request: a handler reaches the use cases through `state.auth`, `state.api_keys`, `state.notes`, `state.projects`, `state.tasks`, `state.reimbursements`, `state.job_applications` and `state.ml`. There is no per-request service constructor. |
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

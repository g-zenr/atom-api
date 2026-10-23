# ATOM — project truth

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** product owner, maintained by the `docs` agent
> **Authority:** this file defines ATOM's *agreed product requirements*. Source
> code does not change it. See [`README.md`](README.md) for the full authority
> order.

This document is the single canonical statement of what ATOM is, what it must
do, and what it must not do. It is written so that a contributor who has never
read a private conversation can work on the project correctly.

Two rules govern every sentence here:

1. **Only agreed requirements appear as requirements.** An idea that was never
   accepted is recorded in [`roadmap.md`](roadmap.md) with the status
   `Proposed`.
2. **The repository's actual implementation is described separately.** The
   *Current implementation status* section at the end is the only place where
   the code and the requirements are compared. Where they disagree, the
   disagreement is recorded rather than resolved by editing this file.

---

## 1. Product name and purpose

**ATOM** is an **API-first personal workspace platform**. It combines, behind
one HTTP API:

- **Personal notes** — a private, searchable knowledge store.
- **GitHub-style project and task tracking** — projects that own tasks with a
  status, a priority and an ordering.
- **Future automation and knowledge-management capabilities** — n8n workflows,
  job and client tracking, prompt/session knowledge management, and local
  retrieval-augmented generation.

"API-first" is a commitment, not a slogan: every capability is reachable
through a documented HTTP endpoint, and the frontend is a separate consumer of
that API rather than the API's reason for existing.

## 2. Long-term vision

ATOM is the personal operating substrate for one person: a single place where
what is known, what is planned, what is being done and what has been applied
live together, with the automation that connects them.

The long-term shape has three layers:

1. **A durable personal record** — notes, projects, tasks, and later
   documents, references and activity history.
2. **A workflow engine on top of that record** — n8n automations that react to
   changes in the record and write back into it.
3. **A personal intelligence layer** — local machine learning and retrieval over
   the user's own data, with no requirement to surrender it to a third party.

Layers 2 and 3 are explicitly *later*. They must not compromise the security
of layer 1.

## 3. Current development phase

**Phase 1 — the backend foundation.** It is currently in progress.

Phase 1 delivers the persistence, security and API foundations that every
later phase depends on:

- a Rust/Axum modular monolith,
- PostgreSQL through SQLx with real migrations,
- authentication and authorization with a real token lifecycle,
- notes, projects and tasks,
- a versioned REST API with an OpenAPI document,
- consistent errors, structured logging and health endpoints,
- unit, integration, security and API tests,
- documentation good enough for an open-source contributor to use the project.

## 4. Agreed features (Phase 1 scope)

Each item below is **Accepted**. The matching feature documents live under
[`features/`](features/).

### 4.1 Foundation

| Requirement | Feature document |
| ----------- | ---------------- |
| Rust backend using Axum, modular monolith | [architecture](architecture/system-overview.md) |
| PostgreSQL persistence through SQLx, with migrations | [database](database/schema.md) |
| Typed, validated configuration read once at start-up | [architecture](architecture/system-overview.md) |
| Centralized error contract with a stable error code | [API conventions](api/error-contract.md) |
| Structured logging with request correlation | [architecture](architecture/request-lifecycle.md) |
| Liveness and readiness endpoints | [architecture](architecture/system-overview.md) |
| Versioned REST API under `/api/v1` | [API conventions](api/conventions.md) |
| OpenAPI document and a local API explorer | [API conventions](api/conventions.md) |
| Documentation suitable for an open-source project | this document and [`README.md`](../README.md) |

### 4.2 Authentication and authorization

| Requirement | Feature document |
| ----------- | ---------------- |
| Registration with a unique normalized email | [authentication](features/authentication/specification.md) |
| Argon2id password hashing, server-side password policy | [authentication](features/authentication/specification.md) |
| Short-lived signed access tokens with issuer/audience validation | [authentication](features/authentication/specification.md) |
| Opaque refresh tokens, stored hashed, rotated on use | [authentication](features/authentication/specification.md) |
| Refresh-token reuse detection and mass revocation | [authentication](features/authentication/specification.md) |
| Logout and per-user session revocation | [authentication](features/authentication/specification.md) |
| Authenticated-user context derived only from the credential | [security](security/threat-model.md) |
| Ownership checks on every private endpoint | [security](security/threat-model.md) |
| Rate limiting on the authentication endpoints | [secure development](security/secure-development.md) |
| A safe public user representation with no secret material | [API conventions](api/conventions.md) |

### 4.3 Notes

| Requirement | Feature document |
| ----------- | ---------------- |
| Create, list, read, update, delete notes | [notes](features/notes/specification.md) |
| Partial update (PATCH) that never nulls an omitted field | [notes](features/notes/specification.md) |
| Archive and unarchive | [notes](features/notes/specification.md) |
| Full-text search over title and content | [notes](features/notes/specification.md) |
| Pagination with a documented `limit`/`offset` convention | [API conventions](api/conventions.md) |
| Strict ownership isolation | [notes](features/notes/specification.md) |

### 4.4 Projects and tasks

| Requirement | Feature document |
| ----------- | ---------------- |
| Create, list, read, update, delete projects | [projects](features/projects/specification.md) |
| Optional search and pagination over projects | [projects](features/projects/specification.md) |
| Create, list, read, update, delete tasks within a project | [tasks](features/tasks/specification.md) |
| Task status (`open`, `in_progress`, `done`) and priority (`low`, `medium`, `high`, `urgent`) | [tasks](features/tasks/specification.md) |
| Filtering by status and priority | [tasks](features/tasks/specification.md) |
| Manual ordering inside a project | [tasks](features/tasks/specification.md) |
| Consistent completion timestamps | [tasks](features/tasks/specification.md) |
| Tasks unreachable through a project the caller does not own | [tasks](features/tasks/specification.md) |
| A defined behaviour when a project is deleted | [projects](features/projects/specification.md) |

### 4.5 Quality

| Requirement | Feature document |
| ----------- | ---------------- |
| Unit tests for business rules, validation, hashing and tokens | [testing](testing/strategy.md) |
| Integration tests against a real PostgreSQL database | [testing](testing/strategy.md) |
| API contract tests | [testing](testing/strategy.md) |
| Security regression tests for authorization and isolation | [testing](testing/strategy.md) |
| `cargo fmt`, `cargo clippy -D warnings` and `cargo test` as quality gates | [verification policy](testing/verification-policy.md) |
| A disposable test database that cannot be a production database | [testing](testing/strategy.md) |

## 5. Planned future features (roadmap)

These are **not** agreed Phase 1 scope. They are recorded in
[`roadmap.md`](roadmap.md) with their dependency order. Implementing one
requires it to be accepted first.

| Feature | Depends on | Notes |
| ------- | ---------- | ----- |
| n8n workflow automation | projects, tasks, notes | external integration; must not weaken core data security |
| Job application and client-response tracking | projects, tasks | reuses the task model |
| Web frontend | the Phase 1 API | separate codebase |
| Mobile frontend | the Phase 1 API | separate codebase |
| OpenCode prompt and session knowledge management | notes | stores prompts and session metadata |
| Local machine learning and RAG | notes | local-first; no vector database service in Phase 1 |
| Labels, comments, activity history, issue numbers | projects, tasks | modelled on GitHub, not implemented yet |
| Team membership and shared workspaces | users, projects | Phase 1 is single-owner by design |

## 6. Explicitly excluded from Phase 1

These are deliberately **not** built now. Their absence is a decision, not an
oversight.

- Frontend or mobile user interface.
- n8n deployment or any n8n integration.
- OnlineJobs.ph scraping and client-response automation.
- LLM providers, AI-generated content, or any outbound AI call.
- Embedding models, a vector database, or retrieval-augmented generation.
- Microservices, message brokers, or event-streaming infrastructure.
- Billing, subscriptions, and public multi-tenant collaboration.
- Enterprise abstractions that no current requirement needs.
- Full-text search over *other* users' data, or any shared note space.

## 7. User-facing behaviour and requirements

1. **One account per person.** A user registers with an email address and a
   password and can sign in from any client.
2. **Data is private by default.** Everything a user creates is visible only
   to that user. Sharing is not a Phase 1 capability.
3. **Resources are addressed by stable identifiers.** Clients must never have
   to guess or enumerate identifiers.
4. **Errors are actionable.** A client receives a stable error code and a
   human-readable message. Field-level validation failures identify the field.
5. **Partial updates are partial.** A client can change one field of a
   resource without knowing the others, and without accidentally clearing one.
6. **Listing is predictable.** Every list endpoint uses the same pagination
   convention and returns the same envelope.
7. **Timestamps are unambiguous.** Every timestamp is UTC, RFC 3339.
8. **The client can always discover the contract.** The OpenAPI document
   describes the API that actually exists.

## 8. Technical constraints

- **Language and edition:** stable Rust, edition 2024.
- **HTTP:** Axum 0.8 on Tokio.
- **Database:** PostgreSQL; SQLx for access; SQLx migrations for schema.
- **Migrations are append-only.** An applied migration is never edited.
- **Layering is fixed:** handlers → services → repositories → PostgreSQL.
- **No global mutable state.** Dependencies are passed explicitly.
- **No `unwrap`/`expect` in a request path.**
- **All SQL is parameterised.**
- **Dependency additions need a recorded reason** in the change log.
- **The build must not require a hosted or paid service.**

## 9. Security and privacy expectations

1. **Passwords are never stored in plaintext** and never logged.
2. **Secrets never enter the repository.** `.env` files, key material and
   credentials are git-ignored; `.env.example` holds placeholders only.
3. **Access tokens are short-lived and signed**; the signing secret comes from
   the environment and is at least 32 bytes.
4. **Refresh tokens are stored hashed**, rotated on use, and reuse of a
   rotated token revokes the user's sessions.
5. **Authentication failures are generic**, so that an attacker cannot learn
   whether an account exists.
6. **Every private endpoint is protected**, and ownership is enforced in SQL.
7. **Inaccessible resources return `404`**, not `403`, so existence is not
   disclosed.
8. **The API never returns** database internals, stack traces, or secret
   material.
9. **AI and automation integrations must not compromise core data security.**
   A future integration may not weaken ownership checks, token handling or
   secret hygiene.
10. **Rate limiting protects authentication**, and production deployments put
    edge rate limiting in front of the API as well.
11. **HTTPS is required in production**; the API never reads the
    `X-Forwarded-For` header, and there is no trusted-proxy declaration. The
    client address the API uses is the peer address of the connection. How a
    trusted proxy would be declared is an open decision, `Q-07` in
    [`engineering/open-questions.md`](engineering/open-questions.md); the
    limitation is recorded as `KI-003` in
    [`engineering/known-issues.md`](engineering/known-issues.md).

## 10. Supported platforms and clients

| Platform | Phase 1 status |
| -------- | --------------- |
| Linux server | Supported. |
| Windows server | Supported. The project is developed and verified on Windows. |
| macOS server | Expected to work; not continuously verified. |
| Web client (browser) | Not built in Phase 1. The API is CORS-enabled against an explicit origin allowlist for a future web client. |
| Mobile client | Not built in Phase 1. |
| CLI client | Not built in Phase 1. |

The API is plain HTTP/1.1 with JSON bodies, so any HTTP client can use it.

## 11. API design principles

1. **Versioned at the root:** every endpoint lives under `/api/v1`.
2. **RESTful resource naming:** plural nouns, resource identifiers in the path.
3. **HTTP semantics are honoured:** `201` for creation, `204` for a
   successful delete with no body, `400` for a malformed request, `401` for
   missing or invalid credentials, `404` for an unknown or inaccessible
   resource, `409` for a state conflict, `422` for a failed validation rule,
   `429` for rate limiting, `500` for an unexpected failure.
4. **One error shape** for every failure.
5. **One pagination convention** (`limit`, `offset`) and one list envelope.
6. **PATCH means partial.** An omitted field is untouched.
7. **Public identifiers are UUIDs.** No sequential identifiers are exposed.
8. **Timestamps are RFC 3339 UTC.**
9. **Requests and responses are JSON**, except for `204` responses which have
   no body.
10. **The OpenAPI document is generated from the handlers**, so it cannot
    drift silently.

## 12. Database and persistence requirements

- PostgreSQL is the primary relational datastore.
- Every table has a UUID primary key generated by the application.
- Every timestamp is `TIMESTAMPTZ`.
- Ownership is modelled as a non-null foreign key and enforced in every query.
- Uniqueness, especially of the normalized email address, is enforced by the
  database, not by a check-then-insert race.
- Controlled state (task status, task priority) is constrained by the
  database so that an out-of-contract value cannot be persisted.
- Invariants that the application also enforces are backed by `CHECK`
  constraints where practical.
- Indexes exist for the access paths the API actually uses: ownership
  lookups, listing order, filters and search.
- Multi-step writes use an explicit transaction, unless an accepted ADR
  records that the ordering fails safe. The refresh-token rotation is the one
  such case: the replacement session is inserted before the presented one is
  claimed, so an interruption leaves the caller with a usable session rather
  than with none. That ordering is recorded in
  [`decisions/ADR-0003-auth-token-design.md`](decisions/ADR-0003-auth-token-design.md)
  and analysed in
  [`architecture/data-architecture.md`](architecture/data-architecture.md) §7.
- Migrations are reproducible: applying them to an empty database produces the
  same schema as applying them in sequence.

## 13. Testing and quality requirements

1. **Unit tests** cover business rules, validation, normalization, password
   hashing, token handling and error mapping.
2. **Integration tests** run against a real PostgreSQL database, not mocks,
   for everything that depends on SQL semantics.
3. **The test database is disposable.** The harness refuses to run against a
   database whose name does not contain `test`.
4. **Tests are independent.** No ordering dependency, no shared mutable state.
5. **Security-sensitive paths have dedicated tests**, including cross-account
   isolation for every private endpoint.
6. **`cargo fmt --check`, `cargo clippy -D warnings` and `cargo test` are the
   quality gates** and must pass before a change is considered complete.
7. **No lint is suppressed** to make a check pass.
8. **No result is claimed without evidence.** A check that did not run is
   reported as not run.

## 14. Terminology

| Term | Meaning in ATOM |
| ---- | --------------- |
| **Workspace** | The overall product. Not a Phase 1 concept. |
| **Account / user** | A registered person. The only actor in Phase 1. |
| **Session** | A refresh-token record. One user may have several. |
| **Access token** | A short-lived signed bearer token. |
| **Refresh token** | A long-lived opaque token, stored hashed, rotated on use. |
| **Note** | A personal, owned, searchable text record. |
| **Project** | An owned container for tasks. |
| **Task** | A unit of work inside a project, with status and priority. |
| **Archive** | A soft-delete marker on a note. Archived notes are hidden by default. |
| **Ownership** | The rule that a resource belongs to exactly one user. |
| **Modular monolith** | One deployable unit with strict internal module boundaries. |
| **Phase** | A delivery stage. Phase 1 is the backend foundation. |
| **Verified** | Implemented *and* covered by a passing recorded check. |

## 15. Dependencies between features

```text
users ──────────────┬──────────────► auth (sessions, tokens)
                     │
                     ├──► notes ──────────► (future) prompt/session knowledge
                     ├──► projects ───────► tasks
                     │        │                  │
                     │        └──────────────────┘
                     │              │
                     │              └──► (future) n8n automation, job tracking
                     │
                     └──► (future) local ML / RAG over notes
```

Hard dependencies:

- **Everything private depends on `users` and `auth`.** No feature may store a
  user identifier that is not a foreign key to `users`.
- **`tasks` depends on `projects`.** A task cannot exist without its project,
  and its authorization is inherited from the project's owner.
- **`notes` is independent of `projects` and `tasks`** in Phase 1. It must stay
  independent, so that a future "notes attached to a task" does not require a
  schema migration of the notes table.

## 16. Decisions awaiting confirmation

These are open. They are tracked in
[`engineering/open-questions.md`](engineering/open-questions.md). None of them
blocks Phase 1 work that does not depend on the answer.

| # | Question | Blocks |
| - | -------- | ------ |
| Q1 | Should a deleted project delete its tasks, or archive them? | Phase 1 implements *delete*, matching the current schema. If the answer becomes *archive*, the task table needs a soft-delete column. |
| Q2 | Should refresh tokens live in an HTTP-only cookie instead of the response body? | The browser client design. Bearer tokens work for both the future web and mobile clients; a cookie choice adds CSRF obligations. |
| Q3 | What is the refresh-token absolute lifetime and idle timeout? | One absolute lifetime, no idle timeout. The implementation matches the absolute-lifetime answer that [`decisions/ADR-0003-auth-token-design.md`](decisions/ADR-0003-auth-token-design.md) already records, and the rotation preserves that deadline. The idle timeout is still open. |
| Q4 | Should note search support ranking and highlighting? | The search UX. Phase 1 uses a plain full-text match. |
| Q5 | Does the future web client need refresh tokens in cookies, and does ATOM need a CSRF strategy? | The web client, not the API. |

## 17. Current implementation status

> This section compares the requirements above with the repository. It is
> maintained in [`engineering/implementation-status.md`](engineering/implementation-status.md),
> which carries the same information per feature. It is summarised here.

| Area | Status | Evidence |
| ---- | ------ | -------- |
| Rust/Axum modular monolith | Implemented | `src/`, `Cargo.toml` |
| Typed configuration with start-up validation | Implemented | `src/config/` |
| Central error contract | Implemented | `src/error.rs`, `src/api/extract.rs` |
| SQLx migrations | Implemented | `migrations/0001..0003` |
| User registration, normalization, uniqueness | Verified | `tests/auth_api.rs` |
| Argon2id hashing and password policy | Verified | `src/auth/password.rs`, `src/users/service.rs` |
| Access tokens, issuer/audience validation | Verified | `src/auth/token.rs`, `tests/auth_api.rs` |
| Refresh rotation, reuse detection, revocation | Verified | `src/auth/service.rs`, `tests/auth_api.rs` |
| Ownership isolation | Verified | `tests/notes_api.rs`, `tests/projects_tasks_api.rs` |
| Rate limiting on authentication | Verified | `src/ratelimit.rs`, `tests/auth_api.rs` |
| Notes CRUD, archive, search, pagination | Verified | `tests/notes_api.rs` |
| Projects CRUD, search, pagination | Verified | `tests/projects_tasks_api.rs` |
| Tasks CRUD, status, priority, filtering, ordering | Verified | `tests/projects_tasks_api.rs` |
| OpenAPI document and API explorer | Verified | `src/api/routes.rs`, served at `/api/v1/openapi.json` and `/api/v1/docs/`; the document declares every status the handlers return |
| Health endpoints | Verified | `src/health.rs`, exercised by an operator run |
| Structured logging and request correlation | Implemented | `src/telemetry.rs`, `src/api/mod.rs` |
| Request timeout and body limit | Verified | `src/api/mod.rs`, `src/api/extract.rs`, `tests/auth_api.rs` |
| Documentation tree | Verified | `docs/` - `cargo run --bin atom-docs` reports no error and no warning |
| CI pipeline | Implemented | `.github/workflows/ci.yml` - the three quality gates, a release build, the documentation check and the OpenCode configuration check |
| Container image | Not started | `docker-compose.yml` is provided for local PostgreSQL only |
| Everything in the roadmap | Not started | by design |

**Known discrepancies and limitations** are recorded in
[`engineering/known-issues.md`](engineering/known-issues.md). Two are worth
repeating here:

- The rate limiter is **per process**. A multi-instance deployment needs edge
  rate limiting in front of the API.
- SQLx queries are **checked at run time**, not at compile time. The
  compile-time workflow (`cargo sqlx prepare`) is documented but not enabled,
  because it would make the build depend on a live database.

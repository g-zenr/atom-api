# ATOM — project truth

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
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

**Phase 2 — the personal-workspace layer.** It is defined in
[`roadmap.md`](roadmap.md) §2 and its requirements are in §18 of this file.
Phase 2 is **partly delivered**: the reimbursement and job application
endpoints exist (§17), the machine-learning endpoints do not. Its delivery
builds on the Phase 1 API, so it started after the Phase 1 close-out, but its
requirements are agreed independently of it.

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

The Phase 2 requirements — reimbursements, job applications and local machine
learning — are accepted separately in §18, so that the Phase 1 scope above
stays a complete and closed description of this phase.

## 5. Planned future features (roadmap)

These are **not** agreed Phase 1 scope. They are recorded in
[`roadmap.md`](roadmap.md) with their dependency order. Implementing one
requires it to be accepted first.

| Feature | Depends on | Notes |
| ------- | ---------- | ----- |
| n8n workflow automation | projects, tasks, notes | external integration; must not weaken core data security. It stays `Proposed` in [`roadmap.md`](roadmap.md) and is outside Phase 2 (owner decision O7). |
| Web frontend | the Phase 1 API | separate codebase |
| Mobile frontend | the Phase 1 API | separate codebase |
| OpenCode prompt and session knowledge management | notes | stores prompts and session metadata |
| Labels, comments, activity history, issue numbers | projects, tasks | modelled on GitHub, not implemented yet |
| Team membership and shared workspaces | users, projects | Phase 1 is single-owner by design |

Two items that were listed here before 2026-10-24 have moved into agreed
scope: job application tracking is now §18.2, and local machine learning is
now §18.3. The **client-response automation and scraping** that the job-tracking
row used to imply is not part of either: it remains excluded by §6.

## 6. Explicitly excluded

These are deliberately **not** built. Their absence is a decision, not an
oversight. The list was written for Phase 1; §6.1 records the one amendment
that Phase 2 makes to it.

- Frontend or mobile user interface.
- n8n deployment or any n8n integration.
- OnlineJobs.ph scraping and client-response automation.
- LLM providers, AI-generated content, or any outbound AI call.
- Embedding models, a vector database, or retrieval-augmented generation.
  *(Amended by §6.1: **local** embedding models and local retrieval are
  permitted from Phase 2. A vector-database **service** and any hosted or paid
  dependency remain excluded.)*
- Microservices, message brokers, or event-streaming infrastructure.
- Billing, subscriptions, and public multi-tenant collaboration.
- Enterprise abstractions that no current requirement needs.
- Full-text search over *other* users' data, or any shared note space.

### 6.1 Amendment of 2026-10-24 — local inference is permitted

**This is an amendment, and it is marked as one.** It amends one bullet above,
states explicitly that a second is unchanged, and leaves every other exclusion
in force.

| Bullet before the amendment | Reading from 2026-10-24 |
| --------------------------- | ----------------------- |
| "Embedding models, a vector database, or retrieval-augmented generation." | **Embedding models are permitted when they run locally in the API process** (§18.3, [`ADR-0008`](decisions/ADR-0008-local-pure-rust-ml.md)). A vector database *service*, and any use of an embedding model through a network call, remain excluded. Retrieval-augmented generation remains excluded, because it would need text generation. |
| "LLM providers, AI-generated content, or any outbound AI call." | **Unchanged.** It is the reason the ML requirement is local-only. No endpoint may generate text, and no request may leave the machine to obtain one. |

What changed, precisely:

1. **Local inference is allowed.** A model that the process loads from a local
   file may compute an embedding or a similarity score. This is what §18.3
   requires.
2. **Outbound AI calls are still excluded.** No hosted embedding API, no
   model server, no LLM provider, no paid service, at build time or at run
   time. This is also §8 ("the build must not require a hosted or paid
   service") and owner decision O6 in
   [`engineering/runs/chain/ledger.md`](engineering/runs/chain/ledger.md).
3. **No second language runtime.** Adding Python, Node, Go or an external
   model server to the API runtime is excluded, by the same decision.
4. **Text generation remains excluded**, including cover letters, summaries
   and "improve this" features in the job-application feature.

The Phase 1 wording above is kept rather than deleted, so that a reader can
see what was excluded and what the amendment changed. A further change to
this boundary needs its own amendment and, if it is structural, its own ADR.


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
| **Phase** | A delivery stage. Phase 1 is the backend foundation. Phase 2 is the personal-workspace layer (§18). |
| **Claim** | An expense reimbursement request owned by a user, with one currency and a set of line items (§18.1). |
| **Line item** | One expense inside a claim, in the claim's currency. |
| **Application** | A job application: one role at one company, with a stage and a timeline of events (§18.2). |
| **Application event** | One dated entry in an application's timeline. |
| **Embedding** | A fixed-length numeric vector computed locally from a note or an application (§18.3). |
| **Lexical search** | Search by PostgreSQL full-text match. It needs no model. |
| **Semantic search** | Search by cosine similarity between the query embedding and stored embeddings. |
| **Hybrid search** | A bounded lexical candidate set re-ranked by cosine similarity in Rust. |
| **Verified** | Implemented *and* covered by a passing recorded check. |

## 15. Dependencies between features

```text
users ──────────────┬──────────────► auth (sessions, tokens)
                     │
                     ├──► notes ──────────► (later) prompt/session knowledge
                     │        │
                     │        └──────────► (Phase 2) local ML over notes
                     ├──► projects ───────► tasks
                     │        │               │
                     │        └───────────────┘
                     │              │
                     │              └──► (later) n8n automation
                     └──► companies ──────► (Phase 2) job applications
```

The Phase 2 edges — claims, applications, events and embeddings — are in
§15.1; the graph above keeps the Phase 1 shape and marks what each later
capability attaches to.

Hard dependencies:

- **Everything private depends on `users` and `auth`.** No feature may store a
  user identifier that is not a foreign key to `users`.
- **`tasks` depends on `projects`.** A task cannot exist without its project,
  and its authorization is inherited from the project's owner.
- **`notes` is independent of `projects` and `tasks`** in Phase 1. It must stay
  independent, so that a future "notes attached to a task" does not require a
  schema migration of the notes table.

### 15.1 Phase 2 edges

Phase 2 adds one new required parent — an application belongs to a company —
two optional links, and one kind of derived data. Every Phase 2 row is still
scoped by its own `user_id`, and the Phase 1 rule that a foreign row is `404`
still holds.

```text
projects ──(optional, ON DELETE SET NULL)──► claims
tasks    ──(optional, ON DELETE SET NULL)──► applications
notes    ─────────────────────────────────► embeddings ──► search, match
applications ─────────────────────────────► embeddings
companies ────────────────────────────────► applications ──► events
```

- **Claims depend on nothing.** A claim may reference a project, but a claim
  without one is a complete claim. Deleting the project clears the reference.
- **Applications depend on a company**, and the company must have the same
  owner. Events depend on their application and are deleted with it.
- **Embeddings depend on their source**, but the source is polymorphic
  (a note or an application), so the database cannot carry a foreign key to
  it. Deleting a source deletes its embeddings in the same transaction, and a
  reindex removes any orphan. The trade-off is recorded in
  [`ADR-0008`](decisions/ADR-0008-local-pure-rust-ml.md).
- **Nothing in Phase 2 makes `notes` depend on `projects` or `tasks`**, and
  nothing makes `tasks` depend on claims or applications.

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
| SQLx migrations | Implemented | `migrations/0001..0005` |
| User registration, normalization, uniqueness | Verified | `tests/auth_api.rs` |
| Argon2id hashing and password policy | Verified | `src/auth/password.rs`, `src/users/service.rs` |
| Access tokens, issuer/audience validation | Verified | `src/auth/token.rs`, `tests/auth_api.rs` |
| Refresh rotation, reuse detection, revocation | Verified | `src/auth/service.rs`, `tests/auth_api.rs` |
| Ownership isolation | Verified | `tests/notes_api.rs`, `tests/projects_tasks_api.rs` |
| Rate limiting on authentication | Verified | `src/ratelimit.rs`, `tests/auth_api.rs` |
| Notes CRUD, archive, search, pagination | Verified | `tests/notes_api.rs` |
| Projects CRUD, search, pagination | Verified | `tests/projects_tasks_api.rs` |
| Tasks CRUD, status, priority, filtering, ordering | Verified | `tests/projects_tasks_api.rs` |
| OpenAPI document and API explorer | Verified | `src/api/routes.rs` derives the document from the handler annotations and serves it at `/api/v1/openapi.json`, with the explorer at `/api/v1/docs/`. The content is asserted, not inferred: `tests/openapi_contract.rs` compares the route table with the document's operations in both directions, asserts every table entry is mounted, pins the version prefix to a single declaration and pins each list operation's published parameter set. The serving itself is asserted by `tests/docs_route.rs::the_explorer_and_its_assets_are_served`, which fetches the explorer and one of its embedded assets, and by `tests/docs_route.rs::the_openapi_document_is_served`, which fetches the `/api/v1/openapi.json` route; both run under the default `docs` feature, which is the build the claim describes. Scope of the claim: the operation set, the methods and the list parameter sets are asserted, **not** a snapshot of every response body or schema. |
| Health endpoints | Verified | `src/health.rs`, exercised by an operator run |
| Structured logging and request correlation | Implemented | `src/telemetry.rs`, `src/api/mod.rs` |
| Request timeout and body limit | Verified | `src/api/mod.rs`, `src/api/extract.rs`, `tests/auth_api.rs` |
| Documentation tree | Verified | `docs/` - `cargo run --bin atom-docs` reports no error and no warning |
| CI pipeline | Implemented | `.github/workflows/ci.yml` - the three quality gates, a release build, the documentation check and the OpenCode configuration check |
| Container image | Not started | `docker-compose.yml` is provided for local PostgreSQL only |
| Reimbursement tracking | Implemented | accepted in §18.1; migration `0004_expense_claims.sql`, `src/reimbursements/` and all twelve published operations exist. `tests/claims_schema.rs` asserts the constraints, `tests/claims_api.rs` the twelve operations and `tests/openapi_contract.rs` the published surface. No reviewer has read the chain that completed it, so the feature is **not** `Verified`; a passing suite is not a review. |
| Job application tracking | Implemented | accepted in §18.2; migration `0005_job_applications.sql`, `src/job_applications/` and all fourteen published operations exist. `tests/jobs_schema.rs` asserts the constraints and `tests/job_applications_api.rs` the fourteen operations, including the stage machine. `ATOM-REVIEW-20261024-010` read the chain, found no code defect and returned FAIL on documentation only; the findings are fixed, but no gate run after those fixes is recorded, so the feature is not `Verified`. |
| Local machine learning | Not started | accepted in §18.3; no code, no migration, no test exists |
| Everything else in the roadmap | Not started | by design |

**Known discrepancies and limitations** are recorded in
[`engineering/known-issues.md`](engineering/known-issues.md). Two are worth
repeating here:

- The rate limiter is **per process**. A multi-instance deployment needs edge
  rate limiting in front of the API.
- SQLx queries are **checked at run time**, not at compile time. The
  compile-time workflow (`cargo sqlx prepare`) is documented but not enabled,
  because it would make the build depend on a live database.

## 18. Agreed Phase 2 requirements

Phase 2 is the **personal-workspace layer**, delivered on the Phase 1
foundation: reimbursement tracking, job application tracking, and local
machine learning over the owner's own notes. The phase itself is defined in
[`roadmap.md`](roadmap.md) §2.

Every requirement in this section was **`Accepted`** on 2026-10-24 — agreed,
and not built at the time. The reimbursement requirements (`REIMB-R01`…`R11`)
are now **`Implemented`**: the code exists for all twelve published operations.
They are not `Verified`, because no reviewer has read the chain that completed
them; a passing suite is not a review. The job-application requirements are now
**`Implemented`**: the code exists for all fourteen published operations, and
`ATOM-REVIEW-20261024-010` read the chain, found no code defect and returned
FAIL on documentation only. They are not `Verified`, because no gate run after
the review's fixes is recorded and the review's findings have not been
re-reviewed. The machine-learning requirements are still `Accepted`. §17 says
the same in its status table. The
feature specifications are:

| Feature | Identifier | Specification |
| ------- | ---------- | ------------- |
| Reimbursement tracking | `ATOM-FEAT-REIMB-001` | [`features/reimbursements/specification.md`](features/reimbursements/specification.md) |
| Job application tracking | `ATOM-FEAT-JOB-001` | [`features/job-applications/specification.md`](features/job-applications/specification.md) |
| Local machine learning | `ATOM-FEAT-ML-001` | [`features/ml/specification.md`](features/ml/specification.md) |

Three decisions are recorded as accepted ADRs and are part of these
requirements:

| ADR | Decision |
| --- | -------- |
| [`ADR-0007`](decisions/ADR-0007-money-representation.md) | Money is `NUMERIC(19,4)` in PostgreSQL and a string with four decimals in JSON. |
| [`ADR-0008`](decisions/ADR-0008-local-pure-rust-ml.md) | Inference is local and pure Rust, with Burn and the `flex` backend. |
| [`ADR-0009`](decisions/ADR-0009-receipt-references.md) | A receipt is a reference, not an uploaded file. |

The **Origin** column names where each requirement comes from. `A1`–`A6` are
the assumptions in [`engineering/runs/chain/plan.md`](engineering/runs/chain/plan.md)
§3; **they were never explicitly confirmed by the owner** (owner decision
O1 in [`engineering/runs/chain/ledger.md`](engineering/runs/chain/ledger.md)).
They are recorded here as **provisional defaults**, not as owner-approved
requirements. §18.4 says what that means for a change.

### 18.1 Reimbursement tracking

| ID | Requirement | Acceptance criteria | Origin |
| -- | ----------- | ------------------- | ------ |
| `REIMB-R01` | An expense claim has a title, an optional description, one ISO 4217 currency and zero or more line items, each with a description, a non-negative amount, an incurred-on date, an optional category and an optional receipt reference. | A claim created without items returns `201` with `total_amount` `"0.0000"`; an item created on the claim is returned by the claim's detail read; an item carries no currency field of its own. | A1 (provisional) |
| `REIMB-R02` | Every line item is denominated in its claim's currency, and a claim has exactly one currency for its whole life. | `currency` is required on create and is immutable: it is absent from `UpdateClaimRequest` and appears in no `SET` clause, so a `PATCH` body carrying it is **accepted with `200` and the field is ignored**. No exchange rate, conversion or second currency exists anywhere in the feature. The `422` this row recorded until 2026-10-24 was the earlier proposal; the published contract in [pi.md](features/reimbursements/api.md) §6 won, because rejecting would require adding the field to the request in order to refuse it. Corrected under §18.4, recorded in the change log. | A1 (provisional) |
| `REIMB-R03` | The claim's `total_amount` always equals the sum of its items' amounts. | After every item insert, update and delete the total equals `SUM(amount)` recomputed in the same transaction; deleting the last item leaves `"0.0000"`; the total is returned as a four-decimal string. | A1 (provisional) |
| `REIMB-R04` | A claim moves through `draft → submitted → approved` or `rejected`, `rejected → draft`, and `approved → paid`; `paid` is terminal. | Each allowed transition returns `200` with the new status and its timestamp; the `rejected → draft` reopen clears both `submitted_at` and `decided_at`; every other transition, including any transition out of `paid`, returns `409 conflict`. | A1 (provisional) |
| `REIMB-R05` | A claim can be submitted only when it has at least one item and a total greater than zero. | A submission with no items is `422` naming `items`; a submission whose total is `"0.0000"` is `422` naming `total_amount`; a valid submission sets `submitted_at` and the status `submitted`. | A1 (provisional) |
| `REIMB-R06` | A line item may be created, changed or deleted only while its claim is a `draft`. | On a claim in any other status an item write returns `409 conflict`; a rejected claim must return to `draft` before its items can change. | Derived from R03 and R04 |
| `REIMB-R07` | Money is stored as `NUMERIC(19,4)` in PostgreSQL and represented in every JSON body as a string with exactly four decimal places. | A JSON number where money is expected is `422`; `"12.5"` and `"12.500"` are `422`; `"12.5000"` round-trips unchanged; no `f32` or `f64` participates in a money path; the largest accepted magnitude is `"999999999999999.9999"`. | A6 (provisional), ADR-0007 |
| `REIMB-R08` | A receipt is a client-supplied URL or reference on a line item; the API does not accept, store or fetch an uploaded file. | The field is optional, at most 2048 characters, and must be an absolute `http` or `https` URL; a `data:`, `file:` or `javascript:` value is `422`; the API never connects to the URL. | A2 (provisional), ADR-0009 |
| `REIMB-R09` | A claim may reference one project owned by the same user, and deleting that project clears the reference without deleting the claim. | A foreign project id is `404`; `project_id` is nullable and returned as `null` when absent; after the project is deleted the claim still exists with `project_id` null. | A1 (provisional), A5 (provisional) |
| `REIMB-R10` | Every read and write of a claim or an item is scoped by the caller's `user_id`, and a claim or item that belongs to another account is `404`. | Every verb and every action on a foreign claim is `404`; no listing contains a foreign claim; an item id from another account's claim is `404`; an item id that belongs to a different claim of the caller is also `404`. | A5 (provisional) |
| `REIMB-R11` | A claim may be deleted in any lifecycle status, and deleting it deletes its items. | `DELETE` on a `draft`, `submitted`, `approved`, `rejected` and `paid` claim each returns `204`; a following `GET` is `404`; the items are gone. | Derived by the intake |

### 18.2 Job application tracking

| ID | Requirement | Acceptance criteria | Origin |
| -- | ----------- | ------------------- | ------ |
| `JOB-R01` | A company has a name that is unique per owner, compared case-insensitively, and optional website and notes. | Creating `Acme` and then `acme` for one owner is `409 conflict`; two owners may each have `Acme`; renaming onto an existing name is `409`. | A3 (provisional) |
| `JOB-R02` | A job application belongs to exactly one company owned by the same user and records a role title, one of seven stages, and optional source, applied-on date, job URL and description. | A create returns `201`; a company id owned by another account is `404`; a create without a stage records `wishlist`, and a create with any other stage is `422`; an unknown stage in the body is `422`. | A3 (provisional), A5 (provisional) |
| `JOB-R03` | A salary range is optional, expressed in one ISO 4217 currency, with `salary_min ≤ salary_max`, and money follows `REIMB-R07`. | A minimum without a currency is `422`; a minimum above the maximum is `422`; both bounds round-trip as four-decimal strings; no conversion is performed. | A3 (provisional), A6 (provisional) |
| `JOB-R04` | An application moves `wishlist → applied → screening → interview → offer`; `rejected` and `withdrawn` are reachable from any non-terminal stage; `offer`, `rejected` and `withdrawn` are terminal. | Each listed edge returns `200`; a skipped stage, a backwards move and any move out of a terminal stage return `409 conflict`; `offer` is reachable only from `interview`. | A3 (provisional) |
| `JOB-R05` | An application event records a kind, an `occurred_at` timestamp and a summary, belongs to one application, and the events of an application are listed newest first by `occurred_at` then `id`. | The order is total and stable; `occurred_at` is required; a future `occurred_at` is accepted, because an interview can be scheduled; an event id under another application of the caller is `404`. | A3 (provisional) |
| `JOB-R06` | Deleting an application deletes its events, and a company that still has applications cannot be deleted. | Deleting an application returns `204` and its events are gone; deleting a company with at least one application returns `409 conflict`; after the last application is deleted the company deletes with `204`. | A3 (provisional); the refusal is derived |
| `JOB-R07` | An application may reference one task as its next action; the task must belong to the same user, and deleting the task clears the reference. | A foreign task id is `404`; after the task is deleted the application still exists with `next_action_task_id` null; the link never moves a task or an application between owners. | A3 (provisional) |
| `JOB-R08` | The feature stores and returns what the owner entered; it never generates text and never calls an outbound AI service. | No endpoint accepts a "generate" request; no HTTP client is added to the module; no response contains text the owner did not store. | A3 (provisional), §6.1 |
| `JOB-R09` | Every read and write of a company, an application and an event is scoped by the caller's `user_id`, and one that belongs to another account is `404`. | Every verb on a foreign row is `404`; no listing contains a foreign row; a company id belonging to another account cannot be used in a create or an update. | A5 (provisional) |

### 18.3 Local machine learning

| ID | Requirement | Acceptance criteria | Origin |
| -- | ----------- | ------------------- | ------ |
| `ML-R01` | All inference runs locally inside the API process; no request is sent to a hosted or paid service and no second language runtime is started. | `src/ml/` contains no outbound HTTP client and spawns no process; the binary builds and serves search with no network available; a test asserts the deterministic embedder performs no I/O. | A4 (provisional), owner decision O6 |
| `ML-R02` | Embeddings are produced by Burn v0.22 on the `flex` CPU backend, with weights loaded from a local SafeTensors file. | `Cargo.toml` selects `flex` and adds none of `wgpu`, `cuda` or `tch`; the model path is a setting; a missing or unreadable file selects the disabled embedder instead of panicking. | A4 (provisional), O6, ADR-0008 |
| `ML-R03` | The embedder is a trait with exactly three implementations — `Burn`, `Deterministic` and `Disabled` — and every automated test uses `Deterministic`. | No test loads a model file; `Deterministic` returns the same vector for the same input and reports its dimension; `Disabled` reports that it is unavailable rather than failing. | A4 (provisional), ADR-0008 |
| `ML-R04` | An embedding is stored as a PostgreSQL `REAL[]` together with its source type, source id, model id and dimension, unique per `(user_id, source_type, source_id, model_id)`. | Re-embedding the same source with the same model leaves one row; a vector whose length differs from its recorded dimension is refused; no PostgreSQL extension is required. | A4 (provisional), ADR-0008 |
| `ML-R05` | Note search accepts the modes `lexical`, `semantic` and `hybrid`; hybrid builds a bounded lexical candidate set and re-ranks it by cosine similarity in Rust. | Lexical mode returns the notes the full-text match returns and no score; hybrid returns at most the configured candidate bound, each with a score in `[0,1]`; semantic mode never scans beyond the caller's own embeddings. | A4 (provisional), ADR-0008 |
| `ML-R06` | When ML is disabled or the model file is missing, a semantic or hybrid request returns lexical results with `200` and names the mode that was applied. | The response reports the requested mode, the applied mode `lexical` and a degradation reason; the status is `200`, never `500`; separate tests cover "ML off" and "model missing"; a `POST /job-applications/{id}/match` when ML is unavailable is `409 conflict`, not a degraded `200`. | A4 (provisional) |
| `ML-R07` | A reindex is idempotent, owner-scoped, and refuses with `409 conflict` when ML is disabled. | Running it twice inserts no duplicate row; it never reads or writes another account's rows; the response reports how many rows were written and skipped. | A4 (provisional) |
| `ML-R08` | Every embedding read and write, every search and every match is scoped by the caller's `user_id`, and a note or application of another account is `404`. | Semantic and hybrid search cannot return a foreign note even if the embedding row exists; a match on a foreign application is `404`; a reindex touches only the caller's rows. | A5 (provisional) |
| `ML-R09` | The index is evaluated with a fixed set of fixture queries and expected notes under the deterministic embedder, and the real model's quality is recorded by a separate manual run. | The fixture set contains 15 queries; each one asserts an expected note is in the top results; the manual run is recorded in the feature status and is not a CI gate. | A4 (provisional) |

### 18.4 Origin, provisional status and what to do about a revision

| Origin | Requirements | What it means |
| ------ | ------------ | ------------- |
| `A1` — claims, line items, one currency, the lifecycle | `REIMB-R01`…`R06`, `R09` | **Provisional default.** The shape is the one the plan works from; the owner has not confirmed it. |
| `A2` — receipts are references | `REIMB-R08` | **Provisional default**, recorded as a decision in ADR-0009. |
| `A3` — applications and events, no text generation | `JOB-R01`…`R09` | **Provisional default.** |
| `A4` — ML is local and never generates text | `ML-R01`…`R09` | **Provisional default.** The *stack* is not provisional: O6 fixes Rust-only, Burn and `flex`. |
| `A5` — single owner, no sharing | `REIMB-R10`, `JOB-R09`, `ML-R08` | **Provisional default**, and consistent with every Phase 1 requirement. |
| `A6` — the money representation | `REIMB-R07`, `JOB-R03` | **Provisional default**, recorded as a decision in ADR-0007. |
| Derived by the intake | `REIMB-R06`, `REIMB-R11`, `JOB-R06` (the refusal), the terminal stages in `JOB-R04` | Not in A1-A6 at all. They are consequences of the other requirements, written down so that the implementing change has an answer. |

**A1–A6 are not owner-approved.** The owner said to proceed without confirming
them (O1), so they are recorded as provisional defaults here and in every
document that relies on them. If the owner revises one:

1. the affected requirement is amended in this section, in a new change with
   its own change-log entry;
2. the feature specification and the affected `api.md` are amended in the same
   change;
3. if the revision changes a structure that is expensive to reverse, a new ADR
   supersedes the old one — ADR-0007, ADR-0008 and ADR-0009 are never edited.

An implementing change that finds a provisional default unworkable stops and
records the conflict in
[`engineering/known-issues.md`](engineering/known-issues.md) rather than
quietly choosing a different answer.

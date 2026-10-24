# Job applications — architecture

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

How the job application feature is intended to be built. The public contract
is in [api.md](api.md); the agreed requirements are in
[specification.md](specification.md).

**Nothing in this document exists yet.** No file under
`src/job_applications/`, no migration and no test is in the repository. The
module names, table names and SQL below are the plan, recorded so that the
implementing change has a shape to follow. The design is **accepted but not
built**, and the text is corrected whenever the implementation and this plan
disagree.

## 1. Responsibilities

| Module | Responsibility |
| ------ | -------------- |
| `src/job_applications/handlers.rs` | HTTP only: extraction, status codes, response serialization. |
| `src/job_applications/service.rs` | Business rules: validation, the stage machine, the company-delete rule, the transaction coordination, the mapping from a repository miss to `404`. |
| `src/job_applications/repository.rs` | SQL: company, application and event statements, every one scoped by `user_id`. |
| `src/job_applications/models.rs` | Persistence shapes: `Company`, `JobApplication`, `ApplicationEvent`. |
| `src/job_applications/dto.rs` | Public contracts: `CompanyResponse`, `CompanyPage`, `ApplicationResponse`, `ApplicationPage`, `EventResponse`, `EventPage`, the create and update requests, the query types and the length limits. |

The feature depends on:

| Dependency | For |
| ---------- | --- |
| `src/api/middleware.rs` (`AuthUser`) | The caller's identity, and therefore the ownership predicate. |
| `src/pagination.rs` | The list envelope and the clamp. |
| `src/helpers/validation.rs` | The shared text validators. |
| `src/error.rs` (`AppError`) | The one error envelope. |
| `src/config/messages.toml` | Every client-facing string. |

It reads `tasks` only to verify the optional next-action link. It reads no
other domain table.

## 2. Interfaces

### 2.1 `JobService`

| Method | Contract |
| ------ | -------- |
| `create_company(owner, request)` | Validates the name, website and notes; inserts; a duplicate name is `409`. |
| `list_companies(owner, query, pagination)` | Applies the search filter and the clamp; returns `Page<CompanyResponse>` ordered by name. |
| `get_company(owner, company_id)` | Loads the company owned by `owner`; `404` otherwise. |
| `update_company(owner, company_id, request)` | Validates the present fields; a rename onto an existing name is `409`. |
| `delete_company(owner, company_id)` | Deletes only when no application references it; otherwise `409`. |
| `create_application(owner, request)` | Validates, verifies the company and the optional task belong to `owner`, inserts. |
| `list_applications(owner, query, pagination)` | Filters by stage, company and applied date; returns `Page<ApplicationResponse>`. |
| `get_application(owner, application_id)` | Loads the application owned by `owner`; `404` otherwise. The events are read through their own paginated endpoint. |
| `update_application(owner, application_id, request)` | Validates the present fields, applies `ensure_transition` when `status` is present, writes in one transaction. |
| `delete_application(owner, application_id)` | Deletes the application; its events cascade. |
| `list_events(owner, application_id, pagination)` | Verifies the application is owned, then lists its events newest first. |
| `create_event(owner, application_id, request)` | Verifies ownership, validates, inserts. |
| `update_event(owner, application_id, event_id, request)` | Partial update inside the application. |
| `delete_event(owner, application_id, event_id)` | Deletes inside the application. |

The `owner` argument is the authenticated `user_id`, and it is the first
parameter of every method.

### 2.2 The repository statements

| Method | SQL (sketch) |
| ------ | ------------ |
| `insert_company(…)` | `INSERT INTO companies (…) VALUES (…) RETURNING …` |
| `find_company_owned(id, user_id)` | `SELECT … WHERE id = $1 AND user_id = $2` |
| `list_companies_owned(user_id, search, limit, offset)` | `SELECT … WHERE user_id = $1 AND ($2::text IS NULL OR name ILIKE '%' \|\| $2 \|\| '%') ORDER BY lower(name), name, id LIMIT $3 OFFSET $4`, plus `COUNT(*)` |
| `delete_company_if_unused(id, user_id)` | `DELETE FROM companies WHERE id = $1 AND user_id = $2 AND NOT EXISTS (SELECT 1 FROM job_applications WHERE company_id = $1 AND user_id = $2)` |
| `insert_application(…)` | `INSERT INTO job_applications (…) VALUES (…) RETURNING …, salary_min::text, salary_max::text` |
| `find_application_owned(id, user_id)` | `SELECT … WHERE id = $1 AND user_id = $2` |
| `list_applications_owned(user_id, filters, limit, offset)` | `SELECT … WHERE user_id = $1 AND ($2::text IS NULL OR status = $2) … ORDER BY created_at DESC, id DESC LIMIT … OFFSET …`, plus `COUNT(*)` |
| `list_events_owned(application_id, user_id, limit, offset)` | `SELECT … WHERE application_id = $1 AND user_id = $2 ORDER BY occurred_at DESC, id DESC LIMIT … OFFSET …`, plus `COUNT(*)` |
| `insert_event(…)` | `INSERT INTO application_events (…) VALUES (…) RETURNING …` |
| `find_event_owned(application_id, event_id, user_id)` | `SELECT … WHERE id = $1 AND application_id = $2 AND user_id = $3` |
| `update_event_owned(application_id, event_id, user_id, kind, occurred_at, summary, details)` | `UPDATE application_events SET kind = COALESCE($4, kind), occurred_at = COALESCE($5, occurred_at), summary = COALESCE($6, summary), details = COALESCE($7, details) WHERE id = $1 AND application_id = $2 AND user_id = $3 RETURNING …` — no timestamp column is written |
| `delete_event_owned(application_id, event_id, user_id)` | `DELETE FROM application_events WHERE id = $1 AND application_id = $2 AND user_id = $3` |

The search fragment is a static `ILIKE` with the term bound as `$2`; no user
input is concatenated into SQL.

### 2.3 The stage machine

One function, `ensure_transition(current, next) -> Result<(), AppError>`,
holds the table from [specification.md](specification.md) §6.3. It is a
`match` on the pair, not a rank comparison, because `rejected` and `withdrawn`
are reachable from four different stages and a numeric rank cannot express
that.

The transition runs **inside** the update transaction:

```text
BEGIN
  SELECT status FROM job_applications WHERE id = $1 AND user_id = $2 FOR UPDATE
  ensure_transition(current, requested)
  UPDATE job_applications SET …, status = $n, updated_at = NOW() WHERE id = $1 AND user_id = $2
COMMIT
```

`FOR UPDATE` is what makes two concurrent stage changes serialise: the second
one sees the first one's status and is refused if the edge no longer exists.
Without it, two requests could both read `applied` and both move to
`screening`, or one could skip a stage.

### 2.4 The company delete rule

`JOB-R06` refuses to delete a company that still has applications. Doing the
check as `SELECT COUNT(*)` and then `DELETE` is a race: an application can be
inserted between the two statements. The statement is therefore a single
`DELETE … AND NOT EXISTS (…)`, and when it affects zero rows the service runs
two cheap reads to decide which answer to give:

| Situation | Answer |
| --------- | ------ |
| The company does not exist or is another account's | `404` |
| The company exists and has at least one application | `409 conflict` |
| The delete succeeded | `204` |

The composite foreign key described in §3 closes the remaining window: an
application inserted against a company that is being deleted fails the
constraint instead of becoming an orphan.

### 2.5 An event is corrected in place, with no `updated_at`

An event row is **edited in place**, not written once and left alone. Its four
content fields — `kind`, `occurred_at`, `summary` and `details` — may be
corrected by `PATCH /job-applications/{application_id}/events/{event_id}`, and
every one of them is optional in that request, like any other `PATCH` in the
API. The row deliberately carries **no `updated_at`** column and the response
has no such field: a timeline entry records **what the owner says happened**,
not when the row was last written. `created_at` is the only row timestamp the
API keeps.

The decision is locked as `A-D1-1` (the orchestrator's Phase 2 intake ruling).
Two consequences follow for a change:

- The event update is a plain `UPDATE` scoped by `id`, `application_id` and
  `user_id`; it sets no timestamp column, and the response must not grow one.
- A reviewer must not "restore" an `updated_at` to the table or the DTO for
  consistency with the other resources. The absence is the decision.

## 3. Data ownership

| Table | Owner column | Meaning |
| ----- | ------------ | ------- |
| `companies` | `user_id` → `users (id) ON DELETE CASCADE` | A company belongs to exactly one account. |
| `job_applications` | `user_id` → `users (id) ON DELETE CASCADE`, `company_id` | An application belongs to one account and one company. |
| `application_events` | `user_id`, `application_id` → `job_applications (id) ON DELETE CASCADE` | An event belongs to one application and one account. |

**The ownership of the parent is enforced by a composite foreign key**, not
only by the service:

- `companies` carries a unique constraint on `(id, user_id)`.
- `job_applications` carries `FOREIGN KEY (company_id, user_id) REFERENCES
  companies (id, user_id)`. A row that points at another account's company
  does not violate a check the service might forget — it violates the schema.
- `application_events` carries `FOREIGN KEY (application_id, user_id)
  REFERENCES job_applications (id, user_id)` for the same reason.

That is stronger than the Phase 1 task model, where the join to `projects`
carries the ownership predicate. It costs one extra column in each parent's
unique index and it removes a class of bug entirely.

The unique company name is `UNIQUE (user_id, lower(name))`, which is what
makes `JOB-R01` a database guarantee rather than a check-then-insert race.

The optional link is `job_applications.next_action_task_id → tasks (id) ON
DELETE SET NULL`.

The physical definition belongs in [`schema.md`](../../database/schema.md) and
is written with the migration. The plan's working sketch is in
[`plan.md`](../../engineering/runs/chain/plan.md) §5.2.

## 4. Invariants

| # | Invariant | Enforced by |
| - | --------- | ----------- |
| 1 | Every row belongs to exactly one user, and a child's owner equals its parent's. | The non-null `user_id` and the composite foreign keys. |
| 2 | A company, application or event of another account is invisible and answers `404`. | `user_id = $n` in every statement. |
| 3 | A company name is unique per owner, ignoring case. | `UNIQUE (user_id, lower(name))`. |
| 4 | An application's stage is one of seven values. | A `CHECK` on `status`. |
| 5 | Only the listed stage edges happen, and three stages are terminal. | `ensure_transition` under `FOR UPDATE`. |
| 6 | A company with applications cannot be deleted. | The `NOT EXISTS` delete and the composite foreign key. |
| 7 | Deleting an application deletes its events. | `ON DELETE CASCADE`. |
| 8 | An event's `occurred_at` is required and may be in the future. | `NOT NULL`, no past-only rule. |
| 9 | The event order is total and stable. | `ORDER BY occurred_at DESC, id DESC`. |
| 10 | The company order is total and stable. | `ORDER BY lower(name), name, id`. |
| 11 | An application's salary range is one currency and ordered. | `CHECK (salary_min <= salary_max)` and the service rule that a bound requires a currency. |
| 12 | A partial update leaves an omitted field untouched. | `Option` fields and `COALESCE`. |
| 13 | A refused stage move writes nothing else from the same request. | The whole `PATCH` is one transaction. |

## 5. Error handling

| Situation | Error | Status |
| --------- | ----- | ------ |
| A field breaks a rule in [specification.md](specification.md) §6.5 | `validation_failed` naming the field | `422` |
| A duplicate company name for one owner | `conflict` | `409` |
| A stage move the machine does not allow | `conflict` | `409` |
| Deleting a company that still has applications | `conflict` | `409` |
| The row does not exist, or belongs to another account | `not_found` | `404` |
| The company, task or application referenced in a body is foreign | `not_found` | `404` |
| The body is not JSON, or a path parameter is not a UUID | `bad_request` | `400` |
| A query parameter cannot be parsed, or is outside its enum | `bad_request` | `400` |
| No or invalid credentials | `unauthorized` | `401` |
| A body over the limit | `payload_too_large` | `413` |
| The database failed | `database_error` | `500` |

## 6. The security boundary

```text
                 ┌─────────────────────────────────────────────────┐
  bearer token ─►│ /companies, /companies/{id}                     │  authenticated
                 │ /job-applications, /job-applications/{id}       │
                 │ /job-applications/{id}/events, …/events/{event} │
                 └─────────────────────────────────────────────────┘
                                    │
                                    │  user_id from the credential only
                                    ▼
              every statement carries `user_id = $n`
              the child tables carry a composite FK to the parent's owner
```

- There is no anonymous route in this feature.
- A company id, a task id and an application id in a **body** are verified
  against the caller's ownership before the write; a foreign id is `404`, not
  `403`.
- An event id is only meaningful **inside** its application: the path carries
  both, and the statement matches both, so an event cannot be moved between
  applications.
- The feature stores no credential and calls no external service. A job URL is
  stored as text and never fetched, for the same SSRF reason as the receipt
  URL ([`ADR-0009`](../../decisions/ADR-0009-receipt-references.md)).
- The stage of an application is personal data: it can reveal that the owner is
  looking for work. It is owner-scoped like every other field, and no listing
  aggregates across accounts.

## 7. The request path

### 7.1 Changing a stage

```text
PATCH /api/v1/job-applications/{application_id}
  {"status": "screening"}
  │
  ├─ AuthUser → user_id
  ├─ Path → application_id          not a UUID → 400
  ├─ Json<UpdateApplicationRequest> not JSON → 400
  ├─ JobService::update_application
  │    ├─ validate the present fields
  │    ├─ BEGIN
  │    │    ├─ SELECT status … FOR UPDATE     no row → 404
  │    │    ├─ ensure_transition(current, requested)   refused → 409
  │    │    └─ UPDATE job_applications SET …, status = $n, updated_at = NOW()
  │    └─ COMMIT
  └─ 200 OK, ApplicationResponse
```

A `PATCH` that carries `status` **and** other fields is one transaction: a
refused move rolls back the other edits too.

### 7.2 Listing events

```text
GET /api/v1/job-applications/{application_id}/events?limit=20&offset=0
  │
  ├─ AuthUser → user_id
  ├─ verify the application is owned        no row → 404
  ├─ SELECT … WHERE application_id = $1 AND user_id = $2
  │         ORDER BY occurred_at DESC, id DESC LIMIT $3 OFFSET $4
  ├─ SELECT COUNT(*) … the same predicate
  └─ 200 OK, EventPage
```

The ownership check and the list are two statements; both carry `user_id`, so
an application deleted between them yields an empty page, not a foreign page.

## 8. Related files

| File | Role | Exists |
| ---- | ---- | ------ |
| `src/api/routes.rs` | Registers the routes and the schemas in the OpenAPI document. | Yes, to be extended. |
| `src/api/extract.rs` | The extractors that keep the error envelope intact. | Yes. |
| `src/api/middleware.rs` | `AuthUser`. | Yes. |
| `src/pagination.rs` | The list envelope and the clamp. | Yes. |
| `src/helpers/validation.rs` | The shared text validators. | Yes. |
| `src/job_applications/**` | The feature itself. | **No** |
| `migrations/00NN_job_applications.sql` | The three tables, the composite foreign keys and the indexes. | **No** |

## 9. Related decisions

- [`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md) —
  ownership as a SQL predicate.
- [`ADR-0007`](../../decisions/ADR-0007-money-representation.md) — the salary
  range's money format.
- [`ADR-0001`](../../decisions/ADR-0001-modular-monolith.md) — why this is a
  module.
- [`project-truth.md`](../../project-truth.md) §6.1 — no text generation, no
  outbound AI call.

## 10. Limitations of this design

- **The composite foreign key needs a redundant `user_id` on the child.** It
  is the price of making the cross-owner link unrepresentable. A row's owner
  is stored twice (in the row and in its parent) and the constraint keeps the
  two equal.
- **`FOR UPDATE` serialises stage changes on one application.** That is the
  point, and it is also the throughput ceiling; a personal workspace will
  never reach it.
- **The stage machine is a service rule.** A direct SQL update can set any
  allowed status value; only the seven-value `CHECK` is enforced by the
  database.
- **The events list is a second query after the ownership check.** A
  single-statement join would be one round trip; the current shape is clearer
  and the race is benign (an empty page, never a foreign row).
- **The company search is `ILIKE '%term%'`.** It cannot use a B-tree index for
  a leading wildcard; the corpus is a personal company list, so this is
  acceptable. A trigram index is the answer if it ever is not.
- **The stage change is not an event.** The timeline does not record when a
  stage moved, only what the owner chose to record.

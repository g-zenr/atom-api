# Job applications — architecture

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

How the job application feature is built, and how the part that is not built yet
is intended to be built. The public contract is in [api.md](api.md); the agreed
requirements are in [specification.md](specification.md).

**The schema half exists; the layering does not.** Chain `C-J1` (2026-10-24)
added `migrations/0005_job_applications.sql` and `src/job_applications/` with
`mod.rs` and `models.rs` only. There is no `handlers.rs`, `service.rs`,
`repository.rs` or `dto.rs`, no route and no endpoint; `/api/v1/openapi.json`
stays at 34 operations. The layering arrives in `J2`-`J4`. Everything in this
document from §2 onwards is therefore the **plan**, recorded so the
implementing change has a shape to follow, and corrected whenever the
implementation and the plan disagree. §1 separates what is real from what is
not.

## 1. What exists today

| Item | In the tree? | Detail |
| ---- | ------------ | ------ |
| `migrations/0005_job_applications.sql` | **Yes** | The two enum types, the three tables, every `CHECK`, both composite keys, the `ON DELETE` actions and every index of [`schema.md`](../../database/schema.md) §11.3–§11.5. |
| `src/job_applications/mod.rs` | **Yes** | Declares `pub mod models;` and re-exports the two enums and the three row structs. The other four layers are deliberately **not** declared: a `pub mod` with no file behind it would not compile, and an empty placeholder would suggest behaviour that does not exist. |
| `src/job_applications/models.rs` | **Yes** | `ApplicationStatus`, `ApplicationEventKind`, `Company`, `JobApplication`, `ApplicationEvent`. The two enums carry `serde(rename_all = "snake_case")` **and** `sqlx(type_name = …)`, so the Rust value and the database value cannot drift. `salary_min` and `salary_max` are `Option<String>`: SQLx cannot decode `NUMERIC` with this crate's feature set, and `ADR-0007` forbids a decimal crate, so the repository will select the columns as `::text` and the value travels as it arrives. |
| `src/lib.rs` | **Yes** | `pub mod job_applications;` |
| `src/bin/atom-seed.rs` | **Yes** | Inserts companies, applications and events; `SEEDED_TABLES` names all three, and `--reset` deletes them by `user_id`. The `dev` counts are 20 applications, 12 companies and 50 events. |
| `handlers.rs`, `service.rs`, `repository.rs`, `dto.rs` | **No** — `J2`-`J4` | The four layers that would turn the schema into behaviour. |
| The routes and the OpenAPI entries | **No** | The document stays at 34 operations. |
| `tests/job_applications_api.rs` | **No** | Planned in [testing.md](testing.md) §2. |
| `tests/jobs_schema.rs` | **Yes** | Eleven cases against the real database; mapped in [testing.md](testing.md) §1. Existence is not a passing run. |

## 2. Responsibilities

The layering the feature will follow, and where each file stands today.

| Module | Responsibility | Exists |
| ------ | -------------- | ------ |
| `src/job_applications/handlers.rs` | HTTP only: extraction, status codes, response serialization. | No — `J2`. |
| `src/job_applications/service.rs` | Business rules: validation, the stage machine, the company-delete rule, the task-ownership check, the transaction coordination, the mapping from a repository miss to `404`. | No — `J2`/`J3`. |
| `src/job_applications/repository.rs` | SQL: company, application and event statements, every one scoped by `user_id`. | No — `J2`/`J3`. |
| `src/job_applications/models.rs` | Persistence shapes: `Company`, `JobApplication`, `ApplicationEvent`, and the two enums. | **Yes.** |
| `src/job_applications/dto.rs` | Public contracts: `CompanyResponse`, `CompanyPage`, `ApplicationResponse`, `ApplicationPage`, `EventResponse`, `EventPage`, the create and update requests, the query types and the length limits. | No — `J4`. |

The feature depends on:

| Dependency | For | Exists |
| ---------- | --- | ------ |
| `src/api/middleware.rs` (`AuthUser`) | The caller's identity, and therefore the ownership predicate. | Yes. |
| `src/pagination.rs` | The list envelope and the clamp. | Yes. |
| `src/helpers/validation.rs` | The shared text validators. | Yes. |
| `src/error.rs` (`AppError`) | The one error envelope. | Yes. |
| `src/config/messages.toml` | Every client-facing string. | Yes. |

It will read `tasks` only to verify the optional next-action link. It reads no
other domain table.

### 2.1 `JobService` — planned

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

### 2.2 The repository statements — planned

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

### 2.3 The company delete rule — planned

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

The composite foreign key in §3.1 closes the remaining window: an application
inserted against a company that is being deleted fails the constraint instead
of becoming an orphan. Today the refusal exists only as that constraint; the
service half is not written.

### 2.4 The stage machine — planned

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
`screening`, or one could skip a stage. Why the edge table is not a database
constraint at all is §3.4.

## 3. Data ownership and the constraints that do real work

| Table | Owner column | Meaning | Exists |
| ----- | ------------ | ------- | ------ |
| `companies` | `user_id` → `users (id) ON DELETE CASCADE` | A company belongs to exactly one account. | Yes. |
| `job_applications` | `user_id` → `users (id) ON DELETE CASCADE`, `company_id` | An application belongs to one account and one company. | Yes. |
| `application_events` | `user_id`, `application_id` → `job_applications (id) ON DELETE CASCADE` | An event belongs to one application and one account. | Yes. |

**The ownership of the parent is enforced by a composite foreign key**, not
only by the service:

- `companies` carries a unique index on `(id, user_id)`
  (`companies_id_user_id_unique_idx`).
- `job_applications` carries `CONSTRAINT job_applications_company_fk FOREIGN KEY
  (company_id, user_id) REFERENCES companies (id, user_id) ON DELETE NO ACTION`.
- `application_events` carries `application_events_application_fk FOREIGN KEY
  (application_id, user_id) REFERENCES job_applications (id, user_id) ON DELETE
  CASCADE`, and its own `application_events_user_fk` to `users`.

That is stronger than the Phase 1 task model, where the join to `projects`
carries the ownership predicate. It costs one extra column in each parent's
unique index and it removes a class of bug entirely.

The unique company name is `companies_user_id_lower_name_unique_idx` on
`(user_id, LOWER(name))`, which is what makes `JOB-R01` a database guarantee
rather than a check-then-insert race. `companies_name_trimmed_chk` is what stops
`"Acme "` from being a second `"Acme"` the folded index cannot see.

The physical definition belongs in [`schema.md`](../../database/schema.md) and
the migration is its enforced copy. The plan's working sketch is in
[`plan.md`](../../engineering/runs/chain/plan.md) §5.2.

### 3.1 `job_applications_company_fk` makes a cross-owner application unrepresentable

`FOREIGN KEY (company_id, user_id) REFERENCES companies (id, user_id)` is not a
prettier way to write a one-column key. It is the difference between "the
service rejects a foreign company" and "a foreign company cannot be stored".

An application row carries **both** its owner and its company. Because the
referenced key is the pair, the row can only exist when the company's owner is
the application's owner. A writer that forgets the service check — a future
repository statement, a seeder branch, a hand-run `UPDATE` — gets `23503`
naming `job_applications_company_fk`, not a row that half-exists under two
accounts. `application_events` has the same shape against
`job_applications (id, user_id)`, so an event cannot be moved between accounts
either.

The price is a redundant `user_id` on each child and a unique index on each
parent. That is recorded in §10 as a limitation, because it is one: the owner is
stored twice and the constraint is what keeps the two copies equal. The gain is
that the client-facing `404` is not the only defence — the `404` is the polite
answer, and the key is why a bug cannot get past it.

### 3.2 `NO ACTION`, not `RESTRICT`, is what lets `DELETE FROM users` succeed

The company key is `ON DELETE NO ACTION`. The obvious choice, `RESTRICT`, would
look equivalent and is not:

| Action | A direct `DELETE FROM companies` while applications remain | `DELETE FROM users`, which cascades to the account's applications and companies in one statement |
| ------ | ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `RESTRICT` | Refused | **Refused.** The check fires as soon as the company row goes, before the cascade has removed the applications, so an account with one application could never be deleted. |
| `NO ACTION` | Refused | Succeeds. The check is deferred to the end of the statement, by which time the cascade has removed both rows and there is nothing left to object. |

`JOB-R06` needs the first column; the ownership cascade needs the second. Only
`NO ACTION` gives both, which is why it is written out rather than left to a
default. `tests/jobs_schema.rs::a_company_with_applications_cannot_be_deleted`
asserts both halves in one case: the direct delete is `23503`, and the account
delete then succeeds with zero companies and zero applications left.

This is the single most easily "fixed" line in the migration, and fixing it
breaks account deletion. A reviewer must not change it to `RESTRICT` for
symmetry with `tasks` or `notes`.

### 3.3 `next_action_task_id` cannot be owner-consistent in SQL

Every other ownership link in this feature is a composite key. This one cannot
be, and the reason is upstream of the feature: `tasks` has no `user_id`. Its
owner is inherited through `projects` (Phase 1, [`schema.md`](../../database/schema.md)
§6), so there is no `(id, user_id)` pair for a composite key to reference.

A `REFERENCES tasks (id)` key therefore says only that the task exists — not
whose it is. Two consequences:

- **`J3` must check ownership in the service**, with the join
  `tasks → projects → user_id`, before writing `next_action_task_id`. A foreign
  task answers `404`, like every other foreign row, not `403` and not a
  `23503`.
- **The test is the only evidence.** No constraint can fail for this rule, so
  `the_next_action_task_must_belong_to_the_caller` in
  [testing.md](testing.md) is what proves it, and removing the service check
  must make that test fail.

`ON DELETE SET NULL` is the part the database does own: deleting the task
clears the link and keeps the application. The partial index
`job_applications_next_action_task_id_idx` is the lookup that maintenance
performs.

### 3.4 The seven-stage machine is not a `CHECK`

A `CHECK` sees one row. The stage machine is a rule about a **transition** — it
needs the previous status as well as the new one — so `wishlist → interview`
cannot be expressed in a row constraint, and neither can "nothing leaves
`offer`".

What the database owns is the **vocabulary**: `job_application_status` is a
native enum, so the column can hold one of seven values and nothing else. A
direct insert of `'onsite'` fails with `22P02` and names no constraint, because
the type refused the token before any constraint ran. The feature documents
called this a `CHECK` until 2026-10-24; the implementation is the enum, which is
the stronger form of the same rule and the convention `task_status` already
uses.

What the service owns is the **edge table**: `ensure_transition`, under
`FOR UPDATE`, in the update transaction (§2.4). The failure modes are
different and both matter:

| Written by | Can store an unknown stage? | Can skip a stage? |
| ---------- | --------------------------- | ----------------- |
| A direct SQL statement | No — the enum refuses it. | **Yes.** The transition table is not in the database. |
| The API, once `J3` lands | No — the enum refuses it. | No — `ensure_transition` returns `409`. |

That gap is deliberate and is recorded again in §10. A trigger could close it,
and is rejected for the same reason the reimbursement lifecycle trigger was: it
would be a second copy of a rule the service already holds, on every write.

### 3.5 An event is corrected in place, with no `updated_at`

An event row is **edited in place**, not written once and left alone. Its four
content fields — `kind`, `occurred_at`, `summary` and `details` — may be
corrected by `PATCH /job-applications/{application_id}/events/{event_id}`, and
every one of them is optional in that request, like any other `PATCH` in the
API. The row deliberately carries **no `updated_at`** column and the response
has no such field: a timeline entry records **what the owner says happened**,
not when the row was last written. `created_at` is the only row timestamp the
API keeps, and `occurred_at` is the event's own time.

The decision is locked as `A-D1-1` (the orchestrator's Phase 2 intake ruling).
Two consequences follow for a change:

- The event update is a plain `UPDATE` scoped by `id`, `application_id` and
  `user_id`; it sets no timestamp column, and the response must not grow one.
- A reviewer must not "restore" an `updated_at` to the table or the DTO for
  consistency with the other resources. The absence is the decision.

`migrations/0005_job_applications.sql` carries no `updated_at` on
`application_events`, so the decision is in the applied schema, not only in a
sentence.

## 4. Invariants

The "in the tree" column says whether the rule is enforced by the schema that
exists today, or waits for the statements of `J2`-`J4`.

| # | Invariant | Enforced by | In the tree |
| - | --------- | ----------- | ----------- |
| 1 | Every row belongs to exactly one user, and a child's owner equals its parent's. | The non-null `user_id` and the two composite foreign keys. | **Yes.** |
| 2 | A company, application or event of another account is invisible and answers `404`. | `user_id = $n` in every statement. | No — no statement exists. |
| 3 | A company name is unique per owner, ignoring case, and stored trimmed. | `companies_user_id_lower_name_unique_idx` and `companies_name_trimmed_chk`. | **Yes.** |
| 4 | An application's stage is one of seven values. | The `job_application_status` enum type. | **Yes.** |
| 5 | Only the listed stage edges happen, and three stages are terminal. | `ensure_transition` under `FOR UPDATE`. | No — `J3`. |
| 6 | A company with applications cannot be deleted. | `ON DELETE NO ACTION` (the refusal) and the `NOT EXISTS` delete (the `409`). | The refusal: **yes**. The `409`: no. |
| 7 | Deleting an application deletes its events. | `ON DELETE CASCADE` on `application_events_application_fk`. | **Yes.** |
| 8 | An event's `occurred_at` is required and may be in the future. | `NOT NULL`, no past-only rule. | **Yes.** |
| 9 | The event order is total and stable. | `ORDER BY occurred_at DESC, id DESC`; the index supplies the leading order only. | The index: **yes**. The order: no. |
| 10 | The company order is total and stable. | `ORDER BY lower(name), name, id`; `companies_user_id_lower_name_name_id_idx` supplies it unsorted. | The index: **yes**. The order: no. |
| 11 | An application's salary range is one currency and ordered. | `job_applications_salary_range_chk` and `job_applications_salary_currency_required_chk`. | **Yes.** |
| 12 | A partial update leaves an omitted field untouched. | `Option` fields and `COALESCE`. | No. |
| 13 | A refused stage move writes nothing else from the same request. | The whole `PATCH` is one transaction. | No. |
| 14 | The next-action task belongs to the caller. | The service's `tasks → projects → user_id` check; SQL cannot express it (§3.3). | No — `J3`. |

## 5. Error handling — planned

Nothing in this table is reachable today, because no operation is mounted.

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

A `23503` that escapes to the client is a defect, not a `409`: the keys are the
last line of defence, and the service is supposed to answer first.

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

- There is no anonymous route in this feature; today there is no route at all.
- A company id, a task id and an application id in a **body** are verified
  against the caller's ownership before the write; a foreign id is `404`, not
  `403`. The company and application halves are additionally unrepresentable
  (§3.1); the task half is a service check only (§3.3).
- An event id is only meaningful **inside** its application: the path carries
  both, and the statement matches both, so an event cannot be moved between
  applications.
- The feature stores no credential and calls no external service. A job URL is
  stored as text and never fetched, for the same SSRF reason as the receipt
  URL ([`ADR-0009`](../../decisions/ADR-0009-receipt-references.md)).
- The stage of an application is personal data: it can reveal that the owner is
  looking for work. It is owner-scoped like every other field, and no listing
  aggregates across accounts.
- Today the only thing protecting a row is the database, and only against
  writers that use the repository the feature has not written yet. There is no
  route to abuse and no route to protect.

## 7. The request path — planned

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
| `src/api/routes.rs` | Registers the routes and the schemas in the OpenAPI document. | Yes, to be extended in `J4`. |
| `src/api/extract.rs` | The extractors that keep the error envelope intact. | Yes. |
| `src/api/middleware.rs` | `AuthUser`. | Yes. |
| `src/pagination.rs` | The list envelope and the clamp. | Yes. |
| `src/helpers/validation.rs` | The shared text validators. | Yes. |
| `src/lib.rs` | Declares `pub mod job_applications;`. | Yes. |
| `src/job_applications/mod.rs`, `models.rs` | The module root and the row types. | **Yes.** |
| `src/job_applications/{handlers,service,repository,dto}.rs` | The rest of the feature. | **No** — `J2`-`J4`. |
| `migrations/0005_job_applications.sql` | The three tables, the two enum types, the composite foreign keys and the indexes. | **Yes.** |
| `src/bin/atom-seed.rs` | The `companies`, `job_applications` and `application_events` seed rows. | Yes, extended by `C-J1`. |
| `tests/jobs_schema.rs` | The eleven schema cases. | **Yes.** |

## 9. Related decisions

- [`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md) —
  ownership as a SQL predicate.
- [`ADR-0007`](../../decisions/ADR-0007-money-representation.md) — the salary
  range's money format; why the models carry `String` rather than a decimal.
- [`ADR-0001`](../../decisions/ADR-0001-modular-monolith.md) — why this is a
  module.
- [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md) — why a stored
  URL is never fetched.
- [`project-truth.md`](../../project-truth.md) §6.1 — no text generation, no
  outbound AI call.

## 10. Limitations of this design

- **The composite foreign key needs a redundant `user_id` on the child.** It is
  the price of making the cross-owner link unrepresentable. A row's owner is
  stored twice (in the row and in its parent) and the constraint keeps the two
  equal.
- **The stage machine is a service rule.** A direct SQL update can set any
  value the enum admits, so it can skip a stage even though it cannot invent
  one. Only the seven-value vocabulary is enforced by the database; the edges
  are `J3`'s (§3.4).
- **The next-action task's owner is a service rule too**, and for a different
  reason: `tasks` has no `user_id` to make a composite key out of (§3.3). This
  is the one ownership link in the feature that the database cannot help with.
- **The feature has no behaviour yet.** The models exist and no repository
  reads them; the tables exist and no route reaches them. Every invariant above
  marked "yes" is a property of the schema, and a property of a schema is not a
  property of the product.
- **`FOR UPDATE` serialises stage changes on one application.** That is the
  point, and it is also the throughput ceiling; a personal workspace will never
  reach it.
- **The events list is a second query after the ownership check.** A
  single-statement join would be one round trip; the current shape is clearer
  and the race is benign (an empty page, never a foreign row).
- **The company search is `ILIKE '%term%'`.** It cannot use a B-tree index for
  a leading wildcard; the corpus is a personal company list, so this is
  acceptable. A trigram index is the answer if it ever is not.
- **The stage change is not an event.** The timeline does not record when a
  stage moved, only what the owner chose to record.

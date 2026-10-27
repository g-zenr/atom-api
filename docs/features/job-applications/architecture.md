# Job applications — architecture

> **Status:** Implemented
> **Last updated:** 2026-10-26
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

How the job application feature is built. The public contract is in
[api.md](api.md); the agreed requirements are in [specification.md](specification.md);
the evidence is in [testing.md](testing.md).

Chain `C-J1` (2026-10-24) added the schema: `migrations/0005_job_applications.sql`
and `src/job_applications/{mod,models}.rs`. Chains `C-J2`, `C-J3` and `C-J4`
added the endpoint layer in one wave: `dto.rs`, `repository.rs`, `service.rs`,
`handlers.rs`, the route registration, the state field and the OpenAPI entries.
`/api/v1/openapi.json` describes **54 operations today** (51 before the API-keys change), fourteen of them this
feature's; it described 48 when these chains landed. No migration changed: the schema `J1` wrote was sufficient, which is
the outcome the plan predicted and is worth stating because it is the exception.

`Implemented` means the code exists. It says nothing about a passing run: no
result is recorded in this document ([status.md](status.md) §1).

## 1. What exists today

| Item | Detail |
| ---- | ------ |
| `migrations/0005_job_applications.sql` | The two enum types, the three tables, every `CHECK`, both composite keys, the `ON DELETE` actions and every index of [`schema.md`](../../database/schema.md) §11.3–§11.5. |
| `src/job_applications/mod.rs` | Declares `dto`, `handlers`, `models`, `repository`, `service` and re-exports the public names. |
| `src/job_applications/models.rs` | `ApplicationStatus`, `ApplicationEventKind`, `Company`, `JobApplication`, `ApplicationEvent`. The two enums carry `serde(rename_all = "snake_case")` **and** `sqlx(type_name = …)`, so the Rust value and the database value cannot drift. `salary_min` and `salary_max` are `Option<String>`: `NUMERIC` travels as `::text` because `ADR-0007` forbids a decimal crate and this crate's SQLx feature set cannot decode it. |
| `src/job_applications/dto.rs` | The six response types, the six request types, the four query types and the length constants. Nullable columns are `Option<Option<T>>` on the update requests, so "absent" (keep) and `null` (clear) stay distinct. `EventResponse` has no `updated_at`. Money publishes a `format = "decimal"` and the four-decimal pattern, never `type: number`. |
| `src/job_applications/repository.rs` | Every SQL statement of the feature: six company, seven application, five event and three ownership-existence statements. No statement reads or writes a row without `user_id = $n`. |
| `src/job_applications/service.rs` | The transition table, the validators, and `JobService` with the fourteen use cases. |
| `src/job_applications/handlers.rs` | Fourteen handlers. HTTP extraction, status codes and serialization only; each carries its `#[utoipa::path]` and its documented status set. |
| `src/api/routes.rs` | Six `route` declarations (14 method/path pairs), the fourteen `ROUTE_TABLE` entries, the fourteen job schemas (twelve body types and the two enums) in `ApiDoc`, and the `companies` and `job-applications` tags. |
| `src/state.rs` | `AppState::job_applications`, built as `JobService::new(pool, messages, pagination)`. |
| `config/messages.toml`, `src/config/messages.rs` | Every client-facing string of the feature. |
| `tests/job_applications_api.rs` | 51 HTTP-level cases. [testing.md](testing.md) §2. |
| `tests/jobs_schema.rs`, `tests/seeder.rs` | Eleven schema cases and one seeder case, unchanged by this chain. |

## 2. Responsibilities

| Module | Responsibility |
| ------ | -------------- |
| `handlers.rs` | Extraction, status codes, response serialization. No business rule. |
| `service.rs` | Validation, the transition table, the create rule, the ownership explanations, the salary triple, the trims. No SQL. |
| `repository.rs` | SQL only: every statement, every bound parameter, every ownership predicate. |
| `models.rs` | Persistence shapes and the two enums. |
| `dto.rs` | The published shapes and their OpenAPI annotations. |

### 2.1 `JobService` as built

| Method | What it does |
| ------ | ------------ |
| `list_companies(owner, query, pagination)` | Normalises the search, clamps the page, returns `Page<CompanyResponse>` ordered by the SQL. |
| `create_company(owner, request)` | Validates, inserts, and maps `CompanyWrite::DuplicateName` to `409`. |
| `get_company(owner, company_id)` | One owner-scoped read; a miss is `404`. |
| `update_company(owner, company_id, request)` | Validates the present fields, applies the `COALESCE`/clear patch, maps a collision to `409`. |
| `delete_company(owner, company_id)` | One guarded delete; on zero rows, one read decides `409` or `404`. |
| `list_applications(owner, query, pagination)` | Validates the range and the search, passes five filters to the statement. |
| `create_application(owner, request)` | `ensure_initial_stage`, validates, inserts through the company/task predicate, names the refusal. |
| `get_application(owner, application_id)` | One owner-scoped read; a miss is `404`. |
| `update_application(owner, application_id, request)` | Validates, resolves the salary triple against the stored one when touched, derives `permitted_sources`, runs the guarded update, explains a zero-row result. |
| `delete_application(owner, application_id)` | One owner-scoped delete; the events go by the schema's cascade. |
| `list_events(owner, application_id, pagination)` | One page statement plus a count; the empty page is checked against `EVENTS_EXISTS_WITH_TOTAL` so a missing application is still `404`. |
| `create_event(owner, application_id, request)` | Validates, inserts through `SELECT … FROM job_applications`, names the `404`. |
| `update_event(owner, application_id, event_id, request)` | Validates the present fields, updates in place, never writes a timestamp. |
| `delete_event(owner, application_id, event_id)` | One delete scoped by id, application and owner. |

`owner` is the authenticated `user_id` and is an argument of every method. It
never comes from a body.

It reads `tasks` and `projects` only to verify the optional next-action link,
and through their owner column. It reads no other domain table.

### 2.2 The list query types

`GET /companies` and `GET /job-applications` each parse the query string
**once** into `CompaniesListQuery` / `ApplicationsListQuery`, whose
`into_parts()` splits the filter half (`CompanyQuery` / `ApplicationQuery`, the
`IntoParams` type the document publishes) from the pagination half
(`PaginationQuery`). The fields are written out rather than composed with
`#[serde(flatten)]`, because `serde_urlencoded` hands a flattened `Option<i64>` a
string and the failure would appear at run time on a real request. Declaring
`limit` and `offset` in both halves would publish them twice; the contract test
asserts each list operation publishes its set once
(`tests/openapi_contract.rs::each_list_endpoint_publishes_its_filter_and_pagination_parameters_once`).

## 3. The rules that do real work

### 3.1 The stage machine: one table, one guarded `UPDATE`

`ALLOWED_MOVES` is the **only** definition of the edge table: twelve
`(from, to)` pairs. Everything else derives from it.

**`ensure_transition(current, next) -> Option<Transition>`**:

| Input | Answer | Meaning |
| ----- | ------ | ------- |
| `current == next` | `Some(Transition::NoOp)` | The published no-op: `200`, nothing changes. |
| the pair is one of the twelve | `Some(Transition::Move)` | The stage changes. |
| anything else | `None` | `409 conflict`. |

It is a `match` over a pair table, not a rank comparison, because `rejected`
and `withdrawn` are reachable from four different stages and a numeric rank
cannot express that. Of the 49 ordered pairs, 12 are moves, 7 are no-ops and 30
are refused.

**`permitted_sources(next)`** is `next` itself plus every `from` of an edge into
`next`. It is the array the write binds, so the predicate that decides the write
and the function that explains the refusal are two readings of the same table.

**The write is one statement.** `UPDATE_APPLICATION_OWNED` carries:

```sql
UPDATE job_applications
   SET …
 WHERE id = $1 AND user_id = $2
   AND ($7::job_application_status IS NULL OR status = ANY($20::job_application_status[]))
   AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM companies c
                                     WHERE c.id = $3 AND c.user_id = $2))
   AND ($18::uuid IS NULL OR EXISTS (SELECT 1 FROM tasks t
                                     JOIN projects p ON p.id = t.project_id
                                     WHERE t.id = $18 AND p.user_id = $2))
RETURNING …
```

`$20` is `permitted_sources(requested)`; `$7` is the requested stage; both are
`NULL` when the request carries no stage. Every other column is written by a
`COALESCE`/`CASE` pair that leaves an omitted field alone. The statement has
**no** preceding read and **no** `FOR UPDATE`.

**This corrects the earlier plan.** [status.md](status.md) and this document
previously described `BEGIN; SELECT status … FOR UPDATE; ensure_transition;
UPDATE; COMMIT`. That shape is not what was built, and a reviewer should not
"restore" it:

- a read followed by a write is itself the race it was meant to close: two
  requests can both see `applied`, both pass the check, and the second writes a
  stage the first has already left;
- the predicate is part of the statement, so PostgreSQL re-checks it against the
  row version the other writer committed. The winner is the one whose statement
  matched; the loser matches no row, and the service answers `409`. No lock is
  needed for that, and none is taken;
- one statement also gives the transaction property for free: a refused move
  cannot write the role title in the same body, because the same statement
  carries both and it matched nothing.

**A zero-row result is ambiguous**, and the service explains it on the error
path, in this order (`explain_update_refusal`):

| Read | Answer |
| ---- | ------ |
| The application is absent or foreign | `404 not_found` |
| A `company_id` in the body is foreign or absent | `404 not_found` |
| A `next_action_task_id` in the body is foreign or absent | `404 not_found` |
| Anything else — the move is refused, or a concurrent move landed first | `409 conflict` |

The explanation read never decides whether to write; it only names why the
write did nothing.

**`updated_at` is conditional.** The application statement stamps
`clock_timestamp()` only when the request touches a column other than `status`,
or when the requested stage differs from the stored one. A `PATCH` whose only
field is the stage the row already carries is the published no-op and leaves the
row byte-identical. The company statement, by contrast, stamps on every update,
because a company has no no-op concept.

### 3.2 The create fixes the stage at birth

`ensure_initial_stage(requested)` accepts `None` and `Some(Wishlist)` and
returns `Wishlist`; every other value is `422 validation_failed` naming `status`.
That is a **different rule** from the transition table and must not be confused
with it:

| Request | Answer |
| ------- | ------ |
| `status` omitted | `201`, stored as `wishlist` |
| `"status": "wishlist"` | `201`, stored as `wishlist` |
| any of the other six stages | `422`, naming `status` |
| a later stage reached by `PATCH` | governed by §3.1: `200` or `409` |

The insert statement carries the literal `'wishlist'::job_application_status` in
its `SELECT` list, so even a future caller that forgot the service check could
not create another stage through this statement.

### 3.3 The company delete: `AND NOT EXISTS`, and nothing deleted to make it succeed

`JOB-R06` refuses to delete a company that still has applications. A
`SELECT COUNT(*)` followed by a `DELETE` is a race: an application can be
inserted between the two statements. The delete is therefore one statement:

```sql
DELETE FROM companies c
 WHERE c.id = $1 AND c.user_id = $2
   AND NOT EXISTS (SELECT 1 FROM job_applications a
                    WHERE a.company_id = $1 AND a.user_id = $2)
```

**Nothing is deleted to make it succeed.** The applications are records the
client did not ask to delete; a cascade here would destroy job history silently,
which is why the key is `NO ACTION` (§4.2) and why the service's answer is a
refusal rather than a cleanup.

Zero rows is ambiguous — absent, foreign, or still in use — and the service runs
**one** owner-scoped read, `company_is_owned`:

| Read | Answer |
| ---- | ------ |
| The company belongs to the caller | `409 conflict` — it still has applications |
| It does not | `404 not_found` |

The read is only the explanation. A company that is deleted between the
statement and the read answers `404`, which is the right answer for a company
that is now gone; a company that gains an application in that window answers
`409`, which is the right answer too. An application inserted between the check
and an actual delete would have been the bug; the composite key (§4.1) turns it
into a `23503` instead of an orphan.

### 3.4 `next_action_task_id`: the check SQL cannot do, and its `404`

Every other ownership link in this feature is a composite key. This one cannot
be: `tasks` has no `user_id`, so there is no `(id, user_id)` pair to reference
(§4.4). A `REFERENCES tasks (id)` key says only that the task exists, not whose
it is.

The rule is enforced by a predicate of **both** statements that can write the
column:

```sql
($12::uuid IS NULL OR EXISTS (SELECT 1 FROM tasks t
                               JOIN projects p ON p.id = t.project_id
                               WHERE t.id = $12::uuid AND p.user_id = $2))
```

- On the insert it is part of `INSERT_APPLICATION`, whose source is
  `SELECT … FROM companies`; zero rows is explained by `explain_insert_refusal`,
  which checks the company first and the task second, and answers `404 not_found`
  for either.
- On the update it is part of the `WHERE` of §3.1, and
  `explain_update_refusal` names it as the third `404`.

A foreign task, another account's task, and a task that does not exist are the
same answer: **`404`**, never `403` and never a `23503`. `{"next_action_task_id":
null}` clears the link. `ON DELETE SET NULL` is the part the database owns:
deleting the task clears the link and keeps the application, which is the
opposite of the application cascade and is deliberate — the reminder is not the
record.

The test is the **only** possible evidence for the ownership rule, because no
constraint can fail for it:
`tests/job_applications_api.rs::the_next_action_task_must_belong_to_the_caller`.

### 3.5 Events: the order, the empty page, and the in-place `PATCH`

**Order.** The listing is `ORDER BY occurred_at DESC, id DESC`. The index
`application_events_application_id_occurred_at_idx` supplies the leading term
only; `id DESC` is the statement's job, and it is what makes the order total.
There is no filter and no client-selectable sort.

**The empty page.** `list_events_owned` runs the page statement, and only when
it returns nothing does it run `EVENTS_EXISTS_WITH_TOTAL`. That keeps the two cases
apart — an application with no events is `200` with an empty page, a missing or
foreign application is `404` — without making every page pay for the check.

**In place, with no `updated_at`.** `kind`, `occurred_at`, `summary` and
`details` are all optional in `PATCH`, and the statement is a plain
`UPDATE` scoped by `id`, `application_id` and `user_id`:

```sql
UPDATE application_events
   SET kind = COALESCE($4::application_event_kind, kind), …
 WHERE id = $1 AND application_id = $2 AND user_id = $3
```

It writes no timestamp: the table has no `updated_at`, and `EventResponse` has
no such field. `created_at` is the only row timestamp, and `occurred_at` is the
event's own time. That is decision `A-D1-1`, and it is now asserted twice: the
table is in `0005`, and
`tests/openapi_contract.rs::the_event_schemas_publish_no_updated_at` pins the
three event schemas.

Because the statement matches both identifiers, an event of the caller's
**other** application matches no row and is `404`: an event id is only meaningful
inside its application.

### 3.6 The salary triple on a `PATCH`

The three salary columns are one value: `min`, `max` and the currency are only
meaningful together. A `PATCH` that touches any of them
(`touches_salary()`) therefore:

1. reads the stored triple (`find_salary_owned`, owner-scoped);
2. resolves the request against it — omitted keeps, `null` clears, a value
   replaces; clearing both bounds clears the currency; a currency with no bound
   left is a `422`, not a silent no-op;
3. validates the resolved triple whole — currency present when a bound is, and
   `min <= max` compared as strings by `compare_money`, never parsed;
4. writes the resolved triple whole, in the same statement as the rest of the
   patch.

Writing the resolved triple, rather than a `COALESCE` per column, is what keeps
the `CHECK` constraints from ever firing on a combination the service did not
validate: the client sees a `422` naming the field or a committed consistent
row, never a `23514` that would surface as a `500`.

The read is the only one before a write in the module, and it never decides
whether the row exists — a miss leaves the triple unresolved and the guarded
`UPDATE` answers `404`.

### 3.7 Trimming: what the schema does not enforce

The length rules are mirrored by `CHECK` constraints and measured the same way
on both sides: `char_len`, which counts characters as `CHAR_LENGTH` does. The
**trim** rules are not all mirrored, and the difference matters to whoever
writes SQL against these tables next.

| Column | Service rule | Schema enforces it? |
| ------ | ------------ | ------------------- |
| `companies.name` | trimmed, 1–200, casing preserved | **Yes** — `companies_name_length_chk` and `companies_name_trimmed_chk` |
| `job_applications.role_title` | trimmed, 1–200 | **No trim check.** Only the length is a constraint; a direct write can store `" Engineer "` |
| `job_applications.source` | trimmed, 1–64, no blank spelling | **Yes** — `job_applications_source_chk` requires the value to equal its own `BTRIM` |
| `application_events.summary` | trimmed, 1–200 | **No trim check.** Only the length is a constraint |
| `companies.notes`, `job_applications.description`, `application_events.details` | bounded, **kept verbatim** | Length only; spaces are stored as sent |
| `companies.website`, `job_applications.job_url` | trimmed, then an absolute `http`/`https` with an authority, at most 2048 | Scheme and length are a `CHECK`; the authority check is the service's |
| `job_applications.salary_currency` | trimmed and upper-cased, three ASCII letters | **Yes** — `CHAR(3)` and `job_applications_salary_currency_chk` |
| the `search` term | trimmed; empty means no filter | no column, no constraint |

So `role_title` and `summary` are the two places where the schema is weaker than
the API: a browser cannot store an untrimmed value, a migration or a hand-run
`INSERT` can. That is recorded as a limitation in §11, not as a defect — the
alternative would be a `CHECK` the service already duplicates.

## 4. Data ownership and the constraints that do real work

| Table | Owner column | Meaning |
| ----- | ------------ | ------- |
| `companies` | `user_id` → `users (id) ON DELETE CASCADE` | A company belongs to exactly one account. |
| `job_applications` | `user_id` → `users (id) ON DELETE CASCADE`, `company_id` | An application belongs to one account and one company. |
| `application_events` | `user_id`, `application_id` → `job_applications (id) ON DELETE CASCADE` | An event belongs to one application and one account. |

**The ownership of the parent is enforced by a composite foreign key**, not only
by the service:

- `companies` carries a unique index on `(id, user_id)`
  (`companies_id_user_id_unique_idx`).
- `job_applications` carries `job_applications_company_fk FOREIGN KEY
  (company_id, user_id) REFERENCES companies (id, user_id) ON DELETE NO ACTION`.
- `application_events` carries `application_events_application_fk FOREIGN KEY
  (application_id, user_id) REFERENCES job_applications (id, user_id) ON DELETE
  CASCADE`, and its own `application_events_user_fk` to `users`.

### 4.1 The composite key makes a cross-owner application unrepresentable

`FOREIGN KEY (company_id, user_id) REFERENCES companies (id, user_id)` is not a
prettier one-column key. It is the difference between "the service rejects a
foreign company" and "a foreign company cannot be stored".

An application row carries **both** its owner and its company. Because the
referenced key is the pair, the row can only exist when the company's owner is
the application's owner. A writer that forgets the service predicate — a future
statement, a seeder branch, a hand-run `UPDATE` — gets `23503` naming
`job_applications_company_fk`, not a row that half-exists under two accounts.
`application_events` has the same shape against `job_applications (id, user_id)`.

The price is a redundant `user_id` on each child and a unique index on each
parent. The gain is that the client-facing `404` is not the only defence: the
`404` is the polite answer, and the key is why a bug cannot get past it.

### 4.2 `NO ACTION`, not `RESTRICT`, is what lets `DELETE FROM users` succeed

| Action | A direct `DELETE FROM companies` while applications remain | `DELETE FROM users`, which cascades to the account's applications and companies in one statement |
| ------ | ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `RESTRICT` | Refused | **Refused.** The check fires as soon as the company row goes, before the cascade has removed the applications, so an account with one application could never be deleted. |
| `NO ACTION` | Refused | Succeeds. The check is deferred to the end of the statement, by which time the cascade has removed both rows. |

`JOB-R06` needs the first column; the ownership cascade needs the second. Only
`NO ACTION` gives both, which is why it is written out rather than left to a
default. `tests/jobs_schema.rs::a_company_with_applications_cannot_be_deleted`
asserts both halves in one case: the direct delete is `23503`, and the account
delete then succeeds with zero companies and zero applications left.

This is the single most easily "fixed" line in the migration, and fixing it
breaks account deletion. A reviewer must not change it to `RESTRICT` for
symmetry with `tasks` or `notes`.

### 4.3 The seven-stage machine is not a `CHECK`

A `CHECK` sees one row. The stage machine is a rule about a **transition** — it
needs the previous status as well as the new one — so `wishlist → interview`
cannot be expressed in a row constraint, and neither can "nothing leaves
`offer`".

What the database owns is the **vocabulary**: `job_application_status` is a
native enum, so the column can hold one of seven values and nothing else. A
direct insert of `'onsite'` fails with `22P02` and names no constraint, because
the type refused the token before any constraint ran.

What the service owns is the **edge table**, through the one guarded statement
of §3.1. The failure modes are different and both matter:

| Written by | Can store an unknown stage? | Can skip a stage? |
| ---------- | --------------------------- | ----------------- |
| A direct SQL statement | No — the enum refuses it. | **Yes.** The transition table is not in the database. |
| The API | No — the enum and the body both refuse it. | No — the `status = ANY(…)` predicate matches no row and the service answers `409`. |

A trigger could close the direct-SQL gap and is rejected for the same reason the
reimbursement lifecycle trigger was: it would be a second copy of a rule the
service already holds, on every write.

### 4.4 Why `next_action_task_id` cannot be owner-consistent in SQL

`tasks` has no `user_id`; its owner is inherited through `projects`
([`schema.md`](../../database/schema.md) §6). There is therefore no
`(id, user_id)` pair for a composite key to reference, and the single-column key
says only that the task exists. The enforcement is the statement predicate of
§3.4, and the evidence is the API case. `ON DELETE SET NULL` and the partial
index `job_applications_next_action_task_id_idx` are the parts the database does
own.

## 5. Invariants

Every invariant is now enforced by code in the tree. The last column names the
layer, because "a schema property is not a product property":

| # | Invariant | Enforced by | Layer |
| - | --------- | ----------- | ----- |
| 1 | Every row belongs to exactly one user, and a child's owner equals its parent's. | The non-null `user_id` and the two composite foreign keys. | Schema, plus `user_id = $n` in every statement. |
| 2 | A company, application or event of another account is invisible and answers `404`. | `user_id = $n` in every statement; the composite keys make the alternative unrepresentable. | SQL + service. |
| 3 | A company name is unique per owner, ignoring case, and stored trimmed. | `companies_user_id_lower_name_unique_idx`, `companies_name_trimmed_chk`, and the `23505` mapping to `409`. | Schema + service. |
| 4 | An application's stage is one of seven values. | The `job_application_status` enum type, on the column and in the request body. | Schema + serde. |
| 5 | Only the listed stage edges happen, and three stages are terminal. | `ALLOWED_MOVES` derived into the `status = ANY(…)` predicate of one `UPDATE`. | Service + SQL, in one statement. |
| 6 | A company with applications cannot be deleted, and nothing is deleted instead. | `AND NOT EXISTS` in the delete statement; `ON DELETE NO ACTION` behind it; the one read that chooses `409`/`404`. | SQL + service. |
| 7 | Deleting an application deletes its events. | `ON DELETE CASCADE` on `application_events_application_fk`, in the same statement. | Schema. |
| 8 | An event's `occurred_at` is required and may be in the future. | `NOT NULL`, no past-only rule, and a `422` only for a value that is not an instant. | Schema + serde. |
| 9 | The event order is total and stable. | `ORDER BY occurred_at DESC, id DESC`; the index supplies the leading term only. | SQL. |
| 10 | The company order is total and stable. | `ORDER BY LOWER(name), name, id`. | SQL. |
| 11 | The application order is total and stable. | `ORDER BY created_at DESC, id DESC`. | SQL. |
| 12 | An application's salary range is one currency and ordered. | `validate_salary_range` / `resolve_salary` and the four salary `CHECK`s; the resolved triple is written whole. | Service + schema. |
| 13 | A partial update leaves an omitted field untouched, and `null` clears a nullable one. | `Option<Option<T>>` in the DTO, `(clear, value)` pairs into the statement, `COALESCE`/`CASE` in SQL. | DTO + SQL. |
| 14 | A refused stage move writes nothing else from the same request. | The whole patch is one `UPDATE` whose `WHERE` excludes the row. | SQL. |
| 15 | The next-action task belongs to the caller. | The `tasks → projects → user_id` predicate in both writing statements; the API case is the only evidence. | Service + SQL. |
| 16 | An event is corrected in place and gains no `updated_at`. | No column, no timestamp in the statement, no field in `EventResponse`. | Schema + SQL + DTO. |

## 6. Error handling

Every row below is reachable now, and every one is the standard `AppError`
envelope.

| Situation | Error | Status |
| --------- | ----- | ------ |
| A field breaks a rule in [specification.md](specification.md) §6.5 | `validation_failed` naming the field | `422` |
| A duplicate company name for one owner | `conflict` | `409` |
| A stage move the machine does not allow | `conflict` | `409` |
| Deleting a company that still has applications | `conflict` | `409` |
| The row does not exist, or belongs to another account | `not_found` | `404` |
| The company, task or application referenced in a body is foreign | `not_found` | `404` |
| The event id belongs to another application | `not_found` | `404` |
| The body is not JSON, or a path parameter is not a UUID | `bad_request` | `400` |
| A query parameter cannot be parsed, or is outside its enum | `bad_request` | `400` |
| No or invalid credentials | `unauthorized` | `401` |
| A body over the limit | `payload_too_large` | `413` |
| The database failed | `database_error` | `500` |

Two mappings are worth naming because they are the ones that could have gone
wrong:

- A `23505` on `companies_user_id_lower_name_unique_idx` is **not** an error: the
  repository turns it into `CompanyWrite::DuplicateName` and the client sees
  `409`. Every other database error stays a `500`.
- A `23503` that escapes to the client would be a defect, not a `409`: the keys
  are the last line of defence and the service is supposed to answer first. The
  predicates of §3.1, §3.3 and §3.4 exist so it cannot.

## 7. The security boundary

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

- There is no anonymous route in this feature: all fourteen require `AuthUser`,
  and `tests/job_applications_api.rs::every_job_application_operation_requires_a_credential`
  asserts the `401` on each.
- A company id, a task id and an application id in a **body** are verified
  against the caller's ownership inside the statement that writes; a foreign id
  is `404`, not `403`. The company and application halves are additionally
  unrepresentable (§4.1); the task half is the predicate of §3.4.
- An event id is only meaningful **inside** its application: the path carries
  both, the statement matches both, and an event cannot be moved between
  applications.
- The feature stores no credential and calls no external service. A job URL is
  stored as text and never fetched, for the same SSRF reason as the receipt URL
  ([`ADR-0009`](../../decisions/ADR-0009-receipt-references.md)).
- The stage of an application is personal data: it can reveal that the owner is
  looking for work. It is owner-scoped like every other field, no listing
  aggregates across accounts, and a `DELETE` of a foreign company answers `404`
  before the `409` question is asked, so the `409` cannot leak that the company
  exists.

## 8. The request path

### 8.1 Changing a stage

```text
PATCH /api/v1/job-applications/{application_id}
  {"status": "screening"}
  │
  ├─ AuthUser → user_id
  ├─ Path → application_id          not a UUID → 400
  ├─ Json<UpdateApplicationRequest> not JSON → 400
  ├─ JobService::update_application
  │    ├─ validate the present fields                       invalid → 422
  │    ├─ if a salary field is present: find_salary_owned    resolves the triple
  │    ├─ sources = permitted_sources(requested)
  │    └─ UPDATE … WHERE id AND user_id AND status = ANY(sources)
  │         ├─ one row → 200 ApplicationResponse
  │         └─ no row  → explain_update_refusal
  │                       ├─ application absent/foreign → 404
  │                       ├─ company foreign           → 404
  │                       ├─ task foreign              → 404
  │                       └─ otherwise                 → 409
  └─ 200 OK, ApplicationResponse
```

A `PATCH` that carries `status` **and** other fields is one statement, so a
refused move writes none of the other fields.

### 8.2 Listing events

```text
GET /api/v1/job-applications/{application_id}/events?limit=20&offset=0
  │
  ├─ AuthUser → user_id
  ├─ SELECT … WHERE application_id = $1 AND user_id = $2
  │         ORDER BY occurred_at DESC, id DESC LIMIT $3 OFFSET $4
  ├─ if the page is empty: SELECT EXISTS (… same predicate)   no row → 404
  ├─ SELECT COUNT(*) … the same predicate
  └─ 200 OK, EventPage
```

An application deleted between the page statement and the count yields an empty
page or a `404`, never a foreign page: the count carries the same predicate.

### 8.3 Deleting a company

```text
DELETE /api/v1/companies/{company_id}
  │
  ├─ AuthUser → user_id
  ├─ DELETE FROM companies WHERE id AND user_id AND NOT EXISTS (applications)
  │    ├─ one row → 204
  │    └─ no row  → SELECT EXISTS (company is the caller's)
  │                  ├─ yes → 409 conflict (it still has applications)
  │                  └─ no  → 404 not_found
```

## 9. Related files

| File | Role |
| ---- | ---- |
| `src/api/routes.rs` | Registers the six route declarations (14 method/path pairs), the fourteen `ROUTE_TABLE` entries and the job schemas. |
| `src/api/extract.rs` | The extractors that keep the error envelope intact. |
| `src/api/middleware.rs` | `AuthUser`. |
| `src/pagination.rs` | The list envelope and the clamp. |
| `src/helpers/validation.rs` | `required_text`, `bounded_text`, `validate_money`, `validate_search`. |
| `src/state.rs` | Builds `JobService` into `AppState`. |
| `src/job_applications/mod.rs`, `models.rs` | The module root and the row types. |
| `src/job_applications/{dto,repository,service,handlers}.rs` | The rest of the feature. |
| `migrations/0005_job_applications.sql` | The three tables, the two enum types, the composite foreign keys and the indexes. |
| `src/bin/atom-seed.rs` | The `companies`, `job_applications` and `application_events` seed rows. |
| `tests/job_applications_api.rs` | The 51 endpoint cases. |
| `tests/jobs_schema.rs` | The eleven schema cases. |

## 10. Related decisions

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

## 11. Limitations of this design

- **The composite foreign key needs a redundant `user_id` on the child.** It is
  the price of making the cross-owner link unrepresentable. A row's owner is
  stored twice and the constraint keeps the two equal.
- **The stage machine is a service rule.** A direct SQL update can set any
  value the enum admits, so it can skip a stage even though it cannot invent
  one. Only the seven-value vocabulary is enforced by the database.
- **The next-action task's owner is a service rule too**, and for a different
  reason: `tasks` has no `user_id` to make a composite key out of. This is the
  one ownership link in the feature that the database cannot help with.
- **`role_title` and event `summary` are trimmed only by the service.** The
  columns enforce the length, not the trim, so a direct write can store an
  untrimmed value. §3.7 lists every column and which half enforces what.
- **The stage change does not serialise by locking.** Two concurrent moves on
  one application are resolved by the single statement's predicate: one wins,
  the other matches no row and is answered `409`. That is the design, and it is
  also why there is no `FOR UPDATE` to tune; the throughput ceiling is the row
  lock the `UPDATE` itself takes.
- **The events list is a second query when the page is empty.** A
  single-statement join would be one round trip; the current shape is clearer,
  and the race is benign (a `404` or an empty page, never a foreign row).
- **The company search is `ILIKE '%term%'`.** It cannot use a B-tree index for a
  leading wildcard; the corpus is a personal company list, so this is
  acceptable. A trigram index is the answer if it ever is not.
- **The application listing is one statement for filtered and unfiltered.** With
  four filters, no single listing index could serve the filtered shape anyway,
  so a second copy of the statement would buy nothing.
- **The flow layer is written but is not a substitute for a review.**
  `tests/job_applications_flow.rs` runs one journey from lead to offer and one
  ownership sweep, complementing the 51 per-operation cases in
  `tests/job_applications_api.rs`. It is a composition check: it proves the
  fourteen operations agree about state, not that each is individually correct.
  The feature is `Implemented`, not `Verified`; [testing.md](testing.md) §6 lists
  the rest of the gaps.

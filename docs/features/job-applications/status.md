**Status:** Accepted

# Job applications — status

> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

## 1. The status

The feature is **`Accepted`**. The requirements `JOB-R01`…`R09` are agreed in
[`project-truth.md`](../../project-truth.md) §18.2, with acceptance criteria.
Chain `C-J1` built the **schema half** on 2026-10-24; the **behaviour half** has
not been started.

Exactly what is now true: `migrations/0005_job_applications.sql` creates the two
enum types `job_application_status` and `application_event_kind` and the three
tables `companies`, `job_applications` and `application_events`, with every
constraint, index and `ON DELETE` action that
[`schema.md`](../../database/schema.md) §11.3–§11.5 defines;
`src/job_applications/` holds `mod.rs` and `models.rs` only — the two enums and
the `Company`, `JobApplication` and `ApplicationEvent` row structs — and is
declared in `src/lib.rs`; and the seeder writes companies, applications and
events (the `dev` profile inserts 20 applications, 12 companies and 50 events;
`minimal` 1 / 5 / 1 and `demo` 40 / 18 / 100), with `--reset` deleting the three
tables by `user_id`.

**No endpoint exists.** There is no handler, service, repository or DTO, no
route is registered, and `/api/v1/openapi.json` still lists **34 operations**.
Nothing in this feature is therefore `Implemented` or `Verified` at the API
level: a table, a row struct and a seeded row are not a behaviour.

The schema is not `Verified` either. `tests/jobs_schema.rs` holds eleven cases
and `tests/seeder.rs` one case for this feature, but no run of them is recorded
in this document, and this document does not claim a measurement it did not
make.

### 1.1 The requirements, half by half

Every requirement is `Accepted` as a whole, because the behaviour half is what
decides whether a requirement is built. The two right-hand columns separate the
halves: "exists" there means the database object is in
`migrations/0005_job_applications.sql`; it never means that a request can reach
it.

| Requirement | State | The schema half — what exists | The behaviour half — what is missing |
| ----------- | ----- | ----------------------------- | ------------------------------------ |
| `JOB-R01` A company name unique per owner, case-insensitively | `Accepted` | `companies` with `companies_name_length_chk`, `companies_name_trimmed_chk` and the functional unique index `companies_user_id_lower_name_unique_idx (user_id, LOWER(name))`: a duplicate for one owner, in any casing, is a `23505` whatever writes it. | The company create, read, update and delete endpoints, the `409 conflict` on a duplicate, the `422` for the name, website and notes rules, and the `404` for a foreign company. |
| `JOB-R02` An application belongs to one company of the same owner, with a role title and one of seven stages | `Accepted` | `job_applications` with `company_id NOT NULL` and `job_applications_company_fk FOREIGN KEY (company_id, user_id) REFERENCES companies (id, user_id) ON DELETE NO ACTION`, so an application whose owner is not its company's owner cannot be stored; `job_applications_role_title_length_chk`; `status job_application_status NOT NULL DEFAULT 'wishlist'`. | The endpoints. "A create accepts only `wishlist` or an omitted stage" is a **service** rule, not a database one: a direct insert can name any value the enum admits. So are the `422` on an unknown stage, the `201`, and the `404` for a foreign company. |
| `JOB-R03` An optional salary range, one currency, `min <= max`, money as `REIMB-R07` | `Accepted` | `salary_min` and `salary_max` as `NUMERIC(19,4)`, `salary_currency CHAR(3)`, and four named checks: `job_applications_salary_non_negative_chk`, `job_applications_salary_range_chk`, `job_applications_salary_currency_required_chk` and `job_applications_salary_currency_chk` (upper-case `[A-Z]{3}`). | The four-decimal **string** contract on the wire, the `::text` projection, the `422` naming the field, and the upper-casing of the code. The columns store four decimals; nothing in the feature serializes them yet. |
| `JOB-R04` The seven-stage machine, three terminal stages | `Accepted` | The **vocabulary only**: the `job_application_status` enum makes any stage outside the seven unrepresentable. It does **not** express a transition — the edge table is not in the database ([architecture.md](architecture.md) §3.4). | `ensure_transition`, the `409 conflict` on a skipped, backwards or terminal move, the `200` no-op on the current stage, and the one transaction that makes a refused `PATCH` write nothing else. |
| `JOB-R05` Events belong to an application, ordered `occurred_at DESC, id DESC` | `Accepted` | `application_events` with `application_events_application_fk FOREIGN KEY (application_id, user_id) … ON DELETE CASCADE`, `occurred_at TIMESTAMPTZ NOT NULL` with no past-only rule, `application_events_summary_length_chk`, and the index `application_events_application_id_occurred_at_idx (application_id, occurred_at DESC)`. The index supplies the leading order only; the `id DESC` tiebreaker is the query's job. | The event endpoints, the ordered listing, the pagination, and the `404` for an event reached under another application. |
| `JOB-R06` Deleting an application deletes its events; a company with applications cannot be deleted | `Accepted` | Both halves are in the schema. The event cascade is `ON DELETE CASCADE` on `application_events_application_fk`. The company refusal is `ON DELETE NO ACTION` on `job_applications_company_fk`, which refuses a direct `DELETE FROM companies` while an application remains. | The `409 conflict` mapping, and the two cheap reads that choose between `404` and `409` after a delete affects zero rows. Today the refusal reaches a caller only as `23503` from a direct statement; no request produces it. |
| `JOB-R07` One optional next-action task, cleared when the task is deleted | `Accepted` | `next_action_task_id` with `job_applications_next_action_task_fk REFERENCES tasks (id) ON DELETE SET NULL`, and the partial index `job_applications_next_action_task_id_idx`. The **owner-consistency** half is impossible in SQL: `tasks` has no `user_id`, so there is no column pair to reference. | The service check `tasks → projects → user_id` before a write, in `J3`, and the `404` it answers for a foreign task. Nothing enforces task ownership today. |
| `JOB-R08` No text generation, no outbound AI call | `Accepted` | Nothing, and nothing to build: the requirement forbids a column as much as an endpoint. `src/job_applications/models.rs` imports no HTTP client. | The absence of a generating operation, which trivially holds at 34 operations because this feature publishes none, and cannot be asserted by a contract test until the fourteen operations exist. |
| `JOB-R09` Owner-scoped reads and writes, a foreign row is `404` | `Accepted` | `user_id NOT NULL` on all three tables with `REFERENCES users (id) ON DELETE CASCADE`, plus the two composite keys that force a child's owner to equal its parent's. That is the data half of the rule. | Every statement. No `user_id = $n` predicate exists, no listing exists, and no `404` is produced by this feature. A foreign row is currently reachable by any caller that can run SQL — which is the test suite, not a client. |

The schema half of `JOB-R01`, `R02`, `R03`, `R05`, `R06`, `R07` and `R09` is in
the database. The behaviour half of **all nine** is not. `JOB-R04` has only its
vocabulary and `JOB-R08` has nothing, because neither can be a column.

### 1.2 What is written, and what it proves

| File | Cases | What it serves | Has a run been recorded? |
| ---- | ----- | -------------- | ------------------------ |
| `tests/jobs_schema.rs` | 11 | The constraints, the two composite keys, the cascades, the enum vocabulary, the four-decimal storage and the migration from an empty database. The case-by-case map is [testing.md](testing.md) §1. | Not in this document. |
| `tests/seeder.rs` | 1 case for this feature (`the_seeder_dev_profile_inserts_the_documented_company_and_application_counts`) | The `dev` counts 20 / 12 / 50, the deterministic ids, and the ownership and salary invariants of the seeded rows. | Not in this document. |

A test that exists is not a test that passed, and a schema test is not an
endpoint test. Both statements are load-bearing here.

## 2. What is still missing to reach `Implemented` and then `Verified`

1. **The rest of the module.** `handlers.rs`, `service.rs`, `repository.rs` and
   `dto.rs` under `src/job_applications/`, with the layering of
   [architecture.md](architecture.md) §2. `models.rs` exists; nothing reads or
   writes through it.
2. **The routes.** The fourteen operations of [api.md](api.md) registered in
   `src/api/routes.rs` and in `ROUTE_TABLE`, with their OpenAPI annotations.
   `/api/v1/openapi.json` is at 34 operations and must publish them.
3. **The endpoint tests.** `tests/job_applications_api.rs` does not exist. Every
   case in [testing.md](testing.md) §2 is a plan.
4. **A recorded run.** `tests/jobs_schema.rs` and the seeder case exist without a
   quoted result; the endpoint suite does not exist. The rule for what must pass
   is [`verification-policy.md`](../../testing/verification-policy.md).
5. **The review.** No reviewer has read `C-J1`. A green gate is not a review.

The feature becomes `Implemented` when the module and the routes exist and
compile, which is the `AGENTS.md` §6 meaning and nothing stronger. It becomes
`Verified` when the schema, endpoint and seeder cases pass in a recorded run.
Neither word is used in this document today.

One status discrepancy is recorded rather than resolved here:
[`schema.md`](../../database/schema.md) §11 still heads its section `Proposed`
and lists `companies`, `job_applications` and `application_events` as
exceptions that exist but have no endpoint. That heading and this feature's
`Accepted` are compatible only if "Proposed" is read as "not served by an
endpoint"; reconciling the wording is the schema document's edit, not this
file's.

## 3. What a future change must do to keep it `Accepted` and then `Verified`

- **Do not change an accepted requirement silently.** A revision goes through
  [`project-truth.md`](../../project-truth.md) §18.4, in a new change.
- **A3 and A6 are provisional defaults**, not owner-approved requirements.
- **`0005` lands once and is then append-only.** A schema change is a new
  migration, never an edit to `0005`. Adding a stage value is
  `ALTER TYPE … ADD VALUE` in that new migration, with the transaction caveat in
  [`schema.md`](../../database/schema.md) §8.
- **A change to the stage list is a breaking contract change**: every terminal
  stage and every edge is published.
- **A change that adds generation of any kind** contradicts
  [`project-truth.md`](../../project-truth.md) §6.1 and needs an amendment
  before any code.
- **A change to the money format** needs a new ADR that supersedes
  [`ADR-0007`](../../decisions/ADR-0007-money-representation.md).
- **A change to the delete behaviour** of a company needs its own decision: the
  current key is `NO ACTION`, and a cascade would destroy job history silently.
- **Do not "restore" the `NO ACTION` key to `RESTRICT` for symmetry.** `RESTRICT`
  would make `DELETE FROM users` fail for any account that has an application,
  which is the opposite of the documented cascade.
- **Do not add `updated_at` to `application_events`.** Its absence is decision
  `A-D1-1` ([architecture.md](architecture.md) §3.5).

## 4. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | What exists today, the planned layering, the three constraints that do real work, the invariants and the request path. |
| [`api.md`](api.md) | The fourteen planned endpoints. **None is built.** |
| [`testing.md`](testing.md) | The eleven schema cases that exist, the seeder case, the planned endpoint suite, and what is deliberately not covered. |
| [`../../api/conventions.md`](../../api/conventions.md) | Pagination, money, filtering and timestamps. |
| [`../../database/schema.md`](../../database/schema.md) | The three tables, created by `migrations/0005_job_applications.sql`. |
| [`../../decisions/ADR-0007-money-representation.md`](../../decisions/ADR-0007-money-representation.md) | The money decision. |

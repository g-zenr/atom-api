**Status:** Accepted

# Job applications — status

> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

## 1. The status

The feature is **Accepted**. The requirements `JOB-R01`…`R09` are agreed in
[`project-truth.md`](../../project-truth.md) §18.2, with acceptance criteria.

The feature is **not** `Implemented` and **not** `Verified`:

- there is no `src/job_applications/` module;
- there is no `companies`, `job_applications` or `application_events`
  migration;
- there is no `tests/job_applications_api.rs` and no route in the router;
- no test has been written, and therefore none has run or passed.

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `JOB-R01` A per-owner unique company name | Not built | — |
| `JOB-R02` Applications with seven stages | Not built | — |
| `JOB-R03` An optional, ordered salary range | Decided | [`ADR-0007`](../../decisions/ADR-0007-money-representation.md); no code |
| `JOB-R04` The seven-state stage machine | Not built | — |
| `JOB-R05` Events ordered by `occurred_at` | Not built | — |
| `JOB-R06` Cascade on delete, and the company-delete refusal | Not built | — |
| `JOB-R07` One optional next-action task | Not built | — |
| `JOB-R08` No text generation, no outbound call | Not built | — |
| `JOB-R09` Owner-scoped reads and writes | Not built | — |

`project-truth.md` §17 records the whole area as **Not started**.

## 2. What is still missing to reach `Verified`

1. **The migration.** The three tables, the composite foreign keys, the
   `UNIQUE (user_id, lower(name))` index, the status `CHECK`, the salary
   `CHECK` and the `ON DELETE` behaviour.
2. **The module.** Handlers, service, repository, models and DTOs.
3. **The routes.** Registered in `src/api/routes.rs` and in `ROUTE_TABLE`,
   with the OpenAPI annotations.
4. **The tests.** Every test in [testing.md](testing.md), run and green.
5. **The seeder.** Companies and applications in every stage, with events, in
   the same chain.
6. **The schema document.** [`schema.md`](../../database/schema.md) describing
   the three tables once they exist.
7. **A recorded run.** The gates and the suite, with the result written down.

Only after 1–7 is the feature `Implemented`; it becomes `Verified` when the
tests pass in a recorded run.

## 3. What a future change must do to keep it `Accepted` and then `Verified`

- **Do not change an accepted requirement silently.** A revision goes through
  [`project-truth.md`](../../project-truth.md) §18.4, in a new change.
- **A3 and A6 are provisional defaults**, not owner-approved requirements.
- A change to the stage list is a breaking contract change: every terminal
  stage and every edge is published.
- A change that adds generation of any kind contradicts
  [`project-truth.md`](../../project-truth.md) §6.1 and needs an amendment
  before any code.
- A change to the money format needs a new ADR that supersedes
  [`ADR-0007`](../../decisions/ADR-0007-money-representation.md).
- A change to the delete behaviour of a company needs its own decision: a
  cascade would destroy job history silently.

## 4. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | The planned modules, interfaces, stage machine, invariants, security boundary, request path and ADRs. |
| [`api.md`](api.md) | The fourteen endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The planned test suite, test by test, and what it deliberately does not cover. |
| [`../../api/conventions.md`](../../api/conventions.md) | Pagination, money, filtering and timestamps. |
| [`../../database/schema.md`](../../database/schema.md) | The tables, once the migration lands. |
| [`../../decisions/ADR-0007-money-representation.md`](../../decisions/ADR-0007-money-representation.md) | The money decision. |

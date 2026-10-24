**Status:** Accepted

# Reimbursements — status

> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

## 1. The status

The feature is **Accepted**. The requirements `REIMB-R01`…`R10` are agreed in
[`project-truth.md`](../../project-truth.md) §18.1, with acceptance criteria.

The feature is **not** `Implemented` and **not** `Verified`:

- there is no `src/reimbursements/` module;
- there is no `expense_claims` or `expense_items` migration;
- there is no `tests/claims_api.rs` and no claims route in the router;
- no test has been written, and therefore none has run or passed.

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `REIMB-R01` Claim with line items | Not built | — |
| `REIMB-R02` One currency per claim | Not built | — |
| `REIMB-R03` Total equals the sum of the items | Not built | — |
| `REIMB-R04` The five-state lifecycle | Not built | — |
| `REIMB-R05` Submission preconditions | Not built | — |
| `REIMB-R06` Items change only in draft | Not built | — |
| `REIMB-R07` Money as `NUMERIC(19,4)` and a string | Decided | [`ADR-0007`](../../decisions/ADR-0007-money-representation.md); no code |
| `REIMB-R08` A receipt is a reference | Decided | [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md); no code |
| `REIMB-R09` Optional project link | Not built | — |
| `REIMB-R10` Owner-scoped reads and writes | Not built | — |
| `REIMB-R11` Deletion in any status | Not built | — |

`project-truth.md` §17 records the whole area as **Not started**, which is the
same statement as this table.

## 2. What is still missing to reach `Verified`

Everything that makes a feature exist:

1. **The migration.** `expense_claims` and `expense_items`, with the ownership
   foreign keys, the status `CHECK`, the amount `CHECK`, the indexes and the
   `ON DELETE` behaviour. Append-only, numbered in sequence.
2. **The module.** Handlers, service, repository, models and DTOs, in the
   layering that `AGENTS.md` §2 fixes.
3. **The routes.** Registered in `src/api/routes.rs` and in `ROUTE_TABLE`, with
   the OpenAPI annotations.
4. **The tests.** Every test in [testing.md](testing.md), run and green.
5. **The seeder.** Claims in every status, with totals that equal their items'
   sums, in the same chain.
6. **The schema document.** [`schema.md`](../../database/schema.md) describing
   the two tables once they exist.
7. **A recorded run.** The gates and the suite, with the result written down
   in the change log and the verification record.

Only after 1–7 is the feature `Implemented`; it becomes `Verified` when the
tests in [testing.md](testing.md) pass in a recorded run. The feature is not
promoted because the code exists.

## 3. What a future change must do to keep it `Accepted` and then `Verified`

- **Do not change an accepted requirement silently.** A revision goes through
  [`project-truth.md`](../../project-truth.md) §18.4, in a new change.
- **A1, A2 and A6 are provisional defaults**, not owner-approved requirements.
  If the owner revises one, the specification and this file are amended
  together.
- A change to the money format is a breaking contract change and needs a new
  ADR that supersedes [`ADR-0007`](../../decisions/ADR-0007-money-representation.md).
- A change that starts storing receipt bytes needs a new ADR that supersedes
  [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md).
- A change to the lifecycle needs a test for every allowed and every forbidden
  edge.

## 4. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | The planned modules, interfaces, invariants, security boundary, request path, related files and ADRs. |
| [`api.md`](api.md) | The twelve endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The planned test suite, test by test, and what it deliberately does not cover. |
| [`../../api/conventions.md`](../../api/conventions.md) | Pagination, money, filtering and timestamps. |
| [`../../database/schema.md`](../../database/schema.md) | The tables, once the migration lands. |
| [`../../decisions/ADR-0007-money-representation.md`](../../decisions/ADR-0007-money-representation.md) | The money decision. |
| [`../../decisions/ADR-0009-receipt-references.md`](../../decisions/ADR-0009-receipt-references.md) | The receipt decision. |

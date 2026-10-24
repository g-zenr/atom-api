**Status:** Accepted

# Reimbursements — status

> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

## 1. The status

The feature is **Accepted**. The requirements `REIMB-R01`…`R11` are agreed in
[`project-truth.md`](../../project-truth.md) §18.1, with acceptance criteria.
The chain `C-R1` (D2.R1) lands the schema and the row types and no endpoint, so
the feature status does not change.

**What is now true.** Migration `0004_expense_claims.sql` creates the
`expense_claims` and `expense_items` tables and the `expense_claim_status`
enum. `src/reimbursements/` holds `mod.rs` and `models.rs` only. There is no
handler, service, repository, DTO or route: no endpoint exists,
`/api/v1/openapi.json` is unchanged, and nothing in this feature is
`Implemented` or `Verified` at the API level. **The schema exists; the
behaviour the requirements name does not.** A column or a `CHECK` cannot answer
a request.

A requirement therefore stays `Not built` until its behaviour exists. Where the
schema half of it now exists, the state says so in parentheses; that
parenthetical is not a promotion.

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `REIMB-R01` Claim with line items | Not built (schema and row types exist) | Migration `0004` creates both tables; `src/reimbursements/models.rs` holds `ExpenseClaim` and `ExpenseItem`. No service, DTO or endpoint. |
| `REIMB-R02` One currency per claim | Not built (column and constraint exist) | `currency CHAR(3)` with `expense_claims_currency_chk`. Immutability is a service rule with a `422`, and it arrives with R2. |
| `REIMB-R03` Total equals the sum of the items | Not built (column and constraint exist) | `total_amount NUMERIC(19,4)` with `expense_claims_total_amount_chk`. The invariant itself is maintained by `SUM` in SQL from R3, when item writes exist. |
| `REIMB-R04` The five-state lifecycle | Not built (enum and constraint exist) | `expense_claim_status` and `expense_claims_lifecycle_timestamps_chk`. The transitions need the four action endpoints (R4). |
| `REIMB-R05` Submission preconditions | Not built | A service rule, because it reads another table; no constraint can express it. R2–R3. |
| `REIMB-R06` Items change only in draft | Not built | A service guard before every item write. R3. |
| `REIMB-R07` Money as `NUMERIC(19,4)` and a string | Not built (columns and row types exist) | Both money columns are `NUMERIC(19,4)` and `models.rs` carries them as `String`. The four-decimal wire form and the `Money` type arrive with `dto.rs` in R2. [`ADR-0007`](../../decisions/ADR-0007-money-representation.md). |
| `REIMB-R08` A receipt is a reference | Not built (column and constraint exist) | `receipt_url TEXT` with `expense_items_receipt_url_chk`. The validator and the endpoint are R2. [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md). |
| `REIMB-R09` Optional project link | Not built (column and foreign key exist) | `project_id` with `ON DELETE SET NULL`, proven by `deleting_a_project_clears_the_claim_link_and_keeps_the_claim`. The ownership check on create and patch is R2. |
| `REIMB-R10` Owner-scoped reads and writes | Not built (schema makes a cross-owner row impossible) | `user_id` on both tables, and the composite key `expense_items_claim_fk (claim_id, user_id)`. The `404` behaviour needs the repository (R2); the database cannot produce it. |
| `REIMB-R11` Deletion in any status | Not built (the cascades exist) | Deleting a claim deletes its items; deleting the account deletes both. The `DELETE` endpoint and its `204`/`404` answers are R2. |

`project-truth.md` §17 still records the area as **Not started** with the note
"no code, no migration, no test exists". That row predates this chain and is
now stale. `docs/project-truth.md` is not a file this agent owns, so the
discrepancy is reported to the orchestrator rather than edited here.

## 2. What is still missing to reach `Verified`

Everything except the schema:

1. **The rest of the module.** `handlers.rs`, `service.rs`, `repository.rs` and
   `dto.rs`, in the layering that `AGENTS.md` §2 fixes. `mod.rs` and
   `models.rs` exist; the other four files do not.
2. **The routes.** The twelve operations registered in `src/api/routes.rs` and
   in `ROUTE_TABLE`, with the OpenAPI annotations. None exists, and
   `/api/v1/openapi.json` is unchanged.
3. **The endpoint tests.** The tests in [testing.md](testing.md) §2, run and
   green. No endpoint test exists, because there is no endpoint to call; the
   schema tests in §1 do exist.
4. **The service rules a constraint cannot hold.** The submit precondition
   (`REIMB-R05`), the draft-only item writes (`REIMB-R06`), the currency
   immutability (`REIMB-R02`) and the total recomputation (`REIMB-R03`) arrive
   with R2 and R3.
5. **The schema document is current.** [`schema.md`](../../database/schema.md)
   §11 now records that migration `0004` creates the two tables while no
   endpoint reads or writes them. It is the `database` agent's document; this
   file links to it rather than restating it.
6. **A recorded run.** The gates and the suite, with the result written down
   in the change log and the verification record.

The **seeder** half is done in this chain: `atom-seed` inserts claims and items
for all three profiles (1 / 25 / 50 claims and 0 / 48 / 98 items) and verifies
that every seeded claim's total equals its items' sum. That is seed data, not
feature behaviour, and it promotes no requirement.

Only after 1–6 is the feature `Implemented`; it becomes `Verified` when the
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
| [`architecture.md`](architecture.md) | What exists (the schema and the row types), the planned modules, interfaces, invariants, security boundary, request path, related files and ADRs. |
| [`api.md`](api.md) | The twelve planned endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The schema tests that exist, the seeder cases, and the endpoint suite as planned. |
| [`../../api/conventions.md`](../../api/conventions.md) | Pagination, money, filtering and timestamps. |
| [`../../database/schema.md`](../../database/schema.md) | The tables; §11 is the column, constraint and index definition that migration `0004` implements. |
| [`../../decisions/ADR-0007-money-representation.md`](../../decisions/ADR-0007-money-representation.md) | The money decision. |
| [`../../decisions/ADR-0009-receipt-references.md`](../../decisions/ADR-0009-receipt-references.md) | The receipt decision. |

**Status:** Accepted

# Reimbursements — status

> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

## 1. The status

The feature is **Accepted**. The requirements `REIMB-R01`…`R11` are agreed in
[`project-truth.md`](../../project-truth.md) §18.1, with acceptance criteria. Two
chains have landed parts of it and neither changes the feature status:

- **`C-R1` (D2.R1)** — the schema: migration `0004_expense_claims.sql` creates
  `expense_claims`, `expense_items` and the `expense_claim_status` enum, and
  `src/reimbursements/models.rs` holds the row types.
- **`C-R2` (D2.R2)** — the claim endpoints: five operations on
  `/api/v1/claims` and `/api/v1/claims/{claim_id}`, and the full module
  layering (`dto`, `service`, `repository`, `handlers`).

**The feature is not `Implemented`.** Items and the lifecycle are the other half
of the accepted scope, and neither is built: there is no `/claims/{claim_id}/items`
route and no `/submit`, `/approve`, `/reject` or `/pay`. A client can create,
list, read, change and delete a claim; it cannot add a single line item to one,
so a claim's `total_amount` stays `"0.0000"` for every claim this API can create,
and every claim it creates stays in `draft`.

A requirement therefore keeps a state that names **which half exists**, and that
parenthetical is not a promotion. "Partly built" below is shorthand for "one
half of the requirement exists"; it is not one of the seven statuses in
`AGENTS.md` §6 and it is not a step towards `Verified`.

| Requirement | State | Which half exists | Evidence |
| ----------- | ----- | ----------------- | -------- |
| `REIMB-R01` Claim with line items | Partly built | Schema (R1) and the claim half of the endpoints (R2). The **item** half — adding, changing, removing a line item — is R3. | Both tables and both row types exist. `POST`, `GET`, `PATCH` and `DELETE` answer for a claim, and `GET /claims/{id}` returns `items`, which is `[]` for every claim the API can create. |
| `REIMB-R02` One currency per claim | Built, at the claim level | Schema (R1) and the endpoint rules (R2). | `currency CHAR(3)` with `expense_claims_currency_chk`; required on create, trimmed and upper-cased, three letters or `422`; **not writable afterwards** — see §1.1. Asserted by `the_claim_currency_is_required_and_upper_cased` and `the_claim_currency_is_not_patchable`. No conversion exists anywhere. |
| `REIMB-R03` Total equals the sum of the items | Not built | Schema (R1) only. The invariant is maintained by item writes, which are R3. | The columns and `expense_claims_total_amount_chk` exist, and the seeder keeps the sum; no endpoint writes an item, so no endpoint can break or prove the invariant. R2 reads the total and never accepts it from a client. |
| `REIMB-R04` The five-state lifecycle | Not built | Schema (R1) only. The transitions need the four action endpoints (R4). | `expense_claim_status` and `expense_claims_lifecycle_timestamps_chk` exist. R2 publishes `status` and can never change it: it is not a field of `UpdateClaimRequest` (`the_claim_status_is_not_patchable`). Every claim the API creates is `draft`. |
| `REIMB-R05` Submission preconditions | Not built | Nothing. It is a service rule over the items and the status. | A service rule, because it reads another table; no constraint can express it. R4. |
| `REIMB-R06` Items change only in draft | Not built | Nothing. There is no item write to guard. | A service guard before every item write. R3. |
| `REIMB-R07` Money as `NUMERIC(19,4)` and a string | Partly built | The **response** half (R1 schema, R2 wire form). The four-decimal **input** rule has no caller until R3. | Both money columns are `NUMERIC(19,4)`, selected as `::text`; `total_amount` and `amount` are JSON strings in the published document and in every response. No `f32`, no `f64`, no decimal crate, no arithmetic in Rust. [`ADR-0007`](../../decisions/ADR-0007-money-representation.md). |
| `REIMB-R08` A receipt is a reference | Partly built | Column (R1) and the published response field (R2). The validator and the endpoint that accept one are R3. | `receipt_url TEXT` with `expense_items_receipt_url_chk`; `ItemResponse.receipt_url` is published. No route accepts a file, so the "never bytes" half holds vacuously. [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md). |
| `REIMB-R09` Optional project link | Built, at the claim level | Schema (R1) and the endpoint rules (R2). | `project_id` with `ON DELETE SET NULL`; create and `PATCH` verify the project belongs to the caller and answer `404` otherwise, in the statement itself. `PATCH {"project_id": null}` clears the link. Asserted by `a_claim_cannot_reference_another_users_project` and `deleting_the_project_clears_the_link_and_keeps_the_claim`. |
| `REIMB-R10` Owner-scoped reads and writes | Partly built | Schema (R1) and the claim half of the endpoints (R2). The item half is R3. | `user_id` on both tables and the composite key `expense_items_claim_fk (claim_id, user_id)`; every claim statement carries `user_id` in its own `WHERE` and a foreign claim answers `404`, never `403`. The item statements do not exist yet. |
| `REIMB-R11` Deletion in any status | Partly built | The endpoint (R2) and the cascades (R1). The statuses it must work in cannot be reached through the API until R4. | `DELETE /api/v1/claims/{claim_id}` answers `204` and has no status guard; `ON DELETE CASCADE` on the composite key removes the items. Only a `draft` claim can be produced by the API today, so "in any status" is unproven until R4 exists. |

### 1.1 The `currency` rule on `PATCH`: ignored with `200`, not a `422`

A `PATCH /api/v1/claims/{claim_id}` body carrying `currency` — or `status`, or
`total_amount` — is **accepted with `200`, and the field is ignored**.

`UpdateClaimRequest` has three fields: `title`, `description` and `project_id`.
`serde` ignores an unknown key by default, so a fourth key never reaches the
service; and the `SET` clause of the one update statement names those three
fields plus `updated_at`, and nothing else. There is no code path in which a
handler could pass a currency through.

**This corrects three documents.** The `C-R1` `I6` line in the run ledger,
[`schema.md`](../../database/schema.md) §11.1, and the earlier §1 and
`REIMB-R02` row of **this** document all called immutability "a service rule
with a `422`". The design that resolves the disagreement is
[`../../architecture/reimbursements-crud-design.md`](../../architecture/reimbursements-crud-design.md)
§9.1, and the published client contract wins:

- [`api.md`](api.md) §6 says `currency`, `status` and the timestamps "are
  **not** updatable. They are absent from the request contract … a client that
  sends them does not change them", and it lists no `422` for them. `api.md` is
  the published contract; the others are internal descriptions of it.
- A `422` for `currency` alone would require **adding** `currency` and `status`
  to `UpdateClaimRequest` in order to reject them. That widens the published
  request schema with two fields nothing uses, and contradicts "they are absent
  from the request contract" in the same paragraph.
- `#[serde(deny_unknown_fields)]` is not the alternative: it would make *every*
  unknown key a `422`, a breaking change for any client that sends one extra
  field.
- Immutability is stronger this way, not weaker. What was dropped is the
  *error*, not the rule.

The behaviour is asserted, not assumed:
`the_claim_currency_is_not_patchable` and `the_claim_status_is_not_patchable`
send the ignored fields, require `200`, and require the stored values to be
unchanged ([testing.md](testing.md) §2.5).

The sibling correction in the database document — `schema.md` §11.1 — and the
one-line correction of the `C-R1` `I6` line in the ledger belong to the
`database` agent and the orchestrator; this document records the ruling and
does not edit their files.

[`project-truth.md`](../../project-truth.md) §17 records the area's progress and
names migration `0004_expense_claims.sql`, `src/reimbursements/` and
`tests/claims_schema.rs`; this document is the feature-level detail behind that
entry.

## 2. What is still missing

1. **Items (`R3`).** Three endpoints — add, change and delete a line item on a
   draft claim — the total recomputation in the same transaction, the money
   input validator, and the draft-only guard (`REIMB-R06`). Everything that
   makes `REIMB-R03` true at the API level is here.
2. **The lifecycle (`R4`).** `POST …/submit`, `…/approve`, `…/reject`, `…/pay`,
   `ensure_transition`, the submission preconditions (`REIMB-R05`) and the only
   writer of `status` and the three lifecycle timestamps.
3. **The flow tests (`R5`).** The end-to-end cases in [testing.md](testing.md)
   §2.3 — a draft submitted, approved and paid — and the ownership sweep over
   all twelve operations. They cannot be written before R3 and R4 exist.
4. **A recorded run.** The five claim operations are `Implemented` in the
   `AGENTS.md` §6 sense: the code exists. `Implemented` also requires that the
   tree compiles, and that is the chain's gate — not this document. **No run of
   `tests/claims_api.rs` is recorded anywhere yet**, so nothing in this feature
   is `Verified`: not a requirement, not an endpoint, not a test file.
5. **The chain's review rows.** The `code-reviewer` and `quality-gate` verdicts
   for `C-R2` are outstanding, as they are for `D1`, `D2.S1` and `D2.R1`. The
   `/api/v1/openapi.json` change is part of what those reviewers read.

Only after 1–3 is the **feature** `Implemented`; it becomes `Verified` when the
tests in [testing.md](testing.md) pass in a recorded run. The feature is not
promoted because the code exists, and an endpoint reaching `Implemented` does
not promote the requirement it serves.

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
- **A change that makes `currency`, `status` or `total_amount` a `422` on
  `PATCH` is a contract change**, not a validation improvement: it contradicts
  [`api.md`](api.md) §6 and §1.1 above, and it needs the published document
  changed first.

## 4. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | The layering that exists, the six claim methods and their nine statements, the two list shapes, the `project_id` tri-state, the invariants and what is still planned. |
| [`api.md`](api.md) | The twelve planned endpoints with their request and response shapes, status codes and error cases. Five answer today. |
| [`testing.md`](testing.md) | The schema tests, the seeder cases, the 34 claim endpoint tests, the two document tests, and what R3–R5 still owe. |
| [`../../api/conventions.md`](../../api/conventions.md) | Pagination, money, filtering and timestamps. |
| [`../../database/schema.md`](../../database/schema.md) | The tables; §11 is the column, constraint and index definition that migration `0004` implements. |
| [`../../decisions/ADR-0007-money-representation.md`](../../decisions/ADR-0007-money-representation.md) | The money decision. |
| [`../../decisions/ADR-0009-receipt-references.md`](../../decisions/ADR-0009-receipt-references.md) | The receipt decision. |

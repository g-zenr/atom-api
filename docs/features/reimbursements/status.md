**Status:** Verified

# Reimbursements — status

> **Last updated:** 2026-10-30
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

## 1. The status

The feature is **`Verified`** as of 2026-10-29.

All twelve published operations answer. The five claim operations landed in
`C-R2`; the three item operations and the four lifecycle actions landed in the
`C-R3`/`C-R4` wave:

- **`C-R1`** — the schema: `migrations/0004_expense_claims.sql` creates
  `expense_claims`, `expense_items` and the `expense_claim_status` enum, and
  `src/reimbursements/models.rs` holds the row types.
- **`C-R2`** — the claim endpoints: `GET`/`POST /api/v1/claims` and
  `GET`/`PATCH`/`DELETE /api/v1/claims/{claim_id}`, with the full module
  layering (`dto`, `service`, `repository`, `handlers`).
- **`C-R3`/`C-R4`** — `POST /claims/{claim_id}/items`,
  `PATCH`/`DELETE /claims/{claim_id}/items/{item_id}` and
  `POST …/submit`, `…/approve`, `…/reject`, `…/pay`; the `SUM` that maintains
  the total inside the item transaction; the transition table; and
  `helpers::validation::validate_money`.

**What made it `Verified`.** Three things, recorded in
[`ATOM-REVIEW-20261029-002`](../../engineering/review-log.md): (1) an
independent read of the final tree — all nineteen statements owner-scoped, the
total maintained by `SUM` in one transaction, the lifecycle one guarded `UPDATE`
per edge, no code defect found; (2) the quality-gate session's own execution of
`scripts/gate.ps1` — six gates at `0`, the 536-test split re-derived
target-for-target (56 endpoint, 3 flow, 10 schema cases and twelve unit tests
in this feature); (3) no regression since the last clean read — no test deleted,
renamed, `#[ignore]`d or weakened since `4df383e`. The status is the
[`AGENTS.md`](../../../AGENTS.md) §6 word: implemented, and covered by recorded
checks that pass. The five unasserted requirement halves remain the feature's
documented gaps (§2.9/§3 of [`testing.md`](testing.md)) — recorded, not hidden.
The historical run sizes (296 → 299 → 511) are in the change-log archive; this
card records the current state.

**The evidence column below names the test written against each rule.** The
recorded run reports the whole suite green, so those are the tests that ran; the
column does not, and cannot, say that any single one of them would fail if its
rule were removed — that is the rule in
[`verification-policy.md`](../../testing/verification-policy.md), and the
implementing changes are where it is shown.

**The cross-cutting evidence is in the tree.** `tests/claims_flow.rs` holds
three cases: `the_whole_reimbursement_journey_from_registration_to_payment`,
`the_second_account_sees_not_found_on_every_reimbursement_operation` and
`the_seeded_claims_are_readable_through_the_api`. The third is
**opportunistic**: it returns with a printed reason when `owner@atom.local` is
not in the database, so the seeded surface is covered on a database a seeder run
has touched, not on every run. [testing.md](testing.md) §2.10 states what each
asserts and what the flow layer still does not cover. A case that exists is not
by itself evidence for `Verified`, and nothing below is promoted on its account.

### 1.1 The requirements

Every requirement now has code, and the feature is `Verified` (2026-10-29,
`ATOM-REVIEW-20261029-002`): each row's State column is `Verified` — the code
exists and the tests named in the last column pass in the recorded gate run.
The **Unproved** notes inside the cells are the feature's remaining, recorded
gaps.

| Requirement | State | What exists | What proves it |
| ----------- | ----- | ----------- | -------------- |
| `REIMB-R01` Claim with line items | `Verified` | Five claim operations and three item operations. `CreateItemRequest` carries `description`, `amount`, `incurred_on`, `category` and `receipt_url` only — an item body cannot set `id`, `claim_id`, `user_id`, `currency` or a timestamp, so the item is denominated in its claim by construction. The detail read lists the items. | `a_claim_can_be_created_read_and_deleted`; `an_item_can_be_added_to_a_draft_claim` (`201`, trimmed description, lower-cased category, no `currency` field on the item, `category`/`receipt_url` `null` when absent, both items readable through the claim); `an_item_can_be_partially_updated`; `the_claim_detail_items_are_in_chronological_order`; `an_item_amount_must_be_a_four_decimal_string` (a refused body creates no item and moves no total). **Unproved:** the 500/501-character item-description bound and a non-date `incurred_on` are not asserted at HTTP level — see §2. |
| `REIMB-R02` One currency per claim | `Verified` | `currency CHAR(3)` with `expense_claims_currency_chk`; required on create, trimmed and upper-cased, three letters or `422`; not writable afterwards (§1.2). An item has no currency column and no currency field, and no conversion exists anywhere in the feature. | `the_claim_currency_is_required_and_upper_cased`; `the_claim_currency_is_not_patchable`; `a_claim_rejects_a_currency_that_is_not_three_upper_case_letters` (`23514`); `an_item_can_be_added_to_a_draft_claim`, which asserts the item response carries no `currency` key. |
| `REIMB-R03` The total equals the sum of the items | `Verified` | This is the requirement that was waiting for the item writes. Every item write is one transaction: `SELECT status … FOR UPDATE` on the claim row, the `INSERT`/`UPDATE`/`DELETE` of the item, then `UPDATE expense_claims SET total_amount = COALESCE((SELECT SUM(amount) FROM expense_items WHERE claim_id = $1 AND user_id = $2), 0)` — the `SUM` runs in SQL, and the commit is the last step, so the item and the total cannot diverge. `SUM` over no rows is `NULL`, so the `COALESCE` stores `0` and the last delete renders `"0.0000"`. | `adding_an_item_recomputes_the_total`; `deleting_an_item_recomputes_the_total` (one item removed, then the last one, then a second delete is `404`); `the_claim_total_equals_the_sum_of_its_items` (add, update and delete each re-read through `GET`, compared with the returned items **and** with an independent `i128` sum, ending at `"0.0000"`); `an_item_can_be_partially_updated` (the total follows a changed amount); `the_whole_reimbursement_journey_from_registration_to_payment`, whose three items sum to `"149.8001"` and whose stored total is compared with the independently computed sum again after `pay`. Data half: `money_is_stored_with_four_decimal_places`, `a_seeded_claim_total_equals_the_sum_of_its_items`. **Unproved:** two concurrent item writes are not exercised, and a direct SQL edit of an item still bypasses the recompute (§2, [architecture.md](architecture.md) §10). |
| `REIMB-R04` The five-state lifecycle | `Verified` | Four action routes over one table of five edges (`draft -submit-> submitted`, `submitted -approve-> approved`, `submitted -reject-> rejected`, `approved -pay-> paid`, `rejected -submit-> draft`); the other fifteen pairs are `409`. Each action is one guarded `UPDATE` whose `WHERE` names the expected source status, so two concurrent actions cannot both win. The reopen clears **`submitted_at` and `decided_at` in the same statement**, because `expense_claims_lifecycle_timestamps_chk` requires all three stamps to be null for a `draft`. `paid` is terminal. | `a_draft_can_be_submitted_approved_and_paid`; `every_forbidden_transition_is_refused` (fifteen cases, each asserting the status and all three stamps are unchanged); `a_submitted_claim_cannot_be_approved_twice`; `a_rejected_claim_reopens_and_can_be_resubmitted` (both stamps null in the response **and** in a following `GET`, then an item write, then a resubmit); `paid_is_terminal`; `the_lifecycle_timestamps_are_set_by_the_transition`. `the_whole_reimbursement_journey_from_registration_to_payment` (§2.10 of [testing.md](testing.md)) asserts which stamp each step sets, that no earlier stamp moves and that the total is undisturbed. Unit: `the_transition_table_allows_exactly_the_five_edges` (5 of 20), `a_reviewed_claim_cannot_be_reopened`. Schema: `the_lifecycle_constraint_accepts_every_consistent_status`, `the_lifecycle_constraint_rejects_a_submitted_claim_without_a_submission_timestamp`. |
| `REIMB-R05` Submission preconditions | `Verified` | The rule is **inside the submit statement**, not a check before it: `WHERE … AND (status = 'rejected' OR (total_amount > 0 AND EXISTS (SELECT 1 FROM expense_items WHERE claim_id = $1 AND user_id = $2)))`. A `draft` that fails it updates no row, and the service then reads the claim once to choose the field: no items → `422` naming `items`, items but a zero total → `422` naming `total_amount`. A `rejected` claim reopens without the precondition, because a rejection freezes its items. | `a_claim_without_items_cannot_be_submitted` (`422` naming `items`; the claim stays `draft` with three null stamps; one item then makes the same call `200`); `a_claim_with_a_zero_total_cannot_be_submitted` (a `"0.0000"` item is legal, so the total is the only fault: `422` naming `total_amount`; `"0.0001"` is enough, which proves "above zero" and not a rounded boundary). |
| `REIMB-R06` Items change only in draft | `Verified` | Each item write locks the claim row and reads its status first (`SELECT status … FOR UPDATE`); a row that is not `draft` returns `409 conflict` with the `claim_not_editable` message, and nothing is written. The reopen is what makes a rejected claim editable again. | `an_item_cannot_be_added_to_a_submitted_claim` (`submitted`, `approved`, `rejected` and `paid` each `409`, item count and total unchanged); `an_item_cannot_be_changed_or_deleted_after_submission` (`PATCH` and `DELETE` both `409`, the item and total intact, status still `submitted`); `a_rejected_claim_reopens_and_can_be_resubmitted` (an item is added **after** the reopen and is accepted with `201`). |
| `REIMB-R07` Money as `NUMERIC(19,4)` and a string | `Verified` | Both money columns are `NUMERIC(19,4)`, selected as `::text`; every request and response money value is a JSON string. The four-decimal **input** rule now has a caller: `validate_money` accepts `^[0-9]{1,15}\.[0-9]{4}$` and **refuses rather than rounds**, because the column rounds silently. No `f32`, no `f64`, no decimal crate, no arithmetic in Rust. | `an_item_amount_round_trips_with_four_decimals` (`"125.8000"`, `"0.0000"`, `"999999999999999.9999"` each echoed and re-read, with the one-item total equal to the amount); `an_item_amount_must_be_a_four_decimal_string` (`"12.5"`, `"12.500"`, `"1,000.0000"`, `"1e3"`, `"-1.0000"`, a JSON number and sixteen integer digits are each `422`; nothing is written); `money_round_trips_with_four_decimals`; `a_money_field_is_never_a_json_number_in_a_response`; `the_claim_money_fields_are_strings_in_the_document`; `the_total_is_not_accepted_from_the_client`. Unit: `money_requires_exactly_four_decimals`, `money_is_bounded_to_fifteen_integer_digits`, `a_negative_amount_is_refused`. Schema: `money_is_stored_with_four_decimal_places`. A JSON number never reaches the validator — the body fails to deserialize and the shared extractor reports one `422 validation_failed` for the whole body; the endpoint test asserts the code; the field-`body` shape is the extractor's documented behaviour ([`../../api/error-contract.md`](../../api/error-contract.md) §3) rather than something this test pins. |
| `REIMB-R08` A receipt is a reference | `Verified` | `receipt_url TEXT` with `expense_items_receipt_url_chk`; the create and patch validators accept an absolute `http`/`https` URL of at most 2048 characters and store it verbatim. The field is a tri-state on `PATCH`: absent keeps it, `null` clears it. Nothing fetches the URL and nothing logs it. | `a_receipt_url_must_be_absolute_http_or_https` (six refused forms — no scheme, `ftp:`, protocol-relative, `javascript:`, relative, single-slash — each `422` naming `receipt_url`; 2049 characters refused and 2048 accepted; `https` and `http` both accepted and stored verbatim; only the accepted items exist afterwards); `an_item_can_be_partially_updated` (`null` clears it); unit `a_receipt_must_be_an_absolute_http_url`. **Unproved:** no test posts a `multipart/form-data` or base64 file body — the "never bytes" half holds because no route parses a file, not because a test refused one (§2). |
| `REIMB-R09` Optional project link | `Verified` | `project_id` with `ON DELETE SET NULL`; create and `PATCH` verify the project belongs to the caller inside the write statement and answer `404` otherwise; `PATCH {"project_id": null}` clears the link. Unchanged by this wave — the item and lifecycle routes carry no project input. | `a_claim_can_be_created_with_a_project_of_the_caller`; `a_claim_cannot_reference_another_users_project`; `deleting_the_project_clears_the_link_and_keeps_the_claim`; `a_project_id_that_is_not_a_uuid_is_refused`; `the_update_claim_request_publishes_a_nullable_project_id`; `deleting_a_project_clears_the_claim_link_and_keeps_the_claim` (schema). |
| `REIMB-R10` Owner-scoped reads and writes | `Verified` | Every claim statement and every item statement carries `user_id` in its own `WHERE`; an item predicate carries **`claim_id` and `user_id`**, so an id from one account cannot be combined with a claim id from another, and an item id under the wrong claim of the same owner matches no row. A miss is `404` with the resource's catalogue message — never `403`. | `a_claim_of_another_user_is_invisible`; `the_listing_only_contains_the_callers_claims`; `a_claim_cannot_reference_another_users_project`; `an_item_of_another_users_claim_is_invisible` (create, patch and delete all `404`; the other account's listing stays empty; the owner's item and total unchanged); `an_item_id_under_the_wrong_claim_is_not_found` (both claims owned by the caller, so ownership is not the reason; also an id that exists nowhere); `a_lifecycle_action_on_another_users_claim_is_not_found` (all four actions, `404`, nothing written, the other account's listing empty); `the_claim_routes_require_a_credential` (all twelve operations, no token and a bad token, `401`, and nothing sent landed); `the_second_account_sees_not_found_on_every_reimbursement_operation` (the eleven claim-addressed operations on each of five fixtures, 55 requests, each `404 not_found`; the list operation is asserted by its empty page and `total` 0, because it publishes `200` rather than `404`). |
| `REIMB-R11` Deletion in any status | `Verified` | `DELETE /api/v1/claims/{claim_id}` is one statement with **no status predicate** — `DELETE FROM expense_claims WHERE id = $1 AND user_id = $2` — so the status cannot be the reason a deletion is refused; the items follow through `expense_items_claim_fk … ON DELETE CASCADE`, in the same statement. The four lifecycle actions now make every status reachable through the API, which they were not in `C-R2`. | `a_claim_can_be_deleted_in_any_status` (each of the five statuses: the claim is driven there by the published lifecycle actions, deleted with `204` and no body, then reads `404`, its item table holds no row, its item id is no longer addressable, the caller's listing is empty, and a bystander's same-status claim still carries its own item); `a_claim_can_be_created_read_and_deleted` (a `draft`: `204`, then `404`); `a_claim_of_another_user_is_invisible` (the foreign delete is `404` and the claim survives); `deleting_a_claim_deletes_its_items` and `deleting_an_account_deletes_its_claims_and_items` (schema). The flow layer adds no delete of its own: the journey ends on a paid claim without deleting it, and the sweep's `DELETE` is a foreign attempt that answers `404`. |

### 1.2 The `currency` rule on `PATCH`: ignored with `200`, not a `422`

A `PATCH /api/v1/claims/{claim_id}` body carrying `currency` — or `status`, or
`total_amount` — is **accepted with `200`, and the field is ignored**.

`UpdateClaimRequest` has three fields: `title`, `description` and `project_id`.
`serde` ignores an unknown key by default, so a fourth key never reaches the
service; and the `SET` clause of the one update statement names those three
fields plus `updated_at`, and nothing else. There is no code path in which a
handler could pass a currency through. The same holds for the item bodies: an
item body carrying `currency`, `claim_id`, `user_id` or a timestamp is accepted
and ignored, because `CreateItemRequest` and `UpdateItemRequest` do not have
those fields.

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

## 2. What is still missing

1. **The flow layer — present, with two caveats.**
   `tests/claims_flow.rs` holds the three committed
   cross-cutting cases: `the_whole_reimbursement_journey_from_registration_to_payment`,
   `the_second_account_sees_not_found_on_every_reimbursement_operation` and
   `the_seeded_claims_are_readable_through_the_api`
   ([testing.md](testing.md) §2.10). What is still owed is narrower than before,
   and it is not nothing:
   - the seeded case is **opportunistic**. It returns with a printed reason when
     `owner@atom.local` is absent, so it proves the seeded surface only on a
     database a seeder run has touched; it also never asserts the seeded *size*,
     because the suite shares one database and whichever profile ran last
     decides it;
   - the flow layer itself adds no deletion of a non-`draft` claim — that half
     of `REIMB-R11` is asserted by the endpoint case
     `a_claim_can_be_deleted_in_any_status` (§1.1), not by a flow case.

   A flow test is a composition check, not a proof of the surface: passing it
   says nothing about the `408`/`500` paths, the `rejected` branch or a deletion
   in a non-draft status ([testing.md](testing.md) §2.10).
2. **The recorded run.** `ATOM-CHANGE-20261024-007` in
   [`change-log.md`](../../engineering/change-log.md) records the six gates
   green, `cargo test --all-targets` exit 0 with 296 tests and a hand run on
   `atom_dev`; `ATOM-CHANGE-20261024-008` records the same six gates green with
   **299** tests once `tests/claims_flow.rs` is in. Both are the orchestrator's
   record, and this document has not re-run the suite.
3. **The review rows — closed.** The promotion to `Verified` was made by
   [`ATOM-REVIEW-20261029-002`](../../engineering/review-log.md): the
   independent read of the final tree returned **PASS** on code, contract and
   regressions, and the quality-gate session re-executed the six gates at `0`
   with the 536-test split re-derived target-for-target.
4. **Five small test gaps inside the operations that now exist**, each a rule
   in the code with no endpoint assertion: the 1–500 item-description bound, a
   non-date `incurred_on` (`422` with field `body`), a non-UUID `item_id`
   (`400`), a `multipart`/file body on the item route, and any concurrency
   case. The last one is design-only: the row lock and the guarded `UPDATE` are
   what prevent two writers from both winning
   ([architecture.md](architecture.md) §2.6–§2.7), and no test runs two writers
   at once. [testing.md](testing.md) §3 lists them with the rule each would
   prove. **Working-tree note (2026-10-30):** five further tests exist
   **uncommitted** in `tests/claims_api.rs` (four of the five gaps above) and
   `tests/claims_flow.rs` (a deterministic minimal-profile seeded read that
   closes the opportunistic caveat); they were executed on 2026-10-30 and all
   five passed (`ATOM-CHANGE-20261030-001`), but until they are committed and
   re-run as part of the recorded tree they are not part of this feature's
   verified record. The concurrency gap remains open.

A requirement is not `Verified` because a test exists next to it — the
promotion was made by [`ATOM-REVIEW-20261029-002`](../../engineering/review-log.md)
(2026-10-29): the independent read of the final tree returned **PASS** on code
with no defect, the quality-gate session re-executed the six gates at `0` with
the 536-test split re-derived target-for-target, and no regression was found
since `4df383e`.

## 3. What a future change must do to keep it `Verified`

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
  edge. The table is proved exhaustively today
  (`the_transition_table_allows_exactly_the_five_edges`); a sixth edge means
  twenty-five combinations, not twenty.
- **A change that makes `currency`, `status` or `total_amount` a `422` on
  `PATCH` is a contract change**, not a validation improvement: it contradicts
  [`api.md`](api.md) §6 and §1.2 above, and it needs the published document
  changed first.
- **A change to an item write must keep the three steps in one transaction**
  (lock, write, recompute). Moving the `SUM` into Rust, or out of the
  transaction, breaks `REIMB-R03` in a way no `CHECK` would catch.

## 4. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | The layering that exists, the twelve service methods and the nineteen statements, the item transaction, the transition table, the two list shapes, the `project_id` tri-state, the invariants and the limitations. |
| [`api.md`](api.md) | The twelve endpoints with their request and response shapes, status codes and error cases. All twelve answer. |
| [`testing.md`](testing.md) | The schema tests, the seeder cases, the 55 endpoint cases, the three `tests/claims_flow.rs` cases, the contract tests, the unit tests, and what is still open. |
| [`../../api/conventions.md`](../../api/conventions.md) | Pagination, money, filtering and timestamps. |
| [`../../database/schema.md`](../../database/schema.md) | The tables; §11 is the column, constraint and index definition that migration `0004` implements. |
| [`../../decisions/ADR-0007-money-representation.md`](../../decisions/ADR-0007-money-representation.md) | The money decision. |
| [`../../decisions/ADR-0009-receipt-references.md`](../../decisions/ADR-0009-receipt-references.md) | The receipt decision. |

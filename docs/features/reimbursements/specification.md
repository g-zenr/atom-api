# Reimbursements — specification

> **Status:** Implemented
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature identifier:** `ATOM-FEAT-REIMB-001`
> **Feature name:** Expense claims, line items, money and the claim lifecycle

## 1. Purpose and the user problem

The owner pays for things that somebody else will repay: a client dinner, a
train ticket for a site visit, a software licence bought for a project. The
record of those expenses is usually a pile of receipts and a mental note, and
the questions that matter arrive later: *what is still unpaid, what did this
project cost, and what did I claim for that trip?*

This feature answers those questions. It records an **expense claim** with its
line items, keeps the total equal to the sum of the parts, and tracks the
claim through a small, explicit lifecycle from draft to paid. It is a
bookkeeping tool, not an accounting system: it records what the owner says and
does not compute tax, apply an exchange rate or talk to a bank.

The problem it solves is **a trustworthy total**. A claim is only useful if
the number at the top is exactly the sum of the numbers below it, and if that
number cannot drift when an item is edited. Everything else in the feature —
the lifecycle, the ownership rule, the money format — exists to keep that
property true.

## 2. Status

**Implemented, not `Verified`.** The requirements are recorded in
[`project-truth.md`](../../project-truth.md) §18.1 as `REIMB-R01`…`R11` and
agreed on 2026-10-24. All twelve published operations are served, with the
migration, the module and the tests to match; no reviewer has read the chain
that completed them, so nothing is `Verified`. See [status.md](status.md).

The claim model and the lifecycle rest on assumptions **A1** and **A6** in
[`../../engineering/runs/chain/plan.md`](../../engineering/runs/chain/plan.md) §3,
which the owner did not explicitly confirm (owner decision O1). They are
recorded as **provisional defaults** in
[`../../project-truth.md`](../../project-truth.md) §18.4 and are **not**
owner-approved requirements.

## 3. Scope

In scope:

- Creating a claim with a title, an optional description, one currency and an
  optional project link.
- Listing the caller's claims with a status filter, a currency filter, a
  search term and a submission-date range, with the shared pagination.
- Reading one claim with its line items.
- Partially updating a claim's title, description and project link.
- Deleting a claim and its items.
- Adding, changing and deleting line items on a draft claim.
- Submitting, approving, rejecting and paying a claim, with the transitions
  the lifecycle allows.
- A total that always equals the sum of the items.
- Money as an exact four-decimal string, in one currency per claim.
- A receipt as an optional URL on a line item.
- Ownership isolation: a claim or item of another account is invisible, and
  the API answers `404`, never `403`.

## 4. Non-goals

These are deliberately **not** part of Phase 2:

- **Currency conversion or a second currency.** One claim, one currency, no
  exchange rate. `REIMB-R02`.
- **Approval by somebody else.** The owner is the only actor. `submitted`,
  `approved` and `paid` record what the owner did, not a workflow between
  people.
- **Uploaded receipt files.** A receipt is a URL or reference
  ([`ADR-0009`](../../decisions/ADR-0009-receipt-references.md),
  `REIMB-R08`).
- **A rejection reason.** A rejection records the state and its timestamp, not
  an explanation.
- **Tax, VAT, mileage rates, per-diem rules, or a currency total per
  category.** No requirement asks for them.
- **Reimbursement by a third party**, payment scheduling, bank integration or
  export to an accounting package.
- **Attachments other than the receipt URL**, comments, or an item history.
- **Partial approval.** A claim is approved or rejected as a whole.
- **A client-selectable sort.** The order of every list is fixed; see
  [api.md](api.md).

## 5. Accepted requirements

The source of truth is [`project-truth.md`](../../project-truth.md) §18.1.

| ID | Requirement | Implementation | Coverage |
| -- | ----------- | -------------- | -------- |
| `REIMB-R01` | A claim has a title, an optional description, one currency and zero or more items. | `src/reimbursements/service.rs`, `dto.rs` | `a_claim_can_be_created_read_and_deleted`, `an_item_can_be_added_to_a_draft_claim`, `an_item_can_be_partially_updated` |
| `REIMB-R02` | One currency per claim, immutable, no conversion. | `currency` column, required and upper-cased on create, absent from every request that could change it | `the_claim_currency_is_required_and_upper_cased`, `the_claim_currency_is_not_patchable` |
| `REIMB-R03` | The total always equals the sum of the items. | `SUM(amount)` recomputed in the item transaction | `adding_an_item_recomputes_the_total`, `deleting_an_item_recomputes_the_total`, `the_claim_total_equals_the_sum_of_its_items` |
| `REIMB-R04` | The five-state lifecycle, with `paid` terminal. | `POST …/submit`, `…/approve`, `…/reject`, `…/pay` | `a_draft_can_be_submitted_approved_and_paid`, `every_forbidden_transition_is_refused`, `paid_is_terminal` |
| `REIMB-R05` | Submission needs at least one item and a positive total. | The `WHERE` of the submit statement; the `422` field is chosen on the error path | `a_claim_without_items_cannot_be_submitted`, `a_claim_with_a_zero_total_cannot_be_submitted` |
| `REIMB-R06` | Items change only while the claim is a draft. | The claim row lock that opens the item transaction | `an_item_cannot_be_added_to_a_submitted_claim`, `an_item_cannot_be_changed_or_deleted_after_submission` |
| `REIMB-R07` | Money is `NUMERIC(19,4)` and a four-decimal JSON string. | `helpers::validation::validate_money` for input, `::text` selects for output ([`ADR-0007`](../../decisions/ADR-0007-money-representation.md)) | `an_item_amount_round_trips_with_four_decimals`, `an_item_amount_must_be_a_four_decimal_string`, `money_round_trips_with_four_decimals` |
| `REIMB-R08` | A receipt is a URL on a line item, never an upload. | `receipt_url` column and validator ([`ADR-0009`](../../decisions/ADR-0009-receipt-references.md)) | `a_receipt_url_must_be_absolute_http_or_https`. A file body is not tested: no route accepts one |
| `REIMB-R09` | An optional project link, cleared when the project is deleted. | `project_id` `ON DELETE SET NULL` | `deleting_the_project_clears_the_link_and_keeps_the_claim` |
| `REIMB-R10` | Every read and write is owner-scoped; a foreign claim is `404`. | `user_id = $n` in every statement ([`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md)) | `a_claim_of_another_user_is_invisible`, `an_item_of_another_users_claim_is_invisible`, `the_listing_only_contains_the_callers_claims`, `a_lifecycle_action_on_another_users_claim_is_not_found` |
| `REIMB-R11` | A claim may be deleted in any lifecycle status, and deleting it deletes its items. | `DELETE` with no status predicate; the items follow through `ON DELETE CASCADE` | `deleting_a_claim_deletes_its_items` (the schema), `a_claim_can_be_created_read_and_deleted` (a `draft`). `a_claim_can_be_deleted_in_any_status` is still to be written |

## 6. User-facing behaviour

### 6.1 Money

- Every amount in a request and in a response is a **string with exactly four
  decimal places**: `"0.0000"`, `"24.5000"`, `"1250.0000"`.
- A JSON number, a string with a different number of decimals, a negative
  amount, or a value above `"999999999999999.9999"` is a `422` naming the
  field.
- The **server** computes `total_amount`; a request never carries a total.
- The currency is a three-letter ISO 4217 code, trimmed and upper-cased. It is
  required on create and cannot be changed afterwards.
- No exchange rate and no conversion are performed anywhere.

The full convention is in [`../../api/conventions.md`](../../api/conventions.md)
§15 and the decision is [`ADR-0007`](../../decisions/ADR-0007-money-representation.md).

### 6.2 The claim lifecycle

```text
          submit                approve                pay
 draft ──────────► submitted ───────────► approved ─────────► paid
   ▲                    │
   │                    └── reject ──────► rejected
   └────────── reopen ────────────────────────┘

  paid: terminal. No transition leaves it.
```

| From | To | Request | Result |
| ---- | -- | ------- | ------ |
| `draft` | `submitted` | `POST /claims/{claim_id}/submit` | `200`; `submitted_at` set. |
| `submitted` | `approved` | `POST /claims/{claim_id}/approve` | `200`; `decided_at` set. |
| `submitted` | `rejected` | `POST /claims/{claim_id}/reject` | `200`; `decided_at` set. |
| `rejected` | `draft` | `POST /claims/{claim_id}/submit` | `200`; the reopen clears **both** `submitted_at` and `decided_at` (`paid_at` is already `null`). It is **not** submitted by the same request. |
| `approved` | `paid` | `POST /claims/{claim_id}/pay` | `200`; `paid_at` set. |
| any other pair | — | any of the four actions | `409 conflict`. |

**The `rejected → draft` transition has no endpoint of its own.** The agreed
endpoint list has no `/reopen`, and adding one would widen the accepted
contract. The reopen therefore shares `POST …/submit`, with one rule that
keeps it unambiguous:

> On a `rejected` claim, `POST …/submit` performs the **reopen only**. It
> returns `200` with the claim in `draft` and does not submit it. The client
> edits the items and calls `POST …/submit` again to submit.

The two-step behaviour is deliberate: a rejection usually means an item is
wrong, and a request that both reopens and submits would make it impossible to
fix the items before the second submission. This is the only place where an
endpoint name is broader than the transition it performs, and
[api.md](api.md) states it for each status.

### 6.3 Creating and editing

- A claim is created in `draft`, with `total_amount` `"0.0000"` and no
  decision or payment timestamps.
- `PATCH` changes only the fields present in the body. An omitted field keeps
  its value. `project_id` is nullable: `{"project_id": null}` clears it, and
  omitting it leaves it untouched.
- `currency` and `status` are not patchable. The lifecycle is driven by the
  action endpoints.
- `DELETE` removes the claim and its items and returns `204`. It is not
  reversible and it is allowed in any status (`REIMB-R11`) — see §9.

### 6.4 Line items

- An item has a required description, a required non-negative amount, a
  required `incurred_on` date, and optional `category` and `receipt_url`.
- After every item write the claim's total is recomputed **in the same
  transaction**, so a client never observes a total that does not match the
  items.
- Items can only be written while the claim is a `draft` (`REIMB-R06`).
- The items of a claim are read in `incurred_on ASC, created_at ASC, id ASC`
  order, so the order is chronological and total.

### 6.5 Listing

- The default order is `created_at DESC, id DESC`.
- Filters: `status`, `currency`, `search` (title and description),
  `submitted_from`, `submitted_to`. Their behaviour is in
  [`../../api/conventions.md`](../../api/conventions.md) §16.
- The listing returns claims **without** their items. The detail read returns
  the items.

### 6.6 Validation

| Field | Rule | Result |
| ----- | ---- | ------ |
| `title` | Trimmed, not empty, at most 200 characters. | `422` naming `title`. |
| `description` | At most 5000 characters. Kept verbatim. | `422` naming `description`. |
| `currency` | Three ASCII letters; trimmed; upper-cased. | `422` naming `currency`. |
| `amount` | Four-decimal string, `>= 0`, at most 15 integer digits. | `422` naming `amount`. |
| item `description` | Trimmed, not empty, at most 500 characters. | `422` naming `description`. |
| `incurred_on` | `YYYY-MM-DD`, a real calendar date. | `422` with field `body`: the date is deserialized, so the shared extractor reports the body failure rather than the field |
| `category` | Optional; trimmed and lower-cased; 1–64 characters; an empty value is stored as `null`. | `422` naming `category`. |
| `receipt_url` | Optional; absolute `http`/`https`; at most 2048 characters. | `422` naming `receipt_url`. |
| `project_id` | Optional UUID; must be owned by the caller. | `404` if it belongs to another account, `422` if it is not a UUID. |
| any `claim_id`/`item_id` in the path | UUID. | `400` if not. |

## 7. Dependencies on other features

| Direction | Relationship |
| --------- | ------------ |
| Reimbursements → Authentication | Every endpoint requires the `AuthUser` context; its `user_id` is the ownership predicate. |
| Reimbursements → Users | `expense_claims.user_id` and `expense_items.user_id` are non-null foreign keys to `users (id) ON DELETE CASCADE`. |
| Reimbursements → Projects | **Optional.** `project_id` is nullable, `ON DELETE SET NULL`. A claim does not need a project, and a foreign project is `404`. |
| Reimbursements → Tasks / Notes | **None.** A claim is not a task, and no note is attached to a claim. |
| Reimbursements → ML | **None.** Claim text is not embedded in Phase 2; `source_type` is `note` or `application` only. |

## 8. Acceptance criteria

A change to this feature is complete when all of the following hold.

- [ ] The three quality gates pass with a disposable `TEST_DATABASE_URL`, and
      `atom-docs` reports no error.
- [ ] Every requirement in §5 has a named test in [testing.md](testing.md),
      and the test fails if the rule is removed.
- [ ] Every allowed transition returns `200` and every forbidden one returns
      `409 conflict`, asserted one case per edge.
- [ ] `total_amount` equals `SUM(amount)` after every item mutation, asserted
      with values that a float would round (`0.1 + 0.2` must be `"0.3000"`).
- [ ] `"12.5000"` survives a create-read round trip unchanged, and a JSON
      number is refused.
- [ ] Every statement that touches a claim or an item carries `user_id = $n`,
      and a foreign claim, item or project is `404`.
- [ ] The pagination clamp and the filter behaviour are asserted on the list
      endpoint.
- [ ] Every path and method is in `/api/v1/openapi.json`, asserted by the
      contract test.
- [ ] The migration applies from an empty database, and
      [`schema.md`](../../database/schema.md) describes it.
- [ ] The seeder creates claims in every status, and a claim's total equals
      its items' sum in the seeded data.
- [ ] [status.md](status.md) reflects the evidence, and nothing is marked
      `Verified` without a recorded passing run.

## 9. Known limitations

- **A claim can be deleted in any status (`REIMB-R11`).** Deleting an approved
  or paid claim destroys the record of it. Phase 2 has no audit trail and no
  soft delete; the owner is the only actor, and the alternative is a lifecycle
  that cannot correct a mistake.
- **No rejection reason.** The state and the timestamp are recorded; the why
  is not.
- **One currency, four decimals, for every currency.** A JPY claim is rendered
  `"1500.0000"`. The scale belongs to the storage, not to the currency
  ([`ADR-0007`](../../decisions/ADR-0007-money-representation.md)).
- **The receipt URL is not verified.** The API checks the scheme and the
  length; it never fetches the target, so link rot is invisible until the
  owner clicks.
- **One receipt per item.** An item with several pages has to pick one.
- **No partial approval and no per-category totals.** The feature is a claim
  record, not an accounting report.
- **`rejected → draft` is reached through `/submit`.** It is the only place
  where the endpoint name is broader than the transition, and it is documented
  in §6.2 and in [api.md](api.md).
- **The claim list does not include items.** A client that needs the items of
  every claim on a page makes one detail request per claim.

## 10. Changelog references

Substantial changes are recorded in
[`change-log.md`](../../engineering/change-log.md). The feature identifier is
stable; the entries are appended and never rewritten.

- The Phase 2 product intake that accepted
  `REIMB-R01`…`R10`, [`ADR-0007`](../../decisions/ADR-0007-money-representation.md)
  and [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md). The
  change-log identifier is assigned when the intake is committed; no
  identifier is claimed here, because the log is the authority for it.

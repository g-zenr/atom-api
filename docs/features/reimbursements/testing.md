# Reimbursements — testing

> **Status:** Implemented
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

What the test suite contains for this feature, and what it deliberately does
not. Four layers are covered:

- the **schema layer**, by `tests/claims_schema.rs` (§1) and two cases in
  `tests/seeder.rs` (§1.1);
- the **endpoint layer**, by `tests/claims_api.rs` — **55 cases at `93fb2f0`**,
  of which 34 are the claim operations and **21 are the item and lifecycle
  operations** (§2) — and the contract cases in `tests/openapi_contract.rs`
  (§2.8);
- the **flow layer**, by `tests/claims_flow.rs` — **three cases, committed at
  `02c5489`**: the journey from registration to payment, the ownership sweep and
  the seeded read (§2.10);
- **ten unit tests** inside `src/reimbursements/`, plus three money tests in
  `src/helpers/validation.rs` (§4).

Every endpoint of the feature is callable and every case named here is on disk,
so no part of §2 is a plan. The cross-cutting cases are **not** in
`tests/claims_api.rs`: they live in `tests/claims_flow.rs`, so the 55 above
counts that file only and does not change.

This document describes the suite. The **result** of a run is not recorded here.
`ATOM-CHANGE-20261024-007` in
[`../../engineering/change-log.md`](../../engineering/change-log.md) records the
296-test suite before `tests/claims_flow.rs` existed, and
`ATOM-CHANGE-20261024-008` records the six gates green with **299** tests after
it. Both are the orchestrator's record, cited as such; this document ran
neither. [status.md](status.md) is where a status is derived from the record.
What must pass before a change is complete is stated in
[`verification-policy.md`](../../testing/verification-policy.md).

**The rule that makes this list evidence:** a test that cannot fail for the
reason it names is not evidence. Each test below states the rule it proves,
and the implementing change must show that removing the rule makes the test
fail — by deleting the predicate, inverting the transition or changing the
assertion, then restoring it.

## 1. The schema layer — `tests/claims_schema.rs`

The chain `C-R1` creates the tables and the row types and no endpoint, so its
test file works at the database layer: it applies migration `0004` to a real
PostgreSQL database and asserts each constraint by attempting the write the
constraint exists to refuse. There is no router call in this file.

| Test | What it proves |
| ---- | -------------- |
| `the_expense_claim_tables_apply_from_an_empty_database` | Migration `0004` applies after `0001`–`0003` from an empty database and creates `expense_claims`, `expense_items` and the `expense_claim_status` enum. |
| `a_claim_rejects_a_currency_that_is_not_three_upper_case_letters` | `expense_claims_currency_chk` refuses `usd` and `US` (`23514`). A four-character code is refused one step earlier, by the `CHAR(3)` type (`22001`), so the value is unrepresentable either way. `USD` is accepted and round-trips unchanged. This is the data half of `REIMB-R02`; the create rule and the immutability are the service's (§2.5). |
| `a_claim_rejects_a_negative_total` | `expense_claims_total_amount_chk`. The `SUM` that maintains the total is the item transaction's (§2.2, [architecture.md](architecture.md) §2.7). |
| `the_lifecycle_constraint_rejects_a_submitted_claim_without_a_submission_timestamp` | `expense_claims_lifecycle_timestamps_chk` refuses a `submitted` row with `submitted_at` null. |
| `the_lifecycle_constraint_accepts_every_consistent_status` | One consistent row per status is accepted, so the constraint is not merely restrictive by accident. |
| `an_item_cannot_belong_to_another_owners_claim` | The composite key `expense_items_claim_fk (claim_id, user_id)` refuses an item whose `user_id` is not its claim's owner. The `404` a client sees is `ClaimService`'s job; this proves the row cannot exist at all. |
| `deleting_a_claim_deletes_its_items` | `ON DELETE CASCADE` on the composite key. |
| `deleting_a_project_clears_the_claim_link_and_keeps_the_claim` | `ON DELETE SET NULL` on `project_id` — the data half of `REIMB-R09` and of invariant 12. The endpoint-level counterpart, which asserts the response, is `deleting_the_project_clears_the_link_and_keeps_the_claim` in §2.4. |
| `deleting_an_account_deletes_its_claims_and_items` | The two `users` cascades. |
| `money_is_stored_with_four_decimal_places` | `NUMERIC(19,4)` read as `::text` renders exactly four decimal places (`"149.8000"`), which is the value the API publishes. The test also records that PostgreSQL **rounds** a value with more decimals to the column's scale rather than rejecting it, so the four-decimal rule on **input** is the service's job, not the column's. |

This file does not prove a requirement end to end. It proves that the tables,
the constraints, the cascades and the money scale are what
[`schema.md`](../../database/schema.md) §11 says; the request-level half of
each requirement is §2.

### 1.1 The seeder's claims — `tests/seeder.rs`

The same chain extends the seeder, so its count check keeps passing, and adds
two cases to the existing seeder suite:

| Test | What it proves |
| ---- | -------------- |
| `the_seeder_dev_profile_inserts_the_documented_claim_counts` | The `dev` profile inserts exactly 25 claims and 48 items, counted **only** in the database and restricted to the two seeded accounts. The test discards the run's output, so the committed rows are the only witness — which is the stronger one, because a report can print the right numbers while the transaction rolls back. The profile counts are 1 / 25 / 50 claims and 0 / 48 / 98 items. |
| `a_seeded_claim_total_equals_the_sum_of_its_items` | Every seeded claim's `total_amount` equals `SUM(amount)` of its items, read back in SQL. This is the one place the total invariant is asserted on data the API did not write. |

The seeded rows are data, not feature behaviour: a claim that exists in the
database does not make any endpoint exist, and no test in this file calls the
router.

## 2. The endpoint layer — `tests/claims_api.rs`

The file grows in waves: `C-R2` adds the five claim operations and 34
cases, and `C-R3`/`C-R4` add the three item operations, the four lifecycle
actions and **21 cases**. The cross-cutting cases of `D2.R5` are not per-operation
cases at all and live in `tests/claims_flow.rs`, not in this file (§2.10), so the
count here stays at 55. Every test drives the real router
against a real PostgreSQL database through the shared harness in
`tests/common/mod.rs`: no mocks, a unique user per test, and — where ownership is
the subject — two accounts, because a cross-account case tested with one
account's token proves nothing.

The file's own comment sections are numbered `12.1`–`12.9`; this document
groups the same cases by the rule they prove, so the two numbering schemes
differ and neither is a link target.

The item and lifecycle wave **removes the need for most SQL fixtures**. It adds
`claim_in_status`, which creates a claim and walks it to a status through the
real action routes, so a lifecycle that cannot reach one of its own states
fails in the fixture instead of quietly making a transition test vacuous. Two
cases still insert rows with SQL — the listing filters, which need fixed
`created_at` instants, and the detail order, which needs scrambled
`incurred_on` values — and every fixture is scoped to the account the test
registered.

Two conventions apply to every row below: a money assertion compares the
**string**, never a parsed number, and an ownership assertion uses the second
account's token and expects `404` with code `not_found`, never `403`. The three
new sum assertions add a third: the expected sum is computed on `i128` units of
one ten-thousandth (`scaled` / `render_money` / `sum_of`), never on a float, so a
wrong total cannot pass through an approximate comparison.

### 2.1 Happy paths

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `a_claim_can_be_created_read_and_deleted` | `POST` returns `201` with `status` `draft`, `total_amount` `"0.0000"`, `items` `[]` and three null timestamps; `GET` returns the same claim; the listing carries it without an `items` key; `DELETE` returns `204`; a following `GET` returns `404`. It also asserts the shape rules: no `user_id` on the wire, and no `claim` wrapper around the detail. | `REIMB-R01` |
| `a_claim_can_be_created_with_a_project_of_the_caller` | A `project_id` that belongs to the caller is echoed back on create and on the read, so the project rule permits the legitimate case as well as refusing the foreign one. | `REIMB-R09` |
| `a_claim_can_be_partially_updated` | `PATCH {"title": "…", "project_id": null}` changes the title, leaves `description` untouched and clears the link; `PATCH {"description": ""}` clears the description and leaves the title; `PATCH {"project_id": "<uuid>"}` relinks; `PATCH {"title": null}` is treated as **omitted** and keeps the title. The persisted state is re-read and agrees with every response. | `REIMB-R01`, `REIMB-R09` |
| `a_patch_body_with_no_fields_changes_only_the_modification_time` | A claim is inserted with a fixed `created_at` as a fixture; `PATCH {}` is `200`, each of eleven fields is asserted equal to the value before the request, `created_at` is unchanged, and `updated_at` is **strictly later** than the fixed creation instant. | `REIMB-R01` |
| `the_claim_list_returns_the_callers_claims_newest_first` | Four claims with fixed `created_at` values, two of them sharing an instant so the identifier is the only tie-breaker, come back as `newest`, `tie-high`, `tie-low`, `oldest` — `created_at DESC, id DESC` — and the pagination block reports the four. | `REIMB-R01`, `REIMB-R10` |
| `an_item_can_be_added_to_a_draft_claim` | **New.** `POST …/items` returns `201` with the item: the description is trimmed (`"  Train, return  "` → `"Train, return"`), the category is trimmed and lower-cased (`"Travel"` → `"travel"`), the amount is the money string it was sent as, `incurred_on` is the day, and `receipt_url` is stored verbatim. `claim_id` is the claim it was posted under; there is **no** `currency` and no `user_id` on the wire; an absent `category` and an absent `receipt_url` are `null`, not `""`. The detail read then lists both items and their sum is `"130.3000"`. | `REIMB-R01`, `REIMB-R02` |
| `an_item_can_be_partially_updated` | **New.** `PATCH {"amount": "129.9000"}` changes the amount and leaves the description, the day, the category and the receipt untouched; the claim's total follows to `"129.9000"`. A second patch changes `description` (trimmed) and `incurred_on` and keeps the amount. `{"category": null, "receipt_url": null}` clears exactly those two, and the persisted item agrees with the last response. | `REIMB-R01`, `REIMB-R08` |

### 2.2 Money and the total

Money is now accepted **and** published. The four-decimal rule therefore has
two halves, and the invariant has a third test that walks all three item writes.

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `money_round_trips_with_four_decimals` | A new claim's total is exactly the string `"0.0000"`. A SQL fixture claim written with `"149.8"` and two items written with `"125.8"` and `"24.0"` are read back as `"149.8000"`, `"125.8000"` and `"24.0000"` — the scale is the column's output, and the comparison is on the string. | `REIMB-R07` |
| `a_money_field_is_never_a_json_number_in_a_response` | `total_amount` and every item `amount` are asserted to be JSON **strings** of one to fifteen integer digits and exactly four decimals, by type check rather than by comparison, so a future `f64` serialisation fails even if the value is arithmetically equal. | `REIMB-R07` |
| `the_total_is_not_accepted_from_the_client` | A create body carrying `"total_amount": "999.0000"` is `201` and the stored total is `"0.0000"`; a create body carrying the JSON **number** `12.5` in the same position is also `201` and ignored, not a `422`; `PATCH {"total_amount": "5.0000"}` is `200` and moves nothing. The server-owned half of the total rule. | `REIMB-R03` (server-owned half) |
| `an_item_amount_round_trips_with_four_decimals` | **New.** For each of `"125.8000"`, `"0.0000"` and `"999999999999999.9999"` on its own claim: the response echoes the amount at scale four, the stored item reads back as the value that was sent, and the one-item total equals it. The largest accepted magnitude is included because a rounding step anywhere in the path would show there and nowhere else. | `REIMB-R07` |
| `an_item_amount_must_be_a_four_decimal_string` | **New.** `"12.5"`, `"12.500"`, `"1,000.0000"`, `"1e3"` and `"-1.0000"` are each `422 validation_failed` naming `amount`; the JSON **number** `12.5` is `422 validation_failed` asserted by code only, because it never reaches the rule — the body fails to deserialize and the shared extractor reports the failure for the whole body; sixteen integer digits are `422` naming `amount`. Afterwards the claim has no item and its total is still `"0.0000"`, so a refused amount writes nothing. | `REIMB-R07` |
| `adding_an_item_recomputes_the_total` | **New.** From `"0.0000"`, three adds move the total to `"125.8000"`, `"149.8000"` and `"149.8001"`, read back through `GET` after each one and compared with the sum of the returned items. The `"0.0001"` step is the one a rounded or truncated total loses. | `REIMB-R03` |
| `deleting_an_item_recomputes_the_total` | **New.** Two items give `"149.8000"`; deleting one gives `"24.0000"`; deleting the last gives `"0.0000"`, not `null` and not a stale value; deleting the same item again is `404`, not `204`. The `204` is also asserted to carry no body. | `REIMB-R03` |
| `the_claim_total_equals_the_sum_of_its_items` | **New.** The sum through all three writes: three adds (`"149.8001"`), one amount replaced (`"224.0001"`), one item deleted (`"200.0001"`), then every remaining item deleted (`"0.0000"`). At each step the stored total is read from the claim — never from the mutating response — and asserted equal to **both** the items the server returned and an independently accumulated `i128` sum. | `REIMB-R03` |

An earlier version of this document listed five names as "R3's rows":
`the_total_equals_the_sum_of_the_items`,
`a_json_number_where_money_is_expected_is_refused`,
`an_amount_with_the_wrong_number_of_decimals_is_refused`,
`a_negative_amount_is_refused` and `an_amount_above_the_maximum_is_refused`.
They are **not** tests in `tests/claims_api.rs`, and this document no longer
claims them. The rules they described are proved by the five cases above under
the names the file actually uses; `a_negative_amount_is_refused` exists only as
a unit test in `src/helpers/validation.rs` (§4).

### 2.3 The lifecycle

Ten of the 21 new cases are here. Each names the edge it proves, and the
forbidden side is one case per pair rather than a sample.

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `a_draft_can_be_submitted_approved_and_paid` | The **flow test** for one claim: create, add an item, `submit` (`200`, `submitted_at` set, `decided_at` and `paid_at` null, the total undisturbed, the response carries the items), `approve` (`200`, `decided_at` set, `submitted_at` unmoved), `pay` (`200`, `paid_at` set, the other two instants unmoved), then `GET` returns the same three instants and the same total — so every step committed and not only answered. | `REIMB-R04` |
| `every_forbidden_transition_is_refused` | One case per forbidden combination, fifteen in all: from `draft` — `approve`, `reject`, `pay`; from `submitted` — `submit`, `pay`; from `approved` — `submit`, `approve`, `reject`; from `rejected` — `approve`, `reject`, `pay`; from `paid` — all four. Each is `409 conflict`, and the claim's status, its three timestamps and its total are asserted unchanged afterwards. The test also asserts that the five allowed pairs are **not** in its refusal table, so it is the complement of the lifecycle and not a second opinion about it. | `REIMB-R04` |
| `a_submitted_claim_cannot_be_approved_twice` | The second `approve` is `409` and does not re-stamp `decided_at`: the instant the first call wrote is still the stored one, and `paid_at` is still null. | `REIMB-R04` |
| `a_rejected_claim_reopens_and_can_be_resubmitted` | `submit` then `reject` leaves `submitted_at` and `decided_at` both set. `POST …/submit` on the rejected claim returns `200` with `draft` and clears **both**, and the row is re-read to prove the nulls are stored — a partial clear would be a `23514` at commit, so a wrong answer here would surface as a `500` nobody read. The reopened claim then accepts a new item (`201`) and a second `submit` returns `submitted` with `decided_at` null and the total of both items. | `REIMB-R04` |
| `paid_is_terminal` | All four actions on a paid claim are `409`, and the status, the three stamps and the total are unchanged after each. The claim is still readable with its item and its three instants. | `REIMB-R04` |
| `the_lifecycle_timestamps_are_set_by_the_transition` | A draft carries no stamp; `submit` sets `submitted_at` and nothing else; `approve` adds `decided_at`, leaves `submitted_at` where it was and asserts `decided_at >= submitted_at`; `pay` adds `paid_at` and asserts `paid_at >= decided_at`; `reject` stamps `decided_at` and never `paid_at`. The three instants are then re-read from `GET` and compared with the ones the responses carried. | `REIMB-R04` |
| `a_claim_without_items_cannot_be_submitted` | `422` with the violation naming `items`; the claim stays `draft` with three null stamps; adding one item makes the same call `200`, so the precondition is the reason and not the operation. | `REIMB-R05` |
| `a_claim_with_a_zero_total_cannot_be_submitted` | An item of `"0.0000"` is legal, so the claim has an item and the only fault is the total: `422` naming `total_amount`, claim still `draft`. An item of `"0.0001"` then makes `submit` `200` with a total of `"0.0001"`, which proves "greater than zero" rather than a rounded boundary. | `REIMB-R05` |
| `an_item_cannot_be_added_to_a_submitted_claim` | For **each** of `submitted`, `approved`, `rejected` and `paid`: `409 conflict`, and after the refusal the item count and the total are exactly what they were. | `REIMB-R06` |
| `an_item_cannot_be_changed_or_deleted_after_submission` | After `submit`, `PATCH` and `DELETE` on the item are both `409`; the item survives with its amount, the total is unchanged and the status is still `submitted`. | `REIMB-R06` |

### 2.4 Ownership

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `a_claim_of_another_user_is_invisible` | Another account's token gets `404` on `GET`, `PATCH` and `DELETE`; the claim is absent from the other account's listing; the other account cannot attach the owner's project to a claim of its own (`404`, and the refused link is not written); and the owner still reads the claim unchanged. | `REIMB-R10` |
| `the_listing_only_contains_the_callers_claims` | Two own claims plus one foreign claim: the first account's listing returns exactly its two with `pagination.total` 2, and the second account's returns exactly its one — the count is scoped like the page, in both directions. | `REIMB-R10` |
| `a_claim_cannot_reference_another_users_project` | A create and a `PATCH` with a **foreign** `project_id` are both `404`, and a project id that does not exist at all gives the same answer; no claim is created or changed; and the owner can then link the same project to its own claim, so the refusals are the ownership rule and not a broken fixture. | `REIMB-R09`, `REIMB-R10` |
| `deleting_the_project_clears_the_link_and_keeps_the_claim` | After the project is deleted through the projects API, `GET` returns the claim with `project_id` `null`, its title and description intact, and it is still in the listing. | `REIMB-R09` |
| `an_item_of_another_users_claim_is_invisible` | **New.** With the owner's claim id and item id: `POST …/items`, `PATCH …/items/{id}` and `DELETE …/items/{id}` from the second account are each `404 not_found`. The second account's listing is still empty (the claim was not adopted), and the owner's item and total are unchanged — so the refusals are not "the write happened and the read lied". | `REIMB-R10` |
| `an_item_id_under_the_wrong_claim_is_not_found` | **New.** Both claims belong to the **caller**, so ownership is not the reason: an item of the second claim addressed under the first is `404` for `PATCH` and for `DELETE`, and an id that exists nowhere is the same `404`. The item is still in the claim it belongs to, untouched, and the first claim still has no item and a `"0.0000"` total. | `REIMB-R10` |
| `a_lifecycle_action_on_another_users_claim_is_not_found` | **New.** The fixture is an `approved` claim, the sharpest case: with ownership ignored, `pay` would succeed and `approve` would answer `409`, so every answer other than `404` is visibly different from "not the caller's". All four actions from the second account are `404 not_found`, the owner's status and three stamps are unchanged, and the second account's listing is empty. | `REIMB-R10` |

### 2.5 Validation and malformed requests

The `400`/`422` split is the one clients get wrong, so each row states which
side it lands on and why. A business rule is a `422` naming a field; a body
that does not match the contract at all is a `422` naming the field `body`; a
body that is not JSON, a path that is not a UUID and a query parameter that
cannot be parsed are `400`.

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `a_claim_title_must_not_be_blank` | `""` and `"   "` are `422` naming `title`, with the message `must not be empty`; a body with **no** `title` key is a `422 validation_failed` asserted by code only (a missing required field is a validation failure, not a `400`); `PATCH {"title": "   "}` is also `422` and the stored title is unchanged. | `REIMB-R01` |
| `a_claim_title_is_bounded` | 200 characters are accepted and returned unchanged; 201 are `422` naming `title` with `must be at most 200 characters long`; the bound counts **characters**, not bytes — 200 two-byte characters are accepted and 201 are refused. | `REIMB-R01` |
| `a_claim_title_is_trimmed_before_it_is_measured` | 200 characters surrounded by spaces (204 on the wire) is accepted and stored trimmed; 201 after trimming is `422` — the bound applies after trimming, not before. | `REIMB-R01` |
| `a_claim_description_is_bounded` | An omitted description is stored as `""`; a description with surrounding spaces is kept **verbatim**, not trimmed; 5000 characters are accepted and stored in full; 5001 are `422` naming `description`. | `REIMB-R01` |
| `the_claim_currency_is_required_and_upper_cased` | On **create**: a missing currency is `422`; `"EU"`, `"EURO"`, `"eur1"`, `""`, `"E1R"` and `"EU R"` are `422` naming `currency`; `"eur"` is accepted, returned and stored as `"EUR"`. The service refuses before the statement, so a four-character code never reaches `CHAR(3)`. | `REIMB-R02` |
| `the_claim_currency_is_not_patchable` | `PATCH {"currency":"USD"}` is `200` and the stored currency is unchanged; `PATCH {"currency":"usd","description":"still euros"}` changes the **description** and leaves the currency alone. This is the `api.md` §6 resolution asserted; [status.md](status.md) §1.2 records why it is not a `422`. | `REIMB-R02` |
| `the_claim_status_is_not_patchable` | `PATCH` with `status` **and** all three lifecycle timestamps is `200`; the status is still `draft` and all three timestamps are still `null`. `status` has no writer outside the four actions, and those write it through their own statements. | `REIMB-R04` |
| `a_project_id_that_is_not_a_uuid_is_refused` | `"not-a-uuid"` on create, `"not-a-uuid"` on `PATCH` and the number `7` on `PATCH` are each `422 validation_failed` with the single violation the extractor produces, whose field is `body`. | `REIMB-R09` |
| `a_reversed_date_range_is_refused` | `submitted_from` later than `submitted_to` is `422` naming `submitted_from`; equal bounds and a forward range are both `200`. | `REIMB-R01` (listing) |
| `a_claim_id_must_be_a_uuid` | `GET`, `PATCH` and `DELETE` on `/api/v1/claims/not-a-uuid` are each `400 bad_request`, on all three verbs that take the path parameter. | `REIMB-R10` |
| `a_malformed_body_is_refused` | A body that is not JSON at all — `{not json` on `POST`, `{oops` on `PATCH` — is `400`; a body that **is** JSON but is the wrong shape, `[]`, is `422` with field `body`. The distinction is the error contract's, asserted rather than assumed. | `REIMB-R01` |
| `an_unknown_status_filter_is_refused` | `?status=archived`, `=Submitted`, `=PAID` and `=` (empty) are each `400`, because the query string cannot be parsed into `ClaimStatus`; all five contract values are accepted in the same test, so the refusals are not "the filter never works". | `REIMB-R04` (the five states) |
| `a_malformed_claim_list_parameter_is_a_bad_request` | Seven unparsable queries — `?limit=abc`, `?offset=1.5`, `?limit=`, three bad `submitted_from` forms and `?submitted_to=31-10-2026` — are `400`; the same parameters with valid values are `200`. | `REIMB-R01` (listing) |
| `a_claims_body_over_the_limit_is_refused` | With a 256-byte body limit, a claim body of about 400 characters returns `413 payload_too_large`, so the refusal is about size and not about validation. | the error contract |
| `a_receipt_url_must_be_absolute_http_or_https` | **New.** Six refused forms — no scheme (`receipts.example.test/…`), `ftp://`, protocol-relative `//…`, `javascript:alert(1)`, a relative path `/receipts/…` and `http:/…` — are each `422` naming `receipt_url`. A 2049-character URL is refused and a 2048-character one is accepted, so the refusal is the bound and not "a long URL is never a URL". `https://` and `http://` are both accepted and stored verbatim, and only the accepted items exist afterwards. The host does not resolve and is never contacted: the test asserts the stored string, not a fetch. | `REIMB-R08` |

Four rules in this area are **implemented and not asserted at HTTP level**: the
1–500 item-description bound, a non-date `incurred_on`, a non-UUID `item_id`
and a `multipart`/file body. §3 records them with the case each still needs.

### 2.6 Listing and pagination

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `the_claims_listing_filters_by_status_and_currency` | Six claims inserted as fixtures (draft/submitted/approved in `EUR` and `USD`) are filtered by `?status=draft`, `?status=submitted`, `?status=approved`, `?currency=usd` (lower case, so the filter upper-cases) and `?status=approved&currency=EUR`; each case asserts the exact titles and that `pagination.total` is the **filtered** count. | `REIMB-R01`, `REIMB-R02` |
| `the_claims_listing_filters_by_submission_dates` | Three claims sit on the edges of their UTC days (23:59:59 and 00:00:00 included) plus one `draft`; five queries — no bound, a lower bound, an upper bound, and two single-day ranges — assert the exact titles **and** the filtered `pagination.total`. A draft is excluded as soon as either bound is present. | `REIMB-R01` |
| `the_claim_search_matches_title_and_description` | `berlin`, `BERLIN` and `Berlin` all match the title of one claim and the description of another (`total` 2); a term in a description only and a term in a title only each return exactly one claim; no match is an empty page, not `404`; `?search=` is "no filter" and returns all three. | `REIMB-R01` |
| `the_claims_listing_honours_the_pagination_bounds` | With a configured default of 7 and maximum of 13 and fifteen claims: `?limit=100000` is clamped to the configured maximum and still reports `total` 15 with `has_more` true; no limit falls back to the configured default; `?limit=0` does not bypass it; `?offset=-5` becomes 0; the second page and the last page assert their sizes and `has_more`; an offset past the end is an empty page with the right `total`, not `404`. The bounds are neither the shipped defaults nor a plausible constant, so a fallback to a literal cannot pass. | `REIMB-R01` |
| `a_currency_filter_that_is_not_three_letters_is_refused` | `?currency=EU`, `=EURO`, `=eur1`, `=E1R` and `=` are `422` naming `currency`. The empty value is a `422` and not "no filter", because `api.md` §2.6 gives that exception to `search` only. `?currency=%20eur%20` is trimmed and upper-cased and returns the one `EUR` claim. | `REIMB-R02` |
| `a_search_term_over_the_bound_is_refused` | A 201-character term is `422` naming `search`, with the shared validator's message; a 200-character term is accepted. | `REIMB-R01` |
| `the_claim_detail_items_are_in_chronological_order` | Three items written as a SQL fixture in a scrambled order come back as morning, midday, late — `incurred_on`, then `created_at`, then `id` — and each item is asserted to carry the claim's id and a money string. The item routes do not change the order, and this remains the only case that fixes it with rows the API did not write. | `REIMB-R01` |

### 2.7 The error codes

| Code | Status | Proving test |
| ---- | ------ | ------------ |
| `bad_request` | `400` | `a_claim_id_must_be_a_uuid`, `an_unknown_status_filter_is_refused`, `a_malformed_body_is_refused`, `a_malformed_claim_list_parameter_is_a_bad_request` |
| `unauthorized` | `401` | `the_claim_routes_require_a_credential` |
| `not_found` | `404` | `a_claim_of_another_user_is_invisible`, `a_claim_cannot_reference_another_users_project`, `an_item_of_another_users_claim_is_invisible`, `an_item_id_under_the_wrong_claim_is_not_found`, `a_lifecycle_action_on_another_users_claim_is_not_found`, `deleting_an_item_recomputes_the_total` (a second delete), and the eleven claim-addressed operations of `the_second_account_sees_not_found_on_every_reimbursement_operation` (§2.10) |
| `conflict` | `409` | `every_forbidden_transition_is_refused`, `a_submitted_claim_cannot_be_approved_twice`, `paid_is_terminal`, `an_item_cannot_be_added_to_a_submitted_claim`, `an_item_cannot_be_changed_or_deleted_after_submission`, and the seven refusals at `paid` in `the_whole_reimbursement_journey_from_registration_to_payment` (§2.10) |
| `payload_too_large` | `413` | `a_claims_body_over_the_limit_is_refused` (the mechanism is already proven by `a_body_over_the_limit_is_refused` in `tests/auth_api.rs`; this one proves it on a claims route) |
| `validation_failed` | `422` | the §2.5 table, plus `a_claim_without_items_cannot_be_submitted`, `a_claim_with_a_zero_total_cannot_be_submitted`, `an_item_amount_must_be_a_four_decimal_string` |
| `database_error` | `500` | **not proved on these routes.** It would need fault injection into a live pool; the mapping is unit-tested in `src/error.rs` and the feature adds no second mapping. §3 records the gap. |
| `timeout` | `408` | **not proved on these routes.** Proven globally by the timeout test in `tests/projects_tasks_api.rs`; the claims routes use the same middleware, which is an argument, not an assertion. §3 records the gap. |

| Test | What it proves |
| ---- | -------------- |
| `the_claim_routes_require_a_credential` | **Extended from five operations to twelve.** Every claim, item and lifecycle operation, called with no token and with a token that is not one, returns `401 unauthorized` with the standard envelope and **no** `details` array. The claim it is aimed at is a real one holding one real item, so an authenticated `DELETE` would have had something to remove and the `401` cannot be confused with a `404`; afterwards the claim is still `draft`, still carries its own title and its one item, and the amount is unchanged, so nothing the unauthenticated calls sent landed. |

### 2.8 The contract — `tests/openapi_contract.rs`

`CONTRACT_OPERATIONS` moves from 27 to **34**. The route/doc agreement tests
need no change of their own: they read `ROUTE_TABLE` and the document, so the
seven new rows are covered the moment both are updated in the same change.

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `each_list_endpoint_publishes_its_filter_and_pagination_parameters_once` | `GET /api/v1/claims` publishes exactly `limit`, `offset`, `status`, `currency`, `search`, `submitted_from` and `submitted_to` — once each, no duplicate, no extra. | `REIMB-R01` |
| `the_claim_money_fields_are_strings_in_the_document` | `ClaimResponse.total_amount` and `ItemResponse.amount` are `type: string` with the four-decimal pattern, never `type: number`, so a generated client cannot be handed a float. | `REIMB-R07` |
| `the_update_claim_request_publishes_a_nullable_project_id` | The `Option<Option<Uuid>>` seam appears in the document as a nullable field, not as a missing one — the published type and the DTO agree. | `REIMB-R09` |
| `the_item_and_lifecycle_operations_publish_their_success_status` | **New.** Each of the seven new operations is in the document with the success status the contract publishes: `201` for the item create, `200` for the item patch and for each of the four actions, `204` for the item delete. A handler whose status code drifts fails here rather than in a client. | `REIMB-R01`, `REIMB-R04` |
| `the_route_table_and_the_document_describe_the_same_operations` | **Unchanged, and now covers seven more rows.** Every table entry is an operation in the document and every operation is a table entry, in both directions, and the surface is pinned at 34. | the contract |
| `every_route_in_the_table_is_served_by_the_router` | **Unchanged, and now covers seven more rows.** Every entry is mounted; an unauthenticated probe must not answer the `404` fallback. | the contract |
| `the_version_prefix_is_declared_once` | **Unchanged.** The new routes take the prefix from the mount, so the literal count stays one. | the contract |

The three older names an earlier version of this document used —
`the_claim_routes_are_in_the_openapi_document`,
`the_money_fields_are_strings_in_the_document` and
`the_claims_listing_publishes_its_parameters_once` — are **not** tests. They
described the rules the real tests above now hold, and this document names the
real ones.

### 2.9 The requirement-to-test map

Every requirement now names at least one test. A requirement whose entry ends
in "still waiting for" is `Implemented` and not fully proved — the code exists,
the assertion does not.

**No row below rests on a planned case.** Every name in this map is a test on
disk; the three cross-cutting cases live in `tests/claims_flow.rs` (§2.10) and
are cited by the name the file actually uses. A requirement is never moved
closer to `Verified` by a name in this document: the promotion needs the review
rows of [status.md](status.md), not another test.

| Requirement | Proved by | Still waiting for |
| ----------- | --------- | ----------------- |
| `REIMB-R01` | §2.1 (seven cases: the five claim cases plus `an_item_can_be_added_to_a_draft_claim` and `an_item_can_be_partially_updated`), §2.5 (title, description, date range, malformed), §2.6 (search, dates, pagination, item order), §2.8 (the parameter set and the new status test) | The 1–500 item-description bound and a non-date `incurred_on`, both at HTTP level (§3) |
| `REIMB-R02` | §1 (`23514`), §2.1 (`an_item_can_be_added_to_a_draft_claim` asserts the item has no currency), §2.5 (`the_claim_currency_is_required_and_upper_cased`, `the_claim_currency_is_not_patchable`), §2.6 (currency filter) | Nothing — the claim-level rule is complete, and the item level has no currency field to test |
| `REIMB-R03` | §2.2 (`adding_an_item_recomputes_the_total`, `deleting_an_item_recomputes_the_total`, `the_claim_total_equals_the_sum_of_its_items`, `an_item_can_be_partially_updated`), §2.10 (the journey's stored total against an independently computed `"149.8001"`, re-asserted after `pay`), §1.1 (the seeded sum), §1 (the column) | A concurrency case: two item writes on one claim. The transaction and the row lock are argued in [architecture.md](architecture.md) §2.7, not measured |
| `REIMB-R04` | §2.3 (the ten cases), §2.10 (the journey asserts which stamp each step sets, that no earlier stamp moves and that the total is undisturbed), §4 (`the_transition_table_allows_exactly_the_five_edges`, `a_reviewed_claim_cannot_be_reopened`), §1 (both lifecycle-constraint cases), §2.7 (`an_unknown_status_filter_is_refused`) | Nothing |
| `REIMB-R05` | §2.3 (`a_claim_without_items_cannot_be_submitted`, `a_claim_with_a_zero_total_cannot_be_submitted`) | Nothing — both refusals and the success case after the fix are asserted |
| `REIMB-R06` | §2.3 (`an_item_cannot_be_added_to_a_submitted_claim`, `an_item_cannot_be_changed_or_deleted_after_submission`), and `a_rejected_claim_reopens_and_can_be_resubmitted` for the editable side | Nothing |
| `REIMB-R07` | §1 (the scale), §2.2 (the input rule, the round trip, the string type, the sum), §2.10 (the journey checks every published total and amount with the money-string type check, at scale four), §2.8 (the document), §4 (three money unit tests) | Nothing |
| `REIMB-R08` | §2.5 (`a_receipt_url_must_be_absolute_http_or_https`), §2.1 (`an_item_can_be_partially_updated` clears it), §4 (`a_receipt_must_be_an_absolute_http_url`), §1 (the column `CHECK`) | A `multipart`/file-body case (§3). The "never fetched" half is unprovable by design: the suite must not reach the network |
| `REIMB-R09` | §1 (`SET NULL`), §2.1, §2.4 (both directions), §2.5 (a bad UUID), §2.8 (the nullable field) | Nothing |
| `REIMB-R10` | §2.4 (seven cases, three of them new), §2.10 (`the_second_account_sees_not_found_on_every_reimbursement_operation`: the eleven claim-addressed operations on each of five fixtures, 55 requests, each `404 not_found`), §2.5 (path and query), §2.7 (all twelve operations without a credential) | A non-UUID `item_id` (`400`), which is the one path shape not asserted (§3) |
| `REIMB-R11` | §1 (`deleting_a_claim_deletes_its_items`, `deleting_an_account_deletes_its_claims_and_items`), §2.1 (`a_claim_can_be_created_read_and_deleted` on a `draft`), §2.4 (the foreign delete is `404` and the claim survives) | `a_claim_can_be_deleted_in_any_status`: the four non-draft statuses are reachable, and no case deletes an **owned** claim in one of them. The three flow cases do not fill this: the journey ends on a paid claim without deleting it, and the sweep's `DELETE` is a foreign attempt that answers `404` (§2.10) |

### 2.10 Cross-cutting — the journey, the sweep and the seeded read (`tests/claims_flow.rs`)

The three cases below are not per-operation cases, and they are not in
`tests/claims_api.rs`. They are in `tests/claims_flow.rs`, committed at
`02c5489`, and `ATOM-CHANGE-20261024-008` records them taking the suite from 296
to **299** with the six gates green. The file states its own
rules in its header, and they are what make the assertions worth having: every
assertion is on the status code and the observable body, never on a log line, a
timing value or a random value; a **stored** value (the total, the stamps) is
asserted on a fresh `GET`, never on the response of the call that wrote it; a sum
is computed independently as `i128` units of one ten-thousandth; every fixture
belongs to an account the file registered, so no count is global; and nothing
sleeps, because every transition's `200` is its own condition.

| Test | What it does | Requirement |
| ---- | ------------ | ----------- |
| `the_whole_reimbursement_journey_from_registration_to_payment` | One account, one database, one request sequence. Registers and reads the account back through `/auth/me`; logs in and asserts the new token authenticates that same account; creates a project; creates a claim whose title is padded and whose currency is lower case, and asserts the normalised values, the field shape of the detail, the three null stamps and a `"0.0000"` money string; adds **three** items; reads the stored total against a sum it computed itself; submits, approves and pays, asserting which stamp each step sets, which stay null and that no earlier stamp moves; then asserts that `paid` refuses all four actions **and** all three item writes with `409 conflict` and that the stored detail is byte-identical after each refusal; lists the claim; and ends with the ownership sweep on a second account, after which the claim is still byte-identical. | `REIMB-R01`, `REIMB-R03`, `REIMB-R04`, `REIMB-R07`, `REIMB-R10` |
| `the_second_account_sees_not_found_on_every_reimbursement_operation` | Five fixtures, one per published status, each holding one item and each built through the action routes. A second account, registered by the test, fires the **eleven claim-addressed operations** at every one of the five — 11 × 5 = **55 requests**, each asserted `404 not_found` with the standard envelope — and then `GET /claims` once per fixture, which the contract publishes as `200` with the caller's own set, so the assertion there is an empty page and `total: 0` rather than a `404`. Afterwards the owner's five claims are byte-identical and its listing still reports five. | `REIMB-R10` |
| `the_seeded_claims_are_readable_through_the_api` | One deterministic half and one opportunistic half. The deterministic half creates two claims through the API and walks a page of one twice (`?limit=1&offset=0`, then `offset=1`), asserting `total` 2, the right `has_more` on each page and that both ids come back from `?limit=2`. The seeded half runs **only** when `owner@atom.local` is in the database: it logs in as the published development account, asserts the account's published identity, lists `?limit=5&offset=0`, and asserts that the envelope and the pagination block describe the set truthfully, that every listed claim is in one of the five statuses with a money string, and that the first claim's stored total equals the sum of its items. | `REIMB-R01`, `REIMB-R03` |

**The journey's arithmetic.** The three items are `"125.8000"`, `"24.0000"` and
`"0.0001"`, which is `"149.8001"` — deliberately not a round number, so a total
that was rounded, truncated or recomputed as a float is visible. The expected
total is accumulated from the amounts the client sent, as `i128` units, and is
asserted equal to **both** the stored total and the sum of the items the server
returned; it is never taken from a response. The dates are posted 21, 19, 20 and
the detail must return them 19, 20, 21, so the documented
`incurred_on, created_at, id` order is asserted rather than assumed. One
description is padded and one category is mixed case, so the trimming and
lower-casing are asserted on the stored value. `created_at` is asserted equal to
the creation value on the read after the item writes and after `submit` and
`approve`, and `updated_at` is asserted not to move backwards when the items are
added.

**The stamps at each step.** `draft`: all three null. `submit`: `submitted_at`
set, `decided_at` and `paid_at` null, the total undisturbed, and a following
`GET` returns the same instant — so it is stored, not echoed. `approve`:
`decided_at` set, `submitted_at` **exactly** where it was, `paid_at` still null,
`decided_at >= submitted_at`. `pay`: `paid_at` set, both earlier instants
unchanged, `paid_at >= decided_at`, and the re-read detail equals the response
the transition returned. The total is re-asserted at every step and again
against the item sum after payment.

**`paid` is terminal for every write, not only for `pay`.** The four actions and
the three item writes are each `409 conflict`, and after each refusal the whole
detail is compared with the snapshot taken before the first one. A field-by-field
check on the status alone would pass while a stamp moved; the whole-document
comparison catches that.

**The sweep, and why `GET /claims` is not in it.** The eleven claim-addressed
operations are: create a claim on the owner's project, read/update/delete the
claim, add/update/delete an item, and `submit`/`approve`/`reject`/`pay`. The
list operation is the twelfth and is asserted separately, because it **cannot**
answer `404`: the contract publishes `200` with the caller's own set. The
property that does hold is asserted instead — the second account's page is empty
and its `total` is 0, since it owns nothing. `409` is what makes this more than a
smoke test: on the `paid` fixture every action answers `409` for *anyone*, so a
chain that checked the status before ownership would surface here as `409` and
fail, while a single foreign read would not notice. The five fixtures are built
through the action routes rather than with SQL, so a lifecycle that cannot reach
one of its own states fails in the fixture instead of making the sweep vacuous.

**What the flow layer does not cover.**

1. **The `408` and `500` paths.** A `408` needs a request slow enough to trip the
   configured timeout and a `500` needs fault injection into a live pool; a
   journey that passes says nothing about either (§3 item 2).
2. **The `rejected` branch.** The journey walks
   `draft → submitted → approved → paid`; reject-and-reopen stays proved only by
   `a_rejected_claim_reopens_and_can_be_resubmitted` (§2.3). The sweep holds a
   `rejected` fixture, so the status is reachable, but no flow walks its edges.
3. **A deletion in a non-draft status.** The journey ends on a paid claim and
   does not delete it; the sweep's `DELETE` is a foreign attempt that answers
   `404`. `a_claim_can_be_deleted_in_any_status` is still missing (§3).
4. **The seeded surface as a guarantee.** The seeded half is **opportunistic**:
   it asks the database for `owner@atom.local` before logging in — deliberately
   before, so "the seeder has not run here" is a skip while "the published
   password no longer authenticates" is a failure — and returns with a printed
   reason when the account is absent. The seeded surface is therefore covered
   when a seeder run has touched the database, not on every run. The seeded
   **size** is never asserted either: the suite shares one database, so whichever
   profile ran last decides it, and only the truthfulness of the envelope, the
   field shapes and the total invariant are asserted.
5. **Concurrency.** No flow case runs two writers at once (§3).

## 3. What is still not covered

This section is the visible gap. Three of its items are the ones that matter
most:

1. **The seeded-claim flow across the API — covered only when the seeder has
   run.** `the_seeded_claims_are_readable_through_the_api` (§2.10) is now the
   case that reads `atom-seed`'s rows through the router: it logs in as the
   published development account, lists its claims and checks one detail's total
   against the sum of its items, so the seeder's five statuses and the endpoint's
   field rules do meet in one test — when that database has been seeded. It is
   **opportunistic**: the case asks the database for `owner@atom.local` and
   returns with a printed reason when the account is absent, so a run on a
   database the seeder has not touched passes without covering the seeded
   surface. It also never asserts the seeded **size**, because the suite shares
   one database and whichever profile ran last decides it. What stays open is a
   case that guarantees the rows exist: nothing in this file runs the seeder.
2. **The `408` and `500` paths on these routes.** `timeout` is proved by the
   timeout test in `tests/projects_tasks_api.rs` and `database_error` is
   unit-tested in `src/error.rs`; neither is asserted on a claim, item or
   lifecycle route. The middleware is shared, which is an argument, not an
   assertion. A `408` would need a request slow enough to trip the configured
   timeout; a `500` needs fault injection into a live pool.
3. **The review rows.** `ATOM-CHANGE-20261024-007` (296 tests) and
   `ATOM-CHANGE-20261024-008` (299 tests, six gates green) record the runs; both
   are the orchestrator's record, so a run is no longer the missing piece. What
   is missing is the `code-reviewer` and `quality-gate` verdict on this chain.
   [status.md](status.md) therefore says `Implemented`, never `Verified`.

Everything else that is missing, with the rule each would prove:

| Missing case | The rule it would prove | Requirement |
| ------------ | ----------------------- | ----------- |
| `a_claim_can_be_deleted_in_any_status` | `DELETE` is `204` on a `submitted`, `approved`, `rejected` and `paid` claim, a following `GET` is `404`, and the items are gone. The code has no status guard, so this is an unasserted rule rather than a suspected defect. | `REIMB-R11` |
| An item-description bound | 500 characters accepted, 501 `422` naming `description`, measured after trimming. `validate_item_description` and `expense_items_description_chk` both hold the rule; no endpoint test does. | `REIMB-R01` |
| A non-date `incurred_on` | `"2026-02-30"` and `"yesterday"` are `422` with field `body`, because `incurred_on` is a `NaiveDate` and the shared extractor reports the body failure. The rule holds by type; no test sends one. | `REIMB-R01` |
| A non-UUID `item_id` | `PATCH /claims/{uuid}/items/not-a-uuid` is `400 bad_request`, as `a_claim_id_must_be_a_uuid` proves for the claim path. | `REIMB-R10` |
| A file body on the item route | A `multipart/form-data` body and a base64-only "file" body create no item. True today because the handler's extractor is `Json`, not because a test refused one. | `REIMB-R08` |
| Two concurrent item writes | Two inserts on one claim serialise on the row lock and the stored total equals the sum of both. Argued from `SELECT … FOR UPDATE`, not measured. | `REIMB-R03` |
| Two concurrent lifecycle actions | Two `approve` calls in flight: exactly one `200`, one `409`, one `decided_at`. Argued from the guarded `UPDATE`, not measured. | `REIMB-R04` |

## 4. Unit tests — in the module

**Eight tests live in `src/reimbursements/service.rs` and two in `dto.rs`**
after this chain, and three money tests were added to
`src/helpers/validation.rs`.

| File | Test | What it proves | Exists |
| ---- | ---- | -------------- | ------ |
| `dto.rs` | `a_project_link_is_kept_cleared_or_set_by_the_three_body_forms` | Three JSON bodies deserialised through `serde_json::from_str` produce `(false, None)`, `(true, None)` and `(false, Some(id))`: the `#[serde(default)]` + `double_option` pair cannot collapse `null` into "absent" ([architecture.md](architecture.md) §2.4). | Yes |
| `dto.rs` | `an_item_category_and_receipt_are_kept_cleared_or_set_by_the_three_body_forms` | The same three body forms through `category_change()` and `receipt_url_change()`: the item's two nullable fields cannot collapse `null` into "absent" either. | Yes — new |
| `service.rs` | `a_currency_is_three_letters_and_is_upper_cased` | `"eur"` → `"EUR"`; `"EU"`, `"EURO"`, `"1US"` and `""` refused. | Yes |
| `service.rs` | `a_reversed_submission_range_is_refused` | The `422` names `submitted_from`; equal bounds are accepted. | Yes |
| `service.rs` | `a_blank_search_means_no_filter` | `"   "` becomes "no filter", not a filter on spaces. | Yes |
| `service.rs` | `a_claim_title_is_trimmed_before_it_is_measured` | 200 characters plus surrounding spaces is accepted and stored trimmed. | Yes |
| `service.rs` | `the_transition_table_allows_exactly_the_five_edges` | Twenty combinations of five statuses and four actions: the **five** allowed pairs are exactly the rows of the lifecycle table (`draft`+`submit`, `submitted`+`approve`, `submitted`+`reject`, `rejected`+`submit` as the reopen, `approved`+`pay`), and the other **fifteen** are refused. Each assertion names the pair, so a failure says which edge moved. | Yes — new |
| `service.rs` | `a_reviewed_claim_cannot_be_reopened` | `approved` and `paid` cannot reach `draft`, and `paid` cannot be approved. These are the three refusals a client is most likely to want reversed. | Yes — new |
| `service.rs` | `a_receipt_must_be_an_absolute_http_url` | `https://…`, `http://…` and an upper-case `HTTPS://…` are accepted; `javascript:`, `file:`, `data:`, a relative path, `https://` alone and the empty string are refused; 2048 characters are accepted and 2049 are not. | Yes — new |
| `service.rs` | `an_item_category_is_trimmed_lower_cased_and_optional` | `"  Travel  "` → `Some("travel")`; an empty or blank value → `None` (the published "store `null`"); 64 characters accepted and 65 refused, naming `category`. | Yes — new |
| `helpers/validation.rs` | `money_requires_exactly_four_decimals` | `"0.0000"`, `"1.0000"`, `"125.8000"` and `"999999999999999.9999"` round-trip unchanged; fourteen refused forms — three decimals, five, no point, a sign, a plus, a grouping separator, an exponent, a currency symbol, surrounding spaces, two points, the empty string — each produce `422` naming `amount` **with the money-scale message**. | Yes — new |
| `helpers/validation.rs` | `money_is_bounded_to_fifteen_integer_digits` | `"999999999999999.0000"` is accepted and `"1000000000000000.0000"` is refused: the column holds nineteen digits, four of them fractional. | Yes — new |
| `helpers/validation.rs` | `a_negative_amount_is_refused` | `"-0.0100"` is refused and `"0.0000"` is accepted, so the rule is "non-negative" and not "positive". | Yes — new |

The three money tests are written against `validate_money` in
`helpers/validation.rs`, not against a `Money` constructor in `dto.rs`. An
earlier version of this document placed them in `dto.rs` and asserted a `Money`
type; the design rejected that type ([architecture.md](architecture.md) §2.3,
[`../../architecture/reimbursements-crud-design.md`](../../architecture/reimbursements-crud-design.md)
§4.3), so the rule they test is one function beside the shared text validators.
A test written against a type that does not exist would test the design, not
the code.

## 5. Deliberately not covered

| Gap | Why |
| --- | --- |
| **A `database_error` or a `timeout` raised through a claims route.** | A `500` would need fault injection into a live pool, and a `408` a request slow enough to trip the configured timeout without making the suite slow and flaky. Both mappings are already tested elsewhere; the feature must not add a second path to either. Recorded as a gap in §3, not as coverage. |
| **Concurrent writes on one claim.** | The item transaction's row lock and the lifecycle's guarded `UPDATE` are what make concurrency safe, and neither is exercised by two writers in one test. A correct concurrent test needs a controlled interleaving, which this harness does not offer; asserting it loosely would pass while the rule was broken. |
| **A direct SQL write that leaves the total stale.** | The API cannot produce it. The seeder's post-insert verification checks the invariant on seeded data, which is the closest honest test. |
| **A receipt URL that does not resolve.** | The API never fetches it; asserting that would test the network, not the API. |
| **The exact wording of most validation messages.** | The message comes from `config/messages.toml`; the tests assert the code and the field name, not the prose. The exceptions are deliberate: the blank-title rule, the three length bounds and the two submission messages **do** assert the exact sentence (`must not be empty`, `must be at most 200 characters long`, `must be at most 5000 characters long`), because those words are what a client shows its user. |
| **The seeded claims' exact contents.** | `tests/seeder.rs` owns that, and a change to the profile would break a claim-specific test for no product reason. |
| **The version-prefix literal count after twelve routes.** | The routes are mounted under the existing nested router, so `the_version_prefix_is_declared_once` stays the one assertion; a second assertion would duplicate it. |

## 6. Conventions for a new test

- A schema-level assertion belongs in `tests/claims_schema.rs`: it writes the
  row the constraint exists to refuse and asserts the database refuses it. It
  proves the data rule, never the response.
- A new endpoint needs a success case, a `401` case, a `404` cross-account
  case, a `400` malformed-path case and a `422` validation case.
- A money assertion compares the **string**, never a parsed number, and an
  expected sum is accumulated on scaled integers, never on a float.
- A body type mismatch is asserted as the field `body`, because that is what
  the shared extractor reports; a business rule is asserted as the field name
  the service names.
- A lifecycle test names the edge it proves; a table-driven test is acceptable
  only if each case has its own message that names the edge. The forbidden side
  is one case per pair, not a sample, and each case re-reads the claim to prove
  the refusal wrote nothing.
- A lifecycle fixture is built with the action routes (`claim_in_status`), not
  with SQL: a state the API cannot reach must fail in the fixture rather than
  make the case vacuous.
- A test that asserts a total re-reads the claim after the item write, so a
  stale column is caught; it never trusts the mutating response.
- A new route is added to `ROUTE_TABLE` in the same change, so the contract
  test covers it without a second list to maintain.
- A cross-cutting case belongs in `tests/claims_flow.rs`, not in
  `tests/claims_api.rs`: the per-operation file stays one operation per case, and
  a flow asserts a stored value on a fresh `GET`, never on the response that
  wrote it.
- A case that depends on another test binary's side effect — the seeder is the
  only one today — asks the database whether that effect happened and skips with
  a printed reason when it did not, rather than failing on a machine where the
  seeder has not run. A skip is recorded as a skip, never counted as coverage.

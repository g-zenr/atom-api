# Reimbursements — testing

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

What the test suite contains for this feature, and what it deliberately does
not. Three layers are covered and one is not:

- the **schema layer**, by `tests/claims_schema.rs` (§1) and two cases in
  `tests/seeder.rs` (§1.1);
- the **claim endpoint layer**, by `tests/claims_api.rs` — 34 cases (§2) —
  and two new cases in `tests/openapi_contract.rs` (§2.8);
- five **unit tests** inside `src/reimbursements/` (§4);
- **not** the item endpoints and the lifecycle: they do not exist, so their rows
  in §2.3, §2.4 and §2.5 are a plan with nothing to call, and §3 collects them
  in one place so the gap is visible.

This document describes the suite. It does not claim that any run happened;
what must pass before a change is complete is stated in
[`verification-policy.md`](../../testing/verification-policy.md), and the
record of a run belongs in that record and in [status.md](status.md), never
here.

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
| `a_claim_rejects_a_negative_total` | `expense_claims_total_amount_chk`. Nothing writes the total yet: the `SUM` that maintains it arrives with the item writes in R3. |
| `the_lifecycle_constraint_rejects_a_submitted_claim_without_a_submission_timestamp` | `expense_claims_lifecycle_timestamps_chk` refuses a `submitted` row with `submitted_at` null. |
| `the_lifecycle_constraint_accepts_every_consistent_status` | One consistent row per status is accepted, so the constraint is not merely restrictive by accident. |
| `an_item_cannot_belong_to_another_owners_claim` | The composite key `expense_items_claim_fk (claim_id, user_id)` refuses an item whose `user_id` is not its claim's owner. The `404` a client sees is `ClaimService`'s job; this proves the row cannot exist at all. |
| `deleting_a_claim_deletes_its_items` | `ON DELETE CASCADE` on the composite key. |
| `deleting_a_project_clears_the_claim_link_and_keeps_the_claim` | `ON DELETE SET NULL` on `project_id` — the data half of `REIMB-R09` and of invariant 12. The endpoint-level counterpart, which asserts the response, is `deleting_the_project_clears_the_link_and_keeps_the_claim` in §2.4. |
| `deleting_an_account_deletes_its_claims_and_items` | The two `users` cascades. |
| `money_is_stored_with_four_decimal_places` | `NUMERIC(19,4)` read as `::text` renders exactly four decimal places (`"149.8000"`), which is the value the API publishes. The test also records that PostgreSQL **rounds** a value with more decimals to the column's scale rather than rejecting it, so the four-decimal rule on **input** is the service's job, not the column's. |

This file does not prove a requirement end to end. It proves that the tables,
the constraints, the cascades and the money scale are what
[`schema.md`](../../database/schema.md) §11 says; a requirement that needs a
request stays partly built until an endpoint test exercises it.

### 1.1 The seeder's claims — `tests/seeder.rs`

The same chain extends the seeder, so its count check keeps passing, and adds
two cases to the existing seeder suite:

| Test | What it proves |
| ---- | -------------- |
| `the_seeder_dev_profile_inserts_the_documented_claim_counts` | The `dev` profile inserts exactly 25 claims and 48 items, counted **only** in the database and restricted to the two seeded accounts. The test discards the run's output, so the committed rows are the only witness — which is the stronger one, because a report can print the right numbers while the transaction rolls back. The profile counts are 1 / 25 / 50 claims and 0 / 48 / 98 items. |
| `a_seeded_claim_total_equals_the_sum_of_its_items` | Every seeded claim's `total_amount` equals `SUM(amount)` of its items, read back in SQL. This is the one place the total invariant is asserted before the feature owns it. |

The seeded rows are data, not feature behaviour: a claim that exists in the
database does not make any endpoint exist.

## 2. The endpoint layer — `tests/claims_api.rs`

The chain `C-R2` adds five claim operations and this file with them. Every test
drives the real router against a real PostgreSQL database through the shared
harness in `tests/common/mod.rs`: no mocks, a unique user per test, and — where
ownership is the subject — two accounts, because a cross-account case tested
with one account's token proves nothing.

The items of a claim are read but never written by an endpoint in this chain,
and no endpoint writes a status other than `draft`. Seven tests therefore seed
their claims — and, in three of them, their items — with a **SQL fixture** owned
by the account the test just registered, and assert on the HTTP response. That
is the only way to exercise the filters, the detail order and the money scale
before R3 exists; every fixture is scoped to that account, so it cannot make an
ownership test pass.

Two conventions apply to every row below: a money assertion compares the
**string**, never a parsed number, and an ownership assertion uses the second
account's token and expects `404` with code `not_found`, never `403`.

### 2.1 Happy paths

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `a_claim_can_be_created_read_and_deleted` | `POST` returns `201` with `status` `draft`, `total_amount` `"0.0000"`, `items` `[]` and three null timestamps; `GET` returns the same claim; the listing carries it without an `items` key; `DELETE` returns `204`; a following `GET` returns `404`. It also asserts the shape rules: no `user_id` on the wire, and no `claim` wrapper around the detail. | `REIMB-R01` |
| `a_claim_can_be_created_with_a_project_of_the_caller` | A `project_id` that belongs to the caller is echoed back on create and on the read, so the project rule permits the legitimate case as well as refusing the foreign one. | `REIMB-R09` |
| `a_claim_can_be_partially_updated` | `PATCH {"title": "…", "project_id": null}` changes the title, leaves `description` untouched and clears the link; `PATCH {"description": ""}` clears the description and leaves the title; `PATCH {"project_id": "<uuid>"}` relinks; `PATCH {"title": null}` is treated as **omitted** and keeps the title. The persisted state is re-read and agrees with every response. | `REIMB-R01`, `REIMB-R09` |
| `a_patch_body_with_no_fields_changes_only_the_modification_time` | A claim is inserted with a fixed `created_at` as a fixture; `PATCH {}` is `200`, each of eleven fields is asserted equal to the value before the request, `created_at` is unchanged, and `updated_at` is **strictly later** than the fixed creation instant. | `REIMB-R01` |
| `the_claim_list_returns_the_callers_claims_newest_first` | Four claims with fixed `created_at` values, two of them sharing an instant so the identifier is the only tie-breaker, come back as `newest`, `tie-high`, `tie-low`, `oldest` — `created_at DESC, id DESC` — and the pagination block reports the four. | `REIMB-R01`, `REIMB-R10` |

### 2.2 Money

In this chain money is **published and never accepted**: `total_amount` is
server-owned and the endpoint that accepts an `amount` is R3. The request-half
tests are marked as such and must not be claimed by `R2`.

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `money_round_trips_with_four_decimals` | A new claim's total is exactly the string `"0.0000"`. A SQL fixture claim written with `"149.8"` and two items written with `"125.8"` and `"24.0"` are read back as `"149.8000"`, `"125.8000"` and `"24.0000"` — the scale is the column's output, and the comparison is on the string. | `REIMB-R07` |
| `a_money_field_is_never_a_json_number_in_a_response` | `total_amount` and every item `amount` are asserted to be JSON **strings** of one to fifteen integer digits and exactly four decimals, by type check rather than by comparison, so a future `f64` serialisation fails even if the value is arithmetically equal. | `REIMB-R07` |
| `the_total_is_not_accepted_from_the_client` | A create body carrying `"total_amount": "999.0000"` is `201` and the stored total is `"0.0000"`; a create body carrying the JSON **number** `12.5` in the same position is also `201` and ignored, not a `422`; `PATCH {"total_amount": "5.0000"}` is `200` and moves nothing. The server-owned half of the total rule; the sum itself is §3. | `REIMB-R03` (server-owned half) |

The following rows are R3's and no `R2` change may claim them:
`the_total_equals_the_sum_of_the_items`,
`a_json_number_where_money_is_expected_is_refused`,
`an_amount_with_the_wrong_number_of_decimals_is_refused`,
`a_negative_amount_is_refused`,
`an_amount_above_the_maximum_is_refused`.

Two of them need their assertions stated precisely, because an earlier version
of this document promised something no code can deliver:

- **`a_json_number_where_money_is_expected_is_refused`** — `{"amount": 12.5}` is
  `422 validation_failed` with **one** violation whose `field` is `body`, not
  `amount`. The shared JSON extractor in
  [`../../../src/api/extract.rs`](../../../src/api/extract.rs) sees only the
  whole-body deserialisation error and reports every type mismatch under
  `body` ([`../../api/error-contract.md`](../../api/error-contract.md) §3). The
  test asserts the status, the code and that the field is `body`; a `Money`
  newtype in `dto.rs` would not change that, which is one of the reasons the
  design rejected the newtype ([architecture.md](architecture.md) §2.3).
- **`the_total_equals_the_sum_of_the_items`** — three items `"0.1000"`,
  `"0.2000"` and `"149.5000"` produce `"149.8000"`; the `0.1 + 0.2` pair is the
  case a float gets wrong.

### 2.3 The lifecycle

**Nothing in this section exists.** R2 can read `status` and cannot write it, so
there is no transition to test until R4 adds the four action endpoints; the two
item-write guards wait for R3.

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `every_forbidden_transition_is_refused` | One case per forbidden combination, fifteen in all. From `draft`: `approve`, `reject`, `pay`. From `submitted`: `submit`, `pay`. From `approved`: `submit`, `approve`, `reject`. From `rejected`: `approve`, `reject`, `pay`. From `paid`: `submit`, `approve`, `reject`, `pay`. Each is `409 conflict`. The unit test is the exhaustive case over all twenty combinations; this one proves the HTTP mapping for every refusal. | `REIMB-R04` |
| `paid_is_terminal` | `pay` on a paid claim is `409`, and the claim is still `paid` afterwards. | `REIMB-R04` |
| `a_claim_without_items_cannot_be_submitted` | `422` with the violation naming `items`; the claim stays `draft`. | `REIMB-R05` |
| `a_claim_with_a_zero_total_cannot_be_submitted` | An item of `"0.0000"` gives a total of `"0.0000"`; `submit` is `422` naming `total_amount`. | `REIMB-R05` |
| `a_draft_can_be_submitted_approved_and_paid` | The **flow test**: create, add item, submit (`200`, `submitted_at` set), approve (`200`, `decided_at` set), pay (`200`, `paid_at` set), then `GET` returns `paid` with all three timestamps. | `REIMB-R04` |
| `a_rejected_claim_reopens_and_can_be_resubmitted` | Reject a submitted claim; `POST …/submit` returns `200` with `draft`, and the reopen clears **both** `submitted_at` and `decided_at`; edit an item; `POST …/submit` again returns `200` with `submitted`. | `REIMB-R04` |
| `an_item_cannot_be_added_to_a_submitted_claim` | `409 conflict`; the total is unchanged. | `REIMB-R06` |
| `an_item_cannot_be_changed_or_deleted_after_submission` | `PATCH` and `DELETE` on the item are both `409`; the item and the total are unchanged. | `REIMB-R06` |

### 2.4 Ownership

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `a_claim_of_another_user_is_invisible` | Another account's token gets `404` on `GET`, `PATCH` and `DELETE`; the claim is absent from the other account's listing; the other account cannot attach the owner's project to a claim of its own (`404`, and the refused link is not written); and the owner still reads the claim unchanged. | `REIMB-R10` |
| `the_listing_only_contains_the_callers_claims` | Two own claims plus one foreign claim: the first account's listing returns exactly its two with `pagination.total` 2, and the second account's returns exactly its one — the count is scoped like the page, in both directions. | `REIMB-R10` |
| `a_claim_cannot_reference_another_users_project` | A create and a `PATCH` with a **foreign** `project_id` are both `404`, and a project id that does not exist at all gives the same answer; no claim is created or changed; and the owner can then link the same project to its own claim, so the refusals are the ownership rule and not a broken fixture. | `REIMB-R09`, `REIMB-R10` |
| `deleting_the_project_clears_the_link_and_keeps_the_claim` | After the project is deleted through the projects API, `GET` returns the claim with `project_id` `null`, its title and description intact, and it is still in the listing. | `REIMB-R09` |
| `an_item_of_another_users_claim_is_invisible` | *Not built* — needs the item endpoints (R3). | `REIMB-R10` |
| `an_item_id_cannot_be_used_under_another_claim_of_the_caller` | *Not built* — an item id from claim A used under claim B, both owned by the caller, must be `404`: not moved, not returned. Needs R3. | `REIMB-R10` |

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
| `the_claim_currency_is_not_patchable` | `PATCH {"currency":"USD"}` is `200` and the stored currency is unchanged; `PATCH {"currency":"usd","description":"still euros"}` changes the **description** and leaves the currency alone. This is the `api.md` §6 resolution asserted; [status.md](status.md) §1.1 records why it is not a `422`. | `REIMB-R02` |
| `the_claim_status_is_not_patchable` | `PATCH` with `status` **and** all three lifecycle timestamps is `200`; the status is still `draft` and all three timestamps are still `null` — `status` has no writer outside the four actions R4 adds. | `REIMB-R04` |
| `a_project_id_that_is_not_a_uuid_is_refused` | `"not-a-uuid"` on create, `"not-a-uuid"` on `PATCH` and the number `7` on `PATCH` are each `422 validation_failed` with the single violation the extractor produces, whose field is `body`. | `REIMB-R09` |
| `a_reversed_date_range_is_refused` | `submitted_from` later than `submitted_to` is `422` naming `submitted_from`; equal bounds and a forward range are both `200`. | `REIMB-R01` (listing) |
| `a_claim_id_must_be_a_uuid` | `GET`, `PATCH` and `DELETE` on `/api/v1/claims/not-a-uuid` are each `400 bad_request`, on all three verbs that take the path parameter. | `REIMB-R10` |
| `a_malformed_body_is_refused` | A body that is not JSON at all — `{not json` on `POST`, `{oops` on `PATCH` — is `400`; a body that **is** JSON but is the wrong shape, `[]`, is `422` with field `body`. The distinction is the error contract's, asserted rather than assumed. | `REIMB-R01` |
| `an_unknown_status_filter_is_refused` | `?status=archived`, `=Submitted`, `=PAID` and `=` (empty) are each `400`, because the query string cannot be parsed into `ClaimStatus`; all five contract values are accepted in the same test, so the refusals are not "the filter never works". | `REIMB-R04` (the five states) |
| `a_malformed_claim_list_parameter_is_a_bad_request` | Seven unparsable queries — `?limit=abc`, `?offset=1.5`, `?limit=`, three bad `submitted_from` forms and `?submitted_to=31-10-2026` — are `400`; the same parameters with valid values are `200`. | `REIMB-R01` (listing) |
| `a_claims_body_over_the_limit_is_refused` | With a 256-byte body limit, a claim body of about 400 characters returns `413 payload_too_large`, so the refusal is about size and not about validation. | the error contract |
| `an_item_description_is_bounded` | *Not built* — a 501-character item description is `422`. Needs R3. | `REIMB-R01` |
| `an_incurred_on_value_must_be_a_date` | *Not built* — `"2026-02-30"` and `"yesterday"` are `422`. Needs R3. | `REIMB-R01` |
| `a_receipt_url_with_a_non_http_scheme_is_refused` | *Not built* — `javascript:alert(1)`, `file:///etc/passwd` and `data:text/plain,x` are `422`; an `https://` URL is accepted and returned verbatim. Needs R3. | `REIMB-R08` |
| `a_receipt_url_is_bounded` | *Not built* — 2049 characters is `422`; 2048 is accepted. Needs R3. | `REIMB-R08` |
| `a_receipt_is_a_url_and_a_file_body_is_refused` | *Not built* — a `multipart/form-data` body and a base64-only "file" body do not create an item; the API never stores bytes. Needs R3. | `REIMB-R08` |
| `an_item_id_must_be_a_uuid` | *Not built* — `PATCH …/items/not-a-uuid` is `400`. Needs R3. | `REIMB-R10` |

### 2.6 Listing and pagination

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `the_claims_listing_filters_by_status_and_currency` | Six claims inserted as fixtures (draft/submitted/approved in `EUR` and `USD`) are filtered by `?status=draft`, `?status=submitted`, `?status=approved`, `?currency=usd` (lower case, so the filter upper-cases) and `?status=approved&currency=EUR`; each case asserts the exact titles and that `pagination.total` is the **filtered** count. | `REIMB-R01`, `REIMB-R02` |
| `the_claims_listing_filters_by_submission_dates` | Three claims sit on the edges of their UTC days (23:59:59 and 00:00:00 included) plus one `draft`; five queries — no bound, a lower bound, an upper bound, and two single-day ranges — assert the exact titles **and** the filtered `pagination.total`. A draft is excluded as soon as either bound is present. | `REIMB-R01` |
| `the_claim_search_matches_title_and_description` | `berlin`, `BERLIN` and `Berlin` all match the title of one claim and the description of another (`total` 2); a term in a description only and a term in a title only each return exactly one claim; no match is an empty page, not `404`; `?search=` is "no filter" and returns all three. | `REIMB-R01` |
| `the_claims_listing_honours_the_pagination_bounds` | With a configured default of 7 and maximum of 13 and fifteen claims: `?limit=100000` is clamped to the configured maximum and still reports `total` 15 with `has_more` true; no limit falls back to the configured default; `?limit=0` does not bypass it; `?offset=-5` becomes 0; the second page and the last page assert their sizes and `has_more`; an offset past the end is an empty page with the right `total`, not `404`. The bounds are neither the shipped defaults nor a plausible constant, so a fallback to a literal cannot pass. | `REIMB-R01` |
| `a_currency_filter_that_is_not_three_letters_is_refused` | `?currency=EU`, `=EURO`, `=eur1`, `=E1R` and `=` are `422` naming `currency`. The empty value is a `422` and not "no filter", because `api.md` §2.6 gives that exception to `search` only. `?currency=%20eur%20` is trimmed and upper-cased and returns the one `EUR` claim. | `REIMB-R02` |
| `a_search_term_over_the_bound_is_refused` | A 201-character term is `422` naming `search`, with the shared validator's message; a 200-character term is accepted. | `REIMB-R01` |
| `the_claim_detail_items_are_in_chronological_order` | Three items written as a SQL fixture in a scrambled order come back as morning, midday, late — `incurred_on`, then `created_at`, then `id` — and each item is asserted to carry the claim's id and a money string. | `REIMB-R01` |

### 2.7 The error codes

| Code | Status | Proving test |
| ---- | ------ | ------------ |
| `bad_request` | `400` | `a_claim_id_must_be_a_uuid`, `an_unknown_status_filter_is_refused`, `a_malformed_body_is_refused`, `a_malformed_claim_list_parameter_is_a_bad_request` |
| `unauthorized` | `401` | `the_claim_routes_require_a_credential` |
| `not_found` | `404` | `a_claim_of_another_user_is_invisible`, `a_claim_cannot_reference_another_users_project` |
| `conflict` | `409` | *Nothing yet* — no `409` is reachable in this chain; `every_forbidden_transition_is_refused` and `an_item_cannot_be_added_to_a_submitted_claim` arrive with R3 and R4 |
| `payload_too_large` | `413` | `a_claims_body_over_the_limit_is_refused` (the mechanism is already proven by `a_body_over_the_limit_is_refused` in `tests/auth_api.rs`; this one proves it on a claims route) |
| `validation_failed` | `422` | the §2.5 table |
| `database_error` | `500` | not provable through the HTTP API without fault injection; the mapping is unit-tested in `src/error.rs` and the feature adds no second mapping |
| `timeout` | `408` | proven globally by the timeout test in `tests/projects_tasks_api.rs`; a claims route uses the same middleware |

| Test | What it proves |
| ---- | -------------- |
| `the_claim_routes_require_a_credential` | All five claim operations, called with no token and with a token that is not one, return `401 unauthorized` with the standard envelope and **no** `details` array; afterwards the claim is unchanged, so nothing the unauthenticated calls sent landed. The seven operations R3 and R4 add must be added to this test in the change that adds them. |

### 2.8 The contract — `tests/openapi_contract.rs`

The existing file gains one row in the parameter-set expectation and two new
cases. The two route/doc agreement tests need no change: they read
`ROUTE_TABLE` and the document, so the five new rows are covered the moment
both are updated in the same change.

| Test | What it proves | Requirement |
| ---- | -------------- | ----------- |
| `each_list_endpoint_publishes_its_filter_and_pagination_parameters_once` | **Extended.** `GET /api/v1/claims` publishes exactly `limit`, `offset`, `status`, `currency`, `search`, `submitted_from` and `submitted_to` — once each, no duplicate, no extra. | `REIMB-R01` |
| `the_claim_money_fields_are_strings_in_the_document` | **New.** `ClaimResponse.total_amount` and `ItemResponse.amount` are `type: string` with the four-decimal pattern, never `type: number`, so a generated client cannot be handed a float. | `REIMB-R07` |
| `the_update_claim_request_publishes_a_nullable_project_id` | **New.** The `Option<Option<Uuid>>` seam appears in the document as a nullable field, not as a missing one — the published type and the DTO agree. | `REIMB-R09` |
| `the_route_table_and_the_document_describe_the_same_operations` | **Unchanged, and now covers five more rows.** Every table entry is an operation in the document and every operation is a table entry, in both directions. | the contract |
| `every_route_in_the_table_is_served_by_the_router` | **Unchanged, and now covers five more rows.** Every entry is mounted; an unauthenticated probe must not answer the `404` fallback. | the contract |
| `the_version_prefix_is_declared_once` | **Unchanged.** The five new routes take the prefix from the mount, so the literal count stays one. | the contract |

The three older names an earlier version of this document used —
`the_claim_routes_are_in_the_openapi_document`,
`the_money_fields_are_strings_in_the_document` and
`the_claims_listing_publishes_its_parameters_once` — are **not** tests. They
described the rules the real tests above now hold, and this document names the
real ones.

### 2.9 The requirement-to-test map

What each requirement has today, and what it still waits for. "Partly" means an
endpoint test proves one half and the other half has no test because the code
does not exist.

| Requirement | Proved today by | Still waiting for |
| ----------- | --------------- | ----------------- |
| `REIMB-R01` | §2.1 (five cases), §2.5 (title, description, date range, malformed), §2.6 (search, dates, pagination, item order), §2.8 (parameter set) | The item half: `an_item_can_be_added_changed_and_removed` (R3) |
| `REIMB-R02` | §1 (`23514`), §2.5 (`the_claim_currency_is_required_and_upper_cased`, `the_claim_currency_is_not_patchable`), §2.6 (currency filter) | Nothing — the claim-level rule is complete |
| `REIMB-R03` | §1 (the column), §1.1 (the seeded sum), §2.2 (the total is server-owned) | The API-level sum: `the_total_equals_the_sum_of_the_items` (R3) |
| `REIMB-R04` | §1 (the constraint), §2.5 (`the_claim_status_is_not_patchable`, `an_unknown_status_filter_is_refused`) | Every transition test in §2.3 (R4) |
| `REIMB-R05` | — | §2.3's two submission cases (R4) |
| `REIMB-R06` | — | §2.3's two item guards (R3) |
| `REIMB-R07` | §1 (the scale), §2.2 (the round trip and the string type), §2.8 (the document) | The four-decimal **input** rule (R3): `a_json_number_where_money_is_expected_is_refused` and its three siblings |
| `REIMB-R08` | §1 (the column) | The validator and the no-bytes cases (R3) |
| `REIMB-R09` | §1 (`SET NULL`), §2.1, §2.4 (both directions), §2.5 (a bad UUID), §2.8 (the nullable field) | Nothing at the claim level |
| `REIMB-R10` | §2.4 (claim half), §2.5 (path and query), §2.7 (the credential) | The item half of §2.4 (R3) |
| `REIMB-R11` | §1 (the cascade) | `a_claim_can_be_deleted_in_any_status`: the `DELETE` endpoint exists, but no API path produces a status other than `draft` until R4 |

## 3. Not built — the item endpoints and the lifecycle

This section is the visible gap. Every row here is a test that **does not
exist**, because the code it would call does not exist. None is counted as
coverage, and no requirement whose only proof is here may be called built.

| Area | Rows |
| ---- | ---- |
| The item endpoints (R3) | `an_item_can_be_added_changed_and_removed`; `an_item_description_is_bounded`; `an_incurred_on_value_must_be_a_date`; `a_receipt_url_with_a_non_http_scheme_is_refused`; `a_receipt_url_is_bounded`; `a_receipt_is_a_url_and_a_file_body_is_refused`; `an_item_id_must_be_a_uuid`; `an_item_of_another_users_claim_is_invisible`; `an_item_id_cannot_be_used_under_another_claim_of_the_caller`; `the_total_equals_the_sum_of_the_items`; the four money-input cases in §2.2 |
| The lifecycle (R4) | the eight rows in §2.3 |
| The cross-feature flow (R5) | `a_claim_can_be_deleted_in_any_status`, and the ownership sweep that calls all twelve operations with the second account's token |

## 4. Unit tests — in the module

Five tests live inside `src/reimbursements/` after this chain, and the rest of
the module's unit surface waits for the code it tests.

| File | Test | What it proves | Exists |
| ---- | ---- | -------------- | ------ |
| `dto.rs` | `a_project_link_is_kept_cleared_or_set_by_the_three_body_forms` | Three JSON bodies deserialised through `serde_json::from_str` produce `(false, None)`, `(true, None)` and `(false, Some(id))`: the `#[serde(default)]` + `double_option` pair cannot collapse `null` into "absent" ([architecture.md](architecture.md) §2.4). | Yes |
| `service.rs` | `a_currency_is_three_letters_and_is_upper_cased` | `"eur"` → `"EUR"`; `"EU"`, `"EURO"`, `"1US"` and `""` refused. | Yes |
| `service.rs` | `a_reversed_submission_range_is_refused` | The `422` names `submitted_from`; equal bounds are accepted. | Yes |
| `service.rs` | `a_blank_search_means_no_filter` | `"   "` becomes "no filter", not a filter on spaces. | Yes |
| `service.rs` | `a_claim_title_is_trimmed_before_it_is_measured` | 200 characters plus surrounding spaces is accepted and stored trimmed. | Yes |
| `service.rs` | `the_transition_table_allows_exactly_the_five_edges` | Twenty combinations of five statuses and four actions: the **five** allowed pairs are exactly the rows of the lifecycle table (`draft`+`submit`, `submitted`+`approve`, `submitted`+`reject`, `rejected`+`submit` as the reopen, `approved`+`pay`), and the other **fifteen** are refused. | No — R4 |
| `service.rs` | `a_reviewed_claim_cannot_be_reopened` | `approved` and `paid` cannot reach `draft`. | No — R4 |
| `service.rs` | `a_receipt_must_be_an_absolute_http_url` | `https://…` and `http://…` are accepted; `javascript:`, `file:`, `data:` and a relative path are refused. | No — R3 |
| `helpers/validation.rs` | `money_requires_exactly_four_decimals` | `"1.0000"` is accepted; `"1"`, `"1.0"`, `"1.000"`, `"1.00000"`, `""` and `"abc"` are refused. | No — R3 |
| `helpers/validation.rs` | `money_is_bounded_to_fifteen_integer_digits` | Sixteen digits are refused, fifteen are accepted. | No — R3 |
| `helpers/validation.rs` | `a_negative_amount_is_refused` | `"-0.0100"` is refused. | No — R3 |

The three money tests are planned against `validate_money` in
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
| **A `database_error` raised through the claims API.** | It would need fault injection into a live pool. The mapping is already unit-tested in `src/error.rs`; the feature must not add a second path to a `500`. |
| **Concurrent writes on one claim.** | In R2 every claim write is one statement, so there is nothing to serialise; the item transaction that would need this test is R3. |
| **A direct SQL write that leaves the total stale.** | The API cannot produce it. The seeder's post-insert verification checks the invariant on seeded data, which is the closest honest test. |
| **A receipt URL that does not resolve.** | The API never fetches it; asserting that would test the network, not the API. |
| **The exact wording of most validation messages.** | The message comes from `config/messages.toml`; the tests assert the code and the field name, not the prose. The exception is deliberate: the blank-title rule and the three length bounds **do** assert the exact sentence (`must not be empty`, `must be at most 200 characters long`, `must be at most 5000 characters long`), because those words are what a client shows its user. |
| **The seeded claims' exact contents.** | `tests/seeder.rs` owns that, and a change to the profile would break a claim-specific test for no product reason. |
| **The version-prefix literal count after five routes are added.** | The five routes are mounted under the existing nested router, so `the_version_prefix_is_declared_once` stays the one assertion; a second assertion would duplicate it. |

## 6. Conventions for a new test

- A schema-level assertion belongs in `tests/claims_schema.rs`: it writes the
  row the constraint exists to refuse and asserts the database refuses it. It
  proves the data rule, never the response.
- A new endpoint needs a success case, a `401` case, a `404` cross-account
  case, a `400` malformed-path case and a `422` validation case.
- A money assertion compares the **string**, never a parsed number.
- A body type mismatch is asserted as the field `body`, because that is what
  the shared extractor reports; a business rule is asserted as the field name
  the service names.
- A lifecycle test names the edge it proves; a table-driven test is acceptable
  only if each case has its own message that names the edge.
- A test that asserts a total re-reads the claim after the item write, so a
  stale column is caught.
- A new route is added to `ROUTE_TABLE` in the same change, so the contract
  test covers it without a second list to maintain.

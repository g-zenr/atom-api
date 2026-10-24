# Reimbursements — testing

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

What the test suite must contain for this feature. **None of these tests
exists yet**: the feature has no code, so every name below is a test to be
written with the implementation chain.

This document describes the planned suite. It does not claim that any run
happened; what must pass before a change is complete is stated in
[`verification-policy.md`](../../testing/verification-policy.md), and the
record of a run belongs in that record and in [status.md](status.md), never
here.

**The rule that makes this list evidence:** a test that cannot fail for the
reason it names is not evidence. Each test below states the rule it proves,
and the implementing change must show that removing the rule makes the test
fail — by deleting the predicate, inverting the transition or changing the
assertion, then restoring it.

## 1. Integration tests — `tests/claims_api.rs`

The tests use the shared harness in `tests/common/mod.rs`: a real router, a
real PostgreSQL database, a unique user per test, no mocks.

### 1.1 Happy paths

| Test | What it proves |
| ---- | -------------- |
| `a_claim_can_be_created_read_and_deleted` | `POST` returns `201` with `status` `draft` and `total_amount` `"0.0000"`; `GET` returns the same claim with `items` `[]`; `DELETE` returns `204`; a following `GET` returns `404`. |
| `an_item_can_be_added_changed_and_removed` | `POST …/items` returns `201`; the detail read returns one item whose `claim_id` matches; `PATCH …/items/{id}` changes only the amount; `DELETE` returns `204` and the item is gone. |
| `a_draft_can_be_submitted_approved_and_paid` | The **flow test**: create, add item, submit (`200`, `submitted_at` set), approve (`200`, `decided_at` set), pay (`200`, `paid_at` set), then `GET` returns `paid` with all three timestamps. |
| `a_rejected_claim_reopens_and_can_be_resubmitted` | Reject a submitted claim; `POST …/submit` returns `200` with `draft`, and the reopen clears **both** `submitted_at` and `decided_at` (`paid_at` is already `null`); edit an item; `POST …/submit` again returns `200` with `submitted`. |
| `a_claim_can_be_partially_updated` | `PATCH {"title": "…"}` changes the title and leaves `description` and `project_id` untouched; `PATCH {"project_id": null}` clears the link; `PATCH {"currency":"USD","status":"paid"}` changes neither: the currency keeps its stored value and the status is still the value it had before the request. This is the `AGENTS.md` §4.6 rule and the proof that `status` is not patchable, asserted rather than assumed. |
| `a_claim_can_be_deleted_in_any_status` | A claim in each of `draft`, `submitted`, `approved`, `rejected` and `paid` is deleted with `204`, and a following `GET` is `404`; its items are gone too (`REIMB-R11`). |
| `the_claim_search_matches_title_and_description` | `?search=` finds a claim by a word in the title and one by a word in the description, and not the third claim. |

### 1.2 Money

| Test | What it proves |
| ---- | -------------- |
| `the_total_equals_the_sum_of_the_items` | Three items `"0.1000"`, `"0.2000"`, `"149.5000"` produce `"149.8000"`. The `0.1 + 0.2` pair is the case that a float gets wrong. |
| `money_round_trips_with_four_decimals` | `"12.5000"` is returned byte-identically by the create and by the read; `"0.0000"` and `"999999999999999.9999"` are accepted. |
| `a_json_number_where_money_is_expected_is_refused` | `{"amount": 12.5}` is `422` and the violation names `amount`. |
| `an_amount_with_the_wrong_number_of_decimals_is_refused` | `"12.5"` and `"12.500"` are `422`; only four decimals are accepted. |
| `a_negative_amount_is_refused` | `"-0.0100"` is `422`. |
| `an_amount_above_the_maximum_is_refused` | `"1000000000000000.0000"` is `422`. |
| `the_total_is_not_accepted_from_the_client` | A create body carrying `total_amount` does not set it: the stored value is `"0.0000"`. |

### 1.3 The lifecycle

| Test | What it proves |
| ---- | -------------- |
| `every_forbidden_transition_is_refused` | One case per forbidden combination, fifteen in all. From `draft`: `approve`, `reject`, `pay`. From `submitted`: `submit`, `pay`. From `approved`: `submit`, `approve`, `reject`. From `rejected`: `approve`, `reject`, `pay`. From `paid`: `submit`, `approve`, `reject`, `pay`. Each is `409 conflict`. The unit test is the exhaustive case over all twenty combinations; this one proves the HTTP mapping for every refusal. |
| `paid_is_terminal` | `pay` on a paid claim is `409`, and the claim is still `paid` afterwards. |
| `a_claim_without_items_cannot_be_submitted` | `422` with the violation naming `items`; the claim stays `draft`. |
| `a_claim_with_a_zero_total_cannot_be_submitted` | An item of `"0.0000"` gives a total of `"0.0000"`; `submit` is `422` naming `total_amount`. |
| `an_item_cannot_be_added_to_a_submitted_claim` | `409 conflict`; the total is unchanged. |
| `an_item_cannot_be_changed_or_deleted_after_submission` | `PATCH` and `DELETE` on the item are both `409`; the item and the total are unchanged. |

### 1.4 Ownership

| Test | What it proves |
| ---- | -------------- |
| `a_claim_of_another_user_is_invisible` | Another account's token gets `404` on `GET`, `PATCH`, `DELETE` and on all four actions; the owner can still read the claim. |
| `an_item_of_another_users_claim_is_invisible` | `PATCH` and `DELETE` on the item with the other account's token are `404`. |
| `an_item_id_cannot_be_used_under_another_claim_of_the_caller` | An item id from claim A used under claim B, both owned by the caller, is `404` — not moved, not returned. |
| `the_listing_only_contains_the_callers_claims` | Two own claims plus one foreign claim: the listing returns two and `pagination.total` is 2. |
| `a_claim_cannot_reference_another_users_project` | A create or `PATCH` with a foreign `project_id` is `404`, and no claim is created or changed. |
| `deleting_the_project_clears_the_link_and_keeps_the_claim` | After the project is deleted, `GET` returns the claim with `project_id` `null` and its items intact. |

### 1.5 Validation and malformed requests

| Test | What it proves |
| ---- | -------------- |
| `a_claim_title_must_not_be_blank` | `"   "` is `422` naming `title`. |
| `a_claim_title_is_bounded` | A 201-character title is `422`; 200 is accepted. |
| `a_claim_description_is_bounded` | 5001 characters is `422`. |
| `the_claim_currency_is_required_and_immutable` | A missing or two-letter currency is `422`; a `PATCH` carrying a different currency leaves the stored one unchanged. |
| `an_item_description_is_bounded` | A 501-character item description is `422`. |
| `an_incurred_on_value_must_be_a_date` | `"2026-02-30"` and `"yesterday"` are `422`. |
| `a_receipt_url_with_a_non_http_scheme_is_refused` | `javascript:alert(1)`, `file:///etc/passwd` and `data:text/plain,x` are `422`; an `https://` URL is accepted and returned verbatim. |
| `a_receipt_url_is_bounded` | 2049 characters is `422`; 2048 is accepted. |
| `a_receipt_is_a_url_and_a_file_body_is_refused` | A `multipart/form-data` body and a base64-only "file" body do not create an item; the API never stores bytes. |
| `a_claim_id_must_be_a_uuid` | `GET /api/v1/claims/not-a-uuid` is `400`. |
| `an_item_id_must_be_a_uuid` | `PATCH …/items/not-a-uuid` is `400`. |
| `a_malformed_body_is_refused` | Truncated JSON is `400`; an empty body is `400`. There is no enum in a claim request body, so there is no body value that produces a `422` from the deserialiser. |
| `an_unknown_status_filter_is_refused` | `?status=archived` is `400`, because the query string cannot be parsed. |

### 1.6 Listing and pagination

| Test | What it proves |
| ---- | -------------- |
| `the_claims_listing_filters_by_status_and_currency` | Only the matching claims are returned, and `pagination.total` matches. |
| `the_claims_listing_filters_by_submission_dates` | `submitted_from`/`submitted_to` are inclusive, and a draft (`submitted_at` null) is excluded when either is present. |
| `a_reversed_date_range_is_refused` | `submitted_from` after `submitted_to` is `422`. |
| `the_claims_listing_honours_the_pagination_bounds` | With five claims, `?limit=2&offset=0` returns 2 items, `total` 5 and `has_more` true; a `limit` above the configured maximum is clamped; `offset` past the end returns an empty page, not `404`. |
| `the_claims_listing_is_newest_first` | The items arrive in `created_at DESC, id DESC` order. |
| `the_detail_items_are_in_chronological_order` | Items arrive in `incurred_on`, `created_at`, `id` order regardless of insertion order. |

### 1.7 The error codes

| Code | Status | Proving test |
| ---- | ------ | ------------ |
| `bad_request` | `400` | `a_claim_id_must_be_a_uuid`, `an_unknown_status_filter_is_refused`, `a_malformed_body_is_refused` |
| `unauthorized` | `401` | `the_claim_routes_require_a_credential` |
| `not_found` | `404` | `a_claim_of_another_user_is_invisible`, `an_item_of_another_users_claim_is_invisible` |
| `conflict` | `409` | `every_forbidden_transition_is_refused`, `an_item_cannot_be_added_to_a_submitted_claim` |
| `payload_too_large` | `413` | `a_claims_body_over_the_limit_is_refused` (the mechanism is already proven by `a_body_over_the_limit_is_refused` in `tests/auth_api.rs`; this one proves it on a claims route) |
| `validation_failed` | `422` | the §1.5 table |
| `database_error` | `500` | not provable through the HTTP API without fault injection; the mapping is unit-tested in `src/error.rs` and the feature adds no second mapping |
| `timeout` | `408` | proven globally by the timeout test in `tests/projects_tasks_api.rs`; a claims route uses the same middleware |

| Test | What it proves |
| ---- | -------------- |
| `the_claim_routes_require_a_credential` | All twelve operations, called without a token and with an invalid token, return `401 unauthorized` with the standard envelope. |
| `a_claims_body_over_the_limit_is_refused` | A claims body above `max_body_bytes` returns `413 payload_too_large`. |

### 1.8 The contract

| Test | What it proves |
| ---- | -------------- |
| `the_claim_routes_are_in_the_openapi_document` | Each of the twelve `(method, path)` pairs is an operation in `/api/v1/openapi.json`, and the document contains no claim operation that the router does not serve. The existing two-direction test in `tests/openapi_contract.rs` does the comparison once the routes are added to `ROUTE_TABLE`. |
| `the_money_fields_are_strings_in_the_document` | The `total_amount`, `amount`, `salary_min` and `salary_max` schemas are `type: string`, never `type: number`. |
| `the_claims_listing_publishes_its_parameters_once` | The list operation publishes `limit`, `offset`, `status`, `currency`, `search`, `submitted_from` and `submitted_to` exactly once each. |

## 2. Unit tests — in the module

| File | Test | What it proves |
| ---- | ---- | -------------- |
| `src/reimbursements/service.rs` | `the_transition_table_allows_exactly_the_five_edges` | Twenty combinations of five statuses and four actions: the **five** allowed pairs are exactly the rows of the lifecycle table (`draft`+`submit`, `submitted`+`approve`, `submitted`+`reject`, `rejected`+`submit` as the reopen, `approved`+`pay`), and the other **fifteen** are refused. |
| `src/reimbursements/service.rs` | `a_reviewed_claim_cannot_be_reopened` | `approved` and `paid` cannot reach `draft`. |
| `src/reimbursements/dto.rs` | `money_requires_exactly_four_decimals` | `"1.0000"` is accepted; `"1"`, `"1.0"`, `"1.000"`, `"1.00000"`, `""` and `"abc"` are refused. |
| `src/reimbursements/dto.rs` | `money_is_bounded_to_fifteen_integer_digits` | Sixteen digits are refused, fifteen are accepted. |
| `src/reimbursements/dto.rs` | `a_negative_amount_is_refused` | `"-0.0100"` is refused by the `Money` constructor. |
| `src/reimbursements/service.rs` | `a_receipt_must_be_an_absolute_http_url` | `https://…` and `http://…` are accepted; `javascript:`, `file:`, `data:` and a relative path are refused. |
| `src/reimbursements/service.rs` | `a_currency_is_three_letters_and_is_upper_cased` | `"eur"` becomes `"EUR"`; `"EU"` and `"EURO"` are refused. |

## 3. Deliberately not covered

| Gap | Why |
| --- | --- |
| **A `database_error` raised through the claims API.** | It would need fault injection into a live pool. The mapping is already unit-tested in `src/error.rs`; the feature must not add a second path to a `500`. |
| **Concurrent item writes on one claim.** | The transaction serialises on the claim row, and the test would be timing-dependent. The invariant is asserted after sequential writes; a concurrency test belongs to the hardening chain if it is added at all. |
| **A direct SQL write that leaves the total stale.** | The API cannot produce it. The seeder's post-insert verification checks the invariant on seeded data, which is the closest honest test. |
| **A receipt URL that does not resolve.** | The API never fetches it; asserting that would test the network, not the API. |
| **The exact wording of a validation message.** | The message comes from `config/messages.toml`; tests assert the code and the field name, not the prose. |
| **The seeded claims' exact contents.** | `tests/seeder.rs` owns that, and a change to the profile would break a claim-specific test for no product reason. |

## 4. Conventions for a new test

- A new endpoint needs a success case, a `401` case, a `404` cross-account
  case, a `400` malformed-path case and a `422` validation case.
- A money assertion compares the **string**, never a parsed number.
- A lifecycle test names the edge it proves; a table-driven test is acceptable
  only if each case has its own message that names the edge.
- A test that asserts a total re-reads the claim after the item write, so a
  stale column is caught.
- A new route is added to `ROUTE_TABLE` in the same change, so the contract
  test covers it without a second list to maintain.

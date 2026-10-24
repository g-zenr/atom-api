# Job applications — testing

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

What the test suite must contain for this feature. **None of these tests
exists yet**: the feature has no code, so every name below is a test to be
written with the implementation chain.

This document describes the planned suite. It does not claim that any run
happened; the rule for what must pass is in
[`verification-policy.md`](../../testing/verification-policy.md), and the
result of a run belongs in the verification record and in [status.md](status.md).

**The rule that makes this list evidence:** a test that cannot fail for the
reason it names is not evidence. Each test must be shown to fail when the rule
it protects is removed — the transition table entry deleted, the `user_id`
predicate dropped, the unique index removed — and pass again when restored.

## 1. Integration tests — `tests/job_applications_api.rs`

The tests use the shared harness in `tests/common/mod.rs`: the real router, a
real PostgreSQL database, a unique user per test, no mocks.

### 1.1 Companies — happy paths

| Test | What it proves |
| ---- | -------------- |
| `a_company_can_be_created_read_updated_and_deleted` | `POST` returns `201` with the trimmed name; `GET` returns it; `PATCH` changes only the website and leaves `notes`; `DELETE` returns `204`; a following `GET` returns `404`. |
| `a_company_name_is_unique_per_owner` | Creating `Acme` and then `acme` with the same token is `409 conflict`; the stored name is unchanged. |
| `two_owners_may_use_the_same_company_name` | The same name under two accounts is two successful creates. |
| `renaming_a_company_onto_an_existing_name_is_refused` | `PATCH` onto the other company's name is `409`; onto the same name in different case is accepted. |
| `a_company_can_be_deleted_once_its_applications_are_gone` | Delete the only application, then the company: `204`, and the company is gone. |
| `a_company_with_applications_cannot_be_deleted` | A company with one application: `DELETE` returns `409 conflict`; both the company and the application are still readable; delete the application; `DELETE` the company returns `204`. This is the test `JOB-R06` names. |

### 1.2 Companies — validation and ownership

| Test | What it proves |
| ---- | -------------- |
| `a_company_name_is_trimmed_and_bounded` | `"  Acme  "` is stored as `"Acme"`; `"   "` is `422`; 201 characters is `422`, 200 is accepted. |
| `a_company_website_must_be_an_http_url` | `javascript:…`, `file:///…` and a relative path are `422`; `https://…` is accepted and returned verbatim. |
| `a_company_notes_are_bounded` | 5001 characters is `422`. |
| `a_company_of_another_user_is_invisible` | Another token gets `404` on `GET`, `PATCH` and `DELETE`; the owner still reads it. |
| `the_companies_listing_only_contains_the_callers_companies` | One foreign company plus two own: `total` is 2. |
| `the_companies_listing_is_alphabetical_and_paginated` | Inserted out of order, the page arrives `lower(name)`, `name`, `id`; `?limit=1&offset=1` returns the second and `has_more` true. |
| `the_companies_listing_searches_by_name` | `?search=acm` matches `Acme` and not `Globex`. |
| `a_company_id_must_be_a_uuid` | `GET /api/v1/companies/not-a-uuid` is `400`. |

### 1.3 Applications — happy paths and the stage machine

| Test | What it proves |
| ---- | -------------- |
| `an_application_can_be_created_and_read` | `POST` returns `201` with the company, the role and the stage; `GET` returns the same application. |
| `an_application_defaults_to_wishlist` | A create without `status` stores `wishlist`. |
| `a_wishlist_application_can_reach_offer` | The **flow test**: create `wishlist`, `PATCH` to `applied`, `screening`, `interview`, `offer`, each `200` with the new stage and a newer `updated_at`. |
| `every_permitted_stage_change_is_accepted` | All twelve listed edges, one case each: the four forward edges and `rejected`/`withdrawn` from each of the four non-terminal stages. |
| `every_forbidden_stage_change_is_refused` | Every one of the **30** ordered pairs that is neither one of the 12 allowed edges nor a self-pair — a skipped stage, a backwards move, a move between `rejected` and `withdrawn`, and every move out of a terminal stage — is `409 conflict` and leaves the stage unchanged. |
| `offer_rejected_and_withdrawn_are_terminal` | One explicit case per terminal stage: no move **out of** a terminal stage is accepted, while re-sending the terminal stage itself is still a no-op (`200`, unchanged). |
| `sending_the_current_stage_is_accepted` | One case per stage: `PATCH {"status":"<current>"}` on each of the seven stages is `200` and changes nothing. These seven self-pairs are no-ops, not refusals. |
| `a_refused_stage_change_does_not_write_the_other_fields` | `PATCH {"status":"interview","role_title":"Changed"}` from `wishlist` is `409` **and** the role title is unchanged, because the request is one transaction. |
| `a_create_with_a_stage_other_than_wishlist_is_refused` | `POST` with `"status":"applied"` is `422`; the same body without `status` creates a `wishlist` application. Only `wishlist` or an omitted stage is accepted on create (`JOB-R02`). |

### 1.4 Applications — money values

| Test | What it proves |
| ---- | -------------- |
| `salary_money_round_trips_with_four_decimals` | `"60000.0000"` and `"75000.0000"` are returned byte-identically; `"0.0000"` is accepted. |
| `a_json_number_where_salary_is_expected_is_refused` | `{"salary_min": 60000}` is `422` naming `salary_min`. |
| `a_salary_bound_requires_a_currency` | `salary_min` without `salary_currency` is `422`. |
| `a_salary_minimum_above_the_maximum_is_refused` | `"80000.0000"` above `"75000.0000"` is `422`; the pair is unchanged. |
| `clearing_both_salary_bounds_clears_the_currency` | `PATCH` with both bounds `null` returns `salary_currency` `null`. |

### 1.5 Applications — validation and ownership

| Test | What it proves |
| ---- | -------------- |
| `an_application_cannot_target_another_users_company` | A create or a `PATCH` with a foreign `company_id` is `404`, and nothing is written. |
| `the_next_action_task_must_belong_to_the_caller` | A foreign `next_action_task_id` is `404`; the caller's own task is accepted. |
| `deleting_the_task_clears_the_next_action` | After `DELETE /tasks/{id}`, the application still exists with `next_action_task_id` `null`. |
| `a_role_title_is_trimmed_and_bounded` | 201 characters is `422`; `"  Engineer  "` is stored trimmed. |
| `an_application_date_must_be_a_date` | `applied_on` `"2026-02-30"` is `422`. |
| `a_job_url_must_be_an_http_url` | A `data:` value is `422`. |
| `an_unknown_stage_in_a_body_is_refused` | `{"status":"hired"}` is `422` (the body parses but breaks the contract), not `400`. |
| `an_application_of_another_user_is_invisible` | Another token gets `404` on `GET`, `PATCH` and `DELETE`. |
| `the_applications_listing_only_contains_the_callers_applications` | `total` excludes the foreign application. |
| `the_applications_listing_filters_by_stage_company_and_dates` | Each filter alone, and `applied_on` null excluded when a bound is present. |
| `the_applications_listing_is_newest_first` | Items arrive `created_at DESC, id DESC`. |
| `the_applications_listing_honours_the_pagination_bounds` | Five applications, `?limit=2&offset=0`: two items, `total` 5, `has_more` true; a limit above the configured maximum is clamped; an offset past the end is an empty page, not `404`. |
| `an_unknown_stage_filter_is_refused` | `?status=hired` is `400`, because the query string cannot be parsed. |
| `an_application_id_must_be_a_uuid` | `GET /api/v1/job-applications/not-a-uuid` is `400`. |

### 1.6 Events

| Test | What it proves |
| ---- | -------------- |
| `an_event_can_be_added_changed_and_deleted` | `POST` returns `201`; `PATCH` corrects `summary`, and a second `PATCH` corrects `kind`, `occurred_at` and `details`; each response is `200` and carries **no `updated_at` field**; `DELETE` returns `204` and the event is gone. The absence of `updated_at` is the locked decision `A-D1-1`, so the test asserts it explicitly rather than ignoring it. |
| `events_are_ordered_by_occurred_at` | Three events inserted out of order arrive `occurred_at DESC`, `id DESC`; two with the same `occurred_at` are ordered by `id DESC`. |
| `an_event_with_a_future_occurred_at_is_accepted` | A scheduled interview dated next month is `201`; there is no past-only rule. |
| `an_event_kind_is_one_of_six` | Each of the six is accepted; `"meeting"` is `422`. |
| `an_event_summary_is_required_and_bounded` | `""` is `422`; 201 characters is `422`. |
| `an_event_details_are_bounded` | 10 001 characters is `422`. |
| `an_event_occurred_at_requires_a_timezone` | `"2026-10-14T13:00:00"` without an offset is `422`. |
| `an_event_of_another_users_application_is_invisible` | `GET`, `POST`, `PATCH` and `DELETE` under a foreign application are `404`. |
| `an_event_id_cannot_be_used_under_another_application` | An event of the caller's application A used under application B is `404`, not moved and not returned. |
| `the_events_listing_honours_the_pagination_bounds` | Three events with `?limit=2` returns two and `has_more` true. |
| `deleting_an_application_removes_its_events` | After the application is deleted, the events endpoint on the old id is `404`, and a direct count is zero. |

### 1.7 The error codes

| Code | Status | Proving test |
| ---- | ------ | ------------ |
| `bad_request` | `400` | `a_company_id_must_be_a_uuid`, `an_unknown_stage_filter_is_refused`, `a_malformed_body_is_refused` |
| `unauthorized` | `401` | `the_job_routes_require_a_credential` |
| `not_found` | `404` | `a_company_of_another_user_is_invisible`, `an_application_of_another_user_is_invisible`, `an_event_of_another_users_application_is_invisible` |
| `conflict` | `409` | `a_company_name_is_unique_per_owner`, `every_forbidden_stage_change_is_refused`, `a_company_with_applications_cannot_be_deleted` |
| `payload_too_large` | `413` | `a_companies_body_over_the_limit_is_refused`; the mechanism is already proven by `a_body_over_the_limit_is_refused` in `tests/auth_api.rs` |
| `validation_failed` | `422` | the §1.2, §1.4 and §1.5 tables |
| `database_error` | `500` | not provable through the API without fault injection; the mapping is unit-tested in `src/error.rs` |
| `timeout` | `408` | proven globally by the timeout test in `tests/projects_tasks_api.rs` |

| Test | What it proves |
| ---- | -------------- |
| `the_job_routes_require_a_credential` | All fourteen operations, without a token and with an invalid one, return `401 unauthorized` in the standard envelope. |
| `a_malformed_body_is_refused` | Truncated JSON is `400`; valid JSON with an unknown `kind` is `422`. |
| `a_companies_body_over_the_limit_is_refused` | A companies body above `max_body_bytes` returns `413 payload_too_large`. |

### 1.8 The contract

| Test | What it proves |
| ---- | -------------- |
| `the_job_routes_are_in_the_openapi_document` | Each of the fourteen `(method, path)` pairs is an operation in `/api/v1/openapi.json`, and the document contains no job-application operation that the router does not serve. The existing two-direction test in `tests/openapi_contract.rs` covers it once the routes are in `ROUTE_TABLE`. |
| `the_salary_fields_are_strings_in_the_document` | `salary_min` and `salary_max` are `type: string` with a four-decimal pattern, never `type: number`. |
| `the_job_listings_publish_their_parameters_once` | The companies, applications and events list operations publish their parameter sets exactly once each. |
| `no_job_operation_generates_text` | No operation in the document has a path or a description that offers generation, and no request schema has a `prompt` field. This is the contract-level half of `JOB-R08`; the other half is an inspection of `src/job_applications/` showing no HTTP client and no model call. |

## 2. Unit tests — in the module

| File | Test | What it proves |
| ---- | ---- | -------------- |
| `src/job_applications/service.rs` | `the_stage_machine_allows_exactly_the_twelve_edges` | Of the **49** ordered pairs, **12** are allowed edges, **7** are accepted no-ops (the self-pairs: `applied → applied` and the other six) and the remaining **30** are refused. The test asserts the three counts and every pair by name. |
| `src/job_applications/service.rs` | `the_three_terminal_stages_have_no_exit` | No move **out of** `offer`, `rejected` or `withdrawn` is allowed; re-sending the terminal stage itself is a no-op, not an exit. |
| `src/job_applications/service.rs` | `a_company_name_is_compared_ignoring_case_and_whitespace` | `" Acme "` and `"acme"` collide; `"Acme"` and `"Acme2"` do not. |
| `src/job_applications/service.rs` | `a_salary_bound_without_a_currency_is_refused` | The pair rule, with `None` on each side in turn. |
| `src/job_applications/dto.rs` | `an_event_kind_is_one_of_six` | Each accepted value and one refused value. |
| `src/job_applications/service.rs` | `a_job_url_must_be_an_absolute_http_url` | Scheme and length checks, mirroring the receipt rule. |

## 3. Deliberately not covered

| Gap | Why |
| --- | --- |
| **Two concurrent stage changes on one application.** | The `SELECT … FOR UPDATE` serialises them, but the test would be timing-dependent. It belongs to the hardening chain if it is added at all; the invariant is stated in [architecture.md](architecture.md) §2.3. |
| **A live `409` from the company-delete race.** | It needs one request to insert while another deletes. The composite foreign key makes the orphan impossible; the race is analysed, not tested. |
| **A `database_error` through the API.** | Fault injection into a live pool; the mapping is already unit-tested in `src/error.rs`. |
| **An event whose `occurred_at` is exactly the same as another's, in a different timezone.** | The comparison is on the stored `timestamptz`, and the tie-break by `id` is tested. A timezone-rendering matrix would test PostgreSQL, not ATOM. |
| **The generated client's date handling.** | Out of scope for the API suite; the contract test asserts the schema type. |
| **A rejection reason, a rating or a stage history.** | They do not exist; a test would invent them. |

## 4. Conventions for a new test

- A new endpoint needs a success case, a `401` case, a `404` cross-account
  case, a `400` malformed-path case and a `422` validation case.
- A stage test names the edge it proves. A pair that is neither listed nor
  obviously forbidden gets its own case.
- A test that asserts a `PATCH` with `status` also asserts that the other
  fields of a refused request are unchanged.
- A listing test asserts the order with rows inserted out of order.
- A money assertion compares the string, never a parsed number.
- A new route is added to `ROUTE_TABLE` in the same change.

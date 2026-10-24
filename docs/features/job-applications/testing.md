# Job applications — testing

> **Status:** Implemented
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

What is covered today, in which file, and by which case.

**Every layer is written; no run is recorded.** `tests/job_applications_api.rs`
holds 51 endpoint cases, `tests/jobs_schema.rs` eleven schema cases,
`tests/job_applications_flow.rs` two flow cases, `tests/openapi_contract.rs`
carries four cases for this feature, and `tests/seeder.rs` one. The module holds
eighteen unit tests, thirteen in `service.rs` and five in `dto.rs`. Every name
below is a test **on disk**; none of them is a claim that a run passed. `cargo`
was not run while this document was written, because other agents were still
writing the tree.

The rule that makes a test evidence is in [`strategy.md`](../../testing/strategy.md)
§7.4: a test that cannot fail for the reason it names is not evidence. Each case
below must be shown to fail when the rule it protects is removed — the transition
table entry deleted, the `user_id` predicate dropped, the unique index removed,
`NO ACTION` changed to `RESTRICT` — and pass again when restored. What must pass
before a change is complete is in
[`verification-policy.md`](../../testing/verification-policy.md).

## 1. The schema suite — `tests/jobs_schema.rs`

Eleven cases against a real, disposable PostgreSQL database. There is no router
involved: a `CHECK`, a composite foreign key, an `ON DELETE` action and a
functional unique index are properties of PostgreSQL, so a test of them speaks
SQL. The file's own rule is that **every rejection names its SQLSTATE and, where
a constraint did the work, the constraint name** — "an error happened" does not
prove which rule fired — and that **every rejection is paired with an accepted
control row**, so a table that refuses everything cannot pass.

| # | Test | The rule it proves | Serves |
| - | ---- | ------------------ | ------ |
| 1 | `the_job_tables_apply_from_an_empty_database` | A scratch database created empty ends with `_sqlx_migrations` at versions `1..=5`, the three tables and the two enum types, and the scratch database is dropped afterwards. | The precondition of `JOB-R01`…`R09`. |
| 2 | `a_company_name_is_unique_per_owner_case_insensitively` | `Acme` stores with its casing; `acme` and `ACME` for the same owner are each `23505` naming `companies_user_id_lower_name_unique_idx`; the same name for a second owner is accepted; no rejected insert left a row. | `JOB-R01`. |
| 3 | `a_company_name_must_be_trimmed` | `"Globex "`, `" Initech"` and `"Umbrella  "` are each `23514` naming `companies_name_trimmed_chk`, after a trimmed control insert. | `JOB-R01`. |
| 4 | `an_application_cannot_belong_to_another_owners_company` | An application whose own `user_id` is honest and whose `company_id` belongs to another owner is `23503` naming `job_applications_company_fk`; the same row with the company's owner is accepted. | `JOB-R02`, `JOB-R09`. |
| 5 | `a_company_with_applications_cannot_be_deleted` | A direct `DELETE FROM companies` while an application remains is `23503` naming `job_applications_company_fk`; a company with no applications deletes; **and `DELETE FROM users` for the owner then succeeds**, taking the company and the application with it. The last step is the `NO ACTION` half — `RESTRICT` would fail it. | `JOB-R06`. |
| 6 | `deleting_an_application_deletes_its_events` | Deleting one application removes its three events through `ON DELETE CASCADE`, and another application's event survives. | `JOB-R06`. |
| 7 | `deleting_an_account_deletes_its_companies_applications_and_events` | `DELETE FROM users` removes the account's rows from all three tables through the `user_id` cascade, and another account's rows survive. | `JOB-R09` — the delete half of the ownership model. |
| 8 | `a_salary_bound_requires_a_currency` | A minimum with no currency, a maximum with no currency, and a currency with no bound are each `23514` naming `job_applications_salary_currency_required_chk`; the all-null shape and a complete range are accepted. | `JOB-R03`. |
| 9 | `a_salary_minimum_above_the_maximum_is_refused` | An inverted pair is `23514` naming `job_applications_salary_range_chk`; equal bounds are accepted; a negative bound is `23514` naming `job_applications_salary_non_negative_chk`, a **different** constraint with its own name. | `JOB-R03`. |
| 10 | `money_is_stored_with_four_decimal_places` | `"1490.5"` and `"2000"` read back through `::text` as `"1490.5000"` and `"2000.0000"`, and `"149.876549"` is **rounded** to `"149.8765"` by the column. The four-decimal **input** rule is the service's, and is asserted in §2.6 and §4. | `JOB-R03`. |
| 11 | `an_unknown_stage_is_unrepresentable` | All seven stages cast and store; `"onsite"` is `22P02` with **no constraint named**, because the enum type refuses the token before a `CHECK` could run. | `JOB-R04` — the vocabulary half. The edge table is §2.3. |

### 1.1 The seeder case — `tests/seeder.rs`

| Test | The rule it proves | Serves |
| ---- | ------------------ | ------ |
| `the_seeder_dev_profile_inserts_the_documented_company_and_application_counts` | The `dev` profile reports and stores 20 applications, 12 companies and 50 events, with zero skipped after `--reset`; the ids are the deterministic `SHA-256("atom-seed/v1/<entity>/<index>")` values; every application resolves a company, and that company has the application's owner; every seeded salary bound carries a currency and no currency appears without a bound. | The fixture half of `JOB-R01`, `JOB-R02`, `JOB-R03` and `JOB-R09`. |

The count itself is derived, not copied: 20 applications alternate between two
owners, so each owner has 10; each owner gets `ceil(10 / 3) + 2 == 6` companies,
which is 12; each application `i` carries `1 + (i % 4)` events, which over five
cycles is 50. `minimal` gives 1 / 5 / 1 and `demo` 40 / 18 / 100.

### 1.2 What the schema suite does **not** cover

Every rule below is enforced by `0005` and is not asserted by a schema test. The
third column names where it is asserted instead, if it is.

| Rule in the schema that no schema test asserts | Where it is asserted now |
| ---------------------------------------------- | ------------------------ |
| `companies_name_length_chk`, `companies_name_trimmed_chk` through the API, `companies_notes_length_chk`, `job_applications_role_title_length_chk`, `application_events_summary_length_chk`, `application_events_details_length_chk` | `a_company_name_is_trimmed_and_bounded`, `a_company_notes_are_bounded`, `a_role_title_is_trimmed_and_bounded`, `an_event_summary_is_required_and_bounded` (§2.2, §2.5). |
| `job_applications_description_length_chk` | **Nowhere.** No case sends a description over 10 000 characters. |
| `companies_website_chk` and `job_applications_job_url_chk` — the scheme half | `a_company_website_must_be_absolute_http_or_https`, `a_job_url_must_be_an_http_url` (§2.1, §2.2). |
| The same two checks — the **length** half (2048) | **Nowhere** through the API; no case sends a 2049-character URL. |
| `job_applications_source_chk` — the trim and the 1..64 bound | The unit test `a_source_is_trimmed_and_has_no_blank_spelling` (§4). No API case sends a blank `source`. |
| The `application_event_kind` vocabulary — no schema case inserts a bad kind | `an_event_kind_is_one_of_six` (§2.5) and the unit case of the same name (§4). |
| `occurred_at NOT NULL`, and that a future value is accepted | `an_event_with_a_future_occurred_at_is_accepted`, `an_event_occurred_at_requires_a_timezone` (§2.5). |
| `job_applications_next_action_task_fk … ON DELETE SET NULL` | `deleting_the_task_clears_the_next_action` (§2.4). |
| The orders the three listing indexes serve | `the_companies_listing_is_alphabetical_and_paginated`, `the_applications_listing_is_newest_first`, `events_are_ordered_by_occurred_at` (§2). An index is an access path, not an order. |

## 2. The endpoint suite — `tests/job_applications_api.rs`

Fifty-one cases. The file drives the real router against a real, disposable
PostgreSQL database: a unique account per test, the shared harness in
`tests/common/mod.rs`, no mocks. One SQL fixture exists —
`pin_created_at` writes an application's creation instant, because the API never
accepts `created_at` and the `created_at DESC, id DESC` order has to be decided
rather than hoped for. The assertion is still on the API response; the SQL only
builds the state.

The file groups its cases the way the contract does, and so does this section.

### 2.1 Companies

| Test | The rule it proves | Serves |
| ---- | ------------------ | ------ |
| `a_company_can_be_created_read_changed_and_deleted` | `201` with `user_id` absent and the optional fields defaulted; `GET`; `PATCH` changing `notes` and clearing `website` with `null` while leaving `name`; `204`; then `404`; the listing holds only the survivor. | `JOB-R01`, `JOB-R09`. |
| `a_company_name_is_unique_per_owner_ignoring_case` | `Acme`, `acme`, `ACME` and `"  acme  "` are each `409 conflict`; the stored row and its casing are unchanged. | `JOB-R01`. |
| `renaming_a_company_onto_an_existing_name_is_refused` | A rename onto another company's name is `409` in three casings and writes nothing; a rename onto the company's **own** name, and a case-only rewrite of it, are `200`. | `JOB-R01`. |
| `two_accounts_may_use_the_same_company_name` | The same name under two accounts is two `201`s, and each listing holds one. | `JOB-R01`. |
| `a_company_name_is_trimmed_and_bounded` | 200 characters surrounded by spaces is stored trimmed; `""` and `"   "` are `422` naming `name`, never `409`; 201 is `422`; the same on a rename. | `JOB-R01`. |
| `a_company_website_must_be_absolute_http_or_https` | A bare host, `ftp://`, `data:` and a protocol-relative value are each `422` naming `website`; `http://` is `201`. | `JOB-R01`. |
| `a_company_notes_are_bounded` | 5 000 characters is `201` and stored whole; 5 001 is `422` naming `notes`. | `JOB-R01`. |
| `a_company_with_applications_cannot_be_deleted` | `409 conflict`; then the company is still readable, the application is still readable with the same `company_id`, and the listing holds it. Nothing was deleted to make the delete succeed. | `JOB-R06`. |
| `a_company_can_be_deleted_once_its_applications_are_gone` | Delete the application, then the company: `204` **with no body**, then `404`, then an empty page. | `JOB-R06`. |
| `the_companies_listing_is_alphabetical_and_paginated` | Three names inserted out of order arrive `lower(name)`; `?limit=1&offset=1` returns the second and `has_more` is `true`. | `JOB-R01`, `JOB-R09`. |
| `the_companies_listing_searches_by_name` | A case-insensitive substring for three casings; no match is an empty page; an empty term is no filter; 201 characters is `422` naming `search`. | `JOB-R01`. |

### 2.2 Applications

| Test | The rule it proves | Serves |
| ---- | ------------------ | ------ |
| `an_application_can_be_created_read_changed_and_deleted` | `201` with every published field, `role_title` trimmed, money as four-decimal strings, `user_id` absent; a minimal create defaults `status` to `wishlist` and every optional field to `null`/`""`; `GET`; `PATCH` clearing `source`, `applied_on` and `job_url` with `null` while leaving the salary, title and description; `204`; then `404`; the company survives. | `JOB-R02`, `JOB-R03`. |
| `a_create_with_a_stage_other_than_wishlist_is_refused` | The other six stages are each `422` naming `status`; `"wishlist"` and an omitted stage are `201`; only those two rows exist afterwards. A create is not a transition, so this is a `422`, never a `409`. | `JOB-R02`, `JOB-R04`. |
| `a_role_title_is_trimmed_and_bounded` | `""` and `"   "` are `422`; 201 is `422`; 200 surrounded by spaces is stored trimmed; the same on a `PATCH`, and a refused `PATCH` leaves the stored title intact. | `JOB-R02`. |
| `an_application_date_must_be_a_date` | `2026-02-30`, `2026-13-01`, `02-10-2026` and `tomorrow` are each `422` naming `applied_on`. | `JOB-R02`. |
| `a_job_url_must_be_an_http_url` | `data:`, a bare host and `ftp://` are each `422` naming `job_url`. | `JOB-R02`. |
| `an_unknown_stage_in_a_body_is_refused` | `"hired"` is `422 validation_failed` — a well-formed body breaking the contract, never `409` and never `400` — and the stage is unchanged. | `JOB-R04`. |
| `the_applications_listing_is_newest_first` | Four applications with pinned creation instants, two of them tied, arrive `created_at DESC, id DESC`; the tie expectation is derived from the identifiers the server assigned. | `JOB-R09`. |
| `the_applications_listing_filters_by_stage_company_and_dates` | Six queries: unfiltered, stage, company, role search, both date bounds, and an upper bound alone; each asserts the titles and the `total`; an application with a `null` `applied_on` is excluded by a date bound; no match is an empty page; `?status=hired` and a non-UUID `company_id` are `400`; a reversed range is `422` naming `applied_from`. | `JOB-R09`. |

### 2.3 The stage machine

| Test | The rule it proves | Serves |
| ---- | ------------------ | ------ |
| `every_permitted_stage_change_is_accepted` | The table is pinned at 12 edges, 30 refusals and 7 self-pairs, and the three partition all 49 pairs. Each of the twelve edges is walked to its `from` stage through the API, then `PATCH`ed: `200` with the new stage, and the stored row agrees. | `JOB-R04`. |
| `sending_the_current_stage_is_accepted_and_changes_nothing` | One case per stage: re-sending the current stage is `200`, every content field is byte-identical before and after (in the response **and** in a later read), and `updated_at` never moves backwards. An implementation that treats "not one of the twelve edges" as "refuse" answers `409` here and passes the twelve-edge test. | `JOB-R04`. |
| `every_forbidden_stage_change_is_refused` | All 30 refused pairs are `409 conflict`; the stage, `created_at` and `updated_at` are unchanged after each, because the row was never written. | `JOB-R04`. |
| `offer_rejected_and_withdrawn_are_terminal` | For each terminal stage, all seven targets: the same stage is a `200` no-op and every other one is `409`, with the stage unchanged. | `JOB-R04`. |
| `a_refused_stage_change_does_not_write_the_other_fields` | `{"status":"interview","role_title":"Changed"}` from `wishlist` is `409`, and the title, the stage and `updated_at` are unchanged: the request is one statement, not a sequence. | `JOB-R04`. |

### 2.4 The cross-links to Phase 1

| Test | The rule it proves | Serves |
| ---- | ------------------ | ------ |
| `the_next_action_task_must_belong_to_the_caller` | A foreign task on create is `404` and nothing is created; the caller's own task is accepted and echoed; a foreign task on `PATCH` is `404` and the existing link is kept; a UUID that names no task is the same `404`. | `JOB-R07`. |
| `deleting_the_task_clears_the_next_action` | After `DELETE /tasks/{id}`, the application survives with `next_action_task_id` `null`, its title and stage intact. | `JOB-R07`. |
| `an_application_can_be_moved_to_another_of_the_callers_companies` | `PATCH {"company_id": other}` is `200` and stored; the old company then deletes with `204` and the new one is `409`, which proves the link moved rather than a second row appearing. | `JOB-R02`, `JOB-R06`. |

### 2.5 Events

| Test | The rule it proves | Serves |
| ---- | ------------------ | ------ |
| `an_event_can_be_added_changed_and_deleted` | `201` with the summary trimmed, `occurred_at` preserved, `user_id` and `updated_at` **absent**; a minimal create defaults `details`; two `PATCH`es correcting one field then the rest, each `200`, each with no `updated_at`, an omitted field left alone and an offset normalised to UTC; the stored row is the corrected one and `created_at` is not rewritten; `204` with no body; the listing holds only the survivor. | `JOB-R05`. |
| `events_are_ordered_by_occurred_at` | Four events posted out of order arrive `occurred_at DESC`; two at the same instant are ordered by `id DESC`, derived from the assigned ids; `?limit=2` returns two with `has_more` true. | `JOB-R05`. |
| `an_event_with_a_future_occurred_at_is_accepted` | A 2099 interview is `201` and echoed: there is no past-only rule. | `JOB-R05`. |
| `an_event_kind_is_one_of_six` | Each of the six is `201` and echoed; `"meeting"` and `"screening"` are `422`; the listing holds exactly six. | `JOB-R05`. |
| `an_event_summary_is_required_and_bounded` | `""` and `"   "` are `422` naming `summary`; 201 is `422`; `details` of 10 001 is `422` naming `details`. | `JOB-R05`. |
| `an_event_occurred_at_requires_a_timezone` | A local time with no offset and a non-timestamp are each `422`; the listing stays empty. | `JOB-R05`. |
| `an_event_id_cannot_be_used_under_another_application` | An event of the caller's application A, addressed under application B, is `404` on `PATCH` and on `DELETE`; it is neither moved nor returned, and B's page is empty. | `JOB-R05`, `JOB-R09`. |
| `deleting_an_application_removes_its_events` | After the application is deleted, the events endpoint is `404`; a direct count of `application_events` for that id is zero, which the `404` alone would not prove; another account's application and event survive. | `JOB-R05`, `JOB-R06`. |

### 2.6 Salary money

| Test | The rule it proves | Serves |
| ---- | ------------------ | ------ |
| `salary_money_round_trips_with_four_decimals` | `"60000.0000"`/`"75000.0000"`, `"0.0000"` and `"999999999999999.9999"` are returned byte-identically through a second operation; a single bound with a currency is accepted and the other stays `null`; clearing the last bound clears the currency. | `JOB-R03`. |
| `a_salary_bound_without_a_currency_is_refused` | Minimum alone, maximum alone and both without a currency are each `422` — the client sees the `422`, not a `23514` presented as a `500` — and nothing is created. | `JOB-R03`. |
| `a_salary_currency_without_a_bound_is_refused` | A currency with no bound is `422` on create, and on `PATCH` it is refused without clearing the stored bounds or currency. | `JOB-R03`. |
| `an_inverted_salary_range_is_refused` | An inverted pair is `422` on create; equal bounds are `201`; inverting a stored pair with a one-bound `PATCH` is `422` and the pair stays as it was. | `JOB-R03`. |
| `a_salary_currency_is_three_upper_case_letters` | `EU`, `EURO`, `E1R` and `EU R` are `422`; `"eur"` is accepted and returned as `"EUR"`. | `JOB-R03`. |
| `a_json_number_where_a_salary_is_expected_is_refused` | `{"salary_min": 60000}` and `"60000.00"` are each `422`, and nothing is created: the wire form is a four-decimal string, and the server refuses rather than rounds. | `JOB-R03`. |

### 2.7 Ownership

| Test | The rule it proves | Serves |
| ---- | ------------------ | ------ |
| `a_company_of_another_user_is_invisible` | Another account gets `404` on `GET`, `PATCH` and `DELETE`; the `DELETE` is `404` **before** the `409` question, so existence does not leak; the owner's company and application are untouched. | `JOB-R09`. |
| `an_application_of_another_user_is_invisible` | Another account gets `404` on `GET`, on a legal stage move, on `DELETE`, and on all four event operations under the application; the owner's stage, title and event survive. | `JOB-R09`. |
| `an_application_cannot_target_another_users_company` | A foreign `company_id` on create and on `PATCH` is `404`, a UUID that names no company is the same, and the stored `company_id`, title and stage are unchanged. | `JOB-R02`, `JOB-R09`. |
| `the_listings_only_contain_the_callers_rows` | Both company listings, both application listings, and a company filter naming another account's company, which matches nothing rather than leaking its applications. | `JOB-R09`. |

### 2.8 The shared status codes

| Test | What it asserts | Serves |
| ---- | --------------- | ------ |
| `every_job_application_operation_requires_a_credential` | All fourteen operations, with no token and with an invalid one, are `401 unauthorized` with no `details`, and nothing they sent landed. The operation list is built once and asserted to hold fourteen entries. | All nine. |
| `every_job_application_operation_is_not_found_for_another_account` | The eleven row-addressed operations are `404` with no `details`; the two lists are `200` with an empty page; a company create is the caller's own, and the owner's rows are untouched. | `JOB-R09`. |
| `every_path_id_must_be_a_uuid` | Twelve path shapes, including both id positions of the event route, are `400 bad_request`. | All nine. |
| `every_write_operation_refuses_a_malformed_body` | Six write operations: truncated JSON and an empty body are `400`; `[]` is `422` naming `body`; nothing landed. | All nine. |
| `every_operation_with_a_documented_validation_failure_is_a_422` | Eight cases across the three resources, each `422 validation_failed` with a non-empty `details`; nothing refused was written. | `JOB-R02`, `JOB-R03`, `JOB-R05`. |
| `a_job_applications_body_over_the_limit_is_refused` | A body above `max_body_bytes` is `413 payload_too_large`, on a state built with a 256-byte limit so the refusal is about size and not validation. | All nine. |

## 3. The contract layer — `tests/openapi_contract.rs`

The document is generated from the handlers, and the route table is declared
independently, so they are held together by assertion. The published surface is
`CONTRACT_OPERATIONS = 48`.

| Test | What it asserts | Serves |
| ---- | --------------- | ------ |
| `the_route_table_and_the_document_describe_the_same_operations` | The route table and the document describe the same operations **in both directions**, and the count is exactly 48. A job route the document omits, or an operation the router does not serve, fails here. | All nine, and `JOB-R08` in part. |
| `every_route_in_the_table_is_served_by_the_router` | Every declared route is actually mounted. | All nine. |
| `each_list_endpoint_publishes_its_filter_and_pagination_parameters_once` | `/api/v1/companies` publishes `limit`, `offset` and `search`; `/api/v1/job-applications` publishes the pagination pair, `status`, `company_id`, `search`, `applied_from` and `applied_to`; neither publishes a parameter twice. | `JOB-R01`, `JOB-R05`. |
| `the_job_application_operations_publish_their_success_status` | The fourteen `(method, path, success status)` triples: three creates are `201`, seven reads and patches are `200`, four deletes are `204`; the table is asserted to hold fourteen rows, so a table that shrank cannot pass with fewer assertions. | All nine. |
| `the_event_schemas_publish_no_updated_at` | `EventResponse`, `CreateEventRequest` and `UpdateEventRequest` publish no `updated_at` property — decision `A-D1-1`, at the contract level. | `JOB-R05`. |
| `each_list_handler_extracts_the_query_string_once` | Reads the handlers and asserts that no list handler parses the query twice, which is what would publish `limit` and `offset` twice. | `JOB-R01`, `JOB-R05`. |

`every_route_in_the_table_is_served_by_the_router` and
`the_route_table_and_the_document_describe_the_same_operations` are shared with
the rest of the API; the other four were written or extended by this chain.

**What is not asserted** is behaviour behind the document: that every documented
status code is reachable, or that a documented schema type is what a response
returns. The `salary_min`/`salary_max` `type: string` claim is asserted for the
**claim** money fields by `the_claim_money_fields_are_strings_in_the_document`,
not for the job salary fields; §5 records that.

## 4. Unit tests in the module

Eighteen tests, in `#[cfg(test)] mod tests` blocks next to the code. They run
without a database.

| File | Test | What it proves |
| ---- | ---- | -------------- |
| `src/job_applications/service.rs` | `the_stage_machine_allows_exactly_the_twelve_edges` | All 49 pairs: 12 moves, 7 no-ops, 30 refusals, with the three counts asserted. |
| `src/job_applications/service.rs` | `the_three_terminal_stages_have_no_exit` | No move out of `offer`, `rejected` or `withdrawn`; a self-pair is a no-op, not an exit. |
| `src/job_applications/service.rs` | `the_permitted_sources_are_the_edges_into_the_stage_plus_itself` | The write predicate's array is the requested stage plus every source of an edge into it — derived, never hand-written. |
| `src/job_applications/service.rs` | `a_create_accepts_only_wishlist_or_an_omitted_stage` | `None` and `wishlist` are accepted; the other six are refused with a field violation naming `status`. |
| `src/job_applications/service.rs` | `a_company_name_is_trimmed_and_keeps_its_casing` | `"  Acme  "` becomes `"Acme"`; a blank is refused; the bound is measured after trimming. |
| `src/job_applications/service.rs` | `a_website_and_a_job_url_must_be_absolute_http_urls` | Scheme, authority and the 2048 bound, on both fields. |
| `src/job_applications/service.rs` | `a_salary_bound_without_a_currency_is_refused` | The pair rule, with `None` on each side in turn, and a currency with no bound. |
| `src/job_applications/service.rs` | `a_salary_minimum_above_the_maximum_is_refused` | Order, with equal bounds accepted. |
| `src/job_applications/service.rs` | `the_money_comparison_is_exact_and_ignores_leading_zeros` | `"05.0000"` and `"5.0000"` compare equal; no parse and no float. |
| `src/job_applications/service.rs` | `a_patched_bound_is_validated_against_the_stored_triple` | A one-bound `PATCH` resolves against the stored triple before it can be written. |
| `src/job_applications/service.rs` | `a_blank_search_means_no_filter` | An empty or spaces-only term is `None`, not an error. |
| `src/job_applications/service.rs` | `a_reversed_applied_range_is_refused` | `from > to` is refused naming `applied_from`; equal bounds are a one-day range. |
| `src/job_applications/service.rs` | `a_source_is_trimmed_and_has_no_blank_spelling` | `"  referral "` is trimmed; `""` and `"   "` are refused; 64 is accepted, 65 is not. |
| `src/job_applications/dto.rs` | `a_nullable_company_field_is_kept_cleared_or_set_by_the_three_body_forms` | `{}`, `{"website":null}` and a value stay three distinct instructions. |
| `src/job_applications/dto.rs` | `a_nullable_application_field_is_kept_cleared_or_set_by_the_three_body_forms` | The same for `source`, `job_url`, `next_action_task_id`, and that any salary field counts as a salary touch. |
| `src/job_applications/dto.rs` | `an_event_kind_is_one_of_six` | Each accepted kind parses; `"meeting"` does not. |
| `src/job_applications/dto.rs` | `an_event_occurred_at_requires_a_timezone` | No offset is refused; `+02:00` is accepted. |
| `src/job_applications/dto.rs` | `a_create_without_a_stage_parses_and_the_stage_is_absent` | An omitted stage is `None` and `description` defaults to `""`, so the parse and the service's rule cannot disagree. |

## 5. The requirement-to-test map

| Requirement | The cases that carry it | What is **not** asserted |
| ----------- | ----------------------- | ------------------------ |
| `JOB-R01` | §1 cases 2, 3; §2.1 (all eleven); §2.8; §3 parameter sets. | — |
| `JOB-R02` | §1 case 4; §2.2 (all eight); §2.4 `an_application_can_be_moved_to_another_of_the_callers_companies`; §2.7 `an_application_cannot_target_another_users_company`; §4 create and role-title unit tests. | A `description` over 10 000 characters. |
| `JOB-R03` | §1 cases 8–10; §2.6 (all six); §2.8 `every_operation_with_a_documented_validation_failure_is_a_422`; §4 money and range unit tests. | A negative money string through the API (refused by the pattern, unasserted); a value above the 15-integer-digit bound through the API; the `type: string` contract for the job salary fields. |
| `JOB-R04` | §1 case 11; §2.2 `a_create_with_a_stage_other_than_wishlist_is_refused`, `an_unknown_stage_in_a_body_is_refused`; §2.3 (all five); §4 the first four tests. | Any concurrency case (§6). |
| `JOB-R05` | §1 cases 5, 6; §2.5 (all eight); §3 `the_event_schemas_publish_no_updated_at`; §4 two event-kind and timezone tests. | Event pagination beyond `?limit=2`: `offset`, the clamp, an offset past the end. |
| `JOB-R06` | §1 cases 5, 6; §2.1 `a_company_with_applications_cannot_be_deleted`, `a_company_can_be_deleted_once_its_applications_are_gone`; §2.5 `deleting_an_application_removes_its_events`; §2.4 the company move. | The delete/insert race (§6). |
| `JOB-R07` | §2.4 both cross-link cases. | The `ON DELETE SET NULL` from a schema-level statement: the API case is the only evidence. |
| `JOB-R08` | §3 `the_route_table_and_the_document_describe_the_same_operations` (the surface is pinned at 48) and `the_job_application_operations_publish_their_success_status` (the fourteen operations are named). | **The absence of generation is asserted by nothing.** No case checks it; the evidence today is a reading of the route table and the module's imports. |
| `JOB-R09` | §1 cases 4, 7; §2.1 `two_accounts_may_use_the_same_company_name`; §2.7 (all four); §2.8 `every_job_application_operation_is_not_found_for_another_account`, `every_job_application_operation_requires_a_credential`; §3 the route table. | — |

Both flow cases (§8) also carry `JOB-R01`…`JOB-R07` and `JOB-R09`, each inside a
composition rather than in isolation. They are named once here rather than
repeated in every row above.

## 6. Deliberately not covered, and not yet covered

The first three are decisions; the rest are gaps with a name and no case, listed
so that nobody reads the map in §5 as completeness.

| Gap | Why or what it would take |
| --- | ------------------------- |
| **Two concurrent stage changes on one application.** | Timing-dependent. The serialisation is the single statement's `status = ANY(…)` predicate: the loser's `WHERE` is re-checked against the committed row version and matches nothing, so it is answered `409`. The invariant is stated in [architecture.md](architecture.md) §3.1; no case exercises it. |
| **A live `409` from the company-delete race.** | It needs one request to insert while another deletes. `NO ACTION` makes the orphan impossible; the race is analysed, not tested. |
| **A `database_error` through the API.** | Fault injection into a live pool; the mapping is unit-tested in [`../../../src/error.rs`](../../../src/error.rs). The `408` timeout is proven globally by the timeout case in `tests/projects_tasks_api.rs`, not by this file. |
| **The application-listing pagination bounds.** | No case sends `?limit=` or `?offset=` to `GET /job-applications`, so the clamp, `has_more` and an offset past the end are unasserted. `GET /companies` has one `?limit=1&offset=1` case; the clamp above the configured maximum is unasserted there too. |
| **The event-listing pagination bounds beyond `limit=2`.** | `has_more` is asserted; `offset`, the clamp and an offset past the end are not. |
| **The length half of the two URL rules.** | No case sends a 2049-character `website` or `job_url`; only the scheme half is asserted. |
| **The application `description` bound.** | No case sends 10 001 characters. The validator is `bounded_text`, but no test names the field. |
| **A blank `source` through the API.** | Refused by `required_text`, asserted only by the unit case. |
| **`JOB-R08`'s absence of generation.** | Nothing asserts it. The strongest evidence is the pinned operation count and the fourteen named operations. |
| **The flow layer's own gaps.** | `tests/job_applications_flow.rs` exists (§8) and closes composition, the delete chain and the ownership sweep. It does not close the gaps above: no `408`/`500` path, no concurrency, no pagination bound beyond one default page, and no seeded read through the API — the seeded rows are asserted by `tests/seeder.rs` instead. Its sweep is a hand-written list of thirteen attempts, so a new route fails the 48-operation pin in `tests/openapi_contract.rs` rather than this file. |
| **A recorded run of anything above.** | The five job files — endpoint, schema, flow, contract and seeder — exist without a quoted result. `Verified` requires that run. |

## 7. Conventions for a new test

- A new endpoint needs a success case, a `401` case, a `404` cross-account
  case, a `400` malformed-path case and a `422` validation case.
- A schema test names its SQLSTATE and, when a constraint fired, the constraint
  name; and it pairs the rejection with an accepted control.
- A stage test names the edge it proves, and one test names all twelve, one all
  seven self-pairs and one all thirty refusals. A create-stage case asserts
  `422`, because a create is not a transition.
- A test that asserts a `PATCH` with `status` also asserts that the other fields
  of a refused request are unchanged.
- A listing test asserts the order with rows inserted out of order, and derives
  an `id` tie-break from the identifiers the server assigned rather than
  guessing.
- A money assertion compares the string and asserts it is not a JSON number.
- Where a row cannot be produced through the API — an exact `created_at` — the
  fixture may be SQL, but the assertion stays on the API response.
- A new route is added to `ROUTE_TABLE` and to `CONTRACT_OPERATIONS` in the same
  change; the contract test compares the two directions.

## 8. The flow layer — `tests/job_applications_flow.rs`

Two cases, added by chain `J5`. Both drive the real router against a real,
disposable database through the shared harness in `tests/common/mod.rs`; neither
waits on a timer and neither asserts a log line. The file contains **one** SQL
statement, and it is a read: a `SELECT COUNT(*)` on `application_events` after
the application delete, because what a cascade did to *rows* is not observable
through the `404` the parent route then answers.

| # | Test | What it does |
| - | ---- | ------------ |
| 1 | `the_whole_job_application_journey_from_lead_to_offer` | Fourteen steps in one run: register and log in; a project and an open task; a fully populated company; a `wishlist` application with every optional field set (padded `role_title` and `source`, money as four-decimal strings); the four forward edges to `offer`, each with the field set asserted exactly and every field but `status` and `updated_at` unchanged; the next-action task attached **while the application is terminal**; three events posted out of chronological order, two of them tied at one instant and one scheduled in 2099; the timeline read back in `occurred_at DESC, id DESC` with the tie-break derived from the assigned identifiers; the whole application read back; the six refused moves out of `offer`, each leaving the row byte-identical, and the accepted `offer`-to-`offer` no-op that writes nothing; the `409` on deleting its company; the application delete, whose events are counted at zero; the company delete that becomes possible afterwards; and both listings empty with `total` 0 while the task survives. |
| 2 | `the_second_account_sees_not_found_on_every_job_application_operation` | One application, already moved to `applied`, under a second account's attempts: thirteen addressed attempts over eleven distinct operations — the application `PATCH` is sent three times, a rename and two company moves, one naming the owner's company and one naming the second account's **own** — each `404 not_found` in the standard envelope, never `403`, and explicitly never `409`. The owner's company, application and timeline are byte-identical afterwards, the application still belongs to the owner's company and keeps its next-action link, the `?company_id=` filter over a foreign company is an empty page with `total` 0 rather than an oracle, and the second account can still delete its own company. |

What case 1 adds is **state coherence across calls**. Every one of the 51
endpoint cases starts from a fixture it built itself and asserts one operation;
none of them can see whether the money, the `created_at`, the next-action link or
the timeline survived the *next* successful call. The journey compares the whole
body after each refused write and reads the whole object at the end, so a field
silently re-defaulted between two `200`s fails even though both calls answered
correctly. It also sends a `PATCH` on a terminal stage, which proves the stage
machine governs `status` only — a distinction no single-operation case makes.
The delete chain (refused company delete, application delete, cascade, company
delete, empty listings) is the one place where the whole cross-feature
composition runs in a single state.

What case 2 adds is **the sweep on one row in one state**. The attempts are
asserted to number thirteen, so a sweep that quietly lost an attempt fails. The
list is **not** derived from `ROUTE_TABLE`: a fifteenth job route would not fail
this file. What fails then is
`tests/openapi_contract.rs::the_route_table_and_the_document_describe_the_same_operations`,
which pins the published surface at 48 and compares the route table with the
document in both directions (§3). The sweep's own job is the `404`-not-`409`
property — the company delete is a `409` for its owner and must stay a `404` for
everyone else — and the untouched-owner-row property. The two collection reads it
does not attempt, `GET /companies` and `GET /job-applications`, cannot answer
`404`; they are asserted by the property they do publish, an empty page with
`total` 0. `POST /companies` is exercised, but only successfully: a create has no
existing row to own.

What the layer does **not** cover is in §6: the `408` and `500` paths, any
concurrency, the pagination bounds beyond one default page, and a seeded read
through the API. No run of either case is recorded here.

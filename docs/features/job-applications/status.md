**Status:** Implemented

# Job applications — status

> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

## 1. The status

The feature is **`Implemented`**, with the **endpoint and flow layers complete**:
the fourteen operations of [api.md](api.md) are mounted in `src/api/routes.rs`
and published in `/api/v1/openapi.json`, which now describes **48 operations**
(34 before this chain). The module's five layers and the two flow cases of
`tests/job_applications_flow.rs` are in the tree, and the requirement table in
§1.2 names, for each of `JOB-R01`…`R09`, the code that exists and the test that
asserts it.

It is **not `Verified`**, and this document says so deliberately:

- **No reviewer has read the chain.** Chains `J2`–`J5` were implemented in one
  wave and no review row exists for them. A green gate is not a review.
- **No run is recorded here.** `tests/job_applications_api.rs` (51 cases),
  `tests/jobs_schema.rs` (11), `tests/job_applications_flow.rs` (2), the job
  cases in `tests/openapi_contract.rs` and the seeder case all exist in the tree.
  Not one of their results is quoted in this document, and this document does not
  invent one.
- `Implemented` is the `AGENTS.md` §6 word for "the code exists and compiles".
  The existence half is checkable against the tree; the compile half belongs to
  the chain's gate. **`cargo` was not run while this document was written**,
  because two agents were writing the tree at the time. The first compile-and-test
  evidence will be the chain's gate, not this file.

### 1.1 What exists

| Item | In the tree? | Detail |
| ---- | ------------ | ------ |
| `migrations/0005_job_applications.sql` | **Yes** | The two enum types, the three tables, every `CHECK`, both composite keys, the `ON DELETE` actions and every index of [`schema.md`](../../database/schema.md) §11.3–§11.5. Unchanged by `J2`–`J4`: the endpoint layer needed no migration. |
| `src/job_applications/models.rs` | **Yes** | `ApplicationStatus`, `ApplicationEventKind`, `Company`, `JobApplication`, `ApplicationEvent`. |
| `src/job_applications/dto.rs` | **Yes** | `CompanyResponse`, `CompanyPage`, `ApplicationResponse`, `ApplicationPage`, `EventResponse`, `EventPage`, the six request types, `CompanyQuery`, `ApplicationQuery`, `CompaniesListQuery`, `ApplicationsListQuery` and the length constants. Money is `Option<String>`, never a number. `EventResponse` has no `updated_at`. Also five unit tests. |
| `src/job_applications/repository.rs` | **Yes** | Every SQL statement of the feature. No statement touches a row without `user_id = $n`. |
| `src/job_applications/service.rs` | **Yes** | `ensure_transition`, `permitted_sources`, `ensure_initial_stage`, `JobService` with fourteen use cases, the field validators, and thirteen unit tests. |
| `src/job_applications/handlers.rs` | **Yes** | Fourteen handlers, each with its `#[utoipa::path]` annotation and its documented status set. |
| `src/job_applications/mod.rs` | **Yes** | Declares and re-exports all five layers. |
| `src/api/routes.rs` | **Yes** | Six route declarations (14 method/path pairs), the fourteen `ROUTE_TABLE` entries, the fourteen operations and twelve schemas in `ApiDoc`, and the `companies` / `job-applications` tags. |
| `src/state.rs` | **Yes** | `AppState::job_applications: JobService`, built from the pool, the message catalogue and the pagination settings. |
| `config/messages.toml`, `src/config/messages.rs` | **Yes** | The client-facing strings of the feature: validation, conflicts, and the resource-not-found messages. |
| `tests/job_applications_api.rs` | **Yes** | 51 HTTP-level cases. The case-by-case map is [testing.md](testing.md) §2. No run is recorded. |
| `tests/jobs_schema.rs` | **Yes** | Eleven cases against the real database; [testing.md](testing.md) §1. No run is recorded. |
| `tests/job_applications_flow.rs` | **Yes** | Two flow cases — `the_whole_job_application_journey_from_lead_to_offer` and `the_second_account_sees_not_found_on_every_job_application_operation`. The map is [testing.md](testing.md) §8. No run is recorded. |
| `tests/openapi_contract.rs` | **Yes** | `CONTRACT_OPERATIONS = 48`; the fourteen success statuses; the published parameter sets of both job list operations; `EventResponse` carrying no `updated_at`. No run is recorded. |
| `tests/seeder.rs` | **Yes** | One case for the seeded `dev` counts and invariants. No run is recorded. |

### 1.2 The requirements, one by one

Every requirement is `Implemented`: the code exists. None is `Verified`: no
result is recorded. "Asserted by" names tests that are **on disk**; it does not
claim they passed. Both flow cases also serve `JOB-R01`…`JOB-R07` and `JOB-R09`
jointly, inside a composition rather than in isolation; they are named once in
§1.5 rather than repeated in every row.

| Requirement | State | What exists | Asserted by | Still unproven |
| ----------- | ----- | ----------- | ----------- | -------------- |
| `JOB-R01` A company name is unique per owner, case-insensitively | `Implemented` | Five company operations. `validate_company_name` trims and measures in characters (1–200). A duplicate is **not** pre-checked: `insert_company`/`update_company_owned` detect the `23505` from `companies_user_id_lower_name_unique_idx` and return `CompanyWrite::DuplicateName`, which the service answers as `409`. The stored casing survives; only the comparison is folded, in SQL. The listing orders `LOWER(name), name, id` and takes `search`, `limit`, `offset`. | `tests/job_applications_api.rs::a_company_name_is_unique_per_owner_ignoring_case`, `::renaming_a_company_onto_an_existing_name_is_refused`, `::two_accounts_may_use_the_same_company_name`, `::a_company_name_is_trimmed_and_bounded`, `::a_company_can_be_created_read_changed_and_deleted`, `::the_companies_listing_is_alphabetical_and_paginated`, `::the_companies_listing_searches_by_name`; `tests/jobs_schema.rs` cases 2 and 3. | A recorded run. |
| `JOB-R02` An application belongs to one company of the same owner, with a role title and one of seven stages | `Implemented` | `INSERT_APPLICATION` inserts through `SELECT … FROM companies c WHERE c.id = $3 AND c.user_id = $2`, so a foreign company inserts **no row** and the service names it `404`. **The create accepts only `wishlist` or an omitted stage** (`ensure_initial_stage`), and the statement writes the literal `'wishlist'`; any other stage is `422` naming `status`. `role_title` is trimmed, 1–200, by `required_text`. A `PATCH` may correct `company_id` to another of the caller's companies, under the same predicate. | `tests/job_applications_api.rs::an_application_can_be_created_read_changed_and_deleted`, `::a_create_with_a_stage_other_than_wishlist_is_refused`, `::an_application_cannot_target_another_users_company`, `::an_application_can_be_moved_to_another_of_the_callers_companies`, `::a_role_title_is_trimmed_and_bounded`, `::an_unknown_stage_in_a_body_is_refused`; `tests/jobs_schema.rs` case 4; `src/job_applications/service.rs::a_create_accepts_only_wishlist_or_an_omitted_stage`. | A recorded run. |
| `JOB-R03` An optional salary range, one currency, `min <= max`, money as `REIMB-R07` | `Implemented` | `validate_salary_range` on create and `resolve_salary` on `PATCH`: four-decimal string (`^[0-9]{1,15}\.[0-9]{4}$`, no sign, no rounding), a bound requires a currency, a currency without a bound is refused, `min <= max` by exact string comparison (`compare_money`, no `f32`/`f64`, no arithmetic). A `PATCH` that touches any salary field reads the stored triple (`find_salary_owned`), resolves and validates the whole triple, and writes it whole. `salary_currency` is trimmed and upper-cased before the `CHAR(3)` write. | `tests/job_applications_api.rs::salary_money_round_trips_with_four_decimals`, `::a_salary_bound_without_a_currency_is_refused`, `::a_salary_currency_without_a_bound_is_refused`, `::an_inverted_salary_range_is_refused`, `::a_salary_currency_is_three_upper_case_letters`, `::a_json_number_where_a_salary_is_expected_is_refused`; `src/job_applications/service.rs::a_salary_bound_without_a_currency_is_refused`, `::a_salary_minimum_above_the_maximum_is_refused`, `::the_money_comparison_is_exact_and_ignores_leading_zeros`, `::a_patched_bound_is_validated_against_the_stored_triple`; `tests/jobs_schema.rs` cases 8–10. | A recorded run, and an API case for a value above `999999999999999.9999` or a negative string (both currently refused by the same pattern, but unasserted through the API). |
| `JOB-R04` The seven-stage machine, three terminal stages | `Implemented` | `ALLOWED_MOVES` is the **only** definition of the edge table (12 pairs). `ensure_transition` returns `Some(NoOp)` for a self-pair, `Some(Move)` for an edge, `None` otherwise. `permitted_sources` derives the write predicate's array from the same table. The change is **one guarded `UPDATE`** — `WHERE … AND status = ANY($20)` — with no read and no `FOR UPDATE` before it; zero rows is explained afterwards as `404` (row/company/task) or `409` (the move). `updated_at` is stamped only when a non-status column is written or the stage actually changes. The full shape is [architecture.md](architecture.md) §3.1. | `tests/job_applications_api.rs::every_permitted_stage_change_is_accepted` (12 edges), `::sending_the_current_stage_is_accepted_and_changes_nothing` (7 self-pairs), `::every_forbidden_stage_change_is_refused` (30 pairs), `::offer_rejected_and_withdrawn_are_terminal`, `::a_refused_stage_change_does_not_write_the_other_fields`, `::a_create_with_a_stage_other_than_wishlist_is_refused`; `src/job_applications/service.rs::the_stage_machine_allows_exactly_the_twelve_edges`, `::the_three_terminal_stages_have_no_exit`, `::the_permitted_sources_are_the_edges_into_the_stage_plus_itself`. | A recorded run, and any concurrency case (see [testing.md](testing.md) §6). |
| `JOB-R05` Events belong to an application, ordered `occurred_at DESC, id DESC` | `Implemented` | Four event operations. The listing orders `occurred_at DESC, id DESC`; the index supplies the leading term only. `list_events_owned` distinguishes "no such application" from "no events" with `APPLICATION_EXISTS`, run only when the page is empty, so the empty page stays an empty page and the missing application stays `404`. Insert, update and delete are each scoped by `application_id`, `id` and `user_id`, so an event of the caller's other application matches no row. `occurred_at` has no past-only rule. | `tests/job_applications_api.rs::events_are_ordered_by_occurred_at`, `::an_event_can_be_added_changed_and_deleted`, `::an_event_id_cannot_be_used_under_another_application`, `::an_event_with_a_future_occurred_at_is_accepted`, `::an_event_kind_is_one_of_six`, `::an_event_summary_is_required_and_bounded`, `::an_event_occurred_at_requires_a_timezone`; `tests/jobs_schema.rs` cases 5 and 6; `src/job_applications/dto.rs::an_event_kind_is_one_of_six`, `::an_event_occurred_at_requires_a_timezone`. | A recorded run, and event pagination beyond `?limit=2` (`offset`, the clamp, an offset past the end). |
| `JOB-R06` Deleting an application deletes its events; a company with applications cannot be deleted | `Implemented` | `DELETE_APPLICATION_OWNED` is one statement; the event cascade is the schema's `ON DELETE CASCADE`. `DELETE_COMPANY_IF_UNUSED` is one statement with `AND NOT EXISTS (SELECT 1 FROM job_applications …)`. **Nothing is deleted to make it succeed**: the applications are records the client did not ask to delete. Zero rows is resolved by one read, `company_is_owned`: owned → `409`, otherwise `404`. | `tests/job_applications_api.rs::deleting_an_application_removes_its_events`, `::a_company_with_applications_cannot_be_deleted`, `::a_company_can_be_deleted_once_its_applications_are_gone`, `::an_application_can_be_moved_to_another_of_the_callers_companies`; `tests/jobs_schema.rs` cases 5 and 6. | A recorded run, and the delete/insert race, which is analysed and not tested. |
| `JOB-R07` One optional next-action task, cleared when the task is deleted | `Implemented` | `next_action_task_id` with `ON DELETE SET NULL` in the schema. The ownership rule cannot be a constraint (`tasks` has no `user_id`) and is a predicate of **both** the insert and the update: `EXISTS (SELECT 1 FROM tasks t JOIN projects p ON p.id = t.project_id WHERE t.id = $n AND p.user_id = $2)`. A foreign task, another account's task and a task that does not exist are one answer: `404`, named by `explain_insert_refusal` / `explain_update_refusal`. `{"next_action_task_id": null}` clears the link. | `tests/job_applications_api.rs::the_next_action_task_must_belong_to_the_caller`, `::deleting_the_task_clears_the_next_action`. | A recorded run. No schema test asserts the `SET NULL`; the API case is the only evidence, and it is the only evidence for the ownership rule that **can** exist. |
| `JOB-R08` No text generation, no outbound AI call | `Implemented` | There is no generating operation among the 48, no `prompt` field in any DTO, and `src/job_applications/` imports no HTTP client. The implementation is the absence. | The surface is pinned: `tests/openapi_contract.rs::the_route_table_and_the_document_describe_the_same_operations` with `CONTRACT_OPERATIONS = 48`, and `::the_job_application_operations_publish_their_success_status` names all fourteen job operations. **No test asserts the absence of a generation field**: the contract case that would have asserted it was not written. | A recorded run, and a check of the absence itself, which today is a reading of the tree rather than a test. |
| `JOB-R09` Owner-scoped reads and writes, a foreign row is `404` | `Implemented` | Every statement carries `user_id = $n`; every child row is written with the caller's id, never the body's. The two composite keys make a cross-owner child unrepresentable. A foreign row is `404`, never `403`; a foreign company `DELETE` is `404` before the `409` question is asked, so the `409` cannot leak existence. | `tests/job_applications_api.rs::a_company_of_another_user_is_invisible`, `::an_application_of_another_user_is_invisible`, `::the_listings_only_contain_the_callers_rows`, `::every_job_application_operation_is_not_found_for_another_account`, `::an_application_cannot_target_another_users_company`; `tests/jobs_schema.rs` cases 4 and 7. | A recorded run. |

### 1.3 The stage machine, exactly

This is the distinction a reader gets wrong, so it is written out once:

| Count | What | Answer |
| ----- | ---- | ------ |
| **12** | The allowed edges: `wishlist→applied`, `applied→screening`, `screening→interview`, `interview→offer`, and `rejected` and `withdrawn` from each of `wishlist`, `applied`, `screening`, `interview`. | `200`, the new stage, stored. |
| **7** | The self-pairs, one per stage (`applied→applied`, …). They are **accepted no-ops, not refusals**. Re-sending the current stage changes nothing and is not a transition. | `200`, unchanged. |
| **30** | Every other ordered pair of the seven stages: a skipped stage, a backwards move, a move between two terminal stages, and every move **out of** `offer`, `rejected` or `withdrawn`. | `409 conflict`, nothing written. |
| **49** | All ordered pairs. 12 + 7 + 30 = 49. | — |

The create is a **different rule** and is not one of the 49 pairs:

- `POST /job-applications` accepts `wishlist` **or an omitted stage**, and
  nothing else. The other six stages are `422 validation_failed` naming
  `status`, not `409`, because the body is asking for a row the contract does
  not allow to exist. `ensure_initial_stage` is the whole rule, and the insert
  statement writes the literal `'wishlist'`, so a caller that bypassed the
  service check could still not create another stage through this statement.
- `offer`, `rejected` and `withdrawn` are terminal. Re-sending the terminal
  stage itself is still a `200` no-op; a move **out of** it is `409`.

### 1.4 The fourteen operations as a surface

Six cases assert the surface rather than one rule, and they serve all nine
requirements:

| Case | What it asserts |
| ---- | --------------- |
| `every_job_application_operation_requires_a_credential` | All fourteen operations, with no token and with an invalid one, answer `401 unauthorized` in the standard envelope, with no `details`, and nothing they sent landed. |
| `every_job_application_operation_is_not_found_for_another_account` | The eleven row-addressed operations answer `404` for another account; the two lists answer `200` with an empty page; a create is the caller's own. |
| `every_path_id_must_be_a_uuid` | Twelve path shapes, including both id positions of the event route, answer `400 bad_request`. |
| `every_write_operation_refuses_a_malformed_body` | Six write operations: non-JSON and empty bodies are `400`; a JSON value of the wrong shape is `422` naming `body`. |
| `every_operation_with_a_documented_validation_failure_is_a_422` | Eight cases across the three resources, each `422` with a non-empty `details`. |
| `a_job_applications_body_over_the_limit_is_refused` | A body above `max_body_bytes` is `413 payload_too_large`, on a state configured with a 256-byte limit. |

### 1.5 What is written, and what it proves

| File | Cases | What it serves | Has a run been recorded? |
| ---- | ----- | -------------- | ------------------------ |
| `tests/job_applications_api.rs` | 51 | The endpoint layer: companies, applications, the stage machine, the Phase 1 cross-links, events, salary money, ownership, and the shared `400`/`401`/`404`/`413`/`422` sweep. Map: [testing.md](testing.md) §2. | Not in this document. |
| `tests/jobs_schema.rs` | 11 | The constraints, the two composite keys, the cascades, the enum vocabulary, the four-decimal storage and the migration from an empty database. Map: [testing.md](testing.md) §1. | Not in this document. |
| `tests/job_applications_flow.rs` | 2 | The flow layer: `the_whole_job_application_journey_from_lead_to_offer` (link, timeline, money, the terminal stage, the delete chain) and `the_second_account_sees_not_found_on_every_job_application_operation` (thirteen addressed attempts, including the company delete that is a `409` for its owner). Map: [testing.md](testing.md) §8. | Not in this document. |
| `tests/openapi_contract.rs` | 4 for this feature | 48 operations in both directions, the fourteen success statuses, the two job parameter sets, and the event schemas carrying no `updated_at`. Map: [testing.md](testing.md) §3. | Not in this document. |
| `tests/seeder.rs` | 1 for this feature | The `dev` counts 20 / 12 / 50, the deterministic ids, and the ownership and salary invariants of the seeded rows. | Not in this document. |
| `src/job_applications/service.rs` | 13 | The transition table (all 49 pairs), the terminal stages, `permitted_sources`, the create rule, the trim rules, the URL rules, the money rules and the range order. | Not in this document. |
| `src/job_applications/dto.rs` | 5 | The tri-state `PATCH` fields, the event-kind vocabulary, a timezone-less `occurred_at`, and the omitted stage on a create. | Not in this document. |

A test that exists is not a test that passed. The statement is load-bearing for
every row above.

## 2. What is still missing to reach `Verified`

1. **A recorded run.** The three quality gates of `AGENTS.md` §3 and the two
   documentation gates of [strategy.md](../../testing/strategy.md) §7.5, with a
   disposable `TEST_DATABASE_URL`, quoted in the verification record. The
   policy is [`verification-policy.md`](../../testing/verification-policy.md).
2. **The review.** No reviewer has read `J2`–`J5`. The review log is the
   evidence, and it does not exist for this chain yet.
3. **The uncovered halves** listed per requirement in §1.2 and collected in
   [testing.md](testing.md) §6: the application-listing pagination bounds, the
   event and application list clamps, the description and `job_url` length
   bounds, the salary magnitude bound, and any assertion that the feature
   generates no text.

The **flow layer is no longer missing**: `tests/job_applications_flow.rs` holds
the journey from registration to `offer` and the ownership sweep over thirteen
addressed attempts, and [testing.md](testing.md) §8 records what each proves that
the 51 per-operation cases cannot. It is counted as evidence in §1.5 and it is
**not** a substitute for the two items above: it has never been run and no
reviewer has read it. Unlike `tests/claims_flow.rs` it has no seeded read; the
seeded rows are asserted by `tests/seeder.rs`.

`Verified` additionally requires that the schema, endpoint, flow, contract and
seeder cases pass in one recorded run. Neither word is used loosely here:
`Implemented` is about the tree, `Verified` is about a measurement.

One status discrepancy is recorded rather than resolved here:
[`schema.md`](../../database/schema.md) §11 still heads its section `Proposed`
and lists `companies`, `job_applications` and `application_events` as tables
that exist but have no endpoint. The second half of that sentence is now false:
all fourteen operations are mounted. Reconciling the wording is the schema
document's edit, not this file's.

## 3. What a future change must do to keep it `Implemented` and reach `Verified`

- **Do not change an accepted requirement silently.** A revision goes through
  [`project-truth.md`](../../project-truth.md) §18.4, in a new change.
- **A3 and A6 are provisional defaults**, not owner-approved requirements.
- **`0005` lands once and is then append-only.** A schema change is a new
  migration, never an edit to `0005`. Adding a stage value is
  `ALTER TYPE … ADD VALUE` in that new migration, with the transaction caveat in
  [`schema.md`](../../database/schema.md) §8.
- **A change to the stage list is a breaking contract change**: every terminal
  stage and every edge is published. `ALLOWED_MOVES` is the single definition;
  do not write a second copy of the table, and do not replace it with a rank
  comparison (`rejected` and `withdrawn` are reachable from four stages).
- **Do not put a read before the guarded `UPDATE`.** `SELECT status … FOR UPDATE`
  followed by an `UPDATE` reintroduces the race the statement's `status = ANY(…)`
  predicate closes ([architecture.md](architecture.md) §3.1).
- **Do not pre-check the company delete with `SELECT COUNT(*)`.** The
  `AND NOT EXISTS` of the single statement is what makes the rule atomic, and
  nothing may be deleted to make the delete succeed.
- **Do not enforce the next-action task's owner by a constraint you cannot
  write.** `tasks` has no `user_id`; the `tasks → projects → user_id` predicate
  in the statement is the enforcement, and its only evidence is the API case.
- **A change that adds generation of any kind** contradicts
  [`project-truth.md`](../../project-truth.md) §6.1 and needs an amendment
  before any code.
- **A change to the money format** needs a new ADR that supersedes
  [`ADR-0007`](../../decisions/ADR-0007-money-representation.md).
- **A change to the delete behaviour** of a company needs its own decision: the
  current key is `NO ACTION`, and a cascade would destroy job history silently.
- **Do not "restore" the `NO ACTION` key to `RESTRICT` for symmetry.** `RESTRICT`
  would make `DELETE FROM users` fail for any account that has an application,
  which is the opposite of the documented cascade.
- **Do not add `updated_at` to `application_events`.** Its absence is decision
  `A-D1-1` ([architecture.md](architecture.md) §3.5), and
  `tests/openapi_contract.rs::the_event_schemas_publish_no_updated_at` now
  asserts it.

## 4. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | What exists, the five layers, the guarded `UPDATE`, the create rule, the company-delete rule, the task check, the event rules, the trimming rules, the invariants and the real request path. |
| [`api.md`](api.md) | The fourteen published operations. |
| [`testing.md`](testing.md) | The schema suite, the 51 endpoint cases, the two flow cases, the contract cases, the unit tests, the requirement-to-test map and the gaps. |
| [`../../api/conventions.md`](../../api/conventions.md) | Pagination, money, filtering and timestamps. |
| [`../../database/schema.md`](../../database/schema.md) | The three tables, created by `migrations/0005_job_applications.sql`. |
| [`../../decisions/ADR-0007-money-representation.md`](../../decisions/ADR-0007-money-representation.md) | The money decision. |

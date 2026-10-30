# ATOM testing strategy

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `docs` agent

This document describes how the ATOM backend is tested: the layers, what each
layer proves, how the suite is kept safe to run, how the tests are named, how
to run them, and — importantly — what is **not** covered today.

The companion document is
[`verification-policy.md`](verification-policy.md), which says what must pass
before a change is complete. This one says how the tests are built.

---

## 1. The four layers

| Layer | What it proves | Needs a database? | Where it lives |
| ----- | -------------- | ----------------- | ------------- |
| **Unit** | A rule behaves as specified, in isolation, with no I/O. | No | A `#[cfg(test)] mod tests` block next to the code it covers, under [`../../src/`](../../src/). |
| **Integration** | The whole request path works: routing, extraction, validation, the service, the repository and the real PostgreSQL semantics. | Yes | [`../../tests/`](../../tests/), with the shared harness in [`../../tests/common/mod.rs`](../../tests/common/mod.rs). |
| **Contract** | The public request and response shape matches the documented API, and the OpenAPI document describes the routes that exist. | Partly | The OpenAPI document is generated from the handlers in [`../../src/api/routes.rs`](../../src/api/routes.rs); the shapes themselves are exercised by the integration suite. |
| **Security** | A hostile caller cannot do what an authorised caller can: no account enumeration, no cross-account access, no credential leakage, no unauthenticated access. | Yes | Dedicated cases inside the integration files; the rules are in [`../security/secure-development.md`](../security/secure-development.md). |

### 1.1 Unit

Unit tests cover the rules that are pure functions of their input: email
normalization, the password policy, the display-name rule, note and project
and task validation, the shared text validators, the pagination convention, the
password hasher, the token service, the error contract and the rate limiter's
algorithm.

They are cheap, they run without a database, and they are where a boundary
value belongs.

**Current size:** **229** unit tests in the library target, plus 3 in
`src/bin/atom-docs.rs` and 13 in `src/bin/atom-seed.rs` — **245** unit-target
tests in all. The 229 are counted from the files on 2026-10-27: they are the 216
of the previous measurement plus the 13 the `api_keys` module adds (`secret.rs` 5,
`service.rs` 4, `models.rs` 3, `dto.rs` 1). The whole-suite figure of the `K1`
gate run is in §1.2. The table below names the Phase 1 modules; only the two rows
re-counted on 2026-10-26 (`config/tests.rs` 58, `helpers/validation.rs` 7) carry a
re-counted figure, and the rest are the count of 2026-10-22.

| Module | Tests |
| ------ | ----- |
| [`../../src/config/tests.rs`](../../src/config/tests.rs) | 58 |
| [`../../src/users/service.rs`](../../src/users/service.rs) | 9 |
| [`../../src/tasks/service.rs`](../../src/tasks/service.rs) | 7 |
| [`../../src/auth/token.rs`](../../src/auth/token.rs) | 7 |
| [`../../src/auth/password.rs`](../../src/auth/password.rs) | 6 |
| [`../../src/error.rs`](../../src/error.rs) | 5 |
| [`../../src/notes/service.rs`](../../src/notes/service.rs) | 5 |
| [`../../src/projects/service.rs`](../../src/projects/service.rs) | 5 |
| [`../../src/pagination.rs`](../../src/pagination.rs) | 5 |
| [`../../src/helpers/validation.rs`](../../src/helpers/validation.rs) | 7 |
| [`../../src/ratelimit.rs`](../../src/ratelimit.rs) | 6 |
| [`../../src/utils/text.rs`](../../src/utils/text.rs) | 2 |

The configuration module [`../../src/config/`](../../src/config/) has **58**
unit tests of its own, in [`../../src/config/tests.rs`](../../src/config/tests.rs).
They cover the defaults, the precedence between the three layers, the required
secrets, every validation rule and the message catalogue.

The table counts the Phase 1 modules only. The reimbursement module adds ten
unit tests of its own in [`../../src/reimbursements/`](../../src/reimbursements/)
— the currency rule, the reversed date range, a blank search, the title trim,
the `project_id` tri-state, the item tri-states, the transition table, the
refusal to reopen a reviewed claim, the receipt URL rule and the category rule —
plus three money tests in `src/helpers/validation.rs`. They are listed one by
one in
[`../features/reimbursements/testing.md`](../features/reimbursements/testing.md)
§4 rather than counted here, because that document is where their rules are
described.

The job-application module adds **eighteen** unit tests — thirteen in
[`../../src/job_applications/service.rs`](../../src/job_applications/service.rs)
and five in
[`../../src/job_applications/dto.rs`](../../src/job_applications/dto.rs) — for
the 49-pair transition table, the derived write predicate, the create-stage
rule, the trims, the URL rules, the money rules, the tri-state `PATCH` and the
event-kind vocabulary. They are listed one by one in
[`../features/job-applications/testing.md`](../features/job-applications/testing.md)
§4. The two binary targets contribute nine more (`src/bin/atom-docs.rs` 3,
`src/bin/atom-seed.rs` 6).

### 1.2 Integration

Integration tests drive the real router with real requests and assert against
the real response. They are the only layer that can prove a SQL predicate, a
database constraint or a `ON DELETE CASCADE` behaves as documented, and they
are the layer that proves the HTTP contract.

**Current size:** 293 integration test functions in seventeen files on a unix
host, and 291 on Windows: the two tests in `tests/shutdown.rs` are unix-only,
because they deliver a real signal. With the 245 unit-target tests (229 in the
library, 3 in `atom-docs` and 13 in `atom-seed`) the whole suite is **538** on a
unix host and **536** on Windows. Measured by `cargo test --all-targets
--locked` in the `K1` gate run on 2026-10-27; the per-file figures below are
counted from the files.

| File | Tests | Covers |
| ---- | ----- | ------ |
| [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 29 | Registration, normalization, duplicate registration, the password policy, login, the generic failure, refresh rotation, the inherited absolute lifetime, the expired session that cannot be extended by a rotation, refresh reuse, logout, credential enforcement, `/auth/me`, cross-account session revocation, the authentication rate limit. |
| [`../../tests/notes_api.rs`](../../tests/notes_api.rs) | 12 | Note CRUD, cross-account invisibility, the listing scope, search, an over-long search term, archiving, an empty title, pagination, the configured page bounds, the reported filtered total, an offset past the end, a malformed identifier. |
| [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) | 25 | Project CRUD, cross-account invisibility, task creation and listing, the completion timestamp, filtering, task isolation through a foreign project, task creation in a foreign project, the project delete cascade, an unknown task status, manual ordering, pagination and the configured page bounds on both listings, an empty page versus a missing project, the reported filtered totals, a request that exceeds the timeout. |
| [`../../tests/docs_route.rs`](../../tests/docs_route.rs) | 2 | The API explorer and one of its assets are served when the `docs` feature is on, and the OpenAPI document carries the configured title. The whole file is gated on the feature, so it compiles to nothing in a `--no-default-features` build. |
| [`../../tests/shutdown.rs`](../../tests/shutdown.rs) | 2 (unix only) | A real `SIGTERM` stops the process without waiting the grace away first, and the drain is bounded when an in-flight request cannot finish. See §1.4. |

The table above is the Phase 1 files. The files the Phase 2 run adds —
`tests/openapi_contract.rs`, `tests/contract_sweep.rs`, `tests/list_query.rs`,
`tests/seeder.rs`, `tests/claims_schema.rs`, `tests/claims_api.rs`,
`tests/claims_flow.rs`, `tests/jobs_schema.rs`, `tests/job_applications_api.rs`,
`tests/job_applications_flow.rs`, `tests/ml_api.rs` and
`tests/api_keys_api.rs` — are not in the table.
Their counts are in the measured split below. The earlier whole-suite figures
are history: **296** in `ATOM-CHANGE-20261024-007`, **299** in
`ATOM-CHANGE-20261024-008` (where the three cases of `tests/claims_flow.rs` are
counted), 481 at the D4 close-out, 483 at the first executable gate and 504 at
`ATOM-REVIEW-20261024-011`. None of them is today's size. See §7 for the Phase 2
layer map.

**The `K1` run, 2026-10-27:** 293 integration test functions in seventeen files,
291 of them on a Windows host because the two `shutdown` cases are unix-only;
245 unit-target tests; a whole-suite count of **538** on a unix host and **536**
on Windows, all passing. The split:
`lib` 229, `atom-docs` 3, `atom-seed` 13, `api_keys_api` 11, `auth_api` 29,
`claims_api` 56, `claims_flow` 3, `claims_schema` 10, `contract_sweep` 2,
`docs_route` 2, `job_applications_api` 51, `job_applications_flow` 2,
`jobs_schema` 11, `list_query` 4, `ml_api` 41, `notes_api` 12,
`openapi_contract` 11, `projects_tasks_api` 25, `seeder` 21,
`shutdown` 0 on Windows (2 on unix).

`lib`, `atom-docs` and the integration files above are counted from the files on
2026-10-27. `atom-seed`'s 13 is carried from the previous measured split: a plain
count of `#[test]` attributes in `src/bin/atom-seed.rs` finds 7, so the figure
comes from a `cargo test --list` run and was not re-derived for this change.

`tests/claims_api.rs` is the largest Phase 2 file: 56 cases covering the twelve
published operations, listed one by one in
[`../features/reimbursements/testing.md`](../features/reimbursements/testing.md)
§2.

### 1.4 Process layer

One file drives the real binary as a child process, because some behaviour only
exists outside the router.

[`../../tests/shutdown.rs`](../../tests/shutdown.rs) spawns `atom-api` on a
reserved ephemeral port, waits for `GET /health/ready` to answer `200`, then sends
a real `SIGTERM` with `kill -TERM`. It asserts two things that a unit test of the
drain cannot:

| Test | What it distinguishes |
| ---- | --------------------- |
| `a_signal_stops_the_process_without_waiting_for_the_grace` | With an 8 s grace and nothing in flight, the process must exit within 4 s. The **original** implementation slept the grace *inside* the signal future before draining, so it would take ~8 s and fail. This is the regression the review found. |
| `the_drain_is_bounded_when_an_in_flight_request_cannot_finish` | With a 2 s grace and a request that never completes, the process is still alive 500 ms after the signal — proving the request is genuinely being drained rather than dropped — and has exited by the deadline plus a margin. Removing the `select!` bound makes the request end only at the 30 s request timeout, outside the window. |

Constraints that follow from this design:

- **unix only.** The file is `#![cfg(unix)]`; on Windows it compiles to nothing and
  `cargo test` reports `0 tests`. A Windows host therefore does **not** prove the
  behaviour, and this file must not be read as if it did.
- **It needs `TEST_DATABASE_URL`**, and skips with a printed reason when the
  variable is unset, rather than failing on a machine that has no database.
- It reuses the harness's database-name guard instead of copying it, so a test run
  still cannot point at a non-test database.

**Verification status.** Both tests pass on `ubuntu`. CI run
[`40011694171`](https://github.com/g-zenr/atom-api/actions/runs/40011694171)
reports `2 passed` for `tests/shutdown.rs`, and the bounded-drain test logs the
warning that proves it reached its deadline with a request still in flight:

```text
WARN atom_api: the shutdown grace elapsed with requests still in flight grace_seconds=2
```

They were written on a Windows machine, where the file compiles to nothing, so
CI was the first execution. `KI-019` records that history and is closed.

### 1.3 Contract

- The OpenAPI document is **generated from the handlers** with `utoipa`, and the
  Swagger UI serves that generated document. The OpenAPI annotation lives next
  to the handler it describes.
- The generated document and the router are declared independently, so they are
  held together by assertion. `tests/openapi_contract.rs` compares the route
  table in `src/api/routes.rs` with the operations in the document **in both
  directions**: a route the document does not describe fails, and an operation
  the router does not serve fails. It also pins the published query parameter
  set of each list operation. The document now describes **54 operations** — 52
  under `/api/v1` and the two health probes; the three API-key operations are the
  most recent addition, the three machine-learning operations came before them,
  the fourteen job-application operations before those, and the count stood at 34
  before those.
  Six list operations have their published parameter set pinned: notes, projects,
  tasks, claims, companies and job applications. The fourteen job operations are
  asserted to hold fourteen rows, each with its published success status.
- The request and response shapes are additionally checked by the integration
  suite, which asserts on the real JSON body: field names, field types, the
  error envelope, the pagination block and the absence of secret material.
- What is still **not** asserted is behaviour behind the document — for example
  that every documented status code is reachable. The document says what the
  operations are, not that each branch was exercised.

### 1.4 Security

There is no separate security test file. The security cases live inside the
integration files, next to the behaviour they protect, so that a reviewer
sees them in context.

Ten of the sixty-eight integration tests are dedicated security cases:

| Test | File |
| ---- | ---- |
| `login_fails_with_a_generic_message` | [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| `an_unknown_email_produces_the_same_failure_as_a_wrong_password` | [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| `protected_routes_reject_missing_and_invalid_credentials` | [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| `reusing_a_rotated_refresh_token_revokes_every_session` | [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| `an_account_cannot_log_out_another_accounts_session` | [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| `a_note_belonging_to_another_user_is_invisible` | [`../../tests/notes_api.rs`](../../tests/notes_api.rs) |
| `the_listing_only_contains_the_callers_notes` | [`../../tests/notes_api.rs`](../../tests/notes_api.rs) |
| `a_project_of_another_user_is_invisible` | [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) |
| `a_task_cannot_be_reached_through_a_project_of_another_user` | [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) |
| `a_task_cannot_be_created_in_a_project_of_another_user` | [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) |

A further case, `registration_returns_a_token_pair_and_a_safe_user`, asserts
that the public user representation carries no secret material.

### 1.5 The configuration loader

The loader in [`../../src/config/loader.rs`](../../src/config/loader.rs)
resolves three layers — the base files, the environment overlay and the
environment variables — and hands the result to the rules in
[`../../src/config/validation.rs`](../../src/config/validation.rs). It is the
one part of the system whose behaviour is decided by files outside the
repository, so it needs coverage of its own:

| Case | What it proves |
| ---- | -------------- |
| The defaults | The committed files in [`../../config/`](../../config/) parse, and the values match the reference table in [`../architecture/deployment.md`](../architecture/deployment.md). |
| An environment-variable override | The variable wins over the file, key by key. |
| The precedence between the layers | The file wins over the compiled default, the overlay wins over the base file, and the variable wins over both. |
| The environment overlay | `config/environments/<APP_ENV>.toml` is applied for the selected environment, and the overlay of the other environment is not. |
| A malformed file | A file that cannot be parsed is a start-up error, not a silent fallback. |
| A missing required secret | `DATABASE_URL` or `APP_JWT_SECRET` missing stops the process before it connects to anything. |
| Each validation rule | Every rule in [`../../src/config/validation.rs`](../../src/config/validation.rs) refuses the value it exists to refuse. |

The configuration module has its own unit tests. The cases above are
covered by the 58 tests in [`../../src/config/tests.rs`](../../src/config/tests.rs),
which inject the environment into the loader instead of reading the process
environment, so they neither mutate the environment of the test process nor
depend on each other. What is still missing is a test of the **resolved**
configuration as the running server sees it: the harness builds its state from
`Config::defaults()`, so the environment-variable layer of the precedence is
covered by the unit tests and not by an end-to-end run.

## 2. Isolation and the production-safety guard

The integration suite talks to a real PostgreSQL database. Two rules keep that
safe.

**The production-safety guard.** The harness refuses to start unless the
database name in `TEST_DATABASE_URL` contains the substring `test`. The check
is an assertion in `test_database_url()` in
[`../../tests/common/mod.rs`](../../tests/common/mod.rs), and it runs before any
connection is opened. A production database therefore cannot be reached by an
accidental `cargo test`, whatever the environment says.

```text
refusing to run integration tests against '<url>': the database name must contain 'test'
```

**The migrations.** The harness applies the embedded migrations itself, so a
test run always starts from the current schema. The application does the same
at start-up, which is why a deployment never depends on files shipped next to
the binary.

**No truncation.** No test truncates a table, and no test deletes data it did
not create. The suite therefore never destroys another test's data, and a
failure in one test cannot cascade into the others.

**No shared mutable state.** Each test builds its own `AppState` and its own
connection pool. There is no global fixture, no `OnceLock` of test data and no
ordering dependency. Any test can be run alone, in any order, and in
parallel.

**A note on the harness configuration.** The harness starts from
`Config::defaults()`, which loads the real files in [`../../config/`](../../config/)
without the environment overlay and without the environment variables, and it
deliberately weakens three settings so that the suite stays fast and
deterministic:

| Setting | Test value | Production default |
| ------- | ---------- | ------------------ |
| Argon2id | 8 MiB, 1 iteration, parallelism 1 | 19 MiB, 2 iterations, parallelism 1 |
| Rate limit | 10 000 requests per 60 seconds | 10 requests per 60 seconds |
| Issuer and audience | `atom-api-test` | `atom-api` |

The "production default" column is the value in the committed files in
[`../../config/`](../../config/), which is what a deployment gets unless it
overrides it.

The harness also supplies the two values that have no file layer: the
database URL and the signing secret. The secret is deliberately **not** a
placeholder, so it satisfies the production rule as well.

The cheap Argon2 parameters and the raised rate limit mean the suite proves
the *logic*, never the *cost*. A test result never says anything about how
expensive a real password hash is.

Because the tests drive the router directly with `tower::ServiceExt::oneshot`,
no socket is bound and no port is used. That is also why the rate limiter sees
no client address in the tests: the connection information is a property of a
real socket.

## 3. The fixture strategy

**Unique data per test.** Every test creates its own data with a unique
identifier, so tests never collide and never need to be cleaned up.

| Helper | What it does |
| ------ | ------------ |
| `unique_email(prefix)` | Returns `prefix-<uuid>@example.test`. |
| `register_user(app, prefix)` | Registers a user with the fixed password `Test-Password-123` and returns the id, the email, the password and both tokens. |
| `create_project(app, user, name)` | Creates a project owned by the user and returns its id. |

**Shared request helpers.** `request` builds a request with an optional bearer
token and an optional JSON body; `send` sends it and parses the response;
`app` builds the router from a state; `test_state` builds the state.
`TestResponse::expect_status` asserts the status code and returns the parsed
body, so a test reads as a sequence of assertions without repetition.

**No fixture files, no seeding script.** There is no `fixtures/` directory and
no seed data. A test that needs a project creates one through the API, which
means the setup path is itself covered.

**What a test asserts.** The status code first, then the body. The body is
asserted on the fields that carry the contract, not on the whole document, so
that an unrelated addition does not break a test.

## 4. Naming conventions

| Thing | Convention | Example |
| ----- | ---------- | ------- |
| Test function | The behaviour, in the present tense, in `snake_case` | `a_note_belonging_to_another_user_is_invisible` |
| Unit test module | `mod tests` inside `#[cfg(test)]`, next to the code | `#[cfg(test)] mod tests` in [`../../src/users/service.rs`](../../src/users/service.rs) |
| Async test | `#[tokio::test]` | `#[tokio::test] async fn login_succeeds_with_the_right_password()` |
| Integration file | The area, with the `_api` suffix | `auth_api.rs`, `notes_api.rs`, `projects_tasks_api.rs` |
| Shared harness | `tests/common/mod.rs`, imported with `pub mod common;` | `use common::{app, register_user, request, send, test_state};` |
| Test module inside an integration file | The harness is `common`; helpers are imported by name | `use common::{create_project, register_user, request, send, test_state};` |
| Assertion helper | `expect_status` | `response.expect_status(StatusCode::NOT_FOUND)` |

A test name states the behaviour, not the mechanism. `the_listing_only_contains_the_callers_notes` says what must hold; `test_list_notes_2` does not.

## 5. Running the suite

### 5.1 The whole suite

The integration suite needs a disposable PostgreSQL database. `docker compose up
-d postgres` brings one up locally, with the `atom_test` database created
alongside `atom`.

Unix:

```bash
export TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test
cargo test --all-targets
```

Windows PowerShell:

```powershell
$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
cargo test --all-targets
```

`make check` runs the three quality gates — formatting, linting and the whole
test suite — in that order.

### 5.2 A subset

```bash
cargo test --lib                 # the unit tests only; no database needed
cargo test --test auth_api       # one integration file
cargo test --test notes_api a_note_belonging_to_another_user_is_invisible
cargo test -- --nocapture        # keep the log output
```

A unit test can be run without a database at all; the integration tests panic
with a clear message if `TEST_DATABASE_URL` is missing or does not name a
`test` database.

### 5.3 The other checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo run --bin atom-docs        # the documentation consistency checker
```

## 6. What is not covered today

This list is a to-do list, not a criticism. Each item is a real gap between
the requirements in [`../project-truth.md`](../project-truth.md) §13 and the
suite as it stands.

1. **No HTTP-level test of the CORS layer.** The configuration layer refuses a
   `*` origin and an origin that is not a valid header value —
   `a_wildcard_cors_origin_is_refused` and
   `a_cors_origin_that_is_not_a_valid_header_is_refused` in
   [`../../src/config/tests.rs`](../../src/config/tests.rs) — but nothing
   asserts that a request carrying a disallowed `Origin` is answered without
   the CORS headers.
2. **No contract test of the OpenAPI document.** Nothing asserts that the
   generated document lists every route, that a documented status code is
   reachable, or that the document can be parsed as OpenAPI.
   *Partly covered since 2026-10-24:* `tests/openapi_contract.rs` asserts that
   the route table and the document describe the same operations in both
   directions, that every route is mounted, and that each list operation
   publishes its parameter set exactly once. The reachability of every
   documented status code is still not asserted.
3. **No test of the login timing equalisation.** The dummy verification exists
   and uses the configured cost parameters; nothing measures that the two
   paths take a comparable time.
4. **No test at the database level of the email constraint.** The normalization
   is proven through the API; the `CHECK` constraint itself is not exercised
   with a direct statement.
5. **No concurrency, load or soak test.** The concurrent refresh of the same
   token is covered, but nothing exercises two requests for the same account
   at the same time, or a sustained request rate.
6. **No fuzzing of the JSON, path and query extractors.**
7. **No test that the health endpoints answer when the database is
   unreachable.** The degraded branch of the readiness probe is untested.
8. **No test that a refresh rotation preserves the absolute deadline.** The
   rotation must give the replacement session the `expires_at` of the session
   it replaces, so that the absolute lifetime is measured from the creation of
   the session and a rotation cannot extend it. The defect that broke this
   property is `KI-015` in
   [`../engineering/known-issues.md`](../engineering/known-issues.md).
   *Covered since 2026-10-22:* `a_rotation_preserves_the_absolute_lifetime_of_the_session`
   in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) rotates a session
   and asserts that the replacement row carries the same `expires_at`.
9. **No test of the `search` length bound.** The shared `validate_search` in
   [`../../src/helpers/validation.rs`](../../src/helpers/validation.rs)
   rejects a term longer than 200 characters, and the bound is part of the
   published contract.
   *Covered since 2026-10-22:* the unit test
   `a_search_term_is_trimmed_and_bounded` asserts the bound and the field
   name, and `a_search_term_over_the_bound_is_refused` in
   [`../../tests/notes_api.rs`](../../tests/notes_api.rs) asserts the `422`
   at HTTP level.

## 7. The Phase 2 test matrix

Phase 2 adds three features — reimbursements, job applications and local
machine learning — plus a database seeder and the hardening of the existing
API. This section says which layer proves which property, so that a gap is
visible before the code is written.

**This is a record as well as a target.** The layer map below is the target; §7.2.1
says what exists in the tree today. The `K1` gate run of 2026-10-27 is six gates
green and **536 passed, 0 failed** on Windows (**538** on a unix host), and every
Phase 2 suite executes. The earlier figures are history, not
today's state: 296 at `ATOM-CHANGE-20261024-007`, 299 at
`ATOM-CHANGE-20261024-008`, 481 at the D4 close-out, 483 at the first executable
gate, and 504 at `ATOM-REVIEW-20261024-011`. The per-feature status is derived in
each feature's own `status.md`; a green suite is not a review, so no Phase 2
feature is `Verified` yet.

### 7.1 Which layer proves what

| Property | Proved by | Why not another layer |
| -------- | --------- | --------------------- |
| **Schema** — the migration applies from empty, the constraints exist, the cascade and `SET NULL` behave | Integration, against a real PostgreSQL | A unit test cannot run a `CHECK`; a mock cannot cascade. |
| **Money** — the four-decimal string, the exact sum, no float | Unit (the format) **and** integration (the round trip and the sum) | The unit test proves the parser; only the database proves that `SUM(numeric)` is exact through the whole path. |
| **Transitions** — every allowed and every forbidden edge | Unit (the table) **and** integration (one case per edge) | A unit test proves the table; the integration test proves that the handler and the service actually call it. |
| **Ownership** — a foreign row is `404` for every verb | Integration | The predicate is SQL; a unit test cannot exercise it. |
| **Contract** — every path and method in the document, the parameter sets, the schema types | Contract, against `/api/v1/openapi.json` | The document is generated from the handlers; only a comparison catches drift. |
| **Degradation** — ML off and model missing return lexical results with `200` | Integration, with two configurations | A unit test can prove the selection function; only the request path proves the response shape and the status. |
| **Seeder** — the guards, determinism, the money and ownership invariants | Integration, driving the binary | The seeder is not reachable through the router. |

One Phase 2 feature has all of its layers in the tree today: the reimbursement
schema (`tests/claims_schema.rs` applies migration `0004` and asserts the
constraints, the cascades and the money scale), its endpoint layer
(`tests/claims_api.rs` drives all twelve published operations, including every
item write and every lifecycle action), its contract layer
(`tests/openapi_contract.rs` pins the published surface, which now stands at 54 operations) and its **flow
layer** (`tests/claims_flow.rs`: the journey, the ownership sweep and the seeded
read), with the two caveats §7.2.1 records.

A second feature, job applications, has **all four** layers in the tree: its
schema layer (`tests/jobs_schema.rs`, chain `C-J1`), its endpoint layer
(`tests/job_applications_api.rs`, 51 cases, chains `C-J2`–`C-J4`), its contract
layer (the surface pinned at 54 operations, the fourteen success statuses, the two
job parameter sets and the event schemas carrying no `updated_at`) and its **flow
layer** (`tests/job_applications_flow.rs`, two cases, chain `J5`). The flow layer
is the journey from registration to `offer` — stage path, next-action link,
timeline, money, the terminal stage and the delete chain — and the ownership
sweep over thirteen addressed attempts on one mid-machine application. Unlike
`tests/claims_flow.rs` it has no seeded read: the seeded rows are asserted by
`tests/seeder.rs`. See §7.2.1.

### 7.2 The layers in the Phase 2 suite

| Layer | Phase 2 responsibility | Where |
| ----- | ---------------------- | ----- |
| Unit | The transition tables, the money format, the URL validators, the embedder selection and the similarity function, the configuration defaults and validation | `src/reimbursements/`, `src/job_applications/`, `src/ml/`, `src/config/tests.rs` |
| Integration | Every endpoint: happy path, `401`, `404` for another owner, `400` malformed id and body, `422` validation, every documented error code, the lists, the flow tests | `tests/claims_api.rs`, `tests/claims_flow.rs`, `tests/job_applications_api.rs`, `tests/job_applications_flow.rs`, `tests/ml_api.rs` — **41 ML cases, all executing** — and `tests/api_keys_api.rs` (`11` cases) |
| Contract | The route table against the document, the published parameter sets, the money and score schema types | `tests/openapi_contract.rs` |
| Process | The seeder's guards and determinism | `tests/seeder.rs` |
| Manual | The real model's retrieval quality and cost | Recorded in `docs/features/ml/status.md` |

The Phase 1 layers and the isolation rules in §2 are unchanged. A Phase 2
test uses the same harness, the same production-safety guard and the same
unique-data-per-test rule.

#### 7.2.1 The state of each Phase 2 feature

The layer map above is a target. This table says what is actually covered in
the tree, so an empty layer is visible instead of assumed.

| Feature | Covered today | Not covered |
| ------- | ------------- | ----------- |
| Reimbursements | The **schema layer**: `tests/claims_schema.rs` applies `0004` and asserts the constraints, the composite ownership key, the cascades, the `SET NULL` and the four-decimal money scale. The **seeded claims**: two cases in `tests/seeder.rs` assert the profile counts and the total invariant on data the API did not write. The **endpoint layer**: 56 cases in `tests/claims_api.rs`. The **claim endpoints** cover create/read/patch/delete, the listing with its filters, pagination and order, money on the wire, the ignored `currency` and `status` fields, ownership with a second account, `400`/`413`/`422` mapping and the credential. The **item endpoints and the lifecycle** account for 21 of the 56 — the three item writes, the total recomputed after every one of them, the four-decimal input rule, the draft-only guard in every non-draft status, the five allowed edges and all fifteen refused ones, the reopen clearing both `submitted_at` and `decided_at`, the submit preconditions and `paid` as terminal. All twelve operations are also asserted to answer `401` without a credential. The **contract layer**: three cases in `tests/openapi_contract.rs` for the money schema types, the nullable `project_id` and the seven new operations' success statuses, with the surface pinned at 48 when that chain landed and at 54 today. The **unit layer**: twelve tests inside `src/reimbursements/` (ten in `service.rs`, two in `dto.rs`) and three money tests in `src/helpers/validation.rs`. The **flow layer**: three cases in `tests/claims_flow.rs` (committed at `02c5489`) — the journey from registration to payment, the ownership sweep (eleven claim-addressed operations on each of five statuses, 55 requests, each `404`; the list operation is asserted by its empty page and `total` 0, because it publishes `200` rather than `404`) and the seeded read. | Two caveats on the flow layer. The seeded case is **opportunistic**: it returns with a printed reason when `owner@atom.local` is absent, so the seeded surface is covered only on a database a seeder run has touched, and the seeded *size* is never asserted because the shared database carries whichever profile ran last. The seeder's five statuses and the endpoint's field rules therefore meet in one test only where a seeder run has happened; the sweep does not supply that, because it registers both of its own accounts. Also open: `a_claim_can_be_deleted_in_any_status` (so `REIMB-R11`'s "in any status" is unasserted) and five smaller rules with no HTTP case — the 1–500 item-description bound, a non-date `incurred_on`, a non-UUID `item_id`, a file body on the item route, and any concurrency case. The `408` and `500` paths are not asserted on a claims route. The suite is recorded at 296 in `ATOM-CHANGE-20261024-007` and at 299 in `ATOM-CHANGE-20261024-008`, but no `code-reviewer` or `quality-gate` row has been written for this chain, so nothing in the feature is promoted to `Verified`. |
| Job applications | The **schema layer**: `tests/jobs_schema.rs` (11 cases, chain `C-J1`) creates a scratch database, applies `0001`-`0005` from empty, and asserts the functional unique index on `(user_id, LOWER(name))`, the trim check, the composite `job_applications_company_fk` that makes a cross-owner application unrepresentable, the `NO ACTION` company refusal together with the `DELETE FROM users` cascade it must not block, the `ON DELETE CASCADE` from an application to its events, the `user_id` cascades of all three tables, the three salary checks, four-decimal storage, and that the `job_application_status` enum refuses an unknown stage with `22P02` and no constraint name. The **seeded jobs**: one case in `tests/seeder.rs` asserts the `dev` counts (20 applications, 12 companies, 50 events), the deterministic ids, and the ownership and salary invariants of the seeded rows. The **endpoint layer**: 51 cases in `tests/job_applications_api.rs` (chains `C-J2`–`C-J4`) covering the five company, five application and four event operations — CRUD, both listings with their filters, order and pagination, the stage machine pair by pair (12 edges, 7 accepted no-ops, 30 refusals, the three terminal stages, and a refused move writing nothing else), the create accepting only `wishlist`, the company-delete refusal and the delete that follows it, the next-action task check and its `SET NULL`, the event order and the in-place `PATCH` with no `updated_at`, the salary rules, ownership on every verb, and a `401`/`400`/`404`/`413`/`422` sweep over all fourteen operations. The **contract layer**: four cases written or extended, with the surface pinned at 54 today (48 when that chain landed, 34 before it) — both directions of the route table, the fourteen success statuses, the published parameter sets of both job lists, and the three event schemas carrying no `updated_at`. The **unit layer**: eighteen tests in the module. The **flow layer**: two cases in `tests/job_applications_flow.rs` — the journey from registration to `offer` (stage path, next-action link, timeline, money, the terminal stage, the delete chain, both listings emptied) and the ownership sweep over thirteen addressed attempts, each `404` and never `409`, with the owner's rows byte-identical afterwards. | The flow layer has **no seeded read through the API**: unlike `tests/claims_flow.rs`, the seeded rows are asserted only by `tests/seeder.rs`. The sweep enumerates by hand and pins its list at thirteen attempts, so a new route is caught by the 51-operation pin in `tests/openapi_contract.rs`, not by this file. No run of either flow case is recorded. Within the endpoint layer, the application listing's pagination bounds (the clamp, an `offset` past the end), the event listing's `offset` and clamp, the length half of the two URL rules, the description bound, a blank `source` through the API, and any assertion that the feature generates no text are also unasserted; [`../features/job-applications/testing.md`](../features/job-applications/testing.md) §5-§6 lists them. These remain the feature's recorded gaps; the promotion evidence is `ATOM-REVIEW-20261029-002` (final-tree read clean, six gates at `0`), and the feature is `Verified`. |
| Local machine learning | **Written and executing.** `tests/ml_api.rs` holds **41 cases** covering the endpoint, degradation, ownership, match, reindex and contract layers - all 41 pass in the `K1` gate run of 2026-10-27 (536 passed, 0 failed; six gates at `0`). The **schema** (`migrations/0006_embeddings.sql`) has no test of its own and is exercised only through those cases. The **configuration** has seven cases in `src/config/tests.rs` (a file of 58) - the defaults, the environment override, the backend vocabulary, the candidate-limit ceiling, the dimension, the timeout ceiling and the removal of the old page-maximum relation - and they pass. `src/ml/` has **64 unit cases**, including six in `burn_embedder.rs` and fifteen in `tokenizer.rs`. | **A real-model run in CI.** The **encoder** is covered by unit cases and the manual `atom-ml-bench` record, but no CI case loads a model file. The **quality evaluation**: the fifteen-query fixture proves the plumbing only; the real-model run is the recorded manual benchmark in [`../features/ml/status.md`](../features/ml/status.md) §5, and its pair count is marked unverified there. The **seeder reset against embeddings**: `--reset` purges `embeddings`, but no case asserts that purge specifically. **Concurrency**: nothing runs two reindexes, or a reindex beside a search. The feature is `Verified`: the encoder span was read clean (`ATOM-REVIEW-20261029-002`, building on `-20261026-001`) and the six gates re-ran at `0`; `ML-R02` is met. The real-model benchmark was reproduced on 2026-10-29 from its documented invocation (`ml/status.md` §5.1a); the remaining recorded limitation is that no CI case loads a model file. |

A schema test is not an endpoint test, and neither is a claim test an item test.
The reimbursement feature is `Verified`: every one of its eleven
requirements (`REIMB-R01`…`R11`) names at least one test, the total invariant
(`R03`) is asserted after insert, update and delete, and the lifecycle (`R04`,
`R05`, `R06`) has one case per edge plus every refusal. The flow layer is
present — `tests/claims_flow.rs`, three cases committed at `02c5489` — and it is
counted as coverage, with the seeded case conditional on a seeder run. Five
requirement halves remain unasserted and are recorded as the feature's
documented gaps, not hidden: `R11`'s deletion in a non-draft status,
`R08`'s refusal of a file body, `R03`'s concurrency case, and the
item-description and `incurred_on` validations. The requirement-by-requirement
map, including the gaps, is
[`../features/reimbursements/testing.md`](../features/reimbursements/testing.md)
§2.9, and the gaps are collected in its §3. The promotion evidence is
`ATOM-REVIEW-20261029-002`: the final-tree read found no code defect, and the
six gates re-ran at `0`.

Job applications has the opposite history with the same verdict: the schema,
endpoint, contract and flow layers are all written, the contract surface is
pinned at 54 operations, and the feature is `Verified`. Its review history is
the reason the promotion is dated 2026-10-29: `ATOM-REVIEW-20261024-010` found
no code defect but returned FAIL on documentation, the repairs were never
re-read, and `ATOM-REVIEW-20261029-002` is the read of the final tree — clean on
code, contract and regressions, with the six gates re-run at `0`. The
requirement-by-requirement map is
[`../features/job-applications/testing.md`](../features/job-applications/testing.md)
§5, and the gaps are collected in its §6.

Local machine learning now has **all four layers written and the automated ones
executing**. `tests/ml_api.rs` covers the endpoint, degradation, ownership,
match, reindex and contract layers in 41 cases, the three operations are mounted,
and the contract surface is pinned at 54. The module carries 64 unit cases, and
the seeder cases pass with the `--with-embeddings` flag; the run is the `K1` gate
run of 2026-10-27 (six gates at `0`, **536 passed, 0 failed** on
Windows; 538 on a unix host). The feature is `Verified`: `ML-R02` is met — the
Burn encoder produces embeddings — the encoder span was read clean by
`ATOM-REVIEW-20261029-002` (building on `ATOM-REVIEW-20261026-001`, which found
no code defect in the same span), and the six gates re-ran at `0`. The
real-model measurement remains the manual `atom-ml-bench` record in
[`../features/ml/status.md`](../features/ml/status.md) §5 — a recorded check
that passes, not a CI gate, and that limitation stands as recorded. The
requirement-by-requirement record is
[`../features/ml/testing.md`](../features/ml/testing.md) and the evidence for
the status is [`../features/ml/status.md`](../features/ml/status.md).

### 7.3 The size target

| Metric | Before Phase 2 | Target | Note |
| ------ | -------------- | ------ | ---- |
| Integration tests | 68 on Windows, 70 on a unix host | at least **218** | The Phase 2 target is **at least 150 new integration tests**. Exceeded: 291 on Windows today. |
| Unit tests | 111 | at least 150 | The new modules bring their own unit tests; the target is a floor, not a ceiling. Exceeded: 229 in the library today. |
| Whole suite | **179** on Windows, **181** on a unix host | **roughly 330** | 179 + 150 = 329. The two `shutdown` tests remain unix-only, so the platform difference stays. Exceeded: 536 on Windows today. |

The old figure of 177 in
[`plan.md`](../engineering/runs/archive/phase-2/plan.md) §4 is stale; the measured
baseline in
[`ledger.md`](../engineering/runs/archive/phase-2/ledger.md) is 179 on Windows.

An indicative allocation, not a quota:

| Area | New integration tests |
| ---- | --------------------- |
| Reimbursements | about 45 |
| Job applications | about 50 |
| Local machine learning | about 30 |
| The seeder | about 15 |
| Contract and hardening | about 20 |

The claim-CRUD chain contributes 34 integration cases in `tests/claims_api.rs`
and five unit tests in the module towards those floors. The item and lifecycle
wave adds 21 endpoint cases to the same file (which holds 56 today), one contract case in
`tests/openapi_contract.rs` and eight unit tests — four in `service.rs`, one in
`dto.rs` and three money tests in `helpers/validation.rs`. `D2.R5` adds three
integration cases in `tests/claims_flow.rs` — the journey, the ownership sweep
and the seeded read — which take the recorded suite from 296 to **299**
(`ATOM-CHANGE-20261024-008`), and they are counted here.
`C-J1` adds eleven integration cases in `tests/jobs_schema.rs` and one in
`tests/seeder.rs`; those are counted from the files, and no run of them is
recorded in this document.
The counted totals are re-measured by the chain's gate and recorded in the run's
ledger; this document does not carry a number it has not seen measured, and the
number of names in a file is not a number of passing tests.

The tree now holds, measured in the `K1` gate run of 2026-10-27: **229** library
unit tests plus 3 in `atom-docs` and 13 in `atom-seed`, and **293** integration
test functions (291 on Windows), a whole-suite count of **538** on a unix host
and **536** on Windows. The Phase 2 floors in the table above — at least 218
integration, at least 150 unit, roughly 330 in the whole suite — are exceeded.
`C-J2`–`C-J4` contribute the 51 cases of
`tests/job_applications_api.rs`, four cases written or extended in
`tests/openapi_contract.rs`, thirteen unit tests in
`src/job_applications/service.rs` and five in `src/job_applications/dto.rs`;
`J5` adds the two flow cases of `tests/job_applications_flow.rs`. The earlier
whole-suite figures — 296 at `ATOM-CHANGE-20261024-007`, 299 at
`ATOM-CHANGE-20261024-008`, 481 at the D4 close-out, 483 at the first executable
gate, 504 at `ATOM-REVIEW-20261024-011` — are runs of earlier trees, not today's
size.

### 7.4 The rule: a test that cannot fail is not evidence

A test is evidence only if it fails when the rule it protects is removed. The
implementing change must show that, for every rule that matters, by making the
rule wrong on purpose and watching the test fail. The three cheap ways to make
a test worthless, and the answer to each:

| Anti-pattern | Example | The answer |
| ------------ | ------- | ---------- |
| **Asserting only the status** | `assert_eq!(response.status(), 200)` on a search that must return the right note | Assert the body field that carries the rule: the note id, the score's presence, the degraded reason. |
| **A test that passes on an empty result** | "hybrid returns at most 200 items" on a corpus of 3 | Build a corpus larger than the bound, then assert the bound. |
| **A test that cannot reach the branch** | A `404` cross-account test with a token for the same account | Register two accounts and use the other one's token, as the Phase 1 tests do. |

Two further rules apply to this phase specifically:

1. **A degradation test asserts all four fields** — `requested_mode`, `mode`,
   `degraded`, `degraded_reason`. Any one alone can pass by accident.
2. **A money test compares the string**, never a parsed number. A test that
   parses `"0.3000"` into `0.3` and compares it to `0.3` proves nothing about
   the representation.

### 7.5 The gates for a Phase 2 change

A Phase 2 feature is not complete until all of these pass and are recorded:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo check --no-default-features --all-targets
cargo run --bin atom-docs
cargo run --bin atom-docs -- --strict
```

The first three are the Phase 1 gates of
[`verification-policy.md`](verification-policy.md). The fourth proves that the
base build still compiles without the optional features, and the last two
prove that the documentation tree is consistent.

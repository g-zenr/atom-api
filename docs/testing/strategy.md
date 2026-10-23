# ATOM testing strategy

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
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

**Current size:** 111 unit tests in 12 modules, counted on 2026-10-22.

| Module | Tests |
| ------ | ----- |
| [`../../src/config/tests.rs`](../../src/config/tests.rs) | 50 |
| [`../../src/users/service.rs`](../../src/users/service.rs) | 9 |
| [`../../src/tasks/service.rs`](../../src/tasks/service.rs) | 7 |
| [`../../src/auth/token.rs`](../../src/auth/token.rs) | 7 |
| [`../../src/auth/password.rs`](../../src/auth/password.rs) | 6 |
| [`../../src/error.rs`](../../src/error.rs) | 5 |
| [`../../src/notes/service.rs`](../../src/notes/service.rs) | 5 |
| [`../../src/projects/service.rs`](../../src/projects/service.rs) | 5 |
| [`../../src/pagination.rs`](../../src/pagination.rs) | 5 |
| [`../../src/helpers/validation.rs`](../../src/helpers/validation.rs) | 4 |
| [`../../src/ratelimit.rs`](../../src/ratelimit.rs) | 6 |
| [`../../src/utils/text.rs`](../../src/utils/text.rs) | 2 |

The configuration module [`../../src/config/`](../../src/config/) has **50**
unit tests of its own, in [`../../src/config/tests.rs`](../../src/config/tests.rs).
They cover the defaults, the precedence between the three layers, the required
secrets, every validation rule and the message catalogue.

### 1.2 Integration

Integration tests drive the real router with real requests and assert against
the real response. They are the only layer that can prove a SQL predicate, a
database constraint or a `ON DELETE CASCADE` behaves as documented, and they
are the layer that proves the HTTP contract.

**Current size:** 70 integration tests in 5 files on a unix host, and 68 on
Windows: the two tests in `tests/shutdown.rs` are unix-only, because they deliver
a real signal. The suite's size therefore depends on the platform, and CI reports
the unix figure — 181 when the unit tests are counted. Counted on 2026-10-24.

| File | Tests | Covers |
| ---- | ----- | ------ |
| [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 29 | Registration, normalization, duplicate registration, the password policy, login, the generic failure, refresh rotation, the inherited absolute lifetime, the expired session that cannot be extended by a rotation, refresh reuse, logout, credential enforcement, `/auth/me`, cross-account session revocation, the authentication rate limit. |
| [`../../tests/notes_api.rs`](../../tests/notes_api.rs) | 12 | Note CRUD, cross-account invisibility, the listing scope, search, an over-long search term, archiving, an empty title, pagination, the configured page bounds, the reported filtered total, an offset past the end, a malformed identifier. |
| [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) | 25 | Project CRUD, cross-account invisibility, task creation and listing, the completion timestamp, filtering, task isolation through a foreign project, task creation in a foreign project, the project delete cascade, an unknown task status, manual ordering, pagination and the configured page bounds on both listings, an empty page versus a missing project, the reported filtered totals, a request that exceeds the timeout. |
| [`../../tests/docs_route.rs`](../../tests/docs_route.rs) | 2 | The API explorer and one of its assets are served when the `docs` feature is on, and the OpenAPI document carries the configured title. The whole file is gated on the feature, so it compiles to nothing in a `--no-default-features` build. |
| [`../../tests/shutdown.rs`](../../tests/shutdown.rs) | 2 (unix only) | A real `SIGTERM` stops the process without waiting the grace away first, and the drain is bounded when an in-flight request cannot finish. See §1.4. |

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

The contract layer is thin today, and that is a deliberate trade-off rather
than an oversight.

- The OpenAPI document is **generated from the handlers** with `utoipa`, and the
  Swagger UI serves that generated document. A route that does not exist
  therefore cannot appear in the document, and a document cannot describe a
  route that does not exist. The OpenAPI annotation lives next to the handler
  it describes.
- The request and response shapes are additionally checked by the integration
  suite, which asserts on the real JSON body: field names, field types, the
  error envelope, the pagination block and the absence of secret material.
- There is **no dedicated contract test file**, and no test that asserts the
  *content* of the OpenAPI document — for example that every documented path
  answers, or that every documented status code is reachable. This is listed as
  a gap in §6.

### 1.4 Security

There is no separate security test file. The security cases live inside the
integration files, next to the behaviour they protect, so that a reviewer
sees them in context.

Ten of the fifty-six integration tests are dedicated security cases:

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

**The configuration module has its own unit tests.** The cases above are
covered by the 50 tests in [`../../src/config/tests.rs`](../../src/config/tests.rs),
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

# ATOM testing strategy

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
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
and task validation, the pagination convention, the password hasher, the token
service, the error contract, the configuration validation and the rate
limiter's algorithm.

They are cheap, they run without a database, and they are where a boundary
value belongs.

**Current size:** 39 unit tests in 10 modules.

| Module | Tests |
| ------ | ----- |
| [`../../src/config.rs`](../../src/config.rs) | 6 |
| [`../../src/users/service.rs`](../../src/users/service.rs) | 6 |
| [`../../src/auth/token.rs`](../../src/auth/token.rs) | 5 |
| [`../../src/auth/password.rs`](../../src/auth/password.rs) | 4 |
| [`../../src/error.rs`](../../src/error.rs) | 4 |
| [`../../src/ratelimit.rs`](../../src/ratelimit.rs) | 3 |
| [`../../src/notes/service.rs`](../../src/notes/service.rs) | 3 |
| [`../../src/pagination.rs`](../../src/pagination.rs) | 3 |
| [`../../src/tasks/service.rs`](../../src/tasks/service.rs) | 3 |
| [`../../src/projects/service.rs`](../../src/projects/service.rs) | 2 |

### 1.2 Integration

Integration tests drive the real router with real requests and assert against
the real response. They are the only layer that can prove a SQL predicate, a
database constraint or a `ON DELETE CASCADE` behaves as documented, and they
are the layer that proves the HTTP contract.

**Current size:** 53 integration tests in 3 files.

| File | Tests | Covers |
| ---- | ----- | ------ |
| [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 26 | Registration, normalization, duplicate registration, the password policy, login, the generic failure, refresh rotation, refresh reuse, logout, credential enforcement, `/auth/me`, cross-account session revocation. |
| [`../../tests/notes_api.rs`](../../tests/notes_api.rs) | 8 | Note CRUD, cross-account invisibility, the listing scope, search, archiving, an empty title, pagination, a malformed identifier. |
| [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) | 19 | Project CRUD, cross-account invisibility, task creation and listing, the completion timestamp, filtering, task isolation through a foreign project, task creation in a foreign project, the project delete cascade, an unknown task status. |

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

Twenty of the fifty-three integration tests are dedicated security cases:

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

## 2. Isolation and the production-safety guard

The integration suite talks to a real PostgreSQL database. Two rules keep that
safe.

**The production-safety guard.** The harness refuses to start unless the
database name in `TEST_DATABASE_URL` contains the substring `test`. The check
is an assertion in `test_config()` in
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

**A note on the harness configuration.** The harness deliberately weakens three
settings so that the suite stays fast and deterministic:

| Setting | Test value | Production default |
| ------- | ---------- | ------------------ |
| Argon2id | 8 MiB, 1 iteration, parallelism 1 | 19 MiB, 2 iterations, parallelism 1 |
| Access token lifetime | 15 minutes | 15 minutes |
| Refresh token lifetime | 1 day | 14 days |
| Rate limit | 10 000 requests per 60 seconds | 10 requests per 60 seconds |
| Issuer and audience | `atom-api-test` | `atom-api` |

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
| Shared harness | `tests/common/mod.rs`, imported with `mod common;` | `use common::{app, register_user, request, send, test_state};` |
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

1. **No test of the CORS layer.** Neither the refusal of a `*` origin at
   start-up nor the rejection of a disallowed origin.
2. **No contract test of the OpenAPI document.** Nothing asserts that the
   generated document lists every route, that a documented status code is
   reachable, or that the document can be parsed as OpenAPI.
3. **No test of the token-issuing configuration.** Nothing asserts that the
   process refuses to start with a short signing secret, or with the
   documented placeholder in production. Only the validation function is unit
   tested.
4. **No test of the login timing equalisation.** The dummy verification exists
   and uses the configured cost parameters; nothing measures that the two
   paths take a comparable time.
5. **No test at the database level of the email constraint.** The normalization
   is proven through the API; the `CHECK` constraint itself is not exercised
   with a direct statement.
6. **No concurrency, load or soak test.** The concurrent refresh of the same
   token is covered, but nothing exercises two requests for the same account
   at the same time, or a sustained request rate.
7. **No fuzzing of the JSON, path and query extractors.**
8. **No test that the health endpoints answer when the database is
   unreachable.** The degraded branch of the readiness probe is untested.
13. **No test that the health endpoints answer when the database is
    unreachable.** The degraded branch of the readiness probe is untested.

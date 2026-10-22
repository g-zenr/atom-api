# ATOM Phase 1 — remediation backlog

**Date:** 2026-10-22
**Source:** [`audit-report.md`](audit-report.md) — five independent audits
**Statuses:** `pending`, `in progress`, `blocked`, `fixed`, `verified`, `deferred`

Every finding `F-01` … `F-83` and every retained item `R-01` … `R-11` has a
disposition below. A task is only `verified` when its acceptance criteria are
met **and** the verification has been run and recorded.

---

## 1. Status at a glance

| Finding | Title (short) | Task | Status | Owner |
| ------- | -------------- | ---- | ------ | ----- |
| `F-01` | The configured pagination bounds were ignored | 001 | verified | orchestrator |
| `F-02` | A refresh rotation extended the absolute lifetime | 002 | verified | orchestrator |
| `F-03` | `app.name` was read by nothing | 003 | verified | orchestrator |
| `F-04` | `field_error` was implemented twice | 007 | verified | orchestrator |
| `F-05` | `body_mismatch` was read by nothing | 003 | verified | orchestrator |
| `F-06` | The catalogue fixture was copied five times | 007 | verified | orchestrator |
| `F-07` | `RequestSettings` duplicated `RequestLimits` | 007 | verified | subagent |
| `F-08` | Dead public surface | 003 | verified | orchestrator |
| `F-09` | The reference named two settings that do not exist | 012 | verified | docs agent |
| `F-10` | The conventions described a body cap the code lacked | 012 | verified | docs agent |
| `F-11` | The password minimum had no floor | 004 | verified | orchestrator |
| `F-12` | The clock skew had no upper bound | 004 | verified | orchestrator |
| `F-13` | The prune grace had no floor and a false ADR reference | 004 | verified | orchestrator |
| `F-14` | The checker could not see the configuration surface | 008 | verified | orchestrator |
| `F-15` | Fifteen of twenty-six validation rules had no test | 004 | verified | orchestrator |
| `F-16` | The implementation status contradicted itself | 012 | verified | docs agent |
| `F-17` | The security architecture understated its evidence | 012 | verified | docs agent |
| `F-18` | A requirement and an ADR contradicted each other | 012 | verified | docs agent |
| `F-19` | Three documents described a CORS `panic!` that is gone | 012 | verified | docs agent |
| `F-20` | The operator never saw the readable start-up error | 005 | verified | orchestrator |
| `F-21` | A migration comment described the wrong mechanism | 012 | verified | docs agent |
| `F-22` | The search filters had no length bound | 006 | verified | orchestrator |
| `F-23` | The `/api/v1` prefix is written 19 times | 020 | deferred | — |
| `F-24` | The `x-request-id` literal was written twice | 014 | verified | orchestrator |
| `F-25` | The `Path` and `Query` rejection handlers were identical | 014 | verified | subagent |
| `F-26` | The search normalisation was written twice | 014 | verified | orchestrator |
| `F-27` | `AuthService` built a second `UserRepository` | 014 | verified | orchestrator |
| `F-28` | `refresh` repeated the session-creation sequence | 014 | verified | orchestrator |
| `F-29` | `AppState::prune_expired_sessions` was a lossy wrapper | 014 | verified | orchestrator |
| `F-30` | The services were exposed two ways | 014 | verified | orchestrator |
| `F-31` | Three test helpers were dead behind a blanket allow | 010 | verified | subagent |
| `F-32` | The `hex` crate was used for one call | 009 | verified | orchestrator |
| `F-33` | Three dependency features were unused | 009 | verified | orchestrator |
| `F-34` | The production overlay set a value to its own value | 014 | verified | orchestrator |
| `F-35` | The request timeout was defined three times | 014 | verified | orchestrator |
| `F-36` | `AppError::body` repeated three match arms | 014 | verified | orchestrator |
| `F-37` | `authenticate_bearer` built the same error three times | 014 | verified | orchestrator |
| `F-38` | Three fallbacks restated configured defaults | 014 | verified | orchestrator |
| `F-39` | The catalogue held three identical templates | 007 | verified | orchestrator |
| `F-40` | `title_too_long` was used for a project *name* | 007 | verified | orchestrator |
| `F-41` | `with_message` silently did nothing | 014 | verified | orchestrator |
| `F-42` | The token tests repeated the lifetime four times | 014 | verified | orchestrator |
| `F-43` | The unit-test inventory had wrong counts | 012 | verified | docs agent |
| `F-44` | 10.73 MiB of Swagger assets in a disabled route | 022 | deferred | orchestrator |
| `F-45` | `/auth/me` ran the identical lookup twice | 020 | deferred | — |
| `F-46` | The auth lookup over-fetched and discarded a DTO | 020 | deferred | — |
| `F-47` | The note-search predicate may not use the GIN index | 017 | verified | efficiency agent |
| `F-48` | The list conversion cloned the whole page | 017 | verified | efficiency agent |
| `F-49` | The list handlers parsed the query string twice | 023 | deferred | orchestrator |
| `F-50` | An empty task list cost an extra round trip | 017 | verified | efficiency agent |
| `F-51` | The rate limiter swept its whole map per request | 017 | verified | efficiency agent |
| `F-52` | The token subject crossed the boundary as a `String` | 014 | verified | orchestrator |
| `F-53` | `validate_settings` was a 190-line function | 014 | verified | orchestrator |
| `F-54` | Four feature specifications contradicted their status | 012 | verified | docs agent |
| `F-55` | The security README described pre-`KI-010` behaviour | 012 | verified | docs agent |
| `F-56` | Q-07 was premised on a fixed defect | 012 | verified | docs agent |
| `F-57` | Q-10 claimed an MIT licence | 012 | verified | docs agent |
| `F-58` | `module-boundaries.md` had no `helpers`/`utils` section | 012 | verified | docs agent |
| `F-59` | The conventions claimed the request id is logged | 012 | verified | docs agent |
| `F-60` | The review log said no review happened | 012 | verified | docs agent |
| `F-61` | The overview listed `subtle` | 012 | verified | docs agent |
| `F-62` | The start-up table hid the connect-info wrapper | 012 | verified | docs agent |
| `F-63` | The notes specification pointed at the wrong section | 012 | verified | docs agent |
| `F-64` | `project-truth.md` described a non-existent mechanism | 012 | verified | docs agent |
| `F-65` | The authentication status misreported `project-truth` | 012 | verified | docs agent |
| `F-66` | The change log recorded a fix that was not applied | 012 | verified | docs agent |
| `F-67` | The strategy said the configuration had no unit test | 012 | verified | docs agent |
| `F-68` | The security-case count was wrong | 012 | verified | docs agent |
| `F-69` | The strategy named the wrong guard function | 012 | verified | docs agent |
| `F-70` | The layouts omitted two modules | 016 | verified | orchestrator |
| `F-71` | The data architecture said "Two gaps" and listed three | 012 | verified | docs agent |
| `F-72` | The deployment reference called the body limit a duration | 012 | verified | docs agent |
| `F-73` | The CORS preflight lifetime was a magic number | 014 | verified | subagent |
| `F-74` | The shutdown drain was hard-coded | 014 | verified | orchestrator |
| `F-75` | The log format followed the build profile | 014 | verified | orchestrator |
| `F-76` | `telemetry::init` fell back to `info` | 014 | verified | orchestrator |
| `F-77` | The timeout message was hard-coded | 014 | verified | orchestrator |
| `F-78` | Five copies of the product name | 014 | verified | subagent |
| `F-79` | A start-up failure could print a raw driver error | 015 | verified | orchestrator |
| `F-80` | The harness printed the connection URL | 015 | verified | orchestrator |
| `F-81` | The loader trimmed twice | 014 | verified | orchestrator |
| `F-82` | `from_toml` and `defaults` were public with no caller | 014 | verified | orchestrator |
| `F-83` | The filter predicate was duplicated in the count | 017 | verified | efficiency agent |
| `R-01` … `R-11` | The eleven deliberate duplications | — | retained | — |

---

## 2. The tasks

### ATOM-TASK-20261022-001 — Apply the configured pagination bounds

**Findings:** `F-01` · **Priority:** 1 · **Severity:** High · **Complexity:** medium
**Owner:** orchestrator · **Status:** verified
**Evidence:** `config/api.toml:24-32`, `src/config/settings.rs:165-175`,
`src/config/validation.rs`, `src/pagination.rs:11-13`, the three list services.
**Correction:** `PaginationQuery::effective_limit` takes the configured bounds;
the three services hold them; the compile-time constants are deleted.
**Interfaces:** the published page size. **Dependencies:** none.
**Acceptance criteria:** the bounds are the only definition · the constants are
gone · the services hold them, built once at start-up · a unit test asserts a
configured maximum is honoured · the published contract stays 20/100.
**Tests:** `the_configured_maximum_is_honoured`,
`the_configured_default_is_honoured`.
**Documentation:** `docs/api/conventions.md`, `docs/architecture/deployment.md`,
`config/README.md`, ADR-0006.
**Verification evidence:** `cargo test --all-targets` → 173 passed, 0 failed.
The live release binary answers `GET /api/v1/notes?limit=1000` with the
configured maximum, not the requested value.

### ATOM-TASK-20261022-002 — Preserve the absolute refresh lifetime

**Findings:** `F-02` · **Priority:** 1 · **Severity:** High · **Complexity:** small
**Owner:** orchestrator · **Status:** verified
**Evidence:** `src/auth/service.rs:215-230` wrote `expires_at = now + ttl`,
against ADR-0003 §"one absolute refresh lifetime".
**Correction:** the replacement session inherits the deadline of the session it
replaces.
**Interfaces:** the refresh-token lifetime. **Dependencies:** none.
**Acceptance criteria:** the deadline survives a whole chain of rotations ·
reuse detection is unaffected · the revoked session keeps its own deadline.
**Tests:** `a_rotation_preserves_the_absolute_lifetime_of_the_session`,
`an_expired_session_cannot_be_extended_by_a_rotation`.
**Documentation:** ADR-0003, the data architecture, the authentication set, `KI-015`.
**Verification evidence:** both tests pass in the suite of 173.

### ATOM-TASK-20261022-003 — Remove the settings nothing read

**Findings:** `F-03`, `F-05`, `F-08` · **Priority:** 4 · **Severity:** Medium
**Owner:** orchestrator · **Status:** verified
**Evidence:** `app.name` and `messages.general.body_mismatch` had no reader;
`AppState::sessions()`, `AppState::tokens`, `AppState::passwords`,
`ConfigLoader::directory()` and `RateLimiter::reset()` had no caller.
**Correction:** the keys and the dead items deleted; `is_blank` used where the
rule was inline.
**Acceptance criteria:** every committed key is read · every public item has a
caller or is gone · the reference tables no longer name `app.name`.
**Tests:** the catalogue completeness test and the configuration defaults test.
**Documentation:** `config/README.md`, `docs/architecture/deployment.md`.
**Verification evidence:** the suite passes; `cargo run --bin atom-docs` is clean.

### ATOM-TASK-20261022-004 — Enforce the configuration floors

**Findings:** `F-11`, `F-12`, `F-13`, `F-15` · **Priority:** 4 · **Severity:** Medium
**Owner:** orchestrator · **Status:** verified
**Evidence:** the password minimum, the clock skew, the prune grace, the two
token lifetimes, the page maximum and the body limit had no bound.
**Correction:** `MIN_PASSWORD_LENGTH` (12), `MAX_CLOCK_SKEW_SECONDS` (300),
`MIN_PRUNE_GRACE_SECONDS` (3600), `MAX_ACCESS_TOKEN_TTL_SECONDS` (86400),
`MAX_REFRESH_TOKEN_TTL_SECONDS` (31536000), `MAX_PAGINATION_LIMIT` (1000) and
`MAX_BODY_BYTES_BACKSTOP` (4 MiB) are enforced; the fifteen untested rules
each gained a test.
**Interfaces:** the configuration surface. **Dependencies:** none.
**Acceptance criteria:** each floor is enforced · each has a test that fails
without it · the committed defaults satisfy every floor.
**Tests:** twenty-three configuration tests.
**Documentation:** `config/README.md` §6, the constraint column of the
reference table, `KI-016`, `KI-017`, `KI-018`.
**Verification evidence:** the suite passes; the checker confirms the new key
`app.limits.shutdown_grace_seconds` in the file and in the reference table.

### ATOM-TASK-20261022-005 — Give the operator a readable start-up failure

**Findings:** `F-20` · **Priority:** 4 · **Severity:** Medium · **Complexity:** small
**Owner:** orchestrator · **Status:** verified
**Correction:** `main` returns `ExitCode` and prints the `Display` form.
**Verification evidence:** the live binary, started without a secret, prints
`error: required environment variable DATABASE_URL is missing or empty`.

### ATOM-TASK-20261022-006 — Bound the search term

**Findings:** `F-22` · **Priority:** 4 · **Severity:** Medium · **Complexity:** small
**Owner:** orchestrator · **Status:** verified
**Correction:** a shared `validate_search` helper with `MAX_SEARCH_LENGTH` of
200; the repositories no longer re-trim.
**Tests:** `a_search_term_is_trimmed_and_bounded`,
`a_search_term_over_the_bound_is_refused`.
**Verification evidence:** both pass.

### ATOM-TASK-20261022-007 — Consolidate the duplicated logic

**Findings:** `F-04`, `F-06`, `F-07`, `F-28`, `F-39`, `F-40` · **Priority:** 5
**Severity:** Medium · **Owner:** orchestrator and subagent · **Status:** verified
**Correction:** one `field_error` · one catalogue `too_long` template ·
`RequestSettings` deleted in favour of the configured limits · one
`new_session` helper · and `F-06`, which this task claimed but had not done:
the five byte-identical `fn messages()` fixtures are replaced by
`config::messages::test_messages()`, defined once next to the type it builds.
The earlier status was wrong and the independent review caught it.
**Interfaces:** the catalogue key names changed; the rendered text did not.
**Verification evidence:** the suite passes and the catalogue completeness test
covers the new key. `grep -c 'fn messages()' src` now returns 0.

### ATOM-TASK-20261022-008 — Extend the documentation checker

**Findings:** `F-14` · **Priority:** 4 · **Severity:** Medium · **Owner:** orchestrator
**Status:** verified
**Correction:** the `ATOM_` prefix is no longer filtered out, the production
overlay is required, and the settings keys are compared with the operator
reference in both directions.
**Verification evidence:** the checker found the two wrong key names and the
missing key on its first run; it is now clean.

### ATOM-TASK-20261022-009 — Remove the unused dependencies and features

**Findings:** `F-32`, `F-33` · **Priority:** 8 · **Severity:** Low · **Owner:** orchestrator
**Status:** verified
**Correction:** `hex`, `axum/macros`, `tokio/sync` and `sqlx/json` removed.
**Regression risk:** the stored hash encoding. **Mitigation:** the literal
assertion `the_stored_hash_is_lower_case_hexadecimal`.
**Verification evidence:** the suite passes; the release build succeeds.

### ATOM-TASK-20261022-010 — Use the shared test helpers

**Findings:** `F-31` · **Priority:** 8 · **Severity:** Low · **Owner:** subagent
**Status:** verified
**Correction:** the blanket `#![allow(dead_code)]` removed; the helpers used.
**Trade-off:** the module is `pub` in the three test binaries, because the
dead-code lint fires per binary. A helper unused in *all* of them is no longer
reported. Recorded in the change log.

### ATOM-TASK-20261022-011 — Shrink the build artifacts

**Findings:** build cost · **Priority:** 6 · **Severity:** Low · **Owner:** orchestrator
**Status:** verified
**Correction:** `[profile.dev] debug = "line-tables-only"`.
**Verification evidence:** the artifacts fell from 4.4 GB to 1.45 GB, a
**measured** 67 % reduction.

### ATOM-TASK-20261022-012 — Remediate the documentation findings

**Findings:** `F-09`, `F-10`, `F-16` … `F-19`, `F-21`, `F-43`, `F-54` … `F-72`
**Priority:** 7 · **Severity:** Medium · **Owner:** docs agent · **Status:** verified
**Correction:** thirty-five documentation defects, in two passes. The second
pass found seven findings the first had not actually closed (`F-54`, `F-56`,
`F-59`, `F-66`, `F-69`, `F-71`) and closed them.
**Verification evidence:** `cargo run --bin atom-docs` and `-- --strict` both
report `OK (0 warning(s))`.

### ATOM-TASK-20261022-013 — Close the independent review

**Findings:** `R-01` … `R-17` · **Priority:** 4 · **Severity:** Medium
**Owner:** orchestrator and the `code-reviewer` subagent · **Status:** verified
**Correction:** the `422` declared on the two list endpoints that can return
it · the error contract names the fourth source · the body-limit documents say
the configuration is refused · two documents name the tests the change added ·
every test count reconciled · the page maximum gained a ceiling · the
body-reader ceiling moved next to the other floors · the dead re-export and the
harness hole closed.
**Verification evidence:** `ATOM-REVIEW-20261022-002` in the review log, with
the re-verification table.

### ATOM-TASK-20261022-014 — The remaining code consolidations

**Findings:** `F-24`, `F-25`, `F-26`, `F-27`, `F-29`, `F-30`, `F-34`, `F-35`,
`F-36`, `F-37`, `F-38`, `F-41`, `F-42`, `F-52`, `F-53`, `F-73` … `F-78`,
`F-81`, `F-82` · **Priority:** 8 · **Severity:** Low · **Owner:** orchestrator
and subagent · **Status:** verified
**Correction, by finding:**
- `F-24` the header name is one literal, in one helper.
- `F-25` one `request_messages` helper serves the three extractors.
- `F-26` the repositories bind the term the service already trimmed.
- `F-27` `AuthService` reaches the user through the `UserService` it holds.
- `F-29` the pruning error is the database error; the lossy wrapper is gone.
- `F-30` `state.auth` is the only idiom; `auth_service()` is gone.
- `F-34` the production overlay no longer sets a value to its own value; it
  explains why the bounds are not overridden per environment.
- `F-35` the request timeout has one definition; the fallback constant is
  gone with the fallback that needed it.
- `F-36` one or-pattern.
- `F-37` one `invalid_access_token` helper.
- `F-38` the duration conversions are total; the fallbacks restate no
  configured default.
- `F-41` the doc comment says the method is a no-op on a non-validation error.
- `F-42` one `ACCESS_TTL` constant in the token tests.
- `F-52` the subject is a `Uuid`; the wire format is unchanged.
- `F-53` `validate_settings` is split into one function per settings group,
  in a fixed order.
- `F-73` the CORS preflight lifetime is a named constant.
- `F-74` the shutdown drain is the setting
  `app.limits.shutdown_grace_seconds`, with an environment override.
- `F-75` the log format follows the environment, not the build profile.
- `F-76` `telemetry::init` returns an error instead of falling back to `info`.
- `F-77` the timeout message comes from the catalogue; the fallback literal is
  gone.
- `F-78` the product name has one source; the dead OpenAPI `info` block is
  gone.
- `F-81` one `normalise` function in the loader.
- `F-82` `Messages::from_toml` is `#[cfg(test)]` and `pub(crate)`.
**Verification evidence:** the suite passes (111 unit, 29 authentication, 12
notes, 25 projects and tasks); `cargo clippy --all-targets --all-features -- -D
warnings` is clean; the new setting is confirmed by the checker in both the file
and the reference table, and has three tests of its own (the default, the
environment override, and the rejected zero).

### ATOM-TASK-20261022-015 — Remove the diagnostic leaks

**Findings:** `F-79`, `F-80` · **Priority:** 4 · **Severity:** Medium
**Owner:** orchestrator · **Status:** verified
**Correction:** the start-up path reports the error kind and the SQLSTATE, and
never the driver message, the connection string or a row; the test harness
reports the database **name**, never the URL.
**Interfaces:** the start-up failure message and the harness refusal message.
**Tests:** the live binary, started against an unreachable database, prints a
message with no URL in it; the harness panic contains the database name only.

### ATOM-TASK-20261022-016 — Correct the repository layout

**Findings:** `F-70` · **Priority:** 8 · **Severity:** Low · **Owner:** orchestrator
**Status:** verified
**Correction:** `src/helpers/` and `src/utils/` added to the layout in
`README.md` and `AGENTS.md`; `src/telemetry.rs` added to the `AGENTS.md` layout.

### ATOM-TASK-20261022-017 — Measure the efficiency findings

**Findings:** `F-47`, `F-48`, `F-50`, `F-51`, `F-83`
**Priority:** 6 · **Severity:** Low · **Owner:** efficiency agent, reviewed and
measured by the orchestrator · **Status:** verified

**Method.** The audit stated these as *hypotheses*. Each was measured before it
was accepted: PostgreSQL 18, a disposable `atom_test` database holding 20 000
notes for one owner plus 5 000 for another, `ANALYZE` run, and
`SET plan_cache_mode = force_generic_plan` so that the plan shown is the one a
prepared statement actually uses rather than the first-five-executions custom
plan. One note in a hundred contained the search term. The figures below are
`EXPLAIN (ANALYZE, BUFFERS)` results, not estimates.

**`F-47` — the search clause, measured.**

| Statement | Plan | Execution | Buffers |
| --------- | ---- | --------- | ------- |
| Old: `($3::text IS NULL OR search_vector @@ …)` | Bitmap index scan on `notes_user_id_archived_idx`, then **19 000 rows removed by filter** | **10.80 ms** | 819 |
| New: the clause in its own statement | `BitmapAnd(notes_user_id_archived_idx, notes_search_vector_idx)` | **0.85 ms** | 21 |

The hypothesis was correct and the effect is large: **12.7× faster, 39× fewer
buffers**, because the GIN index becomes a real index qual instead of a filter
applied after the heap scan. The predicate is a static fragment and the term is
a bound parameter, so the statement is still parameterised.

**`F-83` — the count, measured.** The count used the same disjunction, so it had
the same problem. With the split statement:

| Statement | Execution | Buffers |
| --------- | --------- | ------- |
| Old count | **9.93 ms** | 812 |
| New count | **1.33 ms** | 278 |

**7.5× faster.** The count now reports the size of the set the page was taken
from, using the same filter, and the planner serves it from the same two
indexes.

**`F-48` — the no-search listing, measured.** The `COALESCE($2, false) =
is_archived` form and `is_archived = $2` resolve to the same index scan; the
gain is small and is reported as small:

| Statement | Execution | Buffers |
| --------- | --------- | ------- |
| Old | 0.066 ms | 8 |
| New | 0.032 ms | 5 |

The change is still worth keeping, because it is what makes the *search* path
work, and it removes a construct that hides selectivity from the planner. It
was **not** justified by the no-search figure alone.

**`F-51` — the rate-limiter sweep.** The sweep ran on every check: one visit per
tracked client per request. It now runs at most once per window, so an entry
lives for at most two windows and the map holds the clients of the last two
windows — still bounded, which is the property the limiter must keep. Three
tests pin the *behaviour* (the map does not keep expired clients, the limit
still holds across a sweep, and a client is rejected the same number of times
after its window), and they pass against the old per-check sweep too: they
prove the change preserved correctness, not that it was made. The optimisation
itself is justified by the cost argument above, not by a test, and this is
stated rather than glossed over.

**`F-50` — the empty task list.** The empty page used to cost three round trips
(page, ownership probe, count). One statement now answers the ownership
question and the count together: two round trips. The distinction between "no
such project" (`404`) and "a project with no tasks" (an empty page) is
preserved exactly, and `a_project_without_tasks_is_an_empty_page_and_not_a_not_found`
pins it.

**`F-48` (conversion) — the clone.** `From<&Model>` became `From<Model>`, so a
listed page moves its strings into the response instead of copying them. The
public shape is unchanged; only the Rust signature is.

**Not done, and why.** `F-44` and `F-49` are deferred — see the section below.

**Verification evidence:** the plans above; `cargo test --all-targets` passes
with 12 notes tests and 25 projects/tasks tests.

---

## 3. Deferred

### ATOM-TASK-20261022-020 — Serve the profile from the lookup the extractor made

**Findings:** `F-45`, `F-46` · **Priority:** 6 · **Severity:** Low · **Complexity:** medium
**Status:** deferred
**Problem.** Every authenticated request loads the user row and builds a full
`UserResponse` that is thrown away except for the id and the address; the
profile endpoint loads the same row a second time.
**Why deferred.** The change reaches the authentication boundary, and the
benefit — one `SELECT` per request on a personal API — is small. It wants its
own change with its own review of the boundary.
**Regression risk.** The account-status check must still run on every request.

### ATOM-TASK-20261022-021 — Make the OpenAPI document and the route table agree by construction

**Findings:** `F-23` · **Priority:** 8 · **Severity:** Low · **Complexity:** medium
**Status:** deferred
**Problem.** The `/api/v1` prefix is written in the route table and in each of
the eighteen OpenAPI annotations; nothing checks that the two lists agree.
**Why deferred.** utoipa may not accept a constant expression for the path of
an annotation, in which case the answer is a contract test, and that needs the
route table to be enumerable. Deferred until the mechanism is proven: a
half-applied constant is worse than the duplication.

### ATOM-TASK-20261022-022 — Stop embedding the Swagger bundle in a build that serves no docs

**Findings:** `F-44` · **Priority:** 6 · **Severity:** Low · **Complexity:** medium
**Status:** deferred
**Problem.** 10.73 MiB of Swagger UI assets are embedded in the binary because
`utoipa-swagger-ui` is an unconditional dependency, while the production
overlay sets `api.docs_enabled = false`. The route is not mounted, but the
assets are still linked.
**What was measured.** The audit proposed dropping the `vendored` feature. That
was tried and **rejected on measurement**: with `vendored` removed the build
script downloads the same bundle from GitHub at compile time and `rust-embed`
embeds it anyway — the embedded dist is 10.73 MiB in both builds. The change
cost reproducibility and offline builds and saved nothing, so it was reverted.
**Why deferred.** The only way to remove the assets is to make the dependency
optional behind a Cargo feature and gate `routes::swagger_ui` and the merge in
`api::router` on it. That changes the public API of a published module and adds
a build combination that must be tested. It is a real change, not a one-line
one, and the binary size is a deployment preference rather than a defect.
**Acceptance criteria when taken:** a `docs` feature off by default in release,
the route and its `OpenAPI` document gated on it, the release binary measured
before and after, and both build combinations covered by the gates.

### ATOM-TASK-20261022-023 — Parse the list query string once

**Findings:** `F-49` · **Priority:** 8 · **Severity:** Low · **Complexity:** medium
**Status:** deferred
**Problem.** Each list handler takes two extractors, `Query<FeatureQuery>` and
`Query<PaginationQuery>`, so the URI query string is deserialised twice per
request.
**Why deferred.** The fix is a combined extractor carrying both, which changes
the shape utoipa sees for the OpenAPI `parameters` block — the two
`#[into_params]` derivations would have to be reconciled by hand. The cost it
removes is one deserialisation of a short, already-parsed query string, which is
not measurable next to the database round trip. Trading a documented OpenAPI
output for an unmeasurable gain is the wrong direction.

### ATOM-TASK-20261022-027 — Compare the settings *values* in the checker

**Findings:** `R-13` · **Priority:** 8 · **Severity:** Low · **Complexity:** medium
**Status:** deferred
**Problem.** The checker compares the **names** of the settings, not their
values, and does not read the second reference table in `config/README.md`.
**Why deferred.** The normalisation is the hard part: the reference writes
`1048576` (1 MiB) where the file writes `1048576`. A comparison that is too
naive produces false failures, and a false failure in the gate is worse than a
missing check. The wording in `docs/README.md` §5 was narrowed instead, so the
gate does not claim more than it does.

---

## 4. Retained by design

`R-01` … `R-11` were checked against the decision that requires them and
retained. None was removed. The rationale for each is in
[`audit-report.md`](audit-report.md) §4.

| ID | What stays | Why |
| -- | ----------- | --- |
| `R-01` | The ownership predicate in every statement | ADR-0005 makes the visibility the security property. |
| `R-02` | `Page<T>` beside the three named schemas | utoipa cannot express one named schema per feature. |
| `R-03` | The four `From<Model> for Response` impls | The models/dto split is the point. The conversion consumes the model so that a listed page is not copied. |
| `R-04` | The repeated `AppError::NotFound(catalogue)` | The wording is defined once. |
| `R-05` | The constants mirroring the database `CHECK` constraints | AGENTS.md §4.11. |
| `R-06` | The security floors | A floor that is configurable is a floor an operator can lower. |
| `R-07` | The SQLSTATE codes | Protocol constants. |
| `R-08` | The protocol constants | The `Bearer` scheme is matched case-insensitively, per RFC 7235 §2.1. |
| `R-09` | The repeated OpenAPI response blocks | utoipa 5 has no shared-response construct. |
| `R-10` | The near-identical CRUD shapes | The module boundaries forbid a shared base. |
| `R-11` | The `code` parameter of `validation_with` | A documented extension point in a published contract. |

---

## 5. Not tasks

| Finding | Why it is not a task |
| ------- | -------------------- |
| The project-search scan and the prune scan without an index | Documented, accepted gaps for a personal API at this scale. A trigram index or an index on `expires_at` costs a write per request to save a scan over a small table. |
| The rate limiter being per process | `KI-001`, open, recorded. |
| The run-time SQLx query checking | `KI-004`, open, recorded. |
| The absence of a container image and of a CI pipeline | `KI-006` and `KI-005`, recorded. |

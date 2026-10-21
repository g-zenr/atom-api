# ATOM review log

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent

The record of independent reviews of changes to the ATOM backend: what was
reviewed, what the reviewer found, what was required, what was fixed, and the
evidence that the fix was re-verified.

A review is performed by the `code-reviewer` agent, which is **read-only**: it
inspects the diff and never edits the implementation. The `quality-gate` agent
is also read-only and judges the evidence rather than the code. Both are
described in [`.opencode/agents/`](../../.opencode/agents/).

The rule that governs this log is in
[`../testing/verification-policy.md`](../testing/verification-policy.md): a
finding is not closed until a check has been run again and its output
recorded.

---

## The entries so far

One review has been performed:
`ATOM-REVIEW-20261021-001`, the review of the Phase 1 implementation and the
OpenCode engineering system. It returned **APPROVE WITH COMMENTS** with
eleven findings; all eleven are resolved and the re-review evidence is
recorded with the entry.

The Phase 1 foundation itself, recorded in
[`change-log.md`](change-log.md) as `ATOM-CHANGE-20261021-001`, was **not**
reviewed when it was written. That is stated plainly rather than left as an
empty section, because an empty section reads as "reviewed, nothing found",
and that would be a false claim. The review in
`ATOM-REVIEW-20261021-001` covers that change set as well, so nothing is
left unreviewed today.

What the status of a feature still rests on:

- The recorded checks in [`implementation-status.md`](implementation-status.md).
- The review, where one exists.

The findings in the security and testing documents are the result of reading
the source, which is not the same as an independent review.

## The format of a review record

Every review is recorded as a section in this file, newest first, with the
identifier `ATOM-REVIEW-YYYYMMDD-NNN`. The template is
[`../templates/review-record.md`](../templates/review-record.md).

A record contains:

| Part | What it holds |
| ---- | ------------- |
| Identifier and date | `ATOM-REVIEW-YYYYMMDD-NNN` and the UTC date of the review. |
| Reviewer | The role that performed the review. A reviewer is never named personally, because the review is an independent function, not a person. |
| Scope | The commit, the branch or the file set that was reviewed, and the change-log entry it belongs to. |
| Verdict | `APPROVE`, `APPROVE WITH COMMENTS` or `REQUEST CHANGES`. |
| Findings | Each finding with a severity, the file and location, what breaks, and the concrete fix. |
| Required fixes | The subset of the findings that must be fixed before the change is complete. |
| Resolution | What was done about each required fix, with the change that carried it. |
| Re-review evidence | The commands that were run again after the fix, and their real output. |
| Documentation findings | Where the documentation did not match the implementation. |
| Limitations | What the reviewer could not verify, and why. |

## Severity

| Severity | Meaning |
| -------- | ------- |
| `Blocker` | A security hole, a data-loss risk, a broken contract, or a violation of an accepted requirement. The change must not be merged. |
| `Major` | A defect that will produce wrong behaviour in a realistic case. The change must be fixed before it is merged. |
| `Minor` | A defect with a narrow trigger, a readability problem, or a missing test. It may be fixed in the same change or recorded as a known issue. |
| `Note` | An observation with no defect. Recorded so that a later reader knows it was seen. |

## How a review is recorded

1. The reviewer is given the scope: the diff, the accepted requirements, the
   feature documents, the API contract and the change-log entry.
2. The reviewer reports the findings, ordered by severity, and ends with a
   verdict.
3. The author fixes every `Blocker` and every `Major` finding. A `Minor`
   finding is either fixed or recorded in [`known-issues.md`](known-issues.md)
   with a reason.
4. The three quality gates are run again. The output is captured.
5. The review record is written: the findings, the resolution, and the
   re-review evidence.
6. The quality gate is asked to judge the result. It returns `PASS` or `FAIL`.
   A `FAIL` is resolved and re-checked, and the second attempt is recorded as
   well.

A finding is closed only when the record names the fix **and** the check that
proves it. "Fixed" without a passing check is an open finding with a note
attached.

## Re-review evidence

Re-review is what makes the log useful. When a finding is fixed, the record
must show:

1. The finding identifier, for example `R-01`.
2. The change that fixed it — a file and a change-log entry.
3. The check that was re-run, and its real output.
4. The reviewer's re-inspection of the fix, and the result.

An example of the shape, filled in with the gate command:

```text
R-01  Missing ownership predicate on the task listing.
Fixed in: src/tasks/repository.rs, ATOM-CHANGE-20261021-003.
Re-verified: cargo test --all-targets -> passed, 70 tests.
Re-inspected by: code-reviewer, 2026-10-22 -> APPROVE.
```

The example above is illustrative. **It is not a record of a review that
happened.** No review has taken place yet, and the first real entry will carry
a real identifier and real output.

## Related

- [`change-log.md`](change-log.md) — the changes that are reviewed.
- [`known-issues.md`](known-issues.md) — the findings that were accepted
  instead of fixed.
- [`open-questions.md`](open-questions.md) — the questions a review could not
  answer.
- [`../testing/verification-policy.md`](../testing/verification-policy.md) —
  the rules this log implements.

---

### ATOM-REVIEW-20261021-001 - Review of the Phase 1 implementation and the OpenCode engineering system

**Date:** 2026-10-21 (UTC)
**Reviewer:** the read-only `code-reviewer` agent
**Scope:** the staged implementation - `src/**`, `migrations/*.sql`,
`tests/**`, `Cargo.toml`, `opencode.jsonc`, `.env.example` and
`docker-compose.yml`
**Verdict:** APPROVE WITH COMMENTS
**Status:** Closed - every finding is resolved

#### What the review checked

Correctness against the accepted requirements, authorization and ownership
predicates, input validation, the error contract, SQL safety, Rust idioms and
async correctness, migration and model consistency, regressions, and the test
surface. The reviewer read the specification in
[`../project-truth.md`](../project-truth.md) and the feature specifications
before reading the code, and ran `cargo fmt --all -- --check`,
`cargo clippy --all-targets --all-features -- -D warnings` and
`cargo test --all-targets` read-only.

#### Findings and resolutions

| # | Severity | Finding | Resolution |
| - | -------- | ------- | ---------- |
| 1 | Medium | Argon2id hashing and verification ran on the Tokio worker thread inside the request path, stalling a worker for the whole hash. ADR-0002 calls this rule load-bearing. | **Fixed.** [`../../src/auth/service.rs`](../../src/auth/service.rs) moves both to `tokio::task::spawn_blocking`. |
| 2 | Medium | Refresh-token reuse detection was race-prone: two concurrent refreshes of the same token both succeeded, because the read of `revoked_at` and the revoke were separate statements. | **Fixed.** [`../../src/auth/repository.rs`](../../src/auth/repository.rs) gains `claim`, an atomic `UPDATE ... WHERE revoked_at IS NULL`. The loser is treated as a reuse. Covered by `a_concurrent_refresh_of_the_same_token_succeeds_only_once`. |
| 3 | Medium | The 30-second timeout answered `408` with an empty body, while the published error contract promised the standard envelope. The code and the contract contradicted each other. | **Fixed.** [`../../src/api/mod.rs`](../../src/api/mod.rs) reports the timeout through the envelope, and the timeout is configurable. Covered by `a_slow_request_is_abandoned_with_the_error_envelope`. |
| 4 | Medium | The OpenAPI document omitted status codes the handlers return: no `400`, no `401` on most private endpoints, no `403` on logout or on a disabled account. | **Fixed.** Every `#[utoipa::path]` now declares the status codes it can produce. |
| 5 | Low | `RefreshSessionRepository::revoke` was not scoped by the owner, so it would revoke any session id it was given. | **Fixed.** The predicate carries `AND user_id = $2`, and ADR-0005 records the lookup exception explicitly. |
| 6 | Low | The middleware-order comment in `src/api/mod.rs` listed the order backwards. | **Fixed.** The comment now states the effective order. |
| 7 | Low | `hyper`, `http` and `subtle` were unused; `tower` and `http-body-util` were declared twice. | **Fixed.** [`../../Cargo.toml`](../../Cargo.toml) no longer declares them. |
| 8 | Low | A `CHECK` violation would log the failing row, which for the `users` table includes the password hash. | **Fixed.** Database errors are logged with the SQLSTATE and the kind only, and a check violation becomes a `422`. |
| 9 | Low | The dummy-hash fallback was not a valid Argon2 hash, so the timing equalisation silently did no work. | **Fixed.** [`../../src/auth/password.rs`](../../src/auth/password.rs) holds a valid `DUMMY_HASH`. |
| 10 | Low | Unreferenced public helpers accumulated, and expired refresh sessions were never pruned. | **Fixed.** The helpers are removed and [`../../src/main.rs`](../../src/main.rs) prunes expired sessions every hour. |
| 11 | Low | `APP_MAX_BODY_BYTES` constrained nothing; the effective ceiling was a hard-coded four mebibytes. | **Fixed.** [`../../src/api/extract.rs`](../../src/api/extract.rs) reads the limit the router installs. Covered by `an_oversized_body_is_refused`. |

#### Categories in which the review found nothing

- **SQL injection.** Every statement uses bound parameters; no user input is
  concatenated into SQL.
- **Authorization and data isolation.** Every statement that touches user
  data carries the caller's `user_id`. The task statements join `projects`
  with `p.user_id = $2`, and the task insert is an `INSERT ... SELECT` from
  the owned project, so the check and the write are one statement. The
  reviewer could not construct a path to another account's row.
- **`404` over `403`.** Every inaccessible private resource returns `404`.
- **Secret handling.** The `Debug` implementations redact the signing secret
  and the database URL; no response body, error message or log line carries a
  password, a token, a hash or the connection string.
- **PATCH semantics.** `COALESCE` leaves an omitted field untouched and
  preserves an explicit empty string.
- **`unwrap` and `expect`.** None in a request path; the only ones are in
  tests and in start-up checks.
- **Locks across `.await`.** The rate limiter's mutex is released inside
  `check()` with no await inside the guard.
- **Migration and model consistency.** Every column has a matching field, and
  every projection matches its `FromRow` model.
- **Validation limits.** The Rust limits and the PostgreSQL `CHECK` limits
  agree for every constrained column.

#### Test gaps the review identified, and the tests that closed them

| Gap | Test |
| --- | ---- |
| The concurrent refresh race | `a_concurrent_refresh_of_the_same_token_succeeds_only_once` |
| The HTTP-level rate limit | `the_authentication_endpoints_are_rate_limited` |
| A disabled account | `a_disabled_account_cannot_authenticate` |
| An expired refresh token | `an_expired_refresh_token_is_rejected` |
| An expired access token | `an_expired_access_token_is_rejected` |
| The per-user session revocation | `revoking_every_session_invalidates_the_refresh_tokens` |
| A malformed JSON body | `a_malformed_json_body_is_a_bad_request` |
| An unknown query parameter value | `an_unknown_query_parameter_value_is_a_bad_request` |
| An oversized body | `an_oversized_body_is_refused` |
| The unmatched route | `an_unknown_route_returns_the_error_envelope` |
| The request-id echo | `the_request_id_is_echoed_on_the_response` |
| The request timeout | `a_slow_request_is_abandoned_with_the_error_envelope` |
| A task created as `done` | `a_task_created_as_done_carries_a_completion_timestamp` |
| The task read and delete endpoints | `a_task_can_be_read_and_deleted` |
| The manual ordering | `tasks_are_returned_in_manual_order` |
| The task pagination window | `task_listing_is_paginated` |
| The project search | `projects_can_be_searched` |
| The project pagination window | `project_listing_is_paginated` |
| The project listing isolation | `the_project_listing_only_contains_the_callers_projects` |

#### Re-review

A second review was performed on the fixes. It confirmed that nine of the
eleven findings were genuinely fixed and reported seven further findings: two
Medium defects in the OpenAPI response lists, a Medium defect in the timing
equalisation, and four Low items. All seven are fixed by
`ATOM-CHANGE-20261021-004`.

The most valuable of them was a defect the first review had not found: the
`SetRequestIdLayer` and the `PropagateRequestIdLayer` were in the wrong order,
so a *generated* correlation id was never echoed on the response. The first
review had only exercised the path where the client supplies the header, which
worked. The order is now correct, and
`a_generated_request_id_is_propagated_to_the_response` covers it.

#### Re-review evidence

The fixes were re-verified with the three quality gates and the full suite:

| Check | Result |
| ----- | ------ |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed. |
| `cargo test --all-targets` | Passed: 39 unit, 26 authentication, 8 notes, 19 projects and tasks. 92 in total. |
| `cargo run --bin atom-docs` | Passed, no errors and no warnings. |
| The generated OpenAPI document | 22 operations; every one declares the status codes it can return, with `201` on the creates, `204` and no body on the deletes, and the bearer security scheme registered. |

#### Remaining risks

- The rate limiter is per process. A multi-instance deployment needs edge
  rate limiting in front of the API. Recorded as `KI-001`.
- The per-user session revocation has no endpoint. It is covered at the
  service level only. Recorded as `Q-08`.
- The refresh-token rotation is not wrapped in a transaction: the replacement
  session is written before the old one is claimed. If the process dies
  between the two, the old token stays valid and the new session is orphaned.
  The window is two statements, and the failure mode is fail-open rather than
  fail-closed. It is recorded in
  [`../architecture/data-architecture.md`](../architecture/data-architecture.md).

---


# ATOM review log

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
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

Three reviews have been performed:

- `ATOM-REVIEW-20261022-003`, the review of the final remediation pass recorded
  in [`change-log.md`](change-log.md) as `ATOM-CHANGE-20261022-007`. It returned
  **FAIL** with one blocking defect: the shutdown grace was expressed in
  seconds but carried the old millisecond value, and the drain was applied
  inside the signal future, which delays the shutdown instead of bounding it.
  It also found that `F-06` had been marked `verified` while five copies of the
  fixture still existed, and that three new tests could not distinguish the
  fix they claimed to guard. All of it is corrected and re-verified below.
- `ATOM-REVIEW-20261022-002`, the review of the audit remediation recorded in
  [`change-log.md`](change-log.md) as `ATOM-CHANGE-20261022-003`. It returned
  **ACCEPT WITH REQUIRED FOLLOW-UP** with seventeen findings; the seven it
  required are resolved and the re-verification is recorded with the entry.
- `ATOM-REVIEW-20261021-001`, the review of the Phase 1 implementation and the
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
happened.** The first real entry is below.

## Related

- [`change-log.md`](change-log.md) — the changes that are reviewed.
- [`known-issues.md`](known-issues.md) — the findings that were accepted
  instead of fixed.
- [`open-questions.md`](open-questions.md) — the questions a review could not
  answer.
- [`../testing/verification-policy.md`](../testing/verification-policy.md) —
  the rules this log implements.

---

### ATOM-REVIEW-20261024-001 - The route table, the document and the setting values

**Date:** 2026-10-24 (UTC)
**Reviewer:** the `code-reviewer` agent, read-only
**Change reviewed:** `ATOM-CHANGE-20261024-002` (chain `D0`, contract `C-D0`)
**Verdict:** **PASS WITH FINDINGS** - no blocking finding against the contract;
nine non-blocking findings and four documentation findings.
**Outcome after the follow-up:** every finding fixed in the same change or
recorded with a reason; the six gates then ran green.

#### What the reviewer confirmed was correct

The route table's 20 versioned entries and 2 unversioned entries match the 22
`paths(...)` entries in `ApiDoc`, the table order matches the router, and the
placeholders are in Axum syntax. `"/api/v1"` appears exactly once as a literal in
the mounting layer, on the `pub const API_PREFIX` line, and `src/api/mod.rs`
mounts that constant; the two occurrences inside the operator warning carry a
`/` after `v1` and so are not literals. The explorer paths are built with
`format!`. All three `into_parts` implementations move every field to the correct
half - `search`/`archived`, `search`, `status`/`priority` to the feature type and
`limit`/`offset` to `PaginationQuery` - with no swap and no dropped field. The
query types derive `Deserialize` only and are absent from the component list, and
each list handler takes one extractor. No `utoipa` attribute, request type or
response type was touched, so the published document is unchanged. The value
comparison was traced by hand over the configuration files against both tables:
`config/README.md` has 22 integer or boolean rows, all matching after the
`100`→`10` fix, so the check is not vacuous and would have caught the reported
drift; `deployment.md` compares 11 unique rows and none mismatch, so it produces
no false failure on this tree. No `unwrap`/`expect` outside test scope, no
`allow`, no `ignore`, no `unsafe`, no concatenated SQL, no new dependency, no
migration.

#### Findings, and what was done

| # | Severity | Finding | Disposition |
| - | -------- | ------- | ----------- |
| N-1 | High | Both runner scripts still pointed `$RunDir` at `docs/engineering/runs/phase-2`, which no longer exists: the runner would throw at preflight and the watchdog would silently no-op forever. | Fixed in `scripts/run-phase-2-overnight.ps1` and `scripts/watchdog-phase-2.ps1`. |
| N-2 | Medium | `docs/README.md` §5 said the checker compares names only and does not read the second table - false since the previous change, and the "values" half false since this one. | Rewritten to state both tables, the value comparison and its exact scope. |
| N-3 | Medium | `implementation-status.md` said "**No test asserts its content.**" and carried a stale run count. | Row moved to `Verified` citing the five contract tests; the run paragraph now records 191 on Windows and 193 on CI; `Last updated` bumped. |
| N-4 | Medium | The remediation backlog still reported `F-23` and `F-49` as `deferred`, which is the work this change closes. | Both rows set to `verified`, each with a Correction that names the amendments and the tests. |
| N-5 | Medium | No change-log entry. | `ATOM-CHANGE-20261024-002` added. |
| N-6 | Low | `docs/api/README.md` credited the prefix test with checking the explorer paths, which it does not. | Table rewritten: what each test proves, and explicitly what it does not. |
| N-7 | Low | `conventions.md` said `/api/v1` has one declaration "in the code", which is false against the 20 annotations. | Reworded to "declared exactly once **as a constant**, in the mounting layer", with the annotation reason adjacent. |
| N-8 | Low | `docs/api/README.md` said the health and OpenAPI routes were the only unauthenticated ones, contradicting the next sentence. | Corrected. |
| N-9 | Low | The comment above `ROUTE_TABLE` described the agreement test as router-versus-document; the first direction is table-versus-document. | Reworded to say what the two assertions actually hold. |
| D-1 | Medium | The reason given for `Implemented` on the `project-truth` OpenAPI row was about to become false. | Row moved to `Verified` with the evidence and the scope. |
| D-2 | Medium | The ledger had no checkpoint and the review log had no entry. | Both written in this change. |
| D-3 | Low | `config/README.md` step 7 understated the checker. | Corrected. |
| D-4 | Info | Historical `runs/phase-2/` mentions in the ledger. | Left as written: they record what earlier sessions saw, and rewriting an audit trail is worse. A note in `## Parked` records the rename. |

#### What the reviewer could not verify

It had no shell, so it could not run `git diff`, `git status` or any gate; it
reviewed the working tree and took the recorded exit codes as given. It could not
byte-compare the generated document against a pre-change artefact, so "the
document is unchanged" rests on inspecting every annotation, schema and component
rather than on a diff of the output.

---

### ATOM-REVIEW-20261024-002 - The evidence behind chain D0

**Date:** 2026-10-24 (UTC)
**Reviewer:** the `quality-gate` agent, read-only
**Change reviewed:** `ATOM-CHANGE-20261024-002`
**Verdict:** **FAIL** on the first pass; **PASS after the repairs below.**
**Outcome:** all six blocking or required items resolved, then the change was
re-gated.

#### Why it failed, and what was done

| Category | Item | Disposition |
| -------- | ---- | ----------- |
| Blocking | `implementation-status.md` asserted "No test asserts its content" against a tree that now has five such tests, and was not updated by the change. | Rows 48 and 63 moved to `Verified` with the tests named; run counts corrected. |
| Blocking | `features/notes/architecture.md` and `features/projects/architecture.md` still drew two `Query<…>` extractors in the request path, which is exactly what the chain removed. | Both diagrams now show `Query<NotesListQuery>` / `Query<ProjectsListQuery>` followed by `into_parts()`. |
| Blocking | The `project-truth` OpenAPI row rested on a claim the same commit would falsify. | Moved to `Verified` with the evidence and an explicit scope note. |
| Blocking | No reviewer verdict existed for the chain, so scorecard row 10 had no evidence. | `ATOM-REVIEW-20261024-001` written; row 10 cites it. |
| Required | `I3`, the chain's central invariant, had no test: the three behaviour tests also pass on the two-extractor handler. | `each_list_handler_extracts_the_query_string_once` added, asserting one `Query<` per list handler module and naming the expected type. The ledger states plainly which tests are change-insensitive. |
| Required | The ledger's queue row cited `71600cb` as the D0 commit - the ledger's own baseline identifies that as the previous docs merge - and still said `TasksListQuery` was outstanding. | Row rewritten: `done`, with the actual state and the resume pointer. |

#### What the gate confirmed was honest

The six recorded exit codes reconcile with the tree: the 191 count matches the
test functions physically present, the 179 baseline matches the same files minus
the 11 new ones, the drift the chain exists to fix is genuinely fixed, the
`--no-default-features` gate is plausible because only `utoipa-swagger-ui` is
feature-gated, and the formatting state is what a real `cargo fmt` leaves. Its
own words: no recorded gate was dishonest. What it called over-stated was the
"prove it can fail" claim, which the ledger now states narrowly - breaking a
test's own expectation proves the assertion runs, not that the production change
is what makes it pass.

---

### ATOM-REVIEW-20261022-003 - The final remediation pass

**Date:** 2026-10-22 (UTC)
**Reviewer:** the `code-reviewer` agent, read-only
**Change reviewed:** `ATOM-CHANGE-20261022-007`
**Verdict:** **FAIL**, one blocking defect and nine non-blocking findings.
**Outcome after the follow-up:** all blocking and non-blocking findings
resolved; re-verified by the commands below.

#### What the reviewer confirmed was correct

The token subject is typed `Uuid` end to end and a malformed one still yields
`401`. The start-up redaction cannot leak the connection string on any
`sqlx::Error` or `MigrateError` path. The `RequestContext` extension is outer to
the timeout middleware in the layer order, so requiring it is not a regression.
Every validation rule survived the split into per-group functions. The list
statements are static fragments with the ownership predicate written out, the
tri-state archive filter resolves identically, and no parameter is mis-bound.
The combined task statement preserves both the `404` and the empty-page answer.
`utoipa-swagger-ui` still carries `vendored` and the lockfile agrees.

#### The blocking defect

| Finding | Correction |
| ------- | ---------- |
| The new `shutdown_grace_seconds` was `100`, an integer of **seconds**, while the code had used 100 **milliseconds**: a 1000× change that the change log described as "no change for a deployment". Worse, the sleep sat inside the future passed to `with_graceful_shutdown`, so the listener kept accepting new connections for the whole grace and the drain that followed was **unbounded** — the opposite of what the setting was documented to do. | The signal future now resolves immediately. The listener stops accepting on the signal, and the drain is bounded by a `select!` against a deadline that starts when the signal arrives. The units are honest: `shutdown_grace_seconds = 10`, a real ceiling. The change log states the operational change instead of denying it. |

#### The other findings, and what was done

| # | Finding | Correction |
| - | ------- | ---------- |
| 1 | The new setting had only the rejection test, not the default or the override that AGENTS.md §4.11 requires | Added `the_shutdown_grace_can_be_set_from_the_environment` and a defaults assertion |
| 2 | `F-06` was marked `verified` while five byte-identical `fn messages()` fixtures remained | Consolidated into `config::messages::test_messages()`; the task record now says the earlier status was wrong |
| 3 | Three pagination tests asserted 20/100, the same values as the old constants, so they passed against the pre-fix code | They now build a state with bounds of **7 and 13** through the new `test_state_with_pagination` helper, so a fallback to a literal fails |
| 4 | The rate-limiter tests cannot distinguish the amortised sweep from a per-check one | The backlog now says so: they pin the behaviour, and the optimisation rests on its cost argument |
| 5 | Four documents said the limiter's map is swept on every check | `module-boundaries.md`, `deployment.md`, `threat-model.md` T-11 and `authentication/status.md` corrected, with the boundedness restated honestly |
| 6 | `telemetry::init`'s signature, formatter choice and error behaviour were described as they were before | `module-boundaries.md` §7.6, `system-overview.md` and `deployment.md` §6.1 corrected |
| 7 | Documents still listed `auth_service()` and `RateLimiter::reset`, and said `AuthService` holds a `UserRepository` | All three removed |
| 8 | The test counts in `implementation-status.md` and `testing/strategy.md` were stale | 177 total: 111 unit, 29 authentication, 12 notes, 25 projects and tasks. The configuration module contributes 50 |
| 9 | `notes/testing.md` contradicted itself about the page size and listed a covered case as a gap | Corrected |

Two smaller items were also fixed: the shutdown grace was missing from the
positivity rule in `deployment.md` §1.4, and a comment in `src/api/mod.rs`
called a value copy "not a copy".

#### Re-verification

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Passed.** Exit code 0. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** Exit code 0, no warning. |
| `cargo test --all-targets` | **Passed.** 177 tests, 0 failed. |
| `cargo run --bin atom-docs` | **Passed.** `OK (0 warning(s))`. |
| `cargo run --bin atom-docs -- --strict` | **Passed.** `OK (0 warning(s))`. |
| `scripts/validate-opencode.ps1` | **Passed.** `PASSED: all checks completed (0 warning(s)).` |
| Live process, disposable database | `GET /health/live` → `{"status":"live"}`; `GET /api/v1/docs` → `200`; `GET /api/v1/openapi.json` → `info.title = "ATOM API"`, 13 paths, `422` declared on the notes listing. This exercises the restored `vendored` Swagger bundle. |

**Not verified at runtime:** the shutdown path itself. Delivering a console
Ctrl-C event to a background process on Windows is not something this
environment can do, so the drain bound is verified by reading the code and by
compilation, not by a signal test. That is stated rather than implied.

**Follow-up, 2026-10-24 — partially closed.** Two tests now exist in
[`../../tests/shutdown.rs`](../../tests/shutdown.rs): one proves the signal
future resolves immediately instead of sleeping the grace away, the other proves
the drain is bounded when an in-flight request cannot finish. Together they
distinguish the corrected ordering from the original, which slept inside the
signal future and then drained without a bound.

**Closed by CI, not by assertion.** The file is `#![cfg(unix)]` and the
development machine is Windows, so the tests were written but could not be run
there; `KI-019` recorded that gap rather than letting the test's presence imply a
pass. CI run
[`40011694171`](https://github.com/g-zenr/atom-api/actions/runs/40011694171) then
executed them on `ubuntu`: `test result: ok. 2 passed; 0 failed` for
`tests/shutdown.rs`, with the bounded-drain test logging
`the shutdown grace elapsed with requests still in flight grace_seconds=2` — the
deadline being reached with a request in flight, which is exactly the behaviour
this review could not observe. The finding is closed.

---

### ATOM-REVIEW-20261022-002 - Review of the audit remediation

**Date:** 2026-10-22 (UTC)
**Reviewer:** the read-only `code-reviewer` agent
**Scope:** the whole working tree against `aba97f6` — 70 modified files and
the new [`../audit/`](../audit/) directory
**Verdict:** ACCEPT WITH REQUIRED FOLLOW-UP
**Status:** Closed - every required finding is resolved

#### What the review checked

The correctness of the two behavioural fixes (`F-01`, the applied pagination
bounds, and `F-02`, the preserved refresh deadline), the regression risk of
the new configuration floors, whether each new validation rule is reachable
and tested, the `search` bound, the body-reader ceiling, the extended
documentation checker, dead code and duplication, whether any security control
was weakened, the accuracy of the documentation, and whether the audit report
itself overstates anything.

The reviewer ran `cargo fmt --all -- --check` and
`cargo clippy --all-targets --all-features -- -D warnings` read-only, and
re-implemented the two new checker functions by hand because the permission
layer blocks `cargo run`. It could not run the test suite; the orchestrator
ran it.

#### Findings and resolutions

| # | Severity | Finding | Resolution |
| - | -------- | ------- | ---------- |
| `R-01` | Medium | The list endpoints can now answer `422`, because the `search` bound is enforced in the service, but the OpenAPI annotations did not declare it. | **Fixed.** [`../../src/notes/handlers.rs`](../../src/notes/handlers.rs) and [`../../src/projects/handlers.rs`](../../src/projects/handlers.rs) declare the `422`, and the `search` parameter descriptions name the 200-character bound. |
| `R-02` | Medium | `docs/api/error-contract.md` said "exactly three sources of a `422`"; the search bound is a fourth. | **Fixed.** The document names `validate_search` and the field `search`. |
| `R-03` | Medium | The authentication testing document said the inherited deadline was untested; the test was added in the same change. | **Fixed.** The row now names `a_rotation_preserves_the_absolute_lifetime_of_the_session`. |
| `R-04` | Medium | The new `helpers` table in the module boundaries had a malformed row: two cells were merged with a doubled `\|`. | **Fixed.** The row is split. |
| `R-05` | Medium | Two documents said the `search` bound was untested. | **Fixed.** Both name the unit test and the HTTP-level test. |
| `R-06` | Medium | Four documents described the 4 MiB ceiling as a silent clamp, and named the wrong file for the constant. The code refuses the configuration at start-up. | **Fixed.** The four documents say the configuration is refused, and the constant is named in `src/config/validation.rs`. |
| `R-07` | Medium | `KI-018` described a backstop as if it had always been there. Before the remediation the extractor passed `usize::MAX` and the configured limit was the only cap. | **Fixed.** The record says the backstop and the ceiling were introduced by `ATOM-CHANGE-20261022-003`, and that a deployment configured above 4 MiB previously started and now does not. |
| `R-08` | Low | The pagination maximum gained no ceiling, while every other configurable limit in the change did. | **Fixed.** `MAX_PAGINATION_LIMIT` (1000 items) is enforced, with a test. |
| `R-09` | Low | Seven test counts in the documentation did not match the tree. | **Fixed.** Every count was recounted and corrected. |
| `R-10` | Low | The implementation status said the suite had not been re-run, while the change log recorded a run of the same tree. | **Fixed.** The paragraph now records the 162-test run. |
| `R-11` | Low | The audit report said both High findings were found by three audits; only one was, and its category table double-counted a finding and omitted two. | **Fixed.** |
| `R-12` | Low | The change log used two different finding-identifier schemes without saying so. | **Fixed.** A note now explains the mapping. |
| `R-13` | Low | The new checker compares setting **names**, not values, and does not read the second reference table in `config/README.md`. | **Accepted, with the wording narrowed.** [`../README.md`](../README.md) §5 now says the checker compares names and does not compare the second table. Comparing the default values and the second table is a task in the backlog. |
| `R-14` | Low | The body-reader ceiling sat in the settings module while its five sibling floors sat in the validation module. | **Fixed.** The ceiling moved to [`../../src/config/validation.rs`](../../src/config/validation.rs) with the other floors. |
| `R-15` | Low | The test harness could set a body limit above the ceiling, because it mutates the configuration after validation. | **Fixed.** The harness asserts the ceiling. |
| `R-16` | Low | `MAX_SEARCH_LENGTH` was re-exported from `crate::helpers` with no consumer. | **Fixed.** The re-export is gone. |
| `R-17` | Low | Three documents gave three different counts for the same set of findings. | **Fixed.** One number per set. |

#### The reviewer's clean checks

The reviewer confirmed, with file and line references, that: the configured
bounds are applied on all three list endpoints and the old constants are gone;
the deadline survives an arbitrary chain of rotations and the reuse detection
is unaffected; every floor is consistent with the committed defaults and
none of them is impossible to satisfy; every new rule is reachable and has a
test that would fail without it; the `search` bound is enforced on every path
that accepts the parameter; the new body-limit rule **lowers** the reachable
maximum and cannot be used to bypass or raise a limit; no ownership
predicate, authentication boundary, secret-handling rule, rate limit, CORS
allowlist or error-redaction rule was weakened; the `hex` removal produces the
same hash, pinned by a literal assertion; and the two new checker functions
compare exactly the same 27 keys on both sides.

#### Re-verification

| Check | Result |
| ----- | ------ |
| `cargo fmt --all -- --check` | **Passed**, exit code 0. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed**, exit code 0. |
| `cargo test --all-targets` | **Passed**, 162 tests: 106 unit, 28 authentication, 9 notes, 19 projects and tasks. 0 failed. |
| `cargo run --bin atom-docs` | **Passed**, `atom-docs: OK (0 warning(s))`. |
| `cargo run --bin atom-docs -- --strict` | **Passed**, `atom-docs: OK (0 warning(s))`. |

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
| 4 | Medium | The OpenAPI document omitted status codes the handlers return: no `400`, no `401` on most private endpoints, no `403` on logout or on a disabled account. | **Fixed, in three passes.** Every `#[utoipa::path]` now declares the status codes it can produce. The first pass keyed the lists on the path alone; the re-review found the wrong success status and body on the create and delete endpoints; the quality gate found that `login` and `refresh` still omitted `401` and that no endpoint declared `408`. All three are corrected. |
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
| The generated OpenAPI document | 22 operations; every one declares the status codes it can return, with `201` on the creates, `204` and no body on the deletes, `401` on `login` and `refresh`, `408` everywhere, and the bearer security scheme registered. |

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


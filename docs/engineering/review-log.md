# ATOM review log

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-29
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

Sixteen reviews have been performed:

- `ATOM-REVIEW-20261029-002`, the **Phase 2 close-out** on the final tree at
  `ab5aa00`. The independent read returned `CODE_REVIEW` **PASS** (no defect in
  the reimbursements, job-application or ML spans, statement by statement),
  `NEW_REGRESSIONS` **PASS** (nothing deleted, renamed, ignored or weakened
  since `4df383e`), and `PRODUCT_CONTRACT` **PASS WITH FINDINGS** (three stale
  figures, repaired in the promotion change). The `quality-gate` session
  executed `scripts/gate.ps1` itself — six gates at `0` — and re-derived the
  536-test split target-for-target. **Reimbursements, job applications and
  local ML are `Verified` as of this review**, and Phase 2 is complete per its
  own definition.

- `ATOM-REVIEW-20261029-001`, the **final read of the repaired `K1` tree** at
  `65898e5`. `CODE_REVIEW` **PASS**, `PRODUCT_CONTRACT` **PASS WITH FINDINGS**
  (three index rows still claimed no verdict was recorded — corrected in the
  same change that performs the promotion), `NEW_REGRESSIONS` **PASS**; the
  suite arithmetic was re-derived per file from the tree (536 Windows / 538
  unix). It closes the gap `-027-001` left, and `K1` is **`Verified`** as of
  this review.
- `ATOM-REVIEW-20261027-001`, the API-keys change (`K1`). The `quality-gate`
  returned **PASS** (six gates `0`, 536 tests, 54 operations asserted in both
  directions). The `code-reviewer` read the diff three times and returned
  **FAIL on the documentation figures** each time - eleven stale "51 operations"
  claims, then eight more stale figures, then one - while finding no ownership,
  SQL, secret-handling or contract defect in the code. Every finding is repaired;
  the read of the final tree that did not exist then is recorded as
  `ATOM-REVIEW-20261029-001` above, which promotes the feature to `Verified`.

- `ATOM-REVIEW-20261026-002`, the **re-gate** of `-011` over the whole Phase 2
  run. It executed `scripts/gate.ps1` itself (six gates `0`) and
  `cargo test -- --list` (511 tests) and returned **FAIL on the documentation
  record, PASS on execution**: `B2`, `B5`, `R3`, `R4` fixed, `B1` fixed in
  substance, `R1` still open, `B3`/`B4` fixed then regressed, and the change log
  claiming a re-gate that never ran.
- `ATOM-REVIEW-20261026-001`, the code review of `58f9532..HEAD` — the encoder,
  the score floor, the seeder's `V6` change and the `D5.S5` CI wiring. It found
  no ownership, SQL, money or degradation defect and returned **FAIL on
  documentation**, with seven blocking findings.
- `ATOM-REVIEW-20261024-011`, the first **executable** quality-gate verdict,
  over `D0`–`D5`. It measured all six gates at `0` and 504 tests and returned
  **FAIL on documentation**, with five blocking findings (`B1`–`B5`), all
  repaired; it also confirmed that `ML-R02` was honestly recorded as unmet **at
  the time of that review** (2026-10-24). It is met as of
  `ATOM-CHANGE-20261025-003`; see the correction in the entry below.
- `ATOM-REVIEW-20261024-010`, the code review of job application chains
  `D3.J1`–`J5`. It found no code defect and returned **FAIL on documentation
  only**, with eight blocking findings, all now repaired.
- `ATOM-REVIEW-20261024-006`, the quality-gate audit of the four committed
  chains. It measured the gates itself (225 tests, all green) and returned
  **FAIL** overall on documentation, with `D0` passing and `D1`'s rows 10-11
  `Fail` as the ledger's scorecard records them. It also raised
  three process findings against the run, all now resolved.
- `ATOM-REVIEW-20261024-005`, the code review of the same four chains. It found
  no correctness, security or SQL defect and no weakened test, and returned
  **FAIL** on eight documentation contradictions, all repaired.
- `ATOM-REVIEW-20261024-004`, the security review of the seeder specification
  recorded in [`change-log.md`](change-log.md) as `ATOM-CHANGE-20261024-004`. It
  returned **PASS WITH CONDITIONS** with six conditions, all of which became
  contract amendments `A-C-S1-5`…`A-C-S1-10` before the implementation started.
- `ATOM-REVIEW-20261024-003`, the architect review of the Phase 2 intake
  recorded in [`change-log.md`](change-log.md) as `ATOM-CHANGE-20261024-003`. It
  returned **FAIL** with eleven blocking documentation contradictions, all fixed
  in `f88733c`.
- `ATOM-REVIEW-20261024-002`, the quality-gate review of the contract work
  recorded in [`change-log.md`](change-log.md) as `ATOM-CHANGE-20261024-002`. It
  returned **FAIL** on the first pass and **PASS** after the six blocking or
  required items were repaired.
- `ATOM-REVIEW-20261024-001`, the code review of the same change. It returned
  **PASS WITH FINDINGS**: nine non-blocking findings and four documentation
  findings, all fixed or recorded with a reason.
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

### ATOM-REVIEW-20261029-002 - The Phase 2 close-out: three features verified

**Date:** 2026-10-29 (UTC)
**Reviewers:** two independent read-only sessions on the same tree — the
`code-reviewer` (final-tree reading) and the `quality-gate` (execution)
**Change reviewed:** the final tree at `ab5aa00` — the three Phase 2 feature
spans (`src/reimbursements/`, `src/job_applications/`, `src/ml/`), the seeder,
and every hot document that carries their claims
**Verdict:** `CODE_REVIEW` **PASS** · `PRODUCT_CONTRACT` **PASS WITH FINDINGS**
(repaired in the promotion change) · `NEW_REGRESSIONS` **PASS** · gate
**PASS** — all three Phase 2 features are fit for `Verified`, and Phase 2 is
complete per its own definition.

#### What the code review verified (no defect found)

- **Reimbursements.** All nineteen statements carry `user_id` in their own
  predicate; the project link is enforced inside the write statements via
  `EXISTS`, so a foreign project is `404` with no check-then-act window; item
  writes are lock → write → recompute in one transaction with the
  `NUMERIC(19,4)` range guard in the same statement; money is `::text` end to
  end and `validate_money` refuses rather than rounds; the `double_option`
  tri-state is the correct serde pair; the lifecycle is four guarded `UPDATE`s
  with `clock_timestamp()`, the reopen clearing both stamps in one statement;
  the detail read pairs claim and items under one `REPEATABLE READ, READ ONLY`
  snapshot.
- **Job applications.** `ALLOWED_MOVES` is the single definition of the
  12-edge table and `permitted_sources` is derived from it, so the `409` a
  client sees and the `status = ANY($n)` predicate cannot disagree; the
  exhaustive 49-pair unit test pins 12/7/30; create is pinned to `wishlist`;
  the duplicate company name is detected by the named constraint, not a bare
  `23505`; the company delete is one atomic `DELETE … AND NOT EXISTS`; the
  application delete purges its embeddings in one data-modifying CTE.
- **Local ML.** `select` implements the degradation decision table exactly —
  never a silent fallback; every `embed` funnels through one `normalise` that
  refuses zero/non-finite/wrong-length vectors; inference is CPU-only via
  `spawn_blocking`, bounded by `ml.timeout_seconds`; the score floor filters
  before limit and total in all three scored paths; `reqwest`/`ureq`/`hyper`
  are absent from `Cargo.toml` — no network dependency anywhere.
- **Cross-cutting.** Zero `#[allow]` in `src/`; every `unwrap`/`expect`
  inside `#[cfg(test)]` (the six non-obvious candidates read individually) with
  the one SD-5-documented exception in `src/config/settings.rs`; the sole
  `format!`-built SQL is the seeder's own test over the static `USER_TABLES`
  array; `AppError` never renders an internal string to a client.

#### What the gate measured itself

| Command | Result |
| ------- | ------ |
| `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\gate.ps1` | **exit 0** — `fmt 0, clippy 0, test 0, nodefault 0, docs 0, strict 0` |
| `cargo test --all-targets --locked -- --list` | **536 tests**, matching the documented split target-for-target (229 lib, 13 `atom-seed`, 11 `api_keys_api`, 29 auth, 56 claims, 3 claims flow, 10 claims schema, 2 contract sweep, 2 docs route, 51 jobs, 2 jobs flow, 11 jobs schema, 4 list query, 41 ml, 12 notes, 11 openapi contract, 25 projects/tasks, 21 seeder, 0 shutdown) |

This re-derivation closes the note in `implementation-status.md` that the
536-test split had not been re-derived by a second process. The static evidence
confirmed: `CONTRACT_OPERATIONS = 54` in both test files, `ROUTE_TABLE` 52 + 2,
migrations `0001`–`0007`, the CI seeder job's `--reset`/idempotency/count/
`APP_ENV=prod` refusals, no `#[ignore]` in `tests/`, no secrets in tracked
content.

#### The three contract findings (all repaired in the promotion change)

| # | Severity | Finding | Disposition |
| - | -------- | ------- | ----------- |
| 1 | Non-blocking | `docs/README.md` claimed no re-gate of `ATOM-REVIEW-20261024-011` is recorded, contradicting `-20261026-002` | Rewritten in the promotion change |
| 2 | Non-blocking | `features/reimbursements/testing.md` pinned 55 endpoint cases; the tree holds 56 (the no-fields-PATCH case) | Corrected to 56 with the history named |
| 3 | Non-blocking | Two documents counted ten unit tests in `src/reimbursements/`; the module holds twelve (ten `service.rs`, two `dto.rs`) | Corrected in both |

#### The ML quality limitation, judged

The single recorded manual run (`ATOM-CHANGE-20261025-003`: 15-of-15 retrieval,
88–112 ms mean, 804 ms worst case, 183.8 MiB RSS; weights operator-supplied and
absent, so not re-measurable on this machine) **is compatible with `Verified`**:
`AGENTS.md` §6 requires "a test or a recorded check that passes", the automated
suites pass on this tree, the pair-count caveat is already marked unverified in
the feature card rather than claimed as settled, and the encoder span — the
stated reason the feature was held at `Implemented` — has now been read clean
twice. The limitation stands recorded in the card's §1.3 with its upgrade path
(publishing the artifacts or a checksum for a reproducible `atom-ml-bench` run).

#### Limitations of this review

The reading session's shell was denied (file inspection only); the executing
evidence is the gate session's own run, recorded above. The unix-only 538 figure
was not measurable on this Windows host and stands as documentation-consistent.
The one advisory code wording (`validate_match_limit` reusing the too-long
message for an out-of-range match limit, `src/ml/service.rs`) was accepted
as-is: field-named, `422`, catalogue-driven, no contract broken — a candidate
for a dedicated message in a later change.

---

### ATOM-REVIEW-20261029-001 - The final read of the repaired `K1` tree

**Date:** 2026-10-29 (UTC)
**Reviewer:** the `code-reviewer` agent, read-only, one read
**Change reviewed:** the final tree at `65898e5` — the whole `K1` chain
(`dcda97f` feature + `65898e5` repairs) as it stands, not the diff narrative.
**Verdict:** `CODE_REVIEW` **PASS** · `PRODUCT_CONTRACT` **PASS WITH FINDINGS**
· `NEW_REGRESSIONS` **PASS** — the tree is fit for `K1` to be recorded as
`Verified`.

#### What it verified from the tree

- **Code (no finding).** Ownership statement by statement: `LIST_OWNED`,
  `COUNT_OWNED`, `REVOKE_OWNED` and `TOUCH_LAST_USED` all carry `user_id`;
  `FIND_KEY_IDENTITY` is the one unscoped read, justified as the pre-credential
  digest lookup whose result never reaches a response. The secret exists only
  in `GeneratedKey` and `ApiKeyCreatedResponse`; `ApiKey` has no `key_hash`
  field; zero `unwrap`/`expect` outside `#[cfg(test)]` in `src/api_keys/`; no
  `#[allow]`; no string-built SQL. The `atom_` branch refuses a malformed key
  before hashing and before SQL; the `401` / `403 account_disabled` / `404`
  split is exactly `A-C-K1-1`.
- **Figures re-derived, not quoted.** `CONTRACT_OPERATIONS = 54` in both test
  files, with a decomposition that sums 52 versioned + 2 probes; `ROUTE_TABLE`
  counted by hand at 52; 11 `#[tokio::test]` cases in `tests/api_keys_api.rs`
  with zero `#[ignore]`; unit tests 5+3+4+1 = 13; the full-suite arithmetic
  re-derived per file to **536 on Windows / 538 on unix**; 69 = `atom_` + 32
  bytes × 2 hex; `migrations/` ends at `0007`. No stale current-surface figure
  survives: every remaining "51" is explicit history, the
  `job_applications_api` suite's own 51 cases, or a dated record.
- **Regressions (none).** The repair commit's only test addition is the
  repository-level usage-throttle case (+131 lines, `#[ignore]` count 0); the
  contract assertions and both sweep pins are intact.

#### Findings (all corrected in the same change that records this verdict)

| # | Severity | Finding | Disposition |
| - | -------- | ------- | ----------- |
| 1 | Medium | `docs/README.md`, `docs/features/README.md` and `docs/api/README.md` claimed no independent review and no quality-gate verdict are recorded, contradicting `-027-001` in the same tree | Rewritten in the promotion change: the gate is PASS, the review FAILs are repaired, the final read is this one |
| 2 | Low | `runs/chain/ledger.md` rows 8 and 12 carry review-time "10"/"ten" counts beside post-repair figures without a marker | One dated clause each, marking them as counts at review time |
| 3 | Low | The `-027-001` change-log entry had been corrected in place ("ten" → "eleven" cases) against the log's own append-only rule | A dated **Correction** block appended to that entry, covering both the in-place edit and this promotion |

#### Gate evidence for the promotion

`scripts/gate.ps1` re-run by the orchestrator on the same tree immediately after
this read: **all six gates at `0`** (fmt, clippy, test, nodefault, docs,
strict).

#### Limitations

The reviewer's shell was denied, so the read is by file inspection — the same
posture as the three earlier reads; the executing evidence is the gate run
above. The live n8n integration recorded in the `K1` checkpoint remains an
external claim that is not reproducible from this repository.

---

### ATOM-REVIEW-20261027-001 - Scoped API keys: PASS on execution, FAIL on the documentation figures

**Date:** 2026-10-27 (UTC)
**Reviewer:** the `code-reviewer` agent (read-only, three reads) and the `quality-gate` agent (read-only)
**Change reviewed:** `dcda97f` and the repair working tree on top of it - the API-keys feature: `migrations/0007_api_keys.sql`, `src/api_keys/` (seven modules), the `atom_` branch in `AuthUser::from_request_parts`, three routes, ten contract cases, and the documentation that follows.

#### The verdicts

| Reader | Verdict | What it found |
| ------ | ------- | ------------- |
| `quality-gate` | **PASS** | Six gates `0`; 54 operations asserted in both directions; the ten contract cases present and none ignored; every current document consistent with the code; no secret in the diff |
| `code-reviewer`, read 1 | **FAIL** | One blocking finding: eleven present-tense "51 operations" claims in the feature documents, against `CONTRACT_OPERATIONS = 54`. Six advisories |
| `code-reviewer`, read 2 | **FAIL** | The eleven sites were repaired; eight further stale figures remained (`ml/testing.md` still pinning 51, three `48` references, two `537` unix totals, three "10 cases") |
| `code-reviewer`, read 3 | **FAIL** | One figure remained: this change log's own status line said "ten integration cases" |

**Nothing is promoted to `Verified`.** The code, the schema, the authorization predicates, the secret handling and the contract agreement were read clean in the first pass and never found defective; what kept failing was the prose around the numbers, and no read of the final tree exists. That is the same discipline `-001` and `-002` applied to Phase 2.

#### What the gate confirmed as honest

`powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\gate.ps1` - all six at `0`, including `cargo test --all-targets --locked` at **536 passed, 0 failed** on Windows. `CONTRACT_OPERATIONS = 54` in both test files; `ROUTE_TABLE` 52 + 2 unversioned; the three `/api-keys` paths in the generated document; `migrations/` ending at `0007`; the working tree clean and the diff free of any key, token, password or connection string.

#### What the code review confirmed as sound

- **Ownership, statement by statement.** `LIST_OWNED`, `COUNT_OWNED`, `REVOKE_OWNED` and `TOUCH_LAST_USED` all carry `user_id`; `INSERT_KEY` binds the owner as a value. `FIND_KEY_IDENTITY` is the one unscoped read, and the exception is justified: no caller identity exists before the credential is resolved, it binds a digest rather than returning one, and its result never reaches a response. A stored digest cannot be replayed as a credential, because `is_well_formed` requires `atom_` plus 64 hex and the digest has no prefix.
- **The branch cannot steal a JWT or fail open.** A JWT's first segment is base64url JSON and never begins `atom_`; a malformed `atom_…` value is refused before hashing and before SQL; a revoked key and a disabled account both fail closed; the status split is exactly `A-C-K1-1`.
- **The secret.** It exists in `GeneratedKey`, in `ApiKeyCreatedResponse` and nowhere else; `ApiKey` has no `key_hash` field, so no response type can select it; the only `tracing` call on the path logs a key id and an `AppError`.
- **Layering.** No SQL outside `repository.rs`, no ownership comparison in `handlers.rs`, no `unwrap` or `expect` outside tests, no `#[allow]`, no concatenated SQL.
- **Tests that can fail.** Every one of the ten contract cases goes red under a plausible regression - with the one exception that became advisory 2 below.

#### Findings and their disposition

| # | Severity | Finding | Disposition |
| - | -------- | ------- | ----------- |
| 1 | Blocking | Eleven current-surface "51 operations" claims across `features/ml/{status,api}.md` and `features/job-applications/{architecture,status,testing}.md`, plus eight further stale figures found in the following two reads | Repaired at every site: 54 with explicit history, the `536`/`538` split, and eleven API-keys cases. Counts of another suite's cases (the 51 in `job_applications_api`) were deliberately left alone |
| 2 | Medium | The SQL half of the usage throttle was enforced by nothing that can fail: `last_used_is_recorded_once_an_hour` passes with the `AND (last_used_at ...)` clause deleted, because the service pre-check masks it | Closed: `the_usage_statement_itself_refuses_a_second_write_within_the_hour` calls `ApiKeyRepository::touch_last_used` directly and was proven red with the clause removed and green with it restored |
| 3 | Low | No `tests/api_keys_schema.rs`; the three `CHECK`s, the `UNIQUE` and the cascade are proven by a recorded hand-run, not by a test | **Accepted, not fixed.** The contract did not name a schema test and the constraints were exercised on a scratch database; recorded here so the gap is not mistaken for coverage |
| 4 | Low | `CONTRACT_OPERATIONS`' doc comment counted the scored search twice and summed to 53 | Repaired: the decomposition now sums to 52 |
| 5 | Low | `implementation-status.md` quoted 10 `openapi_contract` cases while the counted split said 11 | Repaired |
| 6 | Low | `ADR-0011` described the contract's superseded 68 without pointing at the amendment | Repaired: it now records that `A-C-K1-2` corrected 68 to 69 |
| 7 | Low, pre-existing | A NUL byte in a text field passes `required_text` and is refused by PostgreSQL with `22021`, so it becomes a `500` rather than a `422`. Repo-wide, not introduced here | **Not fixed, recorded.** It belongs in `required_text`, not in `api_keys`, and changing a shared validator is its own change |

#### Limitations of this review

- The reviewer's shell was denied in all three reads, so it verified by reading the tree and counting from the files; the executing check is the `quality-gate`'s and the gate's own output.
- The `quality-gate` could not run `cargo test --all-targets --locked` with the environment variable itself (its allow-list matches whole command strings); it ran `scripts/gate.ps1`, which sets it, and took the per-binary split from `-- --list`.
- The live n8n integration recorded in the `K1` checkpoint is an external claim. It was performed and its execution id recorded, but it is not reproducible from this repository.

---

Full records of the earlier reviews (`ATOM-REVIEW-20261021-001` through
`ATOM-REVIEW-20261026-002`) are in
[`archive/review-log-2026-10.md`](archive/review-log-2026-10.md), moved
verbatim by `ATOM-CHANGE-20261029-001`. Their findings and dispositions
remain part of the record.

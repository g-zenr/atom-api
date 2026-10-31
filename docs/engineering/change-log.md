# ATOM change log

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-31
> **Owner:** maintained by the `docs` agent

The chronological record of substantial changes to the ATOM backend, newest
first. It explains **how** the system reached its current state; it does not
define the target state. The requirements live in
[`../project-truth.md`](../project-truth.md), the current state in
[`implementation-status.md`](implementation-status.md), and the reviews in
[`review-log.md`](review-log.md).

---

## How to read this log

- **Newest first.** The most recent entry is at the top.
- **Append-only.** An entry is never rewritten. An inaccurate statement is
  corrected by adding a clearly marked **Correction** to the same entry, with
  its own date and a reason.
- **One entry per substantial change.** A change is substantial when it alters
  behaviour, a contract, the schema, a security control or the architecture.
  A typo fix does not need an entry.
- **The checks are recorded with their real results.** A check that did not run
  is recorded as *not run*, with the command needed to run it. A result is
  never claimed that was not observed. See
  [`../testing/verification-policy.md`](../testing/verification-policy.md) §4.

## How to add an entry

1. Copy [`../templates/change-record.md`](../templates/change-record.md).
2. Give it the next identifier: `ATOM-CHANGE-YYYYMMDD-NNN`, where `NNN` is a
   three-digit sequence that has not been used before. Identifiers are never
   reused, and the date is the UTC date of the change.
3. Fill in every section. A section that does not apply says
   `Not applicable: <reason>`; it is never deleted, because a deleted section
   hides the question.
4. Insert the entry directly under the `## Entries` heading, above the previous
   entry.
5. Run the three quality gates and record their real output.

## Entries

Entries from before 2026-10-27 are in
[`archive/change-log-2026-phase-1-2.md`](archive/change-log-2026-phase-1-2.md),
moved verbatim by `ATOM-CHANGE-20261029-001` to keep this file small enough
to read whole. They remain part of the record and are never rewritten.

### ATOM-CHANGE-20261030-001 - Stabilization: documentation reconciled, supply-chain gate added, coverage gaps closed

**Date:** 2026-10-30 (UTC)
**Type:** Documentation + Verification + Hardening - the stabilization change:
reconcile the status documents with the 2026-10-29 promotion, close recorded
test gaps, add the dependency-security gate, and record fresh executed runs
on this host.
**Author:** the orchestrator session of the stabilization task, with two
builder sessions (the coverage cases; the `atom-web` frontend, which lives
outside this repository and is recorded in its own README).
**Status:** Complete - see "What was measured" for the checks that actually
ran on this host.

#### Why

A read-only audit on 2026-10-29/30 (`docs/audit/codebase-audit-2026-10-30.md`)
found no code defect in the shipped backend, but found that the two most
authoritative documents contradicted the 2026-10-29 promotion recorded inside
them: `project-truth.md` §3 said Phase 2 was "partly delivered" and §18's
intro still narrated the features as `not Verified`, while §17 and the roadmap
said `Verified`/`Complete`; `roadmap.md` §1 said "nothing in Phase 2 is
`Verified`" one paragraph above its own completion statement;
`module-boundaries.md` used three different status words for `ml` in one file;
`schema.md` §11 was still headed "Proposed"; the root `README.md` predated
Phase 2 entirely; and the `docs/README.md` decision index omitted `ADR-0010`
and `ADR-0011`. The reimbursement card and the testing strategy also carried a
stale claim: they called the "in any status" delete half of `REIMB-R11`
unasserted, while the committed endpoint case
`a_claim_can_be_deleted_in_any_status` (`tests/claims_api.rs`) asserts exactly
that half. The same audit asked for the dependency-security gate, the coverage
gaps the testing strategy self-declared, and a bounded owner decision on
automation scope. This change repairs and supplies all of it, and records a
real executed run.

#### What changed

| Area | Change |
| ---- | ------ |
| `docs/project-truth.md` | §3 states Phase 2 is delivered and `Verified` (2026-10-29); §17 lists migrations `0001..0007` and gains the scoped-API-keys row (authority `ADR-0011`, not a §18 requirement, as the feature card records); §18's intro is rewritten to the resolved state, keeping the review history (`-20261024-010`, `-011`, the 2026-10-26 re-gate, `-20261026-001`, `-20261029-002`) as the path, not as an open debt; a new **§6.2 amendment (2026-10-30)** accepts the web frontend as a separate codebase (owner instruction of the stabilization task) while the mobile client stays excluded |
| `docs/roadmap.md` | §1 status-column note corrected: the three Phase 2 features are `Verified`; the seeder stays `Implemented` tooling; the Web frontend row becomes `Accepted` (2026-10-30) with the amendment named |
| `README.md` | The Status section now reflects the backend as implemented and `Verified` through Phase 2, the container as CI-built but unpublished, and the frontends/automation as not started |
| `docs/architecture/module-boundaries.md` | The Phase 2 section heading, its "Mostly planned" blockquote, the three module-table rows and the `ml` subsection heading all read `Verified` consistently |
| `docs/database/schema.md` | §11 re-headed "built and `Verified`" with the status blockquote rewritten; the per-migration paragraphs were already accurate and stand |
| `docs/README.md` | The decision index gains `ADR-0010`, `ADR-0011` and `ADR-0012`; the architecture index gains the two preparation documents |
| **Supply chain (new)** | `deny.toml` (cargo-deny config v2: every vulnerability advisory denied unconditionally, unknown sources denied, licences deny-by-default with the allow list to be completed by the first CI run), `.github/dependabot.yml` (cargo + github-actions, weekly), and `ci.yml` hardened: all action references pinned to verified commit SHAs (`actions/checkout` v4.4.0 — the same SHA `container.yml` already pinned, `actions/cache` v4, `dtolnay/rust-toolchain` stable branch, `EmbarkStudios/cargo-deny-action` v2) and a new `supply-chain` job runs `cargo deny check` on every push. Both configs were validated by the independent review, not executed locally (`cargo deny` is not installed on this host); the first execution is the CI job |
| **Decision requests (new)** | `open-questions.md` gains `Q-15`…`Q-19` — the five security decisions the audit raised, each with options, costs and the owner as decider: Argon2 pepper, access-token revocation, registration enumeration, email grammar, refresh-rotation transaction boundaries. No authentication behaviour was changed; the recorded `ADR-0003` ordering stands |
| **Coverage (new)** | 22 new test cases close recorded gaps: `tests/api_keys_schema.rs` (8 — the `0007` constraints against a scratch database), `tests/embeddings_schema.rs` (7 — the `0006` constraints), `tests/cors.rs` (6 — HTTP-level CORS, including the empty-allowlist and disallowed-preflight cases) and `tests/session_prune.rs` (1 — the prune removes only sessions expired beyond the grace). No source file, route or DTO changed |
| **Maintainability (code)** | `src/api/routes.rs`: the stale `GET /notes/search` comment (claiming `400`) now states the test-pinned `405`; `src/bin/atom-ml-bench.rs`: the hard-coded personal-machine model path is replaced by a bare relative default — `ATOM_ML_MODEL` remains the documented invocation. `atom-ml-spike.rs` is left untouched: it is the frozen `ADR-0008` §8 record |
| **Decisions (proposals)** | `ADR-0012-automation-orchestration-scope.md` (`Proposed`, owner decision `O7`): n8n as an external orchestrator reaching ATOM only through authenticated, idempotent endpoints, with the price-monitoring schema sketch as the first use case — no automation code is built by this change |
| **Preparation documents (new)** | `docs/architecture/mobile-preparation.md` and `docs/architecture/ai-roadmap.md` — preparation only, each stating plainly what does not exist |
| Feature cards | `reimbursements/status.md`: the `REIMB-R11` row names `a_claim_can_be_deleted_in_any_status` with what it asserts; §2 records that four of the five small gaps have **uncommitted** working-tree cases (executed 2026-10-30, passing) and that the review rows closed; `job-applications/status.md`: the broken sentence in §1.2 repaired and the four "No run is recorded" cells record the 2026-10-30 execution |
| `docs/testing/strategy.md` | The reimbursement caveat no longer calls `REIMB-R11`'s "in any status" unasserted; dated paragraphs record the 2026-10-30 runs and the WIP isolation race (below) |
| `docs/engineering/implementation-status.md` | The reimbursements row records the fresh run and the corrected gap state; the user's own 2026-10-29 ML-benchmark edits are preserved verbatim |
| `AGENTS.md` | The repository-layout tree names the four modules added since it was written (`api_keys`, `reimbursements`, `job_applications`, `ml`) |

#### What was measured

Executed on this Windows host against the disposable `atom_test` database
(name-guarded by the harness; no other database on the server is touched):
`cargo fmt --all -- --check` **pass**; `cargo clippy --all-targets
--all-features -- -D warnings` **exit 0**; `cargo check --no-default-features
--all-targets` **pass**; `cargo run --bin atom-docs -- --strict` **OK, 0
warnings**. Test runs: the working tree (`96c37b2` + five uncommitted tests)
first executed **541** tests (**540 passed, 1 failed**); after the 22 new
coverage cases landed, the final full run executed **563** (**562 passed, 1
failed**). The single failure
(`the_seeded_claims_are_readable_through_the_api`) is a test-isolation race
introduced by the **uncommitted** minimal-profile seeding case in
`tests/claims_flow.rs`: that case resets the shared database to the minimal
profile while the committed opportunistic case is mid-flight, so the
committed case's existence-check-then-login as `owner@atom.local` loses the
account between the two steps and the login's session insert answers the
foreign-key violation the API maps to `500`. The committed case re-run in
isolation passes (1 passed, 0 failed), and the committed tree alone executes
**536/536 on Windows**, matching the `K1` split. All five uncommitted tests
and all 22 new cases pass. The defect and the proposed fix (serialize the two
seeded cases with the seeder advisory lock, or run the WIP case against its
own scratch database) are recorded for the file's owner; the file is
uncommitted user work and was not modified by this change. The new
`cargo-deny` gate and the Dependabot configuration were not executed locally
(`cargo deny` is not installed on this host); their first execution is the
CI `supply-chain` job, and they are recorded here as *configured, not run*.

#### What did not change

No migration, no published contract, no endpoint, no DTO, and no committed
test was modified or deleted. The source changes are two comment/string
edits that alter no behaviour (the `routes.rs` comment and the `ml-bench`
default path); the 22 new cases are new files; and the five uncommitted test
cases and the three user-modified documentation edits are preserved
byte-for-byte. The one advisory code wording from the 2026-10-29 review (the
match-limit message reusing the too-long template) is recorded in the review
entry as accepted, not fixed here.

#### Documentation

This entry is the documentation change. The reconciliation is verifiable with
`cargo run --bin atom-docs -- --strict`. The `atom-web` frontend scaffold is
a separate codebase outside this repository; its setup, auth design and
delivery checklist are recorded in that codebase's own README, and nothing
about it is claimed in this repository's status documents.


### ATOM-CHANGE-20261031-001 - Owner decisions implemented: tightened email, transactional rotation, and the price-monitoring domain

**Date:** 2026-10-31 (UTC)
**Type:** Feature + Security - the owner's approved decisions (`Q-15`…`Q-19`,
`O7`) implemented as uncommitted, reviewable work.
**Author:** four builder sessions under the orchestrator of the master
development task; uncommitted by owner instruction pending separate
authorization.
**Status:** Complete and **committed** (2026-10-31, owner-authorized, as two
commits — see the remediation note at the end of this entry). The monitoring
feature is `Verified`: reviewed by `ATOM-REVIEW-20261031-001` and remediated
the same day.

#### Why

The owner accepted `Q-15` (no pepper), `Q-16` (15-minute access window, no
revocation list), `Q-17` (registration response unchanged), `Q-18` (tighten
email validation), `Q-19` (transactional refresh rotation) and `O7` (accept
`ADR-0012`, make laptop price monitoring the first automation feature).

#### What changed

| Area | Change |
| ---- | ------ |
| `Q-15`/`Q-16`/`Q-17` (documented) | No code change, by decision; `open-questions.md` records each as **Answered** with the accepted risk and its revisit trigger (password reset/email for `Q-17`) |
| `Q-18` (code) | `validate_email` tightened: one `@`, non-empty parts, no consecutive/leading/trailing dots, well-formed hyphen-free domain labels, local ≤64 / domain ≤253; normalization and the 3..=254 DB contract unchanged. 11 unit + 4 HTTP cases (accept and reject shapes) |
| `Q-19` (code) | Refresh rotation is one transaction (`AuthRepository::rotate`): replacement insert + guarded claim; a lost race rolls the whole transaction back, so no orphan replacement session can exist; reuse detection, mass revocation, absolute-lifetime inheritance and owner scoping unchanged. 3 new regression cases; `ADR-0003` amended, `data-architecture.md` §7, `request-lifecycle.md` §5.2 and `project-truth.md` §12 updated |
| `O7` / `ADR-0012` | Status → **Accepted** (2026-10-31, owner decision) |
| Price monitoring (new domain) | `migrations/0008_price_monitoring.sql` (six tables, every child composite-FK'd `(parent_id, user_id)`, idempotency `UNIQUE (user_id, listing_id, fingerprint)`, duplicate alerts unrepresentable `UNIQUE (rule_id, observation_id)`); `src/monitoring/` (models/dto/repository/service/handlers, money as `::text` strings per `ADR-0007`, every statement user-scoped, rule evaluation in SQL inside the observation transaction); **22 new operations** (54 → 76, contract test updated); seeder extended (1 product / 1 listing / 3 observations) |
| Automation docs (new) | `docs/automation/` — integration contract, local setup, the 9-node importable workflow skeleton (no retailer endpoint asserted), and a data-source checklist with every source `unverified` |
| Feature docs (new) | `docs/features/price-monitoring/` — the five standard documents; status **`Implemented`**, pending review |

#### What was measured

`cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets
--all-features -- -D warnings` exit 0; `cargo run --bin atom-docs --strict`
OK 0 warnings; per-module test runs recorded by each builder
(`monitoring_api` 31, `monitoring_schema` 14, `openapi_contract` 11,
`contract_sweep` 2, `seeder` 21, `auth_api` 34, `claims_flow` 4; users unit
11). A full-suite run is recorded in the session's final report. Nothing was
committed and nothing was pushed: the owner's instruction keeps security and
automation work uncommitted pending separate authorization.

#### Documentation

This entry, the feature docs, the automation docs, the ADR amendments and the
open-questions answers. The change-log's own `Last updated` line moves with
this entry.

#### Remediation and commit (2026-10-31, owner-authorized)

An independent review of the monitoring span
([`ATOM-REVIEW-20261031-001`](review-log.md), session
`ses_e6fd06d3cffePNWCQwujiHsbkk`) returned **VERIFIED WITH CONDITIONS** —
`CODE_REVIEW` PASS WITH FINDINGS, `PRODUCT_CONTRACT` PASS WITH FINDINGS,
`NEW_REGRESSIONS` PASS, no Critical or High code finding, 627 tests / 0
failed on the reviewer's own run. The conditions and findings were remediated
the same day:

- **M-1** — the workflow skeleton rewritten to the real API paths with the
  product→listings fan-out (`POST /api/v1/automation/runs`, paginated
  `GET /api/v1/products?status=active`,
  `GET /api/v1/products/{id}/listings`,
  `POST /api/v1/listings/{id}/observations`,
  `PATCH /api/v1/automation/runs/{run_id}`);
  re-validated: 15 nodes, 15 connections, 0 errors.
- **M-2** — two concurrent-ingestion tests (`tokio::join!`, SQL-verified
  final state; 3× green): distinct fingerprints → both `201`, two rows, two
  alerts; same fingerprint → one `201` + one `200`, one row, one alert.
- **M-3** — the absorption-verification rule in
  `docs/automation/integration-contract.md` §4, mirrored by the skeleton's
  *Verify absorption* node.
- **L-1/L-2/L-3** — pause semantics, the previous-observation definition and
  the target-equality test.
- **I-1…I-5** — re-arming and expected volume documented; ADR-0012 reader
  pointer and the monitoring-write rate-limiting production decision
  recorded; the URL-validator posture checked against
  `validate_receipt_url` and deliberately left identical (I-2); the
  two-commit split (this one).

Post-remediation gates: fmt 0, clippy 0, **630 tests / 0 failed** across 29
binaries, `atom-docs --strict` OK. The monitoring feature is promoted to
**`Verified`** by the review record. The change set landed as two
owner-approved commits: `feat(auth,users): implement the approved security
decisions` and `feat(monitoring): add price monitoring and automation
contract`. Nothing was pushed.

### ATOM-CHANGE-20261030-002 - The WIP seeding race is resolved: the seeded claims read holds the seeder lock

**Date:** 2026-10-30 (UTC)
**Type:** Test - an isolation fix inside `tests/claims_flow.rs`, performed under
the owner's explicit next-session instruction ("Resolve the WIP test race …
without deleting or weakening existing tests").
**Author:** the orchestrator session of the next-execution task; the fix is
the one this change set's independent review
([`ATOM-REVIEW-20261030-001`](review-log.md)) prescribed.
**Status:** Complete - the affected binary ran **8/8** consecutive green runs
and the full suite is **563/563**.

#### Why

`ATOM-CHANGE-20261030-001` recorded a test-isolation race: the committed
opportunistic case (`the_seeded_claims_are_readable_through_the_api`) checked
that `owner@atom.local` exists and then logged in, while the uncommitted
minimal-profile case reset the shared database in parallel - a reset that
commits between the two steps deletes the account under the check, and the
login's session insert answers the foreign-key violation the API maps to
`500`. The fix options were recorded then; the owner's next instruction
authorised applying one.

#### What changed

One lock acquisition and its comment, in `tests/claims_flow.rs`:

- the seeded half of the committed case now calls `seeder_lock()` **before
  its existence check** and holds the `ATOMSEED` advisory key to the end of
  the test, so no seeder reset can commit between the check and the login;
- the `seeder_lock` doc comment names the new party it serialises against.

Nothing else changed: no assertion was weakened, no test deleted or renamed,
and the uncommitted minimal-profile case is untouched (it already held the
same key for its whole run, so the two halves now cannot overlap). The
committed case's assertions are profile-agnostic by design, so whichever
profile ran last is the one it reads - that behaviour is stated in its own
comment and is unchanged.

**Correction to `ATOM-CHANGE-20261030-001`:** its "What was measured" records
the race as standing and the file as unmodified; that was true of that change
at the time it was written. This entry supersedes it: the owner's
next-session instruction authorised the fix, the file is modified accordingly,
and the race is closed.

#### What was measured

- `cargo fmt --all -- --check`: pass (exit 0).
- `cargo test --locked --test claims_flow`, **eight consecutive runs**: 8 ×
  exit 0 (4 tests each; the binary failed 2 of 2 full-suite runs before the
  fix).
- `cargo test --all-targets --locked --no-fail-fast` against the disposable
  `TEST_DATABASE_URL`: **563 executed, 563 passed, 0 failed** (245
  unit-target + 318 integration) - the first recorded full-suite run of this
  tree with zero failures.

### ATOM-CHANGE-20261029-002 - Phase 2 closes: reimbursements, job applications and local ML are `Verified`

**Date:** 2026-10-29 (UTC)
**Type:** Verification - Phase 2 close-out
**Author:** the orchestrator; the promotion evidence is two independent
read-only sessions recorded as
[`ATOM-REVIEW-20261029-002`](review-log.md).
**Status:** Complete - the gate was re-run on the promoted tree: **all six at
`0`** (fmt, clippy, test, nodefault, docs, strict), 536 passed / 0 failed.

#### Why

The roadmap's own completion condition for Phase 2 was unmet on one point only:
the three features were `Implemented` with no recorded read of the repaired
final tree and no quality-gate verdict on it (`ATOM-REVIEW-20261026-002` had
judged the tree before its documentation repairs). Everything else the
condition names was already true: the hardening sweeps were closed by the `D5`
chains, the schema document matched the migrations, the seeder covered every
Phase 2 table, and CI proved it. This change supplies the missing evidence and
performs the promotion.

#### What changed

| Area | Change |
| ---- | ------ |
| Feature cards | `reimbursements/status.md`, `job-applications/status.md` and `ml/status.md` flip to **`Verified`**, each with a promotion-evidence section; the per-requirement tables flip state-by-state; every recorded gap (reimbursements' five unasserted requirement halves, the job-application uncovered halves, the single-run ML benchmark) stays written where it was |
| Overview | `implementation-status.md` rows become single-source pointers dated 2026-10-29; the run paragraph records that the 536-test split has now been re-derived by the second process |
| Indexes | `docs/README.md`, `features/README.md`, `api/README.md` rows point at the cards; `testing/strategy.md` verdict paragraphs record the promotion with the gaps kept visible |
| Roadmap | `docs/roadmap.md` marks Phase 2 **complete (2026-10-29)** with the evidence named |
| Corrections | Three stale figures the review found (the claims-api count 55 → 56 with history, the reimbursements unit-test count 10 → 12, the `docs/README.md` re-gate claim) repaired in this same change |
| Review log | `ATOM-REVIEW-20261029-002` records both sessions: the reader's three verdicts and the gate's six zero exit codes plus the target-for-target split re-derivation |

#### What was measured

Two independent sessions on tree `ab5aa00`: the code-reviewer read the three
spans statement-by-statement (no defect; the full list is in the review entry),
and the quality-gate executed `scripts/gate.ps1` itself (six gates `0`) and
re-derived the 536-test split. The final six-gate run for this entry was
re-executed after the promotion edits.

#### What did not change

No source file, no migration, no contract, no test. The one advisory code
wording from the review (the match-limit message reusing the too-long template)
is recorded in the review entry as accepted, not fixed here.

#### Documentation

This entry, the three feature cards, the three index documents,
`implementation-status.md`, `strategy.md`, `roadmap.md` and the review-log
entry.

### ATOM-CHANGE-20261029-001 - The knowledge base moves its history to an archive

**Date:** 2026-10-29 (UTC)
**Type:** Documentation - knowledge-base structure
**Author:** the orchestrator, on the owner's decision.
**Status:** Complete - the full gate run (`scripts/gate.ps1`) is **all six at
`0`** on the restructured tree (fmt, clippy, test, nodefault, docs, strict);
`cargo test --all-targets --locked` passed with **536 tests, 0 failed**, which
also covers the four source comments that were repointed. The docs checker
reports `OK (0 warning(s))` in both modes after every step of the move.

#### Why

The knowledge base had grown to ~585K tokens across 93 files, and the run
logs dominated it: the change log (~49K tokens), the Phase 2 run ledger
(~41K) and the review log (~19K) - around 110K tokens of append-only history
for a run that is complete and tagged. Every intake read several of them, the
agent context paid for it on every prompt, and the same status evidence was
restated in up to nine files, which is precisely how the `K1` documentation
figures went stale three times. The owner's direction: keep the governance,
shrink the working set, and do not bloat the context a prompt starts with.

#### What changed

| Area | Change |
| ---- | ------ |
| Change log | Entries from 2026-10-21 to 2026-10-26 (3 049 lines) moved to [`archive/change-log-2026-phase-1-2.md`](archive/change-log-2026-phase-1-2.md), entry text unchanged and link targets repointed for the new location; the hot file keeps the instructions and the entries from 2026-10-27 onward, with a pointer under `## Entries` |
| Review log | Full records from `ATOM-REVIEW-20261021-001` through `-20261026-002` (821 lines) moved to [`archive/review-log-2026-10.md`](archive/review-log-2026-10.md), text unchanged and link targets repointed; the hot file keeps the every-review summary list and the two most recent full entries |
| Known issues | The fourteen **closed** `KI` entries moved to [`archive/known-issues-closed.md`](archive/known-issues-closed.md), text unchanged and link targets repointed; the hot file keeps the format, the open issues, and the summary table, which gains the `KI-022` and `KI-024` rows it was missing |
| Phase 2 run | `docs/engineering/runs/chain/` moved whole (git rename, history preserved) to [`runs/archive/phase-2/`](runs/archive/phase-2/ledger.md); a stub [`runs/chain/ledger.md`](runs/chain/ledger.md) records that no run is active and points at the archive. References from archived records into the run ledger land on that stub and follow its pointer one hop onward — accepted deliberately, so the archived bodies keep exactly one kind of edit (their own link depth) |
| Archive index | [`archive/README.md`](archive/README.md) lists what moved, when, and by which change |
| Intake rule | `AGENTS.md` section 5 is now **tiered**: always read the index, the one feature card, the governing `project-truth` section and the real diff; architecture, ADRs, full KI entries, the configuration reference and the standards are retrieved on demand; documents are passed to subagents as paths, never pasted |
| Single sourcing | `implementation-status.md` section 4 records the rule: a feature's status and evidence are written once, in its feature card; the overview row and the index rows carry the status word and a pointer, never a restatement. The `K1` row was trimmed to that shape |

554 relative links were rewritten across the moved and repointed documents,
each resolved against the original tree rather than guessed. Every moved
entry body was hash-verified equal to its pre-move content (24 of 24 known
issues, every archived change-log and review-log heading accounted for in
exactly one place).

#### What did not change

No product behaviour, no contract, no schema, no test assertion, no gate.
Four source comments (`src/bin/atom-seed.rs`, `tests/claims_schema.rs`,
`tests/jobs_schema.rs`, `tests/seeder.rs`) were repointed from the old ledger
path to the archive path; the files compile and the suite is green. Nothing
was deleted: the archive is part of the record.

#### Effect on the working set

The documents a task reads by default drop from roughly 150K tokens (whole
logs plus mandated intake) to well under 40K; the archive holds the rest,
one link away. This entry is the first written under the new shape.

#### Documentation

This entry, `AGENTS.md` section 5, `docs/README.md` (the engineering index
rows and the archive row), `docs/engineering/implementation-status.md`
(the single-sourcing rule and the `K1` row), the archive set itself, and the
repointed references across `docs/` and the four source comments.

### ATOM-CHANGE-20261027-001 - Scoped API keys, so a machine can hold a credential

**Date:** 2026-10-27 (UTC)
**Type:** Feature - authentication
**Author:** the orchestrator with the `database`, `rust-backend`, `test-engineer` and `docs` subagents.
**Status:** Complete - **`Implemented`, not `Verified`.** The six gates are green and the feature is covered by eleven integration cases and thirteen unit tests. The `quality-gate` returned **PASS**; the `code-reviewer` read the diff three times and returned **FAIL on the documentation figures** each time, every finding is repaired, and no read of the final tree exists.

#### Why

The owner is integrating a deployed n8n instance with ATOM. ATOM had no
credential a workflow could hold: `AuthUser` accepted only a user JWT, whose
access token lives fifteen minutes and whose refresh token **rotates with reuse
detection**. A scheduled workflow that stores a refresh token therefore either
stops working the first time it rotates, or - if it retries a stale one -
triggers reuse detection and revokes every session of the account. That is the
actual failure this change removes, not a hypothetical one.

#### What changed

| Area | Change |
| ---- | ------ |
| Schema | `migrations/0007_api_keys.sql`: `api_keys (id, user_id, name, prefix, key_hash, last_used_at, created_at, revoked_at)`, three named `CHECK`s, `UNIQUE (key_hash)`, `INDEX api_keys_user_id_idx`, `ON DELETE CASCADE` from `users` |
| Credential | `atom_` + 32 bytes from the OS CSPRNG as lowercase hex - **69 characters**. `prefix` = `atom_` + the first 8 hex (13). Stored only as a **SHA-256** digest |
| Operations | `POST /api/v1/api-keys` (201, the secret returned once), `GET /api/v1/api-keys` (200, paginated, never a secret), `DELETE /api/v1/api-keys/{key_id}` (204, idempotent) |
| Authentication | `AuthUser::from_request_parts` branches on the `atom_` prefix before it reaches `TokenService`; a key resolves through one `JOIN` to exactly one `user_id` |
| Errors | unknown, malformed or revoked key -> `401 unauthorized`; the key of an inactive account -> `403 forbidden` `account_disabled`, the same answer that account's JWT already gets |
| Write cost | `last_used_at` is stamped by one statement that is a no-op inside the hour, so a hot key does not write on every request |
| Seeder | `api_keys` joins `USER_TABLES` (11 -> 12) so `V6` keeps proving the reset list against `information_schema`; no key is ever inserted |
| Surface | `ROUTE_TABLE` and the OpenAPI document move **51 -> 54** operations (52 under `/api/v1`, 2 unversioned probes) |

#### Two decisions worth not re-litigating

**SHA-256, and not Argon2.** A key is 256 bits of CSPRNG output rendered as
`atom_` plus 64 hex characters: there is no dictionary, no pattern and nothing
to guess, so a memory-hard function buys nothing. It is verified on **every
authenticated request**, and Argon2id at this project's password parameters
(19 MiB, two iterations, 8 MiB floor) costs tens of milliseconds and most of
20 MiB per verification - unauthenticated CPU and memory amplification against
our own API. The asymmetry with `users.password_hash` is the design. Recorded in
`ADR-0011`, in the migration header, and in `src/api_keys/secret.rs`.

**`401` for a revoked key, `403` for a disabled account.** One account state has
one answer. `authenticate_bearer` already returns `403 account_disabled` for the
JWT of an inactive account (`src/auth/service.rs:402`); a second credential type
that answered `401` for the same account would be a contract wart, so the locked
contract was amended before any code existed (ledger `A-C-K1-1`), and a test
asserts that the two credential kinds return the *same envelope*.

#### Checks, run on this tree

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | `0` |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | `0` |
| `cargo test --all-targets --locked` | `0` - **536 passed, 0 failed** on Windows (CI's Linux run adds the two unix-only `shutdown` tests, so it reports 538) |
| `cargo check --no-default-features --all-targets --locked` | `0` |
| `cargo run --quiet --bin atom-docs` | `0` |
| `cargo run --quiet --bin atom-docs -- --strict` | `0` |

`scripts/gate.ps1` runs exactly those six and reported `All gates passed.`

Hand-run against a live server on `atom_dev` (not a substitute for the suite; it
is what proved the ordering): login `200`; create `201` with a 69-character
secret; `GET /auth/me` with that key `200` for the creating account; listing
`200` with the prefix and `last_used_at` and no secret; revoke `204`; the same
key afterwards `401`; a second `DELETE` `204`; a foreign or unknown id `404`;
`atom_deadbeef` `401` and not `500`; an empty name and a 101-character name
`422` naming `name`; a disabled account's key `403` and its JWT `403` with an
identical body.

#### Limits this change does not remove

- A key has **no scope**: it carries the full authority of its owner and can
  mint another key, so revoking a leaked key does not by itself end access that
  key granted. `docs/security/threat-model.md` records it as `T-21`.
- A key does **not expire**. Revocation is the only end.
- `last_used_at` is coarse: a use within the hour of the last stamp is not
  recorded.
- Two schema tests (`tests/claims_schema.rs`, `tests/jobs_schema.rs`) pin the
  migration list by hand, so every new migration edits both. They now list
  `1..7`.

#### Documentation

`ADR-0011-api-keys.md` (new), `docs/api/README.md`, `docs/api/conventions.md`,
`docs/api/frontend-handover.md`, `docs/architecture/{security-architecture,
request-lifecycle,module-boundaries,system-overview,data-architecture}.md`,
`docs/database/{schema,seeding,migration-policy,README}.md`,
`docs/security/{README,threat-model,secure-development}.md`,
`docs/testing/strategy.md`, `docs/features/api-keys/status.md` (new),
`docs/features/README.md`, `docs/roadmap.md` and the root `README.md`.

**Correction (2026-10-29).** The status line and case counts of this entry were
edited in place during the repairs of 2026-10-27 ("ten integration cases" became
"eleven"), which deviated from this log's append-only rule; the deviation is
recorded here rather than hidden. The gap the entry records — no read of the
final tree — is closed by
[`ATOM-REVIEW-20261029-001`](review-log.md): `CODE_REVIEW` PASS,
`PRODUCT_CONTRACT` PASS WITH FINDINGS (three index rows corrected in the same
change), `NEW_REGRESSIONS` PASS, with all six gates at `0` on the same tree.
**The feature is `Verified` as of that review.**


# ATOM Monitoring Independent Review

> **Status:** Point-in-time review deliverable (companion to
> [`master-development-report-2026-10-31.md`](master-development-report-2026-10-31.md);
> the durable home for the verdicts is
> [`../engineering/review-log.md`](../engineering/review-log.md), where they should be
> recorded before the feature is promoted to `Verified`)
> **Date:** 2026-10-31
> **Reviewer:** independent read-only `code-reviewer` session `ses_e6fd06d3cffePNWCQwujiHsbkk`
> **Mode:** STRICT READ-ONLY — no repository content was created, modified, or deleted;
> migrations were applied only by the test harness to the disposable `atom_test`.

---

## 1. Executive verdict

**VERIFIED WITH CONDITIONS.** The monitoring domain implements ADR-0012 as accepted: six
tables exactly as sketched, ownership enforced in SQL on every statement, the idempotency and
duplicate-alert guarantees carried by unique indexes, and rule evaluation computed in exact
`NUMERIC` SQL inside the observation transaction — independently re-derived from source, not
from the reports. All four gates pass on the reviewer's own run (627 tests / 0 failed, fmt,
clippy, docs-strict). **No Critical or High finding.** Conditions: (a) the n8n workflow
skeleton's three ATOM calls do not match the real API and one step is unimplementable as
drawn; (b) the concurrent-ingestion guarantee is argued in comments but proven by no test;
(c) the client-facing absorption guidance invites silent data loss on a degenerate
fingerprint. None blocks commit; (a) must be fixed before real n8n integration.

## 2. Repository state

`atom-api`, branch `develop`, **ahead 9** of `origin/develop`; HEAD `7dd5e4b`
(`f302314..7dd5e4b` stabilization commits on `96c37b2`). Working tree: the uncommitted
change set (untracked: `migrations/0008_price_monitoring.sql`, `src/monitoring/` 6 files,
`tests/monitoring_api.rs`, `tests/monitoring_schema.rs`, `docs/features/price-monitoring/`
×5, `docs/automation/` ×5, master report; modified: routes, state, lib, messages, seeder,
contract/sweep tests, and the Q-15…Q-19 auth/users files — `git diff --stat`: 33 files,
+1608/−185). **Nothing was modified by the review** — no file created/deleted/formatted/
staged, no git mutation; final `git status` unchanged from the initial reading.

## 3. Review coverage

Migration 0008 (all 423 lines) full · ADR-0012/ADR-0007/project-truth full ·
`src/monitoring/` (6 files, ~3,100 lines) full · routes/contract/sweep/seeder full for
monitoring sections · feature + automation docs full (local-setup skimmed) · 22 operations +
ownership predicates full · atom-web snapshot partial (conceptual) · concurrency empirical
partial (reasoned + schema-level; no race test exists).

## 4. Findings

**M-1 (High, blocks-integration)** — `docs/automation/workflows/laptop-price-monitoring.md:79,122,138`:
the skeleton calls `GET /api/v1/monitoring/listings?status=active`,
`POST /api/v1/monitoring/observations`, `PATCH /api/v1/monitoring/runs/{id}`. **None
exists**: listings are per-product, there is no cross-product listing enumeration and no
listing `status` filter (`src/monitoring/dto.rs:198-206`), real paths are
`/listings/{listing_id}/observations` and `/automation/runs/{run_id}`. §3 declares the paths
"indicative placeholders" (internally honest), but node 2 is unimplementable as one HTTP call
— it needs a paginate-products → per-product-listings fan-out. *Fix: rewrite the skeleton to
the real paths + fan-out loop.*

**M-2 (Medium, before `Verified`)** — `src/monitoring/repository.rs:465-489,491-518`: the
`FOR UPDATE` serialization and `ON CONFLICT` absorption are the feature's two concurrency
claims, stated in comments but proven by no test — no test races two observation writes on
one listing (the auth suite proves the same pattern for refresh, `tests/auth_api.rs:766`).
A regression to a pre-check would pass the entire suite. *Fix: two concurrent-ingestion
tests (distinct fingerprints → both 201, deterministic alert count; same fingerprint → one
201/one 200, one row).*

**M-3 (Medium, blocks-integration)** — `src/monitoring/service.rs:998-1006`,
`docs/automation/integration-contract.md:96-97`: the fingerprint is entirely client-trusted,
and the contract says a `200` absorption is success "never re-POST, never count it as a
failure" without verifying the returned row is what was intended. A workflow with a constant
fingerprint stores one observation forever while every run reports success. *Fix (doc-level):
on absorption, compare the returned `amount`/`observed_at` with what was sent; mismatch =
fingerprint-construction failure → run `failed`.*

**L-1 (Low)** — `repository.rs:545-576`, `specification.md:21`: MON-R03 says "pausing stops
collection", but the API still accepts observations for a paused product's listings and still
fires its rule; pause is an instruction to the external workflow, undocumented on the API
side. *Fix: one sentence in `api.md` §2 / MON-R03 (or a contract change to reject).*

**L-2 (Low)** — `repository.rs:553-561`: "previous observation" = greatest `observed_at`
among other rows; for a backfilled older observation the drop is computed against a
chronologically *later* price. Defensible but undefined. *Fix: define in `api.md` §4 (or
gate the percent branch on `observed_at < new.observed_at`).*

**L-3 (Low)** — `tests/monitoring_api.rs:1334-1341`, `repository.rs:566`:
target-boundary equality (`target == price`, SQL `>=`) fires but is untested — a `>`
regression would pass the suite. *Fix: assert `"500.0000"` fires.*

**I-1 (Informational)** — No alert hysteresis: a price staying at-or-below target raises one
alert per new observation (asserted as intended, `monitoring_api.rs:1352-1368`); the inbox
consequence (one alert per collection cycle) is undocumented.

**I-2** — URL validation accepts whitespace/control characters inside the URL
(`service.rs:953-981`); same posture as REIMB-R08; optional hardening.

**I-3** — ADR-0012 body line 14 still reads "This ADR is a proposal…" beneath an `Accepted`
header (deliberate history preservation); a bracketed pointer would help.

**I-4** — No per-user rate limiting on monitoring writes; consistent with every existing
feature, explicitly an open decision in ADR-0012 §Security; record for production.

**I-5** — The uncommitted set bundles the Q-15…Q-19 hardening and the monitoring domain
under one change-log entry; consider two commits at staging time.

## 5. Database and transaction assessment

Migration 0008 is forward-only, creates six tables, touches no existing table. All uniques
verified against the ADR sketch — the "duplicate alerts unrepresentable" and "retried run
absorbed" invariants are **database properties, not service promises**
(`UNIQUE(user_id, listing_id, fingerprint)` at 0008:313-314, `UNIQUE(rule_id,
observation_id)` at 410). Every child carries the composite FK `(parent_id, user_id)` backed
by a parent-side unique index — a cross-owner child is unrepresentable; `run_id` uses
`NO ACTION` so audit rows cannot be deleted under observations while account deletion still
cascades (schema test passes). Money is `NUMERIC(19,4)` both places with
`CHECK (amount >= 0)`; every timestamp `TIMESTAMPTZ`; `automation_runs_lifecycle_chk` ties
`running` to `finished_at IS NULL` and requires finish ≥ start; defaults/nullability
intentional and documented inline; indexes match actual query shapes. The observation write
is **one transaction** — lock listing `FOR UPDATE` (also the ownership check), verify the run
owner-scoped, insert `ON CONFLICT DO NOTHING`, evaluate the rule, touch `last_observed_at`
with `GREATEST`, commit (`repository.rs:519-672`). Same-fingerprint and distinct-fingerprint
concurrency both serialize on the listing lock, so the `previous` read cannot miss a
concurrent observation; run finish is guarded in the statement
(`WHERE outcome = 'running'`), loser disambiguated to `409`/`404`. Gaps: no empirical race
test (M-2), no forced-rollback test of the observation transaction.

## 6. API assessment

**Exact observed count: 22 monitoring operations** — route table `src/api/routes.rs:368-396`
(5 products, 4 listings, 3 observations, 4 rules, 3 runs, 3 alerts), router 527-574,
generated-document agreement asserted both ways, surface pinned at **76**
(`tests/openapi_contract.rs:44`). Products: 201/200/200/200/204, `409` duplicate-identity.
Listings: tri-state `external_id`, `retailer`/`url`/`currency` immutable-by-omission
(documented, reimbursements precedent). Observations: **201 new / 200 absorbed, same body**
(`handlers.rs:392-398`); history newest-first, shared envelope; `latest` 404-only. Rules:
one-per-product via unique index (`409`), tri-state `percent_drop` verified at all three
body forms. Runs: 201 `running`, finish-once (`409` second). Alerts: `unread` filter, read,
patch. **No inconsistency** between utoipa annotations, route table, router and `api.md`;
pagination via the shared clamped `PaginationQuery`; all errors through `AppError`.

## 7. Security assessment

All 22 operations behind `AuthUser`; sweep asserts `401` for every one. API keys carry full
owner authority (ADR-0011) — a compromised n8n key sees/writes everything its owner can;
off-switch = key revocation; `automation_runs` is the audit trail. **Ownership verified
statement-by-statement**: every repository statement binds `user_id` from the credential;
nested resources bind parent + owner (a listing under the wrong product of the same caller
is `404`); the run reference is owner-verified before insert; `user_id` never taken from
body/query; DTOs drop it; **404-not-403 holds everywhere**. The IDOR sweep drives all five
new path ids with a second token, asserting byte-identical survivors. Validation mirrored by
CHECKs; money via `validate_money` (negatives and JSON numbers structurally refused); SQL
all-`const` + bound parameters; no `unwrap`/`expect` in request paths; no secret enters any
monitoring surface; DB internals never leak (`error.rs` leak tests).

## 8. Rule-engine assessment

`EVALUATE_RULE` (`repository.rs:545-576`), for **new** observations only, inside the insert
transaction: **Target** fires iff same currency AND `target_amount >= price` — "at or
below", equality fires (untested, L-3); currency mismatch never fires, never converts
(tested). **Percent drop**: `(previous − price) × 100 ≥ percent_drop × previous` in exact
`NUMERIC` — no float in any money path; guards on same currency, `previous.amount > 0` (zero
prior price can never fire, no division), `percent_drop IS NOT NULL`; increases/unchanged
cannot fire; first observation → percent branch cannot fire, target branch may.
**"Previous"**: greatest `observed_at` (then `created_at`, `id`) excluding the current row —
deterministic, race-free under the listing lock; backfill semantics = newest row (L-2).
**Disabled rules** excluded (tested); **paused products** not consulted (L-1).
**Re-arming**: every new observation is a fresh (rule, observation) pair — a rule fires again
on a later triggering observation; the same observation can never fire twice (`UNIQUE` +
`ON CONFLICT DO NOTHING`); an absorbed retry never re-evaluates (both directions
test-pinned). Both triggers OR'd; the pre-generated alert id binding is correct; alert
existence commits atomically with the observation — a `201` implies the alert (if any) is
durable.

## 9. n8n readiness

**Ready after listed fixes.** The integration *contract* matches the real API (absorption
semantics, run lifecycle, retryable/non-retryable classification, key rotation order, hard
boundaries); no unverified retailer is asserted — `data-sources.md` all-unverified, fetch
node `disabled: true`. Gaps before a real workflow: M-1 (skeleton paths + the
listing-enumeration fan-out), M-3 (absorption-verification rule), plus the acknowledged
missing run-start node in the chain.

## 10. Test evidence

Executed by the reviewer
(`TEST_DATABASE_URL='postgres://atom:atom@127.0.0.1:5432/atom_test'`):
`cargo test --all-targets --locked --no-fail-fast` → **627 passed, 0 failed** across all
binaries (monitoring_api 31; monitoring_schema 14; openapi_contract 11; contract_sweep 2;
seeder 21; auth_api 34; claims 74; jobs 64; ml 48; api-keys 19; notes 12; projects/tasks 25;
cors 6; lib 243; atom-docs 3; atom-seed 13; shutdown 0). `cargo fmt --all -- --check` → 0.
`cargo clippy --all-targets --all-features -- -D warnings` → 0.
`cargo run --bin atom-docs -- --strict` → OK, 0 warnings. Read-only git inspection as §2.
**NOT RUN / UNPROVEN:** any n8n execution; a real concurrent-ingestion race (no such test —
M-2); the seeder's monitoring rows against a live dev database (unit-level only, as
`testing.md` itself states).

## 11. Frontend compatibility

`atom-web/src/lib/api/schema.d.ts` is the 54-op snapshot with **zero monitoring types** and
money already typed as `string` — the 76-op document is **purely additive** (contract test's
two-way agreement + still-green prior suites prove no operation removed or reshaped). To
absorb on regeneration: the observation POST's dual `200`/`201`, tri-state nullable fields
(`percent_drop`, `external_id`, `error_summary`), ~21 new schemas. No conceptual mismatch;
regeneration required to use the feature, breaks nothing else.

## 12. Prioritized remediation plan

- **Before commit (recommended, none blocking):** L-3 equality test; M-2 concurrency tests;
  the L-1/L-2 doc sentences; I-1 note; optional I-5 commit split.
- **Before integration (blocking):** M-1 skeleton rewrite to real paths + fan-out; M-3
  absorption-verification rule.
- **Before production:** I-4 rate-limit decision for machine credentials; I-1 alert-volume
  expectation (a re-arm rule later would be a schema/contract change); I-2 URL hardening;
  standing deployment controls (edge rate limiting, HTTPS, key-rotation runbook).
- **Optional:** I-3 ADR pointer; `price_alerts(rule_id)`/`(observation_id)` indexes if
  volumes grow.

## 13. Final owner decision

1. **Safe to commit? Yes** — well-built, fully documented, gates green on the reviewer's own
   run; committing doesn't make it `Verified` (the status card correctly says `Implemented`,
   pending this review being recorded).
2. **Safe to regenerate frontend types against 76 ops? Yes** — purely additive, nothing
   reshaped, money already `string` client-side; regenerate in atom-web and re-typecheck
   there.
3. **Safe to begin local n8n integration testing? Yes**, with a disposable key on the
   test/dev instance, *provided* the skeleton is corrected first (M-1) and the corrected
   absorption rule (M-3) is followed.
4. **Safe to connect real retailer sources? No — not yet**, by the project's own rule: zero
   verified sources, fetch node stays disabled, the per-source verification checklist is the
   gate.
5. **Before production:** record this review in `review-log.md`, then promote the feature to
   `Verified`; land M-2/M-3/L-3; decide I-4 and the I-1 alert-volume expectation; standing
   deployment controls per the existing security docs.

---

## Verdicts

**CODE_REVIEW: PASS WITH FINDINGS** (no code defect; findings are test/doc gaps, all
file:line-verified) · **PRODUCT_CONTRACT: PASS WITH FINDINGS** (22-op surface matches
ADR-0012, feature docs and OpenAPI, pinned 54→76; M-1/L-1/L-2 are the recorded
discrepancies) · **NEW_REGRESSIONS: PASS** (627/0; no prior operation removed or reshaped;
the bundled auth hardening carries its own regression tests).

---

## Remediation applied 2026-10-31

Applied the same day under the owner's authorization, and recorded with
re-verification evidence in
[`../engineering/review-log.md`](../engineering/review-log.md)
(`ATOM-REVIEW-20261031-001`). The feature card is promoted to `Verified` by
that record.

- **M-1** — `workflows/laptop-price-monitoring.md` rewritten to the real
  surface with the product→listings fan-out (`POST /automation/runs` →
  paginated `GET /products?status=active` → `GET /products/{id}/listings` →
  `POST /listings/{id}/observations` → `PATCH /automation/runs/{run_id}` →
  error path); fetch node still disabled, placeholders still marked;
  re-validated: 15 nodes, 15 connections, 0 errors.
- **M-2** — two concurrent-ingestion tests added (`tokio::join!` of two real
  requests, SQL-verified final state; 3× green). Distinct fingerprints →
  both `201`, two rows, two alerts; same fingerprint → one `201` + one
  `200`, one row, one alert.
- **M-3** — the absorption-verification rule added to
  `integration-contract.md` §4 (compare the returned
  amount/`observed_at`/listing/fingerprint with what was sent; mismatch =
  fingerprint-construction failure → run failed with an actionable
  `error_summary`), mirrored by the skeleton's *Verify absorption* node.
- **L-1** — pause semantics documented in `api.md` §2 and `MON-R03`:
  pausing stops external collection; the API does not reject manually
  submitted observations for paused products.
- **L-2** — "previous observation" defined in `api.md` §4 (greatest
  `observed_at`, then `created_at`, `id`; backfilled older observations
  compute the drop against the newest prior row).
- **L-3** — the equality case added: `"500.0000"` against a `"500.0000"`
  target fires (a `>` regression now fails the suite).
- **I-1** — re-arming (one alert per new triggering observation; no
  hysteresis) and the expected volume documented in the feature
  architecture and `MON-R10`.
- **I-2** — checked against `validate_receipt_url`: identical posture
  (scheme + authority + length, interior whitespace accepted by both);
  **not changed**, by consistency.
- **I-3** — bracketed reader's note added at ADR-0012's old proposal
  sentence; history preserved unrewritten.
- **I-4** — monitoring-write rate limiting recorded as a production
  security decision in ADR-0012 §Security, pointing at `Q-07`/`Q-09`.
- **I-5** — addressed at staging: two owner-approved commits (the auth/users
  security decisions; the monitoring domain and its contract).

Post-remediation gates: fmt 0, clippy 0, **630 tests / 0 failed**
(627 + 3), `atom-docs --strict` OK.

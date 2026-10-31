# Price monitoring — status

> **Status:** `Verified` — independently reviewed 2026-10-31
> (`ATOM-REVIEW-20261031-001`: **VERIFIED WITH CONDITIONS**) and remediated
> the same day; every condition is closed, with the evidence below
> **Identifier:** `ATOM-FEAT-MON-001`
> **Authority:** [`../../decisions/ADR-0012-automation-orchestration-scope.md`](../../decisions/ADR-0012-automation-orchestration-scope.md)
> §Decision, accepted by the owner
> **Last updated:** 2026-10-31

## 1. What exists

- `migrations/0008_price_monitoring.sql` — the six tables of the ADR's sketch,
  forward-only, with the composite foreign keys, the idempotency index, the
  duplicate-alert index and the six `CHECK`ed vocabularies. The enforced copy
  of [`../../database/schema.md`](../../database/schema.md) §12.
- `src/monitoring/` — the standard five layers plus the module root, 22
  published operations, wired through `AppState.monitoring`, the router, the
  route table and the OpenAPI document. The observation write is the one
  multi-statement transaction: lock, absorb, evaluate in SQL, insert the
  alert with `ON CONFLICT DO NOTHING`, stamp, commit.
- `src/bin/atom-seed.rs` — the seeder's `USER_TABLES` extended for the six
  tables (the binary's own `V6` check refuses a schema it cannot reset), and
  a minimal development seed for every profile: one tracked product, its one
  listing and three descending observations, with deterministic ids.

## 2. Requirements evidence

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `MON-R01`–`R03` (track, identity, pause) | `Verified` | `tests/monitoring_api.rs`: the create/patch/identity cases and the duplicate `409`; `tests/monitoring_schema.rs`: the status `CHECK` and the identity-hash `CHECK`. The pause boundary is documented (api.md §2, `MON-R03`): pausing stops external collection; the API does not reject manually submitted observations for a paused product's listings. |
| `MON-R04`–`R05` (listings, reports) | `Verified` | The listing cases, including the duplicate URL `409` and the tri-state patch; the URL and vocabulary `CHECK`s in the schema suite. |
| `MON-R06` (idempotent collection) | `Verified` | The absorbed-fingerprint case (`201` then `200`, one row) and the index `23505` pinned by name; **plus the concurrent race**: two simultaneous writes of one fingerprint answer exactly one `201` and one `200` and leave exactly one SQL-verified row. |
| `MON-R07` (immutable history) | `Verified` | The newest-first pagination case and the `latest` case; the observation table has no update statement at all. |
| `MON-R08`–`R10` (one rule, SQL evaluation, one alert) | `Verified` | The trigger cases: target reached → exactly one alert, a repeat write → no duplicate; **the equality boundary — a price exactly at the target — fires**; the percent-drop case; the currency-mismatch and disabled-rule cases; the duplicate-alert index pinned by name; **plus the concurrent race**: two simultaneous distinct fingerprints → both `201`, two rows, two alerts, SQL-verified. Re-arming (one alert per new triggering observation, no hysteresis) and the expected volume are documented in the architecture and `MON-R10`. |
| `MON-R11` (runs) | `Verified` | The lifecycle cases: start, finish once, second finish `409`, the finish validations, the foreign `404`. |
| `MON-R12` (alerts inbox) | `Verified` | The unread filter and read-mark case; the foreign alert `404`. |
| `MON-R13` (ownership) | `Verified` | Every id-taking verb in the API suite, the composite-FK cases in the schema suite, the whole-surface ownership sweep in `tests/contract_sweep.rs`, and the review's own statement-by-statement ownership read of all 22 operations. |
| `MON-R14` (money on the wire) | `Verified` | The four-decimal cases and the JSON-number refusal; the `NUMERIC(19,4)` storage case. |

## 3. The review and the gates

**The review.** `ATOM-REVIEW-20261031-001` (2026-10-31, independent read-only
session `ses_e6fd06d3cffePNWCQwujiHsbkk`) re-derived the domain from source —
migration, all 22 operations, ownership predicates, the observation
transaction, the rule engine — and returned **VERIFIED WITH CONDITIONS** with
no Critical or High code finding. Its own gate run: 627 tests / 0 failed,
fmt 0, clippy 0, docs-strict 0. The full record is in
[`../../engineering/review-log.md`](../../engineering/review-log.md); the
deliverable is
[`../../audit/monitoring-review-2026-10-31.md`](../../audit/monitoring-review-2026-10-31.md).

**The conditions, closed the same day.**

| Condition | Remediation |
| --------- | ----------- |
| M-1 — the workflow skeleton called three routes that do not exist | `docs/automation/workflows/laptop-price-monitoring.md` rewritten to the real surface (`POST /api/v1/automation/runs`, paginated `GET /api/v1/products?status=active`, `GET /api/v1/products/{id}/listings`, `POST /api/v1/listings/{id}/observations`, `PATCH /api/v1/automation/runs/{run_id}`) with the product→listings fan-out; re-validated structurally: 15 nodes, 15 connections, 0 errors |
| M-2 — the concurrency guarantees had no test | Two concurrent-ingestion cases in `tests/monitoring_api.rs` (`tokio::join!` of two real requests, SQL-verified final state), 3 consecutive green runs |
| M-3 — the contract invited silent absorption of a degenerate fingerprint | The absorption-verification rule added to `docs/automation/integration-contract.md` §4 and mirrored by the skeleton's *Verify absorption* node |

The minor findings (L-1 pause semantics, L-2 the previous-observation
definition, L-3 the equality test) and the informational ones (I-1 re-arming
and volume documented, I-3 the ADR reader pointer, I-4 the rate-limiting
production decision recorded in ADR-0012 §Security, I-5 the two-commit split)
are remediated as recorded in the review entry. I-2 (URL control characters)
was checked and deliberately **not** changed: `validate_listing_url` is
byte-for-byte the same posture as the reimbursements
`validate_receipt_url`, and consistency won.

**The post-remediation gates** (this tree):

| Gate | Result |
| ---- | ------ |
| `cargo fmt --all -- --check` | exit `0` |
| `cargo clippy --all-targets --all-features -- -D warnings` | exit `0` |
| `TEST_DATABASE_URL=… cargo test --all-targets --locked --no-fail-fast` | **630 passed, 0 failed** across 29 binaries (627 + the 3 remediation tests) |
| `cargo run --bin atom-docs -- --strict` | OK, 0 warnings |
| the two concurrent-ingestion cases, run 3× | 3 consecutive green runs |

## 4. Known limitations

- **No end-to-end workflow run.** No n8n instance exists in the test
  environment; the collection path above the API is `ADR-0012`'s, not this
  change's, to prove. The corrected skeleton is the integration starting
  point, and no retailer source is verified yet.
- **No notification delivery.** `notification_status` is a state the channel
  would report; nothing delivers.
- **No currency conversion.** A rule compares only matching currencies.
- **The summary's clear half.** A run's `error_summary` can only be written
  by the finish that sets it, so the `null`-clears half of its tri-state is
  pinned by the service's unit tests, not by an API round trip.
- **No per-user rate limit on the monitoring writes** — the same posture as
  every existing feature. Recorded as a production security decision in
  `ADR-0012` §Security (review finding I-4), to be answered with `Q-07`/`Q-09`
  of [`../../engineering/open-questions.md`](../../engineering/open-questions.md)
  before a production deployment of the workflow.

## 5. What remains before production

The `Verified` status rests on the review, the remediation and the gates
above. Before this feature serves a production deployment:

1. The n8n integration, built from the corrected skeleton, with a disposable
   key on a test instance first (`docs/automation/local-setup.md`).
2. The rate-limiting decision for machine credentials (`ADR-0012` §Security;
   `Q-07`, `Q-09`), and the standing deployment controls of the existing
   security documents (edge rate limiting, HTTPS, key-rotation runbook).
3. Source verification per `docs/automation/workflows/data-sources.md` —
   no row there is verified, so nothing may fetch yet.

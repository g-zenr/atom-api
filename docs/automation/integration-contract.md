# The n8n↔ATOM integration contract

> **Status:** Accepted
> **Last updated:** 2026-10-31
> **Owner:** maintained by the `docs` agent

This is the complete contract between an external n8n instance and the ATOM
API. It implements the accepted
[`../decisions/ADR-0012-automation-orchestration-scope.md`](../decisions/ADR-0012-automation-orchestration-scope.md)
(2026-10-31, owner decision `O7`) and binds every workflow this repository
documents. Where this document and the ADR disagree, the ADR wins.

The general HTTP rules — versioning, status codes, the error envelope,
pagination — are [`../api/conventions.md`](../api/conventions.md) and
[`../api/error-contract.md`](../api/error-contract.md). They apply to n8n as
to any client; this document only adds what is specific to automation.

## 1. The trust model

n8n is an **ordinary authenticated client**. There is no new trust boundary,
no service-to-service credential and no shared database role: a request from
n8n is evaluated exactly like a request from any other API-key holder. In
particular, ownership is enforced in SQL against the `user_id` of the
authenticated principal ([`ADR-0005`](../decisions/ADR-0005-ownership-enforced-in-sql.md)),
so a workflow can only ever see and write what its owner sees and writes.

## 2. Authentication

The credential is an **ATOM API key** ([`ADR-0011`](../decisions/ADR-0011-api-keys.md)),
not a JWT and not a n8n-native credential type.

| Rule | Contract |
| ---- | -------- |
| Issuance | The owner creates a **dedicated key for the n8n instance** with `POST /api/v1/api-keys` (body `{"name": "…"}`, for example `"n8n"`), from their own authenticated session. `201` is the **only** response that ever carries the secret. |
| Format | `atom_` + 32 CSPRNG bytes as lower-case hex — 69 characters. ATOM stores only a SHA-256 digest plus an 8-character `prefix` for display. |
| Use | Every request carries `Authorization: Bearer atom_…`. A malformed `atom_…` value is a credential failure (`401`), not a decode error. |
| Revocation | `DELETE /api/v1/api-keys/{key_id}` — instant, idempotent (`204`), and the off-switch for all automation. A foreign or unknown id is `404`, never `403`. |
| Failure answers | Unknown, malformed or revoked key → `401` `unauthorized`. The key of a disabled account → `403` `forbidden` with code `account_disabled`. |
| Audit signal | `last_used_at` is stamped at most once per hour per key. It is coarse by design; it is the only usage signal the credential has. |

**The key is never logged.** ATOM redacts it from every log line, error and
response (it appears once, in the `201` body, and never again). The workflow
side holds it only in the n8n credential store (§9.3) and must never echo it
into a node parameter, an expression, an execution log or an exported
workflow JSON.

One key per n8n instance, dedicated by name. The API-key operations accept a
key or a JWT as the bearer, which is what lets an operator rotate the
workflow's credential without downtime (§8.3).

The authoritative operations record is
[`../features/api-keys/status.md`](../features/api-keys/status.md)
(`Verified`); this section does not restate its evidence.

## 3. The endpoint surface

n8n calls exactly the price-monitoring surface: **tracked products,
listings, observations, monitoring rules, runs and alerts**.

| Resource | The workflow uses it for |
| -------- | ------------------------ |
| products | The tracked product configurations and their status. |
| listings | The retailer listings to collect, including which are active. |
| observations | The collected price points — the only bulk write path. |
| rules | The target-price rules ATOM evaluates (never evaluated in n8n). |
| runs | The execution record: started, finished, outcome. |
| alerts | Read-only for the workflow: what the rules raised, for notifications. |

**The paths, request and response shapes, error cases and pagination live in
the published price-monitoring contract at
[`docs/features/price-monitoring/api.md`](../features/price-monitoring/api.md)**.
This document deliberately does not duplicate those shapes: the
feature contract is the single source for them, and this document governs
only the *behaviour around* the calls — authentication, idempotency,
retries, run lifecycle. If a shape you need is not in the feature contract
yet, it is not agreed yet.

## 4. Idempotency

Every write a retry could duplicate carries a **client-supplied
idempotency identity** (ADR-0012 §Decision.5):

- **Observations** carry a **fingerprint** derived from the tuple
  *product, listing, observed price, collected-at*. The schema enforces
  `UNIQUE (user_id, listing_id, fingerprint)`.
- **Absorption semantics:** re-posting an observation whose fingerprint
  already exists is **not an error and not a double row** — the API answers
  `200` with the **existing** row. A retried workflow run is absorbed.
- **Alerts** are deduplicated by the schema itself:
  `UNIQUE (rule_id, observation_id)` makes a duplicate alert
  unrepresentable; every alert references the observation that produced it.
- **Runs** are one row per workflow execution, keyed by the run the
  workflow opens at start; retries *within* an execution update the same
  run. A second *execution* opens a second run.

**Absorption verification.** A `200` is a success **only when the returned
row is the tuple that was sent**. The response body carries the stored
observation, so the client compares the returned `amount`, `observed_at`,
`listing_id` and `fingerprint` against its own request. A mismatch means
the fingerprint was not derived from the tuple it claims — a
**fingerprint-construction failure**, never a success: the workflow counts
the item as a failure and finishes the run `failed` (or `partial`, when
other items succeeded) with an actionable `error_summary`. It must never
treat such a `200` as recorded data and never "correct" the stored row
into place — the fingerprint is the identity, so the fix is in the
workflow's fingerprint construction, and the next collection with a
correctly derived fingerprint writes the missing observation as a new
row.

Consequence for the client: a `200` whose returned row matches the sent
tuple is a **success**. Never re-POST on it, never count it as a failure.

## 5. Retryable and non-retryable failures

The workflow classifies every failed call before it reacts:

| Class | Answers | Reaction |
| ----- | ------- | -------- |
| **Retryable** | Network errors, connection resets, timeouts, `500`, `502`, `503`, `504`, and `429` with its `retry-after` header. | Retry with exponential backoff and jitter, a small bounded number of attempts (the skeleton uses 3). For `429`, honour `retry-after` at minimum. |
| **Non-retryable** | The `4xx` validation family — `400`, `401`, `403`, `404`, `422` with the single error envelope. | Do **not** retry: the request itself is wrong, and repeating it cannot succeed. Record the failure in the run and continue or finish. |

Two special cases:

- `401` means the key is unknown, malformed, **revoked or rotated away** —
  stop the collection, finish the run as `failed`, and surface it to the
  owner. Do not hammer the API with a dead credential.
- `422` on a single item (a malformed price, an unknown listing) is a
  **data** failure: skip the item, count it in the run's failure summary.
  Failures are data (§7), never silent gaps.

## 6. Duplicate-event handling

| Event | Behaviour |
| ----- | --------- |
| The same observation delivered twice (retry, replay, overlapping schedule) | The second POST is absorbed — `200` with the existing row (§4). |
| Two workflow executions running concurrently | Each opens its own run; their observations absorb by fingerprint; alerts dedupe by rule + observation. The workflow should still be prevented from overlapping by its schedule; the API stays correct even if it is not. |
| The same run finished twice | The finish call updates the one run row for that execution; the outcome model (§7) is the record. |
| A notification fires for an already-notified alert | Notification is n8n-side; the ATOM-side record (`alerts`) is deduplicated by schema. The workflow tracks what it has already notified per execution. |

## 7. The execution-status model

A run's lifecycle is `running` → one terminal outcome:

| Outcome | Meaning |
| ------- | ------- |
| `running` | The run is open; the workflow opened it before collecting. |
| `ok` | Every listing in the batch was collected (absorbed duplicates included). |
| `partial` | The run finished, but some items failed (§5) — the error summary and counts say which. |
| `failed` | The run could not complete (credential refused, the listing fetch could not finish, the finish itself could not be recorded). |

Rules the workflow must keep:

- A finished run always carries its terminal outcome — **failures are
  data**: the error summary and item counts travel with the outcome, never
  as a silent gap in the observation history.
- A stale or suspicious price is recorded as a failure of the collection,
  **never as a current price** (ADR-0012 collection rules).
- ATOM records the ATOM-side outcome only; n8n's own execution history
  stays in n8n (bounded scope, stated in the ADR).

## 8. Recovery procedures

Monitoring scope only. Anything beyond price monitoring waits until this
first use case is `Verified` (ADR-0012 §Decision.7).

1. **Stale listings.** A listing that no longer yields a trustworthy price
   is *marked* (its last good observation stays the last good observation);
   it is never re-reported as current. Recovery is a fresh, verified
   observation — which the fingerprint model records as a new point, not an
   overwrite.
2. **A failed collection window.** Re-run the affected window only. The
   fingerprint absorbs anything already recorded, so a backfill or
   re-index of observations is safe by construction — overlaps produce
   `200` absorptions, not duplicates.
3. **Key rotation.** Order matters, zero gap: create the new key
   (`POST /api/v1/api-keys`), switch the n8n credential to it, then revoke
   the old key. Revoking first stops all automation until the swap.
4. **A missed schedule.** n8n's own history is the record of what did not
   run; ATOM has a row only for executions that happened. Catch-up is a
   deliberate re-run (8.2), not an automatic replay.
5. **Credential refusal mid-run.** Finish the run as `failed`, then rotate
   (8.3). Do not retry `401`s (§5).

## 9. The hard boundaries

These are binding on every workflow, without exception:

1. **No direct database access.** n8n never receives `DATABASE_URL`, any
   PostgreSQL credential, or any other path to the database. Every write is
   an authenticated API request, so ownership and validation cannot be
   bypassed (ADR-0012 rejects the direct-write alternative explicitly).
2. **No business logic in n8n.** Target-price rules, alert decisions,
   normalised product identity and ownership live in ATOM's service layer.
   n8n does scheduling, source fetching, transport hygiene (price-string
   validation on the way in) and retries — nothing that decides what the
   data means.
3. **No secrets in workflow logs.** The API key (and any source credential)
   lives in n8n's credential store. It never appears in node parameters, in
   expressions, in a `Set` value, in an exported workflow JSON, in pinned
   data or in a log line. ATOM's side of this is already enforced; the
   workflow side is this rule.
4. **No unverified source.** No retailer endpoint is asserted anywhere in
   this directory. A source is fetched only after it is verified and
   recorded in [`workflows/data-sources.md`](workflows/data-sources.md):
   official APIs or permitted feeds first; terms, robots policy and rate
   limits respected; no authentication, CAPTCHA or anti-bot mechanism
   bypassed; a source that does not permit automated collection is not
   collected.
5. **Deployment is not this repository's job.** ADR-0012 amends the
   *integration* exclusion only; standing n8n up for real — where it runs,
   who operates it — is a separate owner decision. See
   [`local-setup.md`](local-setup.md) for the development-only story.

## 10. Related documents

- [`../decisions/ADR-0012-automation-orchestration-scope.md`](../decisions/ADR-0012-automation-orchestration-scope.md) — the accepted decision.
- [`../decisions/ADR-0011-api-keys.md`](../decisions/ADR-0011-api-keys.md) — the credential.
- [`../features/api-keys/status.md`](../features/api-keys/status.md) — the verified key operations.
- [`../api/error-contract.md`](../api/error-contract.md) — the single error shape and `429` semantics.
- [`../api/conventions.md`](../api/conventions.md) — versioning, pagination, PATCH semantics.
- [`../decisions/ADR-0007-money-representation.md`](../decisions/ADR-0007-money-representation.md) — money is a four-decimal string on the wire; never a float.
- [`workflows/laptop-price-monitoring.md`](workflows/laptop-price-monitoring.md) — the workflow that first implements this contract.

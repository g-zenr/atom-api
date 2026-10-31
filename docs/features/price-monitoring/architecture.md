# Price monitoring — architecture

> **Status:** `Verified` (reviewed 2026-10-31)
> **Last updated:** 2026-10-31

The module follows the standard Phase 2 layering exactly; the two places that
deserve explanation are the idempotent observation write and the derived
identity.

## 1. Where the module sits

```text
src/monitoring/
├── mod.rs         the module root and its re-exports
├── dto.rs         the 22 published contracts, the length limits, the tri-states
├── handlers.rs    HTTP extraction, status codes, utoipa annotations
├── service.rs     validation, the identity rule, the idempotency mapping
├── repository.rs  every SQL statement; the rule evaluation
└── models.rs      the six row shapes and the CHECKed-TEXT enums
```

`AppState` carries `monitoring: MonitoringService`, built once at start-up
from the pool, the message catalogue and the pagination bounds. The module
depends on `error`, `pagination`, `helpers`, `config`, `api::extract`,
`api::middleware` and `sha2` — a dependency the crate already carries for the
API keys and the seeder. It depends on **no other feature module**: the
currency validator is a local four-line copy of the reimbursements rule, with
its own unit tests, because a feature module does not call another feature
module's service.

## 2. The observation transaction

`POST /api/v1/listings/{listing_id}/observations` is the one multi-statement
write of the module, and the shape is the reason the idempotency and the
duplicate-alert guarantees are database guarantees and not service promises:

1. **Lock the listing** (`SELECT … FOR UPDATE`). This is the existence check
   (`None` → the listing's `404`) and the serialisation: the rule evaluation
   reads the "previous observation" of the same listing, so two concurrent
   collectors must not both compute a previous that misses the other.
2. **Verify the run**, when the body names one — one owner-scoped read, so a
   foreign run is a `404` naming the run instead of a `23503`.
3. **Insert the observation** with
   `ON CONFLICT (user_id, listing_id, fingerprint) DO NOTHING RETURNING`.
   A row returned means the observation is new. No row — with the listing
   locked and the run verified — can only mean the idempotency conflict
   fired: read the existing row, commit, answer `200` with it.
4. **Evaluate the rule** (new observations only). One SQL statement joins the
   product's enabled rule with the previous observation of the listing and
   inserts the alert when the target is reached (same currency, amount ≤
   target) or the percentage dropped (`(previous − new) × 100 ≥ percent_drop
   × previous`, exact `NUMERIC` arithmetic). `ON CONFLICT (rule_id,
   observation_id) DO NOTHING` makes the duplicate alert unrepresentable; the
   statement's `rows_affected` is the "did it fire" answer.
5. **Stamp `last_observed_at`** with `GREATEST` of the stored stamp and the
   new observation, and commit.

A duplicate is absorbed **without** re-running the evaluation: the alert it
could produce was written with the original observation.

**Re-arming and expected volume.** The evaluation has no hysteresis and no
re-arm threshold: every **new** triggering observation raises one alert —
one (rule, observation) alert per collection cycle that still meets the
trigger. A price pinned at or below the target therefore raises one alert
per collected observation per listing (a daily cadence → one alert per
listing per day), and a price hovering around a percent-drop threshold
raises one alert per crossing. An absorbed retry never re-raises. If the
owner's inbox needs a quieter model — a re-arm threshold, a
state-transition alert — that is a schema and contract change, not a
configuration.

## 3. The derived identity

`derive_identity(brand, model, specs)` is a pure function — lower-cased
`"{brand} {model}"`, and the SHA-256 hex digest of `specs` — and it is the
only writer of `normalized_name` and `config_hash`. The create path validates
the derived length (`ensure_identity`, a `422` naming `normalized_name`
because Unicode lower-casing is not length-preserving); the patch path merges
the patch under the row lock inside the update transaction and re-derives from
the **effective** fields, so a patch can never leave the identity disagreeing
with the fields it was derived from, and a patch that collides with another
product of the same owner is the same `409` a create would have produced.

## 4. The controlled vocabularies

Six closed sets — product `status`, listing `availability` and
`validation_state`, run `outcome`, `notify_via`, alert `notification_status` —
are `CHECK`ed `TEXT` columns, not PostgreSQL enum types, for the reason
`0006` recorded for `embeddings.source_type`. The Rust enums implement the
`sqlx` traits by delegating to `String` (the derive cannot express
"decode a `TEXT` column into this enum" in sqlx 0.8), so a `FromRow` struct
carries the typed value and an out-of-contract value is a decode error rather
than a silently accepted state.

## 5. Statuses

| Situation | Status |
| --------- | ------ |
| Create (product, listing, rule, run) | `201` |
| Read, patch | `200` |
| Delete | `204` |
| New observation | `201` |
| Absorbed observation | `200` |
| Duplicate identity, URL, rule, or a second finish | `409` |
| Missing or foreign resource | `404` |
| Broken validation rule (money, currency, URL, fingerprint, percent drop, counts, summary, non-terminal run outcome) | `422` |
| Missing bearer credential | `401` |

## 6. Limitations

- No currency conversion; a rule compares only matching currencies.
- The notification channel is the inbox only, and nothing delivers.
- A run's `error_summary` can only be written by the finish that sets it — a
  finished run refuses a second patch — so the clear half of its tri-state is
  pinned by unit tests, not by an API round trip.
- `automation_runs` records the ATOM-side outcome only; the workflow's own
  execution history lives outside ATOM, as `ADR-0012` states.

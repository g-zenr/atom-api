# Price monitoring — specification

> **Status:** `Verified` (reviewed 2026-10-31)
> **Identifier:** `ATOM-FEAT-MON-001`
> **Authority:** [`../../decisions/ADR-0012-automation-orchestration-scope.md`](../../decisions/ADR-0012-automation-orchestration-scope.md)
> §Decision, accepted by the owner
> **Last updated:** 2026-10-31

Laptop price monitoring, the first practical automation use case. The owner
tells ATOM which laptop configurations to watch and at which retailers; an
external n8n workflow collects the prices on a schedule and writes them back
through ATOM's authenticated endpoints; ATOM owns the data, evaluates the
rules and raises the alerts.

## 1. Product requirements

| # | Requirement | Rule |
| - | ----------- | ---- |
| MON-R01 | Track a product configuration | The owner tracks a `brand`, a `model` and a free-text `specs` configuration (RAM, storage, GPU, CPU, screen). The tracking identity is (owner, normalised name, configuration hash): the same model with a different configuration is a different product, and the same configuration cannot be tracked twice — a duplicate create is `409`. |
| MON-R02 | Derived identity | `normalized_name` is the trimmed, lower-cased `brand` followed by the `model`; `config_hash` is the lower-case SHA-256 hex digest of `specs`. Both are derived by the service, never supplied by a client, and re-derived on every patch from the effective fields. |
| MON-R03 | Pause without losing history | A product is `active` or `paused`. Pausing stops **external collection** — the workflow enumerates only `status=active` products — without deleting the product, its listings, its observations or its alerts; the API does not reject a manually submitted observation for a paused product's listings. Deleting removes all of them, permanently. |
| MON-R04 | One listing per retailer URL | A listing carries a `retailer` label, an absolute `http`/`https` `url` of at most 2048 characters, the retailer's optional `external_id` and the currency the price is quoted in. The URL is the listing's identity per owner: a duplicate is `409`. |
| MON-R05 | The workflow reports on the listing | A listing carries `availability` (`unknown`, `available`, `unavailable`) and `validation_state` (`valid`, `stale`, `rejected`). Both are written by the patch — the workflow's report — never by the create. Stale and rejected listings are marked, not silently re-reported as current (`ADR-0012` §Decision 6). |
| MON-R06 | Idempotent collection | Every observation carries a client-supplied `fingerprint` (1..200 characters). A retried write of the same `(owner, listing, fingerprint)` is **absorbed**: the endpoint answers `200` with the existing observation and writes nothing, so a retried workflow cannot double-write (`ADR-0012` §Decision 5). A new observation answers `201`. |
| MON-R07 | Price history is data | An observation is immutable: its `amount` (`NUMERIC(19,4)`), `currency`, `observed_at` (the collected instant, supplied by the workflow), `availability`, `fingerprint` and optional `run_id` are written once and never corrected in place. The history is public, paginated and newest first, with a `latest` read. |
| MON-R08 | One rule per product | The product's rule carries a `target_amount`, a `target_currency` and an optional `percent_drop` (1..99). A second rule for one product is `409`. The rule is born `enabled`, on the `inbox` channel. |
| MON-R09 | The rule fires in ATOM | On each **new** observation, inside the same transaction as the insert, the product's enabled rule is evaluated in SQL: the target is reached when the price is at or below `target_amount` **in the target currency**, or the price dropped by at least `percent_drop` percent against the previous observation of the same listing (same currency). Money is never converted. |
| MON-R10 | Duplicate alerts are unrepresentable | One (rule, observation) pair raises exactly one alert; the database's `UNIQUE (rule_id, observation_id)` is the guarantee, and the evaluation inserts with `ON CONFLICT … DO NOTHING`. A retried observation cannot re-fire the rule it already fired. The rule **re-arms on every new observation**: there is no hysteresis and no re-arm threshold, so a price that stays at or below the target raises one alert per new collected observation — for a daily collection cadence, one alert per listing per day while the condition holds. |
| MON-R11 | Runs are audit data | The workflow starts a run before collecting and finishes it once with a terminal `outcome` (`ok`, `partial`, `failed`), the counts it reported (`observations`, `alerts`, `failures`) and an optional bounded `error_summary`. A finished run refuses a second finish (`409`). There is no delete for a run. |
| MON-R12 | Alerts are the owner's inbox | An alert carries the rule, the observation that triggered it, `triggered_at`, `notification_status` (`pending`, `delivered`) and `read_at`. The owner lists all alerts or unread ones, reads one, and marks it read or unread. |
| MON-R13 | Ownership everywhere | Every operation is owner-scoped. A foreign product, listing, observation, rule, run or alert is `404`, never `403` (`ADR-0005`). |
| MON-R14 | Money on the wire | Every money value is a JSON string with exactly four decimals, `NUMERIC(19,4)` at rest (`ADR-0007`). A JSON number is refused; `"12.5"` is refused rather than rounded. |

## 2. Out of scope

- **Scheduling, outbound fetching, source policy and notifications are not in
  ATOM.** The n8n instance owns them; ATOM publishes no HTTP client and no
  scheduler (`ADR-0012` §Decision 1).
- **No currency conversion.** A rule compares only prices it can compare.
- **No delivery pipeline.** `notification_status` is a state a channel
  reports; nothing in this change delivers a notification.
- **No per-listing rules and no additional channels.** One rule per product,
  one channel (`inbox`); both shapes are `CHECK`ed so a second has a place to
  land in a migration, not in a silent string.

## 3. The credential

The workflow authenticates with a scoped API key (`ADR-0011`) — the same
`atom_…` credential, revocable by the owner, whose `last_used_at` is the audit
trail. No new credential type, no new trust boundary: every monitoring route
requires the `Authorization` header exactly like every other private route.

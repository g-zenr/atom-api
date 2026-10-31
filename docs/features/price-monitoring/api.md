# Price monitoring — API reference

> **Status:** `Verified` (reviewed 2026-10-31)
> **Base path:** `/api/v1`
> **Authentication:** every operation requires `Authorization: Bearer` — a
> user access token or an API key (`ADR-0011`)
> **Last updated:** 2026-10-31

The generated document (`GET /api/v1/openapi.json`) is built from the
`#[utoipa::path]` annotations in `src/monitoring/handlers.rs`; this reference
is the same contract in prose. Money is a JSON string with exactly four
decimals (`ADR-0007`). An absent or foreign resource is `404`, never `403`
(`ADR-0005`). Errors use the single envelope of
[`../../api/error-contract.md`](../../api/error-contract.md).

## 1. Operation inventory

Twenty-two operations.

| Method | Path | Success | Purpose |
| ------ | ---- | ------- | ------- |
| `GET` | `/products` | `200` | List the caller's products, newest first. |
| `POST` | `/products` | `201` | Track a product. |
| `GET` | `/products/{product_id}` | `200` | Read one product. |
| `PATCH` | `/products/{product_id}` | `200` | Partial update; the identity is re-derived. |
| `DELETE` | `/products/{product_id}` | `204` | Untrack, with everything under it. |
| `GET` | `/products/{product_id}/listings` | `200` | List the product's listings. |
| `POST` | `/products/{product_id}/listings` | `201` | Track a listing. |
| `PATCH` | `/products/{product_id}/listings/{listing_id}` | `200` | The workflow's report on a listing. |
| `DELETE` | `/products/{product_id}/listings/{listing_id}` | `204` | Untrack a listing, with its history. |
| `POST` | `/listings/{listing_id}/observations` | `201` / `200` | Record a price, idempotently. |
| `GET` | `/listings/{listing_id}/observations` | `200` | The price history, newest first. |
| `GET` | `/listings/{listing_id}/observations/latest` | `200` | The newest observation. |
| `GET` | `/products/{product_id}/rules` | `200` | List the product's rules (0 or 1). |
| `POST` | `/products/{product_id}/rules` | `201` | Create the rule. |
| `PATCH` | `/products/{product_id}/rules/{rule_id}` | `200` | Toggle the switch, the target, the percent drop. |
| `DELETE` | `/products/{product_id}/rules/{rule_id}` | `204` | Delete the rule and its alerts. |
| `POST` | `/automation/runs` | `201` | Start a run (any body is ignored). |
| `PATCH` | `/automation/runs/{run_id}` | `200` | Finish a running run. |
| `GET` | `/automation/runs` | `200` | The run history, newest first. |
| `GET` | `/alerts` | `200` | List alerts; `?unread=true` filters. |
| `GET` | `/alerts/{alert_id}` | `200` | Read one alert. |
| `PATCH` | `/alerts/{alert_id}` | `200` | Mark read/unread; the delivery status. |

Common failures: `400` (malformed JSON, unparsable path or query parameter),
`401` (no valid bearer credential), `413` (body too large), `422` (a
validation rule), `500`. The sections below list the operation-specific ones.

## 2. Products

### `POST /products`

Body: `brand` (1..100, trimmed), `model` (1..100, trimmed), `specs` (verbatim,
≤ 2000, default `""`).

```json
{ "brand": "Lenovo", "model": "ThinkPad X1 Carbon", "specs": "32GB RAM, 1TB SSD" }
```

The response is the product, born `active`, with the derived
`normalized_name` (`"lenovo thinkpad x1 carbon"`) and the lower-case
`config_hash` (64 hex characters).

- `409` — this owner already tracks that name and configuration.
- `422` — `brand`, `model`, `specs` or the derived `normalized_name` bound.

### `GET /products`

Query: `status` (`active` | `paused`), `search` (case-insensitive substring
over brand, model and the normalised name, ≤ 200 characters), plus the shared
`limit`/`offset`. The page envelope is the standard one.

Pausing is an instruction to the external collection workflow — a paused
product is excluded from the `status=active` enumeration the workflow
reads; the API itself does not reject a manually submitted observation for
a paused product's listings.

### `PATCH /products/{product_id}`

Body: any of `brand`, `model`, `specs` (`""` clears it), `status`. An omitted
field is left untouched. `normalized_name` and `config_hash` are **not**
fields: they are re-derived from the effective fields.

- `409` — the patch collides with another product of the same owner.

### `DELETE /products/{product_id}`

`204`. The listings, observations, rules and alerts go with it, in the same
statement.

## 3. Listings

### `POST /products/{product_id}/listings`

Body: `retailer` (1..100), `url` (absolute `http`/`https`, ≤ 2048, unique per
owner), `currency` (three letters, upper-cased), `external_id` (optional,
1..200). The listing is born `availability: "unknown"`,
`validation_state: "valid"`, `last_observed_at: null`.

- `404` — the product is missing or foreign.
- `409` — this owner already tracks that URL.

### `PATCH /products/{product_id}/listings/{listing_id}`

Body: `external_id` (tri-state: absent leaves, `null` clears, a value sets),
`availability`, `validation_state`. `retailer`, `url` and `currency` are
immutable — the URL is the listing's identity — and a body carrying them is
accepted and ignored.

- `404` — the listing does not exist under **that** product of the caller.

### `GET /products/{product_id}/listings`

The shared pagination pair only. An empty page probes the product once, so a
foreign product is `404` and a product with no listing is an honest empty
page.

## 4. Observations

### `POST /listings/{listing_id}/observations` — idempotent

Body:

```json
{
  "amount": "1289.9900",
  "currency": "EUR",
  "observed_at": "2026-10-31T09:15:00Z",
  "fingerprint": "sha256:9f2b1c",
  "availability": "available",
  "run_id": "0d1f…"
}
```

- `amount` — four-decimal money string; a JSON number is refused.
- `observed_at` — the collected instant, supplied by the workflow.
- `fingerprint` — 1..200 characters, the idempotency identity.
- `run_id` — optional; must belong to the caller.

**A new observation answers `201` with the row it wrote. The same fingerprint
is absorbed: `200` with the existing row, nothing written.** The body is the
same `ObservationResponse` either way — the status code is the whole
difference. The product's enabled rule is evaluated in the same transaction
as the insert, so when the observation triggers, the alert exists when the
`201` is answered; the absorbed retry raises nothing.

The **previous observation** of a listing — what the percent-drop trigger
compares against — is the row with the greatest `observed_at` among the
listing's other observations (ties broken by `created_at` then `id`,
newest first). A backfilled observation older than the newest row is still
compared against that newest row, so a backfilled older price computes its
drop against the chronologically later price, not against nothing.

- `404` — the listing is foreign, or the named run is foreign.

### `GET /listings/{listing_id}/observations`

The shared pagination pair. Newest first — `observed_at DESC`, with
`created_at DESC, id DESC` as the total-order tiebreakers. A foreign listing
is `404`.

### `GET /listings/{listing_id}/observations/latest`

`200` with the newest observation, or `404` — for a listing with no
observation yet **and** for a foreign listing, because the caller may not
learn which of the two it is.

## 5. Rules

### `POST /products/{product_id}/rules`

Body: `target_amount` (four-decimal string), `target_currency` (three
letters), `percent_drop` (optional, 1..99). Born `enabled`, `notify_via:
"inbox"`.

- `409` — the product already has a rule.
- `422` — `target_amount`, `target_currency` or `percent_drop` bound.

### `GET /products/{product_id}/rules`

The shared pagination pair; at most one item.

### `PATCH /products/{product_id}/rules/{rule_id}`

Body: `enabled`, `target_amount`, `target_currency`, and `percent_drop` as a
tri-state (absent leaves, `null` clears — "does not watch percentages" is a
real state).

### `DELETE /products/{product_id}/rules/{rule_id}`

`204`. The alerts it raised go with it.

## 6. Automation runs

### `POST /automation/runs`

No body. `201` with the run in its `running` state: `finished_at` null, the
three counts `0`, `error_summary` null. A body, if any, is ignored — a run
carries no client-supplied state.

### `PATCH /automation/runs/{run_id}`

Body: `outcome` (**required**, one of `ok`, `partial`, `failed` — `running` is
a `422`), the three counts (optional, ≥ 0), `error_summary` (tri-state,
trimmed, 1..1000).

- `200` — finished; `finished_at` stamped.
- `404` — the run is missing or foreign.
- `409` — the run has already finished; the guard is the statement's
  `WHERE outcome = 'running'`, so two concurrent finishes cannot both win.

### `GET /automation/runs`

The shared pagination pair, newest first.

## 7. Alerts

### `GET /alerts`

Query: `unread` (`true` → only alerts with no `read_at`; omitted or `false` →
all), plus the shared pagination pair. Newest first.

### `GET /alerts/{alert_id}`

`200` with the alert, or `404` for a foreign one.

### `PATCH /alerts/{alert_id}`

Body: `read` (`true` stamps `read_at`, `false` clears it), `notification_status`
(`pending` | `delivered`). Both optional.

## 8. Ownership

Every id-taking operation addresses a row of the caller and nobody else's.
The path binds both the parent and the child (a listing is addressed as
`/products/{product_id}/listings/{listing_id}`), so a row that exists under a
different parent of the same caller is also `404`. `tests/contract_sweep.rs`
sweeps every one of these operations with a second account's token.

# Reimbursements — API

> **Status:** Implemented
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

The HTTP contract of the reimbursement feature. Every path is relative to the
API root. The general rules are in
[`conventions.md`](../../api/conventions.md) and the error codes in
[`error-contract.md`](../../api/error-contract.md).

**All twelve operations answer.** The claim operations, the three item
operations and the four lifecycle actions are served, appear in
`/api/v1/openapi.json`, and are asserted by `tests/claims_api.rs` and
`tests/openapi_contract.rs`. The feature is `Implemented`; it is not `Verified`,
because no reviewer has read the chain that completed it.

## 1. Endpoint summary

| Method | Path | Authentication | Success |
| ------ | ---- | -------------- | ------- |
| `GET` | `/api/v1/claims` | bearer token | `200` `ClaimPage` |
| `POST` | `/api/v1/claims` | bearer token | `201` `ClaimDetailResponse` |
| `GET` | `/api/v1/claims/{claim_id}` | bearer token | `200` `ClaimDetailResponse` |
| `PATCH` | `/api/v1/claims/{claim_id}` | bearer token | `200` `ClaimDetailResponse` |
| `DELETE` | `/api/v1/claims/{claim_id}` | bearer token | `204` no body |
| `POST` | `/api/v1/claims/{claim_id}/items` | bearer token | `201` `ItemResponse` |
| `PATCH` | `/api/v1/claims/{claim_id}/items/{item_id}` | bearer token | `200` `ItemResponse` |
| `DELETE` | `/api/v1/claims/{claim_id}/items/{item_id}` | bearer token | `204` no body |
| `POST` | `/api/v1/claims/{claim_id}/submit` | bearer token | `200` `ClaimDetailResponse` |
| `POST` | `/api/v1/claims/{claim_id}/approve` | bearer token | `200` `ClaimDetailResponse` |
| `POST` | `/api/v1/claims/{claim_id}/reject` | bearer token | `200` `ClaimDetailResponse` |
| `POST` | `/api/v1/claims/{claim_id}/pay` | bearer token | `200` `ClaimDetailResponse` |

Every endpoint is authenticated. There is no anonymous route in this feature,
and a request without a valid token is `401 unauthorized` on all twelve.

## 2. Shared shapes

### 2.1 Money

**Every money value in every request and response body is a JSON string with
exactly four decimal places.** `"125.5000"`, `"0.0000"`,
`"999999999999999.9999"`. A JSON number is refused with `422`. The convention
is in [`conventions.md`](../../api/conventions.md) §15 and the decision is
[`ADR-0007`](../../decisions/ADR-0007-money-representation.md).

### 2.2 `ClaimResponse`

The claim as the **listing** returns it: no items.

```json
{
  "id": "6b0f1c2a-3d4e-4f50-8a91-0b1c2d3e4f50",
  "title": "Client visit to Berlin",
  "description": "Two days on site",
  "currency": "EUR",
  "total_amount": "149.8000",
  "status": "submitted",
  "project_id": "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d",
  "submitted_at": "2026-10-21T09:15:00.000000Z",
  "decided_at": null,
  "paid_at": null,
  "created_at": "2026-10-20T18:02:11.000000Z",
  "updated_at": "2026-10-21T09:15:00.000000Z"
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `id` | UUID string | The claim identifier. |
| `title` | string | 1–200 characters. |
| `description` | string | May be empty. |
| `currency` | string | ISO 4217, three uppercase letters. Immutable. |
| `total_amount` | money string | Always the sum of the items. Read-only. |
| `status` | enum | `draft`, `submitted`, `approved`, `rejected`, `paid`. |
| `project_id` | UUID string or `null` | The optional project link. |
| `submitted_at` | timestamp or `null` | Set on submission; cleared when a rejected claim is reopened. |
| `decided_at` | timestamp or `null` | Set on approve or reject; cleared on reopen. |
| `paid_at` | timestamp or `null` | Set on pay. |
| `created_at`, `updated_at` | timestamp | RFC 3339 UTC. |

### 2.3 `ClaimDetailResponse`

`ClaimResponse` plus `items`, in `incurred_on`, `created_at`, `id` order:

```json
{
  "id": "6b0f1c2a-3d4e-4f50-8a91-0b1c2d3e4f50",
  "title": "Client visit to Berlin",
  "description": "Two days on site",
  "currency": "EUR",
  "total_amount": "149.8000",
  "status": "submitted",
  "project_id": null,
  "submitted_at": "2026-10-21T09:15:00.000000Z",
  "decided_at": null,
  "paid_at": null,
  "created_at": "2026-10-20T18:02:11.000000Z",
  "updated_at": "2026-10-21T09:15:00.000000Z",
  "items": [
    {
      "id": "0c1d2e3f-4a5b-4c6d-8e7f-9a0b1c2d3e4f",
      "claim_id": "6b0f1c2a-3d4e-4f50-8a91-0b1c2d3e4f50",
      "description": "Train, return",
      "amount": "125.8000",
      "incurred_on": "2026-10-20",
      "category": "travel",
      "receipt_url": "https://receipts.example.test/2026/10/20-train.pdf",
      "created_at": "2026-10-20T18:04:00.000000Z",
      "updated_at": "2026-10-20T18:04:00.000000Z"
    }
  ]
}
```

### 2.4 `ItemResponse`

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `id` | UUID string | The item identifier. It is only addressable inside its claim. |
| `claim_id` | UUID string | The claim it belongs to. |
| `description` | string | 1–500 characters. |
| `amount` | money string | `>= 0`, four decimals, in the claim's currency. An item that would push the claim's **total** above `999999999999999.9999` is refused, and the item write is rolled back with it. |
| `incurred_on` | date string | `YYYY-MM-DD`. A calendar day, not an instant. |
| `category` | string or `null` | Trimmed and ASCII-lower-cased; 1-64 characters; `null` when absent. A non-ASCII value is `422` because the column's `LOWER()` and Rust's Unicode case folding disagree outside ASCII. |
| `receipt_url` | string or `null` | Absolute `http`/`https` with a non-empty authority, at most 2048 characters. `https:///` is `422`. Never fetched. |
| `created_at`, `updated_at` | timestamp | RFC 3339 UTC. |

`ItemResponse` carries no `currency`: the currency is the claim's.

### 2.5 `ClaimPage`

The shared list envelope.

```json
{
  "items": [ { "...": "ClaimResponse" } ],
  "pagination": { "limit": 20, "offset": 0, "total": 7, "has_more": false }
}
```

`pagination.total` counts the claims that match the filters, not the whole
collection. The envelope is described in
[`conventions.md`](../../api/conventions.md) §5.

### 2.6 Query parameters — `GET /api/v1/claims`

| Parameter | Type | Default | Meaning |
| --------- | ---- | ------- | ------- |
| `limit`, `offset` | integer | `20`, `0` | The shared pagination parameters. |
| `status` | enum | — | Only claims with that status. An unknown value is `400`. |
| `currency` | string | — | Trimmed and upper-cased; must be three ASCII letters, otherwise `422`. |
| `search` | string | — | Case-insensitive substring over `title` and `description`. At most 200 characters; longer is `422`. Empty means no filter. |
| `submitted_from` | date | — | Inclusive lower bound on `submitted_at`, `YYYY-MM-DD`, UTC. |
| `submitted_to` | date | — | Inclusive upper bound on `submitted_at`, `YYYY-MM-DD`, UTC. |

When either date bound is present, a claim with `submitted_at` `null` (a draft)
is excluded. `submitted_from` later than `submitted_to` is a `422` naming
`submitted_from`.

The order is fixed: `created_at DESC`, then `id DESC`. There is no
client-selectable sort in Phase 2.

### 2.7 Validation limits

| Field | Limit | Error |
| ----- | ----- | ----- |
| claim `title` | 1–200 characters after trimming | `422` |
| claim `description` | 0–5000 characters | `422` |
| `currency` | exactly three ASCII letters | `422` |
| item `description` | 1–500 characters after trimming | `422` |
| `amount` | four-decimal string, `0` to `"999999999999999.9999"` | `422` |
| `incurred_on` | `YYYY-MM-DD`, a real date | `422` |
| `category` | 0–64 characters; an empty value is stored as `null` | `422` |
| `receipt_url` | absolute `http`/`https`, 1–2048 characters | `422` |
| `receipt_url` | absolute `http`/`https` with a host, 1-2048 characters | `422` |

### 2.8 Status and code summary

Every endpoint can answer the following. The per-endpoint tables below list
only what is specific to the endpoint.

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, a path parameter is not a UUID, a query parameter cannot be parsed or is outside its enum. |
| `401` | `unauthorized` | No token, an invalid token, or a token for an account that no longer exists. |
| `404` | `not_found` | The claim or item does not exist, belongs to another account, or is addressed under a claim it does not belong to. A foreign `project_id` in a body is also `404`. |
| `409` | `conflict` | A lifecycle transition the claim's status does not allow, or an item write on a claim that is not a draft. |
| `413` | `payload_too_large` | The body exceeds the configured limit. |
| `422` | `validation_failed` | A field breaks a rule in §2.7, or a submission precondition fails. `details` names the field. |
| `500` | `database_error` | The database failed. |
| `408` | `timeout` | The request exceeded the configured timeout. |

## 3. `GET /api/v1/claims`

Lists the caller's claims, newest first, without items.

**Authentication:** bearer token required.

**Query parameters:** §2.6.

### Response — `200 OK`

A `ClaimPage`. An empty page is `{"items":[],"pagination":{…,"total":0}}`, not
`404`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `limit`/`offset` not integers, or `status` not one of the five values. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `422` | `validation_failed` | `search` over 200 characters, `currency` not three letters, or `submitted_from` after `submitted_to`. |
| `500` | `database_error` | The database failed. |

```bash
curl -s "http://127.0.0.1:3000/api/v1/claims?status=submitted&submitted_from=2026-10-01&submitted_to=2026-10-31&limit=20" \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 4. `POST /api/v1/claims`

Creates a claim in `draft`.

**Authentication:** bearer token required.

### Request

```json
{
  "title": "Client visit to Berlin",
  "description": "Two days on site",
  "currency": "EUR",
  "project_id": null
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `title` | string | yes | Trimmed, 1–200 characters. |
| `description` | string | no | Defaults to `""`. At most 5000 characters. |
| `currency` | string | yes | Three ASCII letters; trimmed and upper-cased. |
| `project_id` | UUID string or `null` | no | Must belong to the caller; otherwise `404`. |

The body **never** carries `total_amount`, `status` or a timestamp. They are
server-owned.

### Response — `201 Created`

A `ClaimDetailResponse` with `status` `draft`, `total_amount` `"0.0000"`, no
decision or payment timestamp and `items` `[]`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | `project_id` belongs to another account or does not exist. |
| `422` | `validation_failed` | A field breaks §2.7, or `project_id` is not a UUID. |
| `500` | `database_error` | The database failed. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/claims \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"title":"Client visit to Berlin","currency":"EUR"}'
```

## 5. `GET /api/v1/claims/{claim_id}`

Reads one claim **with its items**.

**Authentication:** bearer token required.

### Response — `200 OK`

A `ClaimDetailResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `claim_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim does not exist, or belongs to another account. |

## 6. `PATCH /api/v1/claims/{claim_id}`

Partially updates a claim. Every field is optional; an omitted field is left
untouched.

**Authentication:** bearer token required.

### Request

```json
{ "title": "Client visit to Berlin (Oct)", "project_id": null }
```

| Field | Type | Rules when present |
| ----- | ---- | ------------------ |
| `title` | string | Trimmed, 1–200 characters. |
| `description` | string | At most 5000 characters. `""` clears it. |
| `project_id` | UUID string or `null` | `null` clears the link; a UUID must belong to the caller. |

`currency`, `status` and the timestamps are **not** updatable. They are absent
from the request contract, and the only way to change the status is one of the
four action endpoints, so a client that sends them does not change them.

### Response — `200 OK`

The updated `ClaimDetailResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or `claim_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim or the project does not exist, or belongs to another account. |
| `422` | `validation_failed` | A field breaks §2.7. |

## 7. `DELETE /api/v1/claims/{claim_id}`

Deletes a claim and its items permanently. Allowed in any status
(`REIMB-R11`).

**Authentication:** bearer token required.

### Response — `204 No Content`

No body.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `claim_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim does not exist, or belongs to another account. |

## 8. `POST /api/v1/claims/{claim_id}/items`

Adds a line item to a **draft** claim and recomputes the total.

**Authentication:** bearer token required.

### Request

```json
{
  "description": "Train, return",
  "amount": "125.8000",
  "incurred_on": "2026-10-20",
  "category": "travel",
  "receipt_url": "https://receipts.example.test/2026/10/20-train.pdf"
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `description` | string | yes | Trimmed, 1–500 characters. |
| `amount` | money string | yes | Four decimals, `>= 0`, at most 15 integer digits. The claim total must stay at or below `999999999999999.9999`; an item that would exceed it is `422` naming `amount`. |
| `incurred_on` | date string | yes | `YYYY-MM-DD`, a real date. |
| `category` | string | no | Trimmed and ASCII-lower-cased; 1-64 characters; a non-ASCII value is `422`. |
| `receipt_url` | string | no | Absolute `http`/`https` with a non-empty authority, at most 2048 characters. Never fetched. |

### Response — `201 Created`

The `ItemResponse`. The claim's new total is read with
`GET /claims/{claim_id}`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or `claim_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim does not exist, or belongs to another account. |
| `409` | `conflict` | The claim is not a `draft`. |
| `422` | `validation_failed` | A field breaks §2.7. |
| `500` | `database_error` | The database failed. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/claims/6b0f1c2a-3d4e-4f50-8a91-0b1c2d3e4f50/items \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"description":"Train, return","amount":"125.8000","incurred_on":"2026-10-20"}'
```

## 9. `PATCH /api/v1/claims/{claim_id}/items/{item_id}`

Partially updates an item on a **draft** claim and recomputes the total.

**Authentication:** bearer token required.

### Request

Every field is optional. An omitted field is left untouched.

```json
{ "amount": "129.9000", "category": null }
```

| Field | Type | Rules when present |
| ----- | ---- | ------------------ |
| `description` | string | Trimmed, 1–500 characters. |
| `amount` | money string | Four decimals, `>= 0`. |
| `incurred_on` | date string | `YYYY-MM-DD`. |
| `category` | string or `null` | Trimmed and ASCII-lower-cased; 1-64 characters; `null` clears it; a non-ASCII value is `422`. |
| `receipt_url` | string or `null` | Valid URL; `null` clears it. |

### Response — `200 OK`

The updated `ItemResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or an id is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim or the item does not exist, belongs to another account, or the item does not belong to that claim. |
| `409` | `conflict` | The claim is not a `draft`. |
| `422` | `validation_failed` | A field breaks §2.7. |

## 10. `DELETE /api/v1/claims/{claim_id}/items/{item_id}`

Deletes an item from a **draft** claim and recomputes the total.

**Authentication:** bearer token required.

### Response — `204 No Content`

No body.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | An id is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim or the item does not exist, or the item does not belong to that claim. |
| `409` | `conflict` | The claim is not a `draft`. |

## 11. `POST /api/v1/claims/{claim_id}/submit`

Submits a draft claim, or reopens a rejected one.

**Authentication:** bearer token required.

### Request

No body.

### Behaviour by current status

| Current status | Result |
| -------------- | ------ |
| `draft` | The preconditions are checked. With at least one item and a positive total: `200`, `status` `submitted`, `submitted_at` set. Otherwise `422`. |
| `rejected` | The claim is reopened: `200`, `status` `draft`; the reopen clears **both** `submitted_at` and `decided_at` (`paid_at` is already `null`). It is **not** submitted by this request. |
| `submitted`, `approved`, `paid` | `409 conflict`. |

### Response — `200 OK`

A `ClaimDetailResponse` in the new status.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `claim_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim does not exist, or belongs to another account. |
| `409` | `conflict` | The claim is `submitted`, `approved` or `paid`. |
| `422` | `validation_failed` | A `draft` has no items (names `items`) or a total of `"0.0000"` (names `total_amount`). |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/claims/6b0f1c2a-3d4e-4f50-8a91-0b1c2d3e4f50/submit \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 12. `POST /api/v1/claims/{claim_id}/approve`

Approves a submitted claim.

**Authentication:** bearer token required.

### Request

No body.

### Response — `200 OK`

A `ClaimDetailResponse` with `status` `approved` and `decided_at` set.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `claim_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim does not exist, or belongs to another account. |
| `409` | `conflict` | The claim is not `submitted`. |

## 13. `POST /api/v1/claims/{claim_id}/reject`

Rejects a submitted claim.

**Authentication:** bearer token required.

### Request

No body. No reason is recorded in Phase 2.

### Response — `200 OK`

A `ClaimDetailResponse` with `status` `rejected` and `decided_at` set.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `claim_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim does not exist, or belongs to another account. |
| `409` | `conflict` | The claim is not `submitted`. |

## 14. `POST /api/v1/claims/{claim_id}/pay`

Marks an approved claim paid.

**Authentication:** bearer token required.

### Request

No body.

### Response — `200 OK`

A `ClaimDetailResponse` with `status` `paid` and `paid_at` set.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `claim_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The claim does not exist, or belongs to another account. |
| `409` | `conflict` | The claim is not `approved`. `paid` is terminal, so a second `pay` is also `409`. |

## 15. Notes on the OpenAPI document

All twelve operations are registered in `src/api/routes.rs` and appear in the
document at `/api/v1/openapi.json` with the `ClaimResponse`,
`ClaimDetailResponse`, `ClaimPage`, `ItemResponse`, `CreateClaimRequest`,
`UpdateClaimRequest`, `CreateItemRequest` and `UpdateItemRequest` schemas. The
`bearer` security scheme applies to all of them. The list operation publishes
`limit`, `offset`, `status`, `currency`,
`search`, `submitted_from` and `submitted_to` **once each**, and the contract
test asserts that set exactly
([`conventions.md`](../../api/conventions.md) §5).

The money fields are `type: string` with `format: decimal` and an explicit
four-decimal pattern in the schema, so a generated client sees a string and
not a number.

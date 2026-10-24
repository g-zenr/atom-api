# Job applications — API

> **Status:** Implemented
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-JOB-001`](specification.md)

The HTTP contract of the job application feature. Every path is relative to
the API root. The general rules are in
[`conventions.md`](../../api/conventions.md) and the error codes in
[`error-contract.md`](../../api/error-contract.md).

**All fourteen operations answer.** They are served, published in
`/api/v1/openapi.json`, and asserted by `tests/job_applications_api.rs` and
`tests/openapi_contract.rs`. The feature is `Implemented`; it is not `Verified`,
because no gate run after the review's fixes is recorded and
`ATOM-REVIEW-20261024-010`, which read the chain and found no code defect,
returned FAIL on documentation only.

## 1. Endpoint summary

| Method | Path | Authentication | Success |
| ------ | ---- | -------------- | ------- |
| `GET` | `/api/v1/companies` | bearer token | `200` `CompanyPage` |
| `POST` | `/api/v1/companies` | bearer token | `201` `CompanyResponse` |
| `GET` | `/api/v1/companies/{company_id}` | bearer token | `200` `CompanyResponse` |
| `PATCH` | `/api/v1/companies/{company_id}` | bearer token | `200` `CompanyResponse` |
| `DELETE` | `/api/v1/companies/{company_id}` | bearer token | `204` no body |
| `GET` | `/api/v1/job-applications` | bearer token | `200` `ApplicationPage` |
| `POST` | `/api/v1/job-applications` | bearer token | `201` `ApplicationResponse` |
| `GET` | `/api/v1/job-applications/{application_id}` | bearer token | `200` `ApplicationResponse` |
| `PATCH` | `/api/v1/job-applications/{application_id}` | bearer token | `200` `ApplicationResponse` |
| `DELETE` | `/api/v1/job-applications/{application_id}` | bearer token | `204` no body |
| `GET` | `/api/v1/job-applications/{application_id}/events` | bearer token | `200` `EventPage` |
| `POST` | `/api/v1/job-applications/{application_id}/events` | bearer token | `201` `EventResponse` |
| `PATCH` | `/api/v1/job-applications/{application_id}/events/{event_id}` | bearer token | `200` `EventResponse` |
| `DELETE` | `/api/v1/job-applications/{application_id}/events/{event_id}` | bearer token | `204` no body |

Every endpoint is authenticated. There is no anonymous route in this feature,
and a request without a valid token is `401 unauthorized` on all fourteen.

## 2. Shared shapes

### 2.1 `CompanyResponse`

```json
{
  "id": "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d",
  "name": "Acme GmbH",
  "website": "https://acme.example.test",
  "notes": "Referred by Dana",
  "created_at": "2026-10-01T08:00:00.000000Z",
  "updated_at": "2026-10-01T08:00:00.000000Z"
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `id` | UUID string | The company identifier. |
| `name` | string | Trimmed, 1–200 characters, unique per owner ignoring case. |
| `website` | string or `null` | Absolute `http`/`https`, at most 2048 characters. Never fetched. |
| `notes` | string | May be empty; at most 5000 characters. |
| `created_at`, `updated_at` | timestamp | RFC 3339 UTC. |

### 2.2 `ApplicationResponse`

```json
{
  "id": "0f1e2d3c-4b5a-4968-8776-5a4b3c2d1e0f",
  "company_id": "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d",
  "role_title": "Backend engineer",
  "source": "referral",
  "status": "interview",
  "applied_on": "2026-10-02",
  "salary_min": "60000.0000",
  "salary_max": "75000.0000",
  "salary_currency": "EUR",
  "job_url": "https://acme.example.test/jobs/42",
  "description": "Platform team, Rust and PostgreSQL.",
  "next_action_task_id": "3c2d1e0f-9a8b-4c6d-8e7f-0a1b2c3d4e5f",
  "created_at": "2026-10-02T10:30:00.000000Z",
  "updated_at": "2026-10-12T16:45:00.000000Z"
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `id` | UUID string | The application identifier. |
| `company_id` | UUID string | The company it belongs to. It can be corrected with a `PATCH`, and the new company must be the caller's. |
| `role_title` | string | Trimmed, 1–200 characters. |
| `source` | string or `null` | At most 64 characters. |
| `status` | enum | `wishlist`, `applied`, `screening`, `interview`, `offer`, `rejected`, `withdrawn`. |
| `applied_on` | date string or `null` | `YYYY-MM-DD`. |
| `salary_min`, `salary_max` | money string or `null` | Four decimals, `>= 0`, in `salary_currency`. |
| `salary_currency` | string or `null` | ISO 4217, three uppercase letters. Required when either bound is present. |
| `job_url` | string or `null` | Absolute `http`/`https`, at most 2048 characters. Never fetched. |
| `description` | string | May be empty; at most 10 000 characters. |
| `next_action_task_id` | UUID string or `null` | A task of the caller's. Cleared when the task is deleted. |
| `created_at`, `updated_at` | timestamp | RFC 3339 UTC. |

**Every money value is a JSON string with four decimal places**
([`ADR-0007`](../../decisions/ADR-0007-money-representation.md)). A JSON
number is refused with `422`.

### 2.3 `EventResponse`

```json
{
  "id": "5e4d3c2b-1a09-4876-9543-2f1e0d9c8b7a",
  "application_id": "0f1e2d3c-4b5a-4968-8776-5a4b3c2d1e0f",
  "kind": "interview",
  "occurred_at": "2026-10-14T13:00:00.000000Z",
  "summary": "Technical interview with the platform team",
  "details": "Two exercises, Rust and SQL.",
  "created_at": "2026-10-12T16:50:00.000000Z"
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `id` | UUID string | The event identifier, meaningful only inside its application. |
| `application_id` | UUID string | The application it belongs to. |
| `kind` | enum | `note`, `email`, `call`, `interview`, `offer`, `rejection`. |
| `occurred_at` | timestamp | RFC 3339, required, may be in the future. |
| `summary` | string | Trimmed, 1–200 characters. |
| `details` | string | May be empty; at most 10 000 characters. |
| `created_at` | timestamp | RFC 3339 UTC. |

An event has **no `updated_at`**. Its `kind`, `occurred_at`, `summary` and
`details` may be corrected with a `PATCH`, and the row carries no timestamp for
the correction: a timeline entry records **what the owner says happened**, not
when the row was last written. `created_at` is the only row timestamp the API
keeps for it. The table deliberately has no `updated_at` column
([`schema.md`](../../database/schema.md)). This is the locked decision
`A-D1-1`.

### 2.4 The pages

`CompanyPage`, `ApplicationPage` and `EventPage` use the shared envelope:

```json
{
  "items": [ { "...": "CompanyResponse" } ],
  "pagination": { "limit": 20, "offset": 0, "total": 3, "has_more": false }
}
```

### 2.5 Query parameters

**`GET /api/v1/companies`**

| Parameter | Type | Default | Meaning |
| --------- | ---- | ------- | ------- |
| `limit`, `offset` | integer | `20`, `0` | The shared pagination parameters. |
| `search` | string | — | Case-insensitive substring over `name`. At most 200 characters. |

Order: `lower(name)`, then `name`, then `id`. **`GET /api/v1/job-applications`**

| Parameter | Type | Default | Meaning |
| --------- | ---- | ------- | ------- |
| `limit`, `offset` | integer | `20`, `0` | The shared pagination parameters. |
| `status` | enum | — | Only applications in that stage. An unknown value is `400`. |
| `company_id` | UUID | — | Only applications for that company. Not a UUID is `400`. |
| `search` | string | — | Case-insensitive substring over `role_title`. At most 200 characters. |
| `applied_from`, `applied_to` | date | — | Inclusive bounds on `applied_on`. |

Order: `created_at DESC`, then `id DESC`. An application with `applied_on`
`null` is excluded when either date bound is present.

**`GET /api/v1/job-applications/{application_id}/events`**

| Parameter | Type | Default | Meaning |
| --------- | ---- | ------- | ------- |
| `limit`, `offset` | integer | `20`, `0` | The shared pagination parameters. |

Order: `occurred_at DESC`, then `id DESC` (newest first). There is no filter.

There is no client-selectable sort in Phase 2; each order is fixed and
documented.

### 2.6 Status and code summary

Every endpoint can answer the following. The per-endpoint tables list only
what is specific to the endpoint.

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, a path parameter is not a UUID, a query parameter cannot be parsed or is outside its enum. |
| `401` | `unauthorized` | No token, an invalid token, or a token for an account that no longer exists. |
| `404` | `not_found` | The row does not exist, belongs to another account, or is addressed under a parent it does not belong to. A foreign `company_id` or `next_action_task_id` in a body is also `404`. |
| `409` | `conflict` | A duplicate company name, a stage move the machine refuses, or deleting a company that still has applications. |
| `413` | `payload_too_large` | The body exceeds the configured limit. |
| `422` | `validation_failed` | A field breaks a rule. `details` names the field. |
| `500` | `database_error` | The database failed. |
| `408` | `timeout` | The request exceeded the configured timeout. |

## 3. `GET /api/v1/companies`

Lists the caller's companies, alphabetically.

**Authentication:** bearer token required.

### Response — `200 OK`

A `CompanyPage`. An empty page is not `404`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `limit`/`offset` are not integers. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `422` | `validation_failed` | `search` is over 200 characters. |

```bash
curl -s "http://127.0.0.1:3000/api/v1/companies?search=acme" \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 4. `POST /api/v1/companies`

Creates a company.

**Authentication:** bearer token required.

### Request

```json
{
  "name": "Acme GmbH",
  "website": "https://acme.example.test",
  "notes": "Referred by Dana"
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `name` | string | yes | Trimmed, 1–200 characters, unique per owner ignoring case. |
| `website` | string | no | Absolute `http`/`https`, at most 2048 characters. |
| `notes` | string | no | Defaults to `""`. At most 5000 characters. |

### Response — `201 Created`

The `CompanyResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `409` | `conflict` | The caller already has a company with that name, ignoring case. |
| `422` | `validation_failed` | A field is invalid. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/companies \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"name":"Acme GmbH"}'
```

## 5. `GET /api/v1/companies/{company_id}`

Reads one company.

**Authentication:** bearer token required.

### Response — `200 OK`

The `CompanyResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `company_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The company does not exist, or belongs to another account. |

## 6. `PATCH /api/v1/companies/{company_id}`

Partially updates a company. An omitted field is left untouched.

**Authentication:** bearer token required.

### Request

```json
{ "website": "https://acme.example.test/careers" }
```

| Field | Type | Rules when present |
| ----- | ---- | ------------------ |
| `name` | string | Trimmed, 1–200 characters; a collision is `409`. |
| `website` | string or `null` | Valid URL; `null` clears it. |
| `notes` | string | At most 5000 characters; `""` clears it. |

### Response — `200 OK`

The updated `CompanyResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or `company_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The company does not exist, or belongs to another account. |
| `409` | `conflict` | The new name collides with another company of the caller. |
| `422` | `validation_failed` | A field is invalid. |

## 7. `DELETE /api/v1/companies/{company_id}`

Deletes a company that has no applications.

**Authentication:** bearer token required.

### Response — `204 No Content`

No body.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `company_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The company does not exist, or belongs to another account. |
| `409` | `conflict` | At least one application still references the company. Delete or move them first. |

## 8. `GET /api/v1/job-applications`

Lists the caller's applications, newest first.

**Authentication:** bearer token required.

**Query parameters:** §2.5.

### Response — `200 OK`

An `ApplicationPage`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `limit`/`offset`/`company_id` cannot be parsed, or `status` is not one of the seven values. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `422` | `validation_failed` | `search` is over 200 characters, or `applied_from` is after `applied_to`. |

```bash
curl -s "http://127.0.0.1:3000/api/v1/job-applications?status=interview&limit=20" \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 9. `POST /api/v1/job-applications`

Creates an application.

**Authentication:** bearer token required.

### Request

```json
{
  "company_id": "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d",
  "role_title": "Backend engineer",
  "source": "referral",
  "status": "wishlist",
  "applied_on": "2026-10-02",
  "salary_min": "60000.0000",
  "salary_max": "75000.0000",
  "salary_currency": "EUR",
  "job_url": "https://acme.example.test/jobs/42",
  "description": "Platform team, Rust and PostgreSQL.",
  "next_action_task_id": null
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `company_id` | UUID | yes | Must belong to the caller, otherwise `404`. |
| `role_title` | string | yes | Trimmed, 1–200 characters. |
| `source` | string | no | At most 64 characters; a value that is blank after trimming is `422`. |
| `status` | enum | no | Defaults to `wishlist`. **Only `wishlist` or an omitted stage is accepted on create**; any other value is `422`. The machine governs later changes. |
| `applied_on` | date | no | `YYYY-MM-DD`. |
| `salary_min`, `salary_max` | money string | no | Four decimals, `>= 0`; `min <= max`. |
| `salary_currency` | string | when a bound is present | Three ASCII letters. A currency with neither bound is `422`. |
| `job_url` | string | no | Absolute `http`/`https`, at most 2048 characters. |
| `description` | string | no | Defaults to `""`. At most 10 000 characters. |
| `next_action_task_id` | UUID | no | Must be the caller's task, otherwise `404`. |

### Response — `201 Created`

The `ApplicationResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | `company_id` or `next_action_task_id` does not exist or is another account's. |
| `422` | `validation_failed` | A field is invalid, including a stage other than `wishlist`. |

## 10. `GET /api/v1/job-applications/{application_id}`

Reads one application. The events are read from §11.

**Authentication:** bearer token required.

### Response — `200 OK`

The `ApplicationResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `application_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The application does not exist, or belongs to another account. |

## 11. `PATCH /api/v1/job-applications/{application_id}`

Partially updates an application, and **moves it through the stage machine**
when `status` is present.

**Authentication:** bearer token required.

### Request

```json
{ "status": "screening" }
```

| Field | Type | Rules when present |
| ----- | ---- | ------------------ |
| `company_id` | UUID | Must belong to the caller, otherwise `404`. |
| `role_title` | string | Trimmed, 1–200 characters. |
| `source` | string or `null` | At most 64 characters; a blank value is `422`; `null` clears it. |
| `status` | enum | Must be a permitted move from the current stage, otherwise `409`. The current stage sent again is accepted and is not a transition. |
| `applied_on` | date or `null` | `YYYY-MM-DD`; `null` clears it. |
| `salary_min`, `salary_max`, `salary_currency` | money string / money string / string or `null` | Validated together; a currency with no bound is `422`; `null` clears a bound, and clearing both clears the currency. |
| `job_url` | string or `null` | Valid URL; `null` clears it. |
| `description` | string | At most 10 000 characters; `""` clears it. |
| `next_action_task_id` | UUID or `null` | `null` clears the link; a UUID must be the caller's. |

The request is **one transaction**: if the stage move is refused, no other
field of the same request is written either.

### Allowed moves

| From | To |
| ---- | -- |
| `wishlist` | `applied`, `rejected`, `withdrawn` |
| `applied` | `screening`, `rejected`, `withdrawn` |
| `screening` | `interview`, `rejected`, `withdrawn` |
| `interview` | `offer`, `rejected`, `withdrawn` |
| `offer`, `rejected`, `withdrawn` | nothing |

### Response — `200 OK`

The updated `ApplicationResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or `application_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The application, the company or the task does not exist or belongs to another account. |
| `409` | `conflict` | The requested stage is not a permitted move from the current one, including any move out of a terminal stage. |
| `422` | `validation_failed` | A field is invalid, including an unknown stage value. |

```bash
curl -i -X PATCH http://127.0.0.1:3000/api/v1/job-applications/0f1e2d3c-4b5a-4968-8776-5a4b3c2d1e0f \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"status":"screening"}'
```

## 12. `DELETE /api/v1/job-applications/{application_id}`

Deletes an application and its events permanently.

**Authentication:** bearer token required.

### Response — `204 No Content`

No body.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `application_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The application does not exist, or belongs to another account. |

## 13. `GET /api/v1/job-applications/{application_id}/events`

Lists the events of one application, newest first.

**Authentication:** bearer token required.

### Response — `200 OK`

An `EventPage`. The order is `occurred_at DESC`, then `id DESC`.

A client that wants a chronological timeline reverses the page, or pages to
the end and reverses the whole list.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `application_id` is not a UUID, or `limit`/`offset` cannot be parsed. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The application does not exist, or belongs to another account. |

```bash
curl -s "http://127.0.0.1:3000/api/v1/job-applications/0f1e2d3c-4b5a-4968-8776-5a4b3c2d1e0f/events?limit=50" \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 14. `POST /api/v1/job-applications/{application_id}/events`

Adds an event to an application.

**Authentication:** bearer token required.

### Request

```json
{
  "kind": "interview",
  "occurred_at": "2026-10-14T13:00:00Z",
  "summary": "Technical interview with the platform team",
  "details": "Two exercises, Rust and SQL."
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `kind` | enum | yes | `note`, `email`, `call`, `interview`, `offer`, `rejection`. |
| `occurred_at` | timestamp | yes | RFC 3339 with a timezone. May be in the future. |
| `summary` | string | yes | Trimmed, 1–200 characters. |
| `details` | string | no | Defaults to `""`. At most 10 000 characters. |

### Response — `201 Created`

The `EventResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or `application_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The application does not exist, or belongs to another account. |
| `422` | `validation_failed` | A field is invalid, including an unknown `kind`. |

## 15. `PATCH /api/v1/job-applications/{application_id}/events/{event_id}`

Corrects an event. `kind`, `occurred_at`, `summary` and `details` are all
optional, and an omitted field is left untouched.

**Authentication:** bearer token required.

### Request

```json
{ "summary": "Technical interview — rescheduled", "occurred_at": "2026-10-15T09:00:00Z" }
```

All four fields are optional and follow §14.

### Response — `200 OK`

The updated `EventResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or an id is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The application or the event does not exist, belongs to another account, or the event does not belong to that application. |
| `422` | `validation_failed` | A field is invalid. |

## 16. `DELETE /api/v1/job-applications/{application_id}/events/{event_id}`

Deletes one event.

**Authentication:** bearer token required.

### Response — `204 No Content`

No body.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | An id is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The application or the event does not exist, or the event does not belong to that application. |

## 17. Notes on the OpenAPI document

The fourteen operations are registered in
`src/api/routes.rs` and appear in the document at `/api/v1/openapi.json` with
the `CompanyResponse`, `CompanyPage`, `ApplicationResponse`,
`ApplicationPage`, `EventResponse`, `EventPage`, `CreateCompanyRequest`,
`UpdateCompanyRequest`, `CreateApplicationRequest`,
`UpdateApplicationRequest`, `CreateEventRequest` and `UpdateEventRequest`
schemas. The `bearer` security scheme applies to all fourteen.

The two job list operations — `GET /api/v1/companies` and
`GET /api/v1/job-applications` — publish their parameters exactly once each,
and the contract test
`each_list_endpoint_publishes_its_filter_and_pagination_parameters_once`
asserts those two published sets exactly
([`conventions.md`](../../api/conventions.md) §5). The event listing publishes
only `limit` and `offset` and is not in that test's table. The salary fields
are `type: string` in the document, never `type: number`.

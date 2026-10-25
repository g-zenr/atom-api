# ATOM API — frontend handover

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent

This document is for an engineer building the first client against a running
ATOM API, with no prior exposure to this repository. It collects the
cross-cutting behaviour a client depends on — authentication, errors,
pagination, money, list parameters, the two stage machines, ML degradation and
versioning — and it names the file that decides each statement, so every
sentence can be checked.

It is **not** a second specification. The endpoint-level authority is the
per-feature reference, and the general rules are in
[`conventions.md`](conventions.md) and [`error-contract.md`](error-contract.md).
Where this document, another document and the code disagree, the code is what
the server does today; §10 records the three disagreements I found while writing
this one.

Locally the API listens on `http://127.0.0.1:3000` (`cargo run`, see
[`AGENTS.md`](../../AGENTS.md) §3). Every path below is relative to that origin
unless it is written in full. Every product endpoint is under `/api/v1`; the two
health probes are not (§8).

---

## 1. Sign-in and staying signed in

### 1.1 The five endpoints

| Method | Path | Credential | Request body | Success | Response body |
| ------ | ---- | ---------- | ------------ | ------- | ------------- |
| `POST` | `/api/v1/auth/register` | none | `{ "email", "password", "display_name" }` | `201` | `AuthResponse` |
| `POST` | `/api/v1/auth/login` | none | `{ "email", "password" }` | `200` | `AuthResponse` |
| `POST` | `/api/v1/auth/refresh` | none | `{ "refresh_token" }` | `200` | `AuthResponse` |
| `POST` | `/api/v1/auth/logout` | bearer | `{ "refresh_token" }` | `204`, no body | — |
| `GET` | `/api/v1/auth/me` | bearer | none | `200` | `UserResponse` |

The handlers are in [`src/auth/handlers.rs`](../../src/auth/handlers.rs); the
request and response types are in [`src/auth/dto.rs`](../../src/auth/dto.rs) and
[`src/users/dto.rs`](../../src/users/dto.rs). `register` answers `201`, not
`200` — the account was created. The three token endpoints take no bearer token
by design, and each is rate limited by client address (§1.7).

`AuthResponse` is the same shape on all three token endpoints:

```json
{
  "tokens": {
    "access_token": "<HS256 JWT>",
    "refresh_token": "<opaque, base64url, no padding>",
    "token_type": "Bearer",
    "expires_in": 900,
    "expires_at": "2026-10-24T09:15:00.000000Z"
  },
  "user": {
    "id": "0192...",
    "email": "ada@example.com",
    "display_name": "Ada",
    "is_active": true,
    "created_at": "2026-10-24T09:00:00.000000Z"
  }
}
```

- `expires_in` is the **access** token's lifetime in seconds and `expires_at`
  its absolute expiry; both are built from the token service's configured
  lifetime, so the advertised lifetime and the `exp` claim cannot drift apart
  ([`src/auth/service.rs`](../../src/auth/service.rs) `token_pair`).
- **The refresh token's expiry is not in the response.** Do not wait for a
  deadline you cannot see: attempt the call, and refresh on `401` (§1.4).
- `user` is the public representation only: no hash, no session material
  ([`src/users/dto.rs`](../../src/users/dto.rs) `UserResponse`).

### 1.2 What the two tokens are

| | Access token | Refresh token |
| - | ------------ | ------------- |
| Format | HS256 JWT | 32 random bytes, base64url, no padding |
| Claims / content | `sub`, `email`, `iss`, `aud`, `exp`, `iat`, `jti` | none; opaque |
| Where the server keeps it | nowhere — verified statelessly from the signing secret | only its SHA-256 hash, in `refresh_sessions.token_hash` |
| Lifetime | `auth.access_token_ttl_seconds`, **900 s (15 min)** by default | `auth.refresh_token_ttl_seconds`, **1 209 600 s (14 days)**, absolute |
| Lifetime can be extended? | no | no — a rotation inherits the original `expires_at` |
| Reusable? | until expiry | **once** |

Sources: [`src/auth/token.rs`](../../src/auth/token.rs) (claims, generation,
hashing), [`config/auth.toml`](../../config/auth.toml) (both lifetimes),
[`ADR-0003`](../decisions/ADR-0003-auth-token-design.md) §1–§2 (the decision).
The 14-day figure is the committed default; a deployment can change it, and
the configuration refuses a refresh lifetime that is not longer than the access
lifetime (`config/auth.toml` comment, `ADR-0003` §2).

Access-token validation checks signature, algorithm, issuer, audience and
expiry with a **30-second clock skew** allowance
(`auth.clock_skew_seconds`, [`config/auth.toml`](../../config/auth.toml)).
A client clock that is off by more than that will see unexplained `401`s.

### 1.3 Rotation, and the one thing that breaks accounts

`POST /auth/refresh` **revokes the presented session and issues a replacement**
([`src/auth/service.rs`](../../src/auth/service.rs) `refresh`). Three
consequences, in order of how badly they bite:

1. **Store the new pair every time.** The old refresh token is dead the moment a
   refresh succeeds. A client that keeps using the old one presents a revoked
   token.
2. **A second use of a revoked refresh token is treated as theft.** The server
   revokes **every** active session of that account and answers `401`
   (`src/auth/service.rs`, the `revoked_at.is_some()` branch; asserted by
   `reusing_a_rotated_refresh_token_revokes_every_session` in
   [`tests/auth_api.rs`](../../tests/auth_api.rs)). The user is signed out on
   every device.
3. **Therefore: serialise your refreshes.** Have at most one refresh in flight;
   queue the callers that arrive while it runs and give them the new token.
   Two concurrent refreshes with the same token produce exactly one success and
   one `401` — the loser triggers rule 2
   (`a_concurrent_refresh_of_the_same_token_succeeds_only_once`, same file).
   This is a known, accepted cost of reuse detection and is stated as such in
   `ADR-0003` under "Consequences and trade-offs".

Rotation does **not** extend the session: the replacement inherits the original
deadline (`src/auth/service.rs` `refresh`, `expires_at = session.expires_at`;
asserted by `a_rotation_preserves_the_absolute_lifetime_of_the_session`).
The 14 days are measured from the sign-in that started the chain, so rotating
forever does not keep a session alive; after that the chain ends and the user
signs in again.

### 1.4 What `401` means and exactly what to do

`401` with code `unauthorized` means the credential is missing, malformed,
expired, signed with another secret, issued for another issuer or audience, or
belongs to an account that no longer exists. It is also the answer to a wrong
password, an unknown email address, and an unknown, expired or reused refresh
token ([`error-contract.md`](error-contract.md) §2). The server deliberately
does not say which, and the wording is generic — **never parse `message`**.

The client rule:

```text
protected call  ──401──►  refresh once with the stored refresh token
                            ├─ 200 ──►  store the new pair, retry the original call ONCE
                            └─ 401 ──►  discard both tokens, show the sign-in screen
```

- Do not retry more than once. A `401` after a fresh token is not a token
  problem.
- A `401` from `POST /auth/refresh` is final for that session. It is also what
  a user sees after someone tripped reuse detection, so do not loop.
- **Logout does not invalidate an access token.** There is no revocation list;
  `jti` is carried but not consulted (`ADR-0003`, "The access token cannot be
  revoked before it expires"). `POST /auth/logout` revokes the refresh session
  only, and the test asserts that the *refresh* token stops working
  (`logout_revokes_the_session`). The client must delete the access token
  locally.
- Logout needs **both** credentials: a valid bearer access token (the handler
  extracts `AuthUser`) and the refresh token in the body. A client whose access
  token has already expired should refresh first, or simply discard its tokens
  locally and accept that the server-side session lives until it expires.
- Logout is idempotent for an unknown refresh token — it answers `204`, not
  `404` (`src/auth/service.rs` `logout`;
  `logout_with_an_unknown_refresh_token_is_accepted`). But do **not** present a
  token you already logged out with a second time at `/auth/refresh`: that is a
  reuse and revokes everything.

### 1.5 What `403` means

`403` with code `forbidden` is *not* an expired session. It means the
credential was valid and the action still is not permitted: the account is
disabled, or the caller tried to revoke a session that belongs to somebody else
([`error-contract.md`](error-contract.md) §2 and §6). Show a different message,
and do not refresh.

### 1.6 There is no cookie, so there is no CSRF surface — and what that costs

The credential travels **only** in `Authorization: Bearer <access_token>`
([`conventions.md`](conventions.md) §12; the extractor is `AuthUser` in
[`src/api/middleware.rs`](../../src/api/middleware.rs)). Nothing in `src/`
writes a `Set-Cookie` header, the refresh token is carried in a JSON body, and
the CORS layer never enables credentials ([`src/api/mod.rs`](../../src/api/mod.rs)
`cors_layer`; [`conventions.md`](conventions.md) §10).

Consequences for the client:

- **Good:** no cookie is attached automatically, so a cross-site form post
  cannot act as the user. There is no CSRF token to obtain and no same-site
  cookie policy to get right.
- **The cost:** a bearer token is only as safe as where you put it. A browser
  client must keep both tokens out of URLs, logs, crash reports and any
  scriptable storage reachable from injected script; whoever holds the token
  acts as the owner (`ADR-0003` §5). This is accepted risk AR-2 in
  [`../security/threat-model.md`](../security/threat-model.md) and `KI-007` in
  [`../engineering/known-issues.md`](../engineering/known-issues.md), and the
  HTTP-only-cookie alternative is still an open question (Q-02 in
  [`../engineering/open-questions.md`](../engineering/open-questions.md)). If
  the answer to Q-02 changes, this section changes with it.
- Because the credential is a header, CORS must allow `Authorization` — it does
  ([`conventions.md`](conventions.md) §10) — and the deployment must name your
  origin in `api.cors.allowed_origins`. It is **empty** in the committed
  configuration, which disables CORS entirely; a wildcard is refused at
  start-up. Ask the operator to set `APP_CORS_ALLOWED_ORIGINS`.

### 1.7 Rate limiting

`register`, `login` and `refresh` are rate limited per process, keyed by the
peer address; the committed budget is **10 requests per 60-second window**
(`api.rate_limit.max_requests` and `window_seconds` in
[`config/api.toml`](../../config/api.toml)). Exceeding it answers `429` with
code `rate_limited` and a `retry-after` header carrying the number of seconds
([`src/api/middleware.rs`](../../src/api/middleware.rs) `enforce_rate_limit`;
[`error-contract.md`](error-contract.md) §7). Read the header; do not retry in
a tight loop. If the API sits behind a proxy that does not terminate the
connection, all clients share one budget (`ADR-0003`, "Security and operational
implications").

### 1.8 Getting a credential before you have a login screen

If the database was seeded, two accounts are stable across reseeds:

| Email | Password | Display name |
| ----- | -------- | ------------ |
| `owner@atom.local` | `atom-dev-password-1234` | `Owner` |
| `other@atom.local` | `atom-dev-password-1234` | `Other` |

Sources: [`../database/seeding.md`](../database/seeding.md) §8.1 (the table) and
§12 (why the accounts are stable). The password is published on purpose and the
accounts are live credentials: never seed a database that is reachable from a
network you do not control.

---

## 2. The error contract

### 2.1 One envelope

Every non-2xx response produced by ATOM's error type has this shape
([`src/error.rs`](../../src/error.rs), [`error-contract.md`](error-contract.md)
§1); the exception is the `405` in §2.5.

```json
{
  "error": {
    "code": "validation_failed",
    "message": "The request failed validation.",
    "details": [
      { "field": "password", "message": "must be at least 12 characters long" }
    ]
  }
}
```

| Field | Presence | Use it for |
| ----- | -------- | ---------- |
| `error.code` | always | **switch on this**; it is stable and never changes meaning |
| `error.message` | always | logging and diagnostics; it may be reworded, so never parse it and never show it as the only thing a user sees |
| `error.details` | only when there are field failures | it is **omitted entirely** when empty, not `[]` (`skip_serializing_if` on `ErrorDetail::details`) |
| `error.details[].field` | always, within a detail | the request field name; the same field may appear once per broken rule |
| `error.details[].message` | always, within a detail | what is wrong with that field |

### 2.2 Every code, and the status it travels with

The eleven codes are defined once, in `code` in
[`src/error.rs`](../../src/error.rs), and mapped to statuses by
`AppError::status` in the same file. This table is that mapping
([`error-contract.md`](error-contract.md) §2):

| `error.code` | HTTP | Meaning |
| ------------ | ---- | ------- |
| `bad_request` | `400` | malformed request: body is not JSON, path parameter is not a UUID, query parameter cannot be parsed |
| `unauthorized` | `401` | credential missing, malformed, expired, mis-signed, wrong issuer/audience, account gone; wrong password; unknown, expired or reused refresh token |
| `forbidden` | `403` | credential valid, action not permitted: account disabled, or revoking somebody else's session |
| `not_found` | `404` | the resource does not exist **or** the caller may not know that it does; also an unmatched route |
| `conflict` | `409` | the request conflicts with the current state: duplicate email, an illegal stage change, items on a non-draft claim |
| `payload_too_large` | `413` | body over the limit (1 MiB by default, `app.limits.max_body_bytes`) |
| `validation_failed` | `422` | well-formed request that breaks a business rule |
| `rate_limited` | `429` | authentication rate limit; carries `retry-after` |
| `timeout` | `408` | the server abandoned the request after `app.limits.request_timeout_seconds` (30 s default) |
| `database_error` | `500` | the database failed; the driver message is never returned |
| `internal_error` | `500` | unexpected failure; the detail is never returned |

Both `500` codes render the same message, `The request could not be
completed.`, and never carry details (`src/error.rs` `body`).

### 2.3 `400` versus `422` — the split is deliberate

| Situation | Result |
| --------- | ------ |
| Body is not JSON at all — syntax error, or empty body | `400 bad_request` |
| Body is JSON but does not match the contract — missing required field, wrong type, unknown enum variant | `422 validation_failed` |
| Path parameter is not a UUID | `400 bad_request` |
| Query parameter cannot be parsed — `?limit=abc` | `400 bad_request` |
| Query parameter has a value outside the contract — `?status=archived` | `400 bad_request` |

Sources: [`src/api/extract.rs`](../../src/api/extract.rs) (the JSON extractor:
`error.is_syntax() || error.is_eof()` → `400`, everything else → `422` with one
detail) and [`error-contract.md`](error-contract.md) §4. The asymmetry is
intentional: an unknown enum in a **body** is a business-rule failure, while the
same value in a **query string** could not be parsed into the parameter type at
all. `tests/list_query.rs` `a_malformed_list_parameter_is_a_bad_request`
asserts the query side.

### 2.4 A well-formed body of the wrong shape reports `body`, not your field

If the JSON parses but does not fit the contract, the single detail names the
field **`body`**, with the deserialiser's own description:

```json
{
  "error": {
    "code": "validation_failed",
    "message": "The request failed validation.",
    "details": [
      {
        "field": "body",
        "message": "unknown variant `archived`, expected one of `open`, `in_progress`, `done`"
      }
    ]
  }
}
```

This surprises every client once: the developer expects
`details[0].field = "status"` and gets `"body"`. It happens in
[`src/api/extract.rs`](../../src/api/extract.rs), in the branch that is neither
a syntax error nor end-of-input, and it is documented in
[`error-contract.md`](error-contract.md) §3.

Practical rule: map known field names to inputs, and fall back to a form-level
message when `field` is `body` (or when the detail is missing altogether, as it
is for a database `CHECK` the application disagrees with).

A deserialisation failure is also why a **JSON number in a money field** comes
back as `422` with `field: "body"` rather than the field name — see §4.

### 2.5 Where the envelope does and does not appear

Two failures never pass through a handler, and both still use the envelope:

- an unmatched route — the router's fallback is `middleware::not_found`, which
  returns code `not_found` and the message `The requested route does not
  exist.`;
- a panic — `handle_panic` returns `500 internal_error`.

One does not: **`405`**. A method that an existing path does not declare is
answered by the router itself, outside `AppError`; the repository's tests assert
the status and nothing about the body
(`post_notes_search_is_not_shadowed_by_the_note_read` asserts
`StatusCode::METHOD_NOT_ALLOWED` only). Do not assume a `405` carries the
envelope — check the status first (§9, item 4).

---

## 3. Pagination

Every list endpoint takes the same two parameters and returns the same block.

| Parameter | Type | Committed default | Rule |
| --------- | ---- | ----------------- | ---- |
| `limit` | integer | `20` | above `max_limit` it is clamped to it; zero or negative falls back to the default |
| `offset` | integer | `0` | negative falls back to `0` |

The bounds are **configured values**, resolved once at start-up:
`api.pagination.default_limit` = `20` and `api.pagination.max_limit` = `100` in
[`config/api.toml`](../../config/api.toml). The clamp is
`PaginationQuery::effective_limit` / `effective_offset` in
[`src/pagination.rs`](../../src/pagination.rs); the unit tests there pin the
clamp, the configured maximum and the non-positive fallback. Changing either
bound is a breaking change for a client
([`conventions.md`](conventions.md) §5).

```json
{
  "items": [ { "...": "the resources on this page" } ],
  "pagination": {
    "limit": 20,
    "offset": 0,
    "total": 42,
    "has_more": true
  }
}
```

| Field | Meaning |
| ----- | ------- |
| `pagination.limit` | the limit **applied**, after clamping — echo it, do not assume yours won |
| `pagination.offset` | the offset **applied**, after clamping |
| `pagination.total` | how many rows match the **filter**, not how many exist |
| `pagination.has_more` | `offset + limit < total` |

`Pagination::new` computes `has_more` exactly that way
([`src/pagination.rs`](../../src/pagination.rs)). Two consequences a UI must
respect:

- **`total` is the filtered count.** With `?status=submitted` it counts
  submitted rows only; "42 claims" on that screen means 42 matching claims, not
  42 claims in the account. It is also what you page with: stop when
  `has_more` is `false`, using `offset += pagination.limit`.
- A parameter that is not an integer is `400`, not `422` (§2.3).

Search results reuse the same block but not the same meaning of `total` in
`hybrid` mode — see §7.5.

---

## 4. Money

A money value is a **JSON string with exactly four decimal places**. Never a
number, never a float, never a formatted amount
([`conventions.md`](conventions.md) §15,
[`ADR-0007`](../decisions/ADR-0007-money-representation.md)).

```json
{ "amount": "125.5000", "currency": "EUR" }
```

| Rule | Detail |
| ---- | ------ |
| Accepted form | `^[0-9]{1,15}\.[0-9]{4}$`, enforced by `validate_money` in [`src/helpers/validation.rs`](../../src/helpers/validation.rs) |
| Examples | `"125.5000"`, `"0.0000"`, `"999999999999999.9999"` (the maximum) |
| Rejected | `12.5` (number), `"12.5"`, `"12.500"`, `"1,000.0000"`, `"1e3"`, `"-1.0000"`, and a sixteenth integer digit |
| Rounding | **none.** A value that is not already at the scale is refused, never rounded |
| Currency | separate field, ISO 4217, three letters, trimmed and upper-cased (`"eur"` is stored as `"EUR"`) |
| One currency per claim | the claim carries it; its items carry none and are denominated in it |
| Totals | computed by the server from the items and read-only in a request |
| Storage | `NUMERIC(19,4)`, selected as text, so the wire form is exactly four decimals |

The affected fields are `total_amount` on a claim, `amount` on a claim item, and
`salary_min`/`salary_max` on a job application (with `salary_currency` beside
them). The OpenAPI schema publishes each of those as `type: string` with the
four-decimal pattern, never `type: number`
(`the_claim_money_fields_are_strings_in_the_document` in
[`tests/openapi_contract.rs`](../../tests/openapi_contract.rs)), so a generated
client hands you a string.

What this means in practice:

- **Not matching the rule is `422`, not a rounded value.** `"12.500"` on an item
  is refused with `details[0].field = "amount"`
  (`an_item_amount_must_be_a_four_decimal_string` in
  [`tests/claims_api.rs`](../../tests/claims_api.rs)).
- **A JSON number is a different failure.** `"amount": 12.5` never reaches the
  money rule — the body does not deserialize — so it is `422` with
  `details[0].field = "body"` (§2.4). Same test.
- **`"0.0000"` is a legal value** and the total of a claim with no items. A
  zero-amount item is accepted; a claim whose total is zero **cannot be
  submitted** — that is a `422` naming `total_amount`
  (`a_claim_with_a_zero_total_cannot_be_submitted`).
- **Amounts above the range are refused per field.** Two individually legal
  items can still push the total past `NUMERIC(19,4)`; the write is rolled back
  and the answer names `amount` (`src/reimbursements/service.rs`,
  `ItemWrite::TotalExceedsMaximum`).
- **A client-supplied total is ignored, not stored.** `total_amount` in a create
  or patch body is accepted and has no effect
  (`the_total_is_not_accepted_from_the_client`).
- A salary bound without `salary_currency`, or a currency with neither bound, is
  a `422` naming `salary_currency`; `salary_min` greater than `salary_max` is
  also `422` (`src/job_applications/service.rs`, `resolve_salary`).

A similarity score is **not** money: it is a JSON number in `[0,1]` (§7.5).

---

## 5. The list endpoints do not filter alike

Every list takes `limit` and `offset`. Everything else differs per feature, and
each endpoint publishes its own set — asserted exactly, in both directions, by
`each_list_endpoint_publishes_its_filter_and_pagination_parameters_once` in
[`tests/openapi_contract.rs`](../../tests/openapi_contract.rs).

### 5.1 The published sets

| Endpoint | Filters besides `limit`/`offset` | Order |
| -------- | -------------------------------- | ----- |
| `GET /api/v1/notes` | `search`, `archived` | `created_at DESC, id DESC` |
| `GET /api/v1/projects` | `search` | `created_at DESC, id DESC` |
| `GET /api/v1/projects/{project_id}/tasks` | `status`, `priority` | `position ASC, created_at ASC, id ASC` |
| `GET /api/v1/claims` | `status`, `currency`, `search`, `submitted_from`, `submitted_to` | `created_at DESC, id DESC` |
| `GET /api/v1/companies` | `search` | `lower(name)`, `name`, `id` |
| `GET /api/v1/job-applications` | `status`, `company_id`, `search`, `applied_from`, `applied_to` | `created_at DESC, id DESC` |
| `GET /api/v1/job-applications/{application_id}/events` | **none** — pagination only | `occurred_at DESC, id DESC` |

Sources: the filter structs in [`src/notes/dto.rs`](../../src/notes/dto.rs)
`NoteQuery`, [`src/projects/dto.rs`](../../src/projects/dto.rs) `ProjectQuery`,
[`src/tasks/dto.rs`](../../src/tasks/dto.rs) `TaskQuery`,
[`src/reimbursements/dto.rs`](../../src/reimbursements/dto.rs) `ClaimQuery`,
[`src/job_applications/dto.rs`](../../src/job_applications/dto.rs)
`CompanyQuery`/`ApplicationQuery`; the events list takes only
`Query<PaginationQuery>` in
[`src/job_applications/handlers.rs`](../../src/job_applications/handlers.rs).
The ordering rules are in [`conventions.md`](conventions.md) §16.3 and, for the
tasks list, in [`../features/tasks/api.md`](../features/tasks/api.md) — see
§10 item 3 for the sentence in `conventions.md` that does not match the SQL.

### 5.2 The value rules differ by *kind* of filter, not by endpoint

| Kind | Form | Failure |
| ---- | ---- | ------- |
| Enum | exact lowercase value, e.g. `status=submitted`, `priority=urgent` | unknown value → `400` (the query could not be parsed) |
| Identifier | UUID, e.g. `company_id=…` | not a UUID → `400` |
| Free text | `search`, trimmed, case-insensitive substring | over 200 characters → `422` naming `search`; empty means "no filter" |
| Date range | `<field>_from` / `<field>_to`, `YYYY-MM-DD`, UTC, inclusive of both ends | `_from` later than `_to` → `422`; a row with a `null` field is excluded when either bound is present |
| Currency | three letters, trimmed and upper-cased | not three letters → `422` naming the field — `currency` on a claim, `salary_currency` on an application |

Sources: [`conventions.md`](conventions.md) §16.2, `validate_search` in
[`src/helpers/validation.rs`](../../src/helpers/validation.rs), and the
`validate_submission_range` / `validate_applied_range` /
`validate_currency`/`validate_salary_order` functions in the two feature
services.

Notes on the individual filters:

- `archived` on notes is a **tri-state in practice**: `true` returns archived
  notes; `false` or an omitted parameter returns active notes. There is no
  "both" (`src/notes/dto.rs` `NoteQuery`).
- Tasks are listed under their project. There is no `/tasks` collection, and a
  project with no tasks is an **empty page**, not a `404`
  (`a_project_without_tasks_is_an_empty_page_and_not_a_not_found` in
  [`tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs)).
- Claims filter on `submitted_at`, so a `draft` claim never matches
  `submitted_from`/`submitted_to` (the SQL makes both comparisons `NULL` and the
  row drops out — `src/reimbursements/repository.rs`).
- Applications filter on `applied_on`, which is `null` while the stage is
  `wishlist`.

### 5.3 Unknown parameters are ignored

An unknown **name** on a list endpoint is dropped by the query parser, so
`?sort=title` does nothing. An unknown **value** of a known enum is a `400`.
There is no client-selectable sort in this version; the order of each list is
fixed as in §5.1 ([`conventions.md`](conventions.md) §16.3).

---

## 6. The two stage machines

Both machines are **server-enforced state**, not a status field a client may
set at will. A UI that computes "is this button enabled" from a naive rule will
be wrong in the ways this section names.

### 6.1 Expense claims — five states, five moves, one reopen

States: `draft`, `submitted`, `approved`, `rejected`, `paid`
(`ClaimStatus` in [`src/reimbursements/models.rs`](../../src/reimbursements/models.rs)).

The whole machine is `ensure_transition` in
[`src/reimbursements/service.rs`](../../src/reimbursements/service.rs):

| From | Action (route) | To |
| ---- | -------------- | -- |
| `draft` | `POST …/submit` | `submitted` |
| `submitted` | `POST …/approve` | `approved` |
| `submitted` | `POST …/reject` | `rejected` |
| `approved` | `POST …/pay` | `paid` |
| `rejected` | `POST …/submit` | `draft` — **the reopen** |

Every other one of the twenty combinations is `409 conflict` — fifteen refused
pairs, all asserted by `every_forbidden_transition_is_refused` in
[`tests/claims_api.rs`](../../tests/claims_api.rs). That includes re-sending the
action that produced the current state: `submit` on a `submitted` claim,
`approve` on an `approved` one, `reject` on a `rejected` one and `pay` on a
`paid` one are all conflicts, unlike the job-application machine below where
sending the current stage is a `200` no-op. `paid` is terminal.

Details a claim UI needs:

- The action is **the route**, not a body field. There is nothing to send but
  the bearer token (`src/reimbursements/service.rs`, `LifecycleAction`).
- **`status` is not patchable.** `PATCH /claims/{id}` with a `status` is
  accepted and ignored; the transition is the only way to move
  (`the_claim_status_is_not_patchable`). The same is true of `currency`.
- **`submit` has two preconditions on a draft**, checked in SQL: at least one
  item, and a total above zero. They fail differently:
  no items → `422` with `details[0].field = "items"`; zero total → `422` with
  `field = "total_amount"` (`explain_refusal` in the service, and
  `a_claim_with_a_zero_total_cannot_be_submitted`).
- **Items are editable only while the claim is a draft.** A create, patch or
  delete on an item of a non-draft claim is `409 conflict`, with the message
  "Items can only be added, changed or deleted while the claim is a draft."
  (`ItemWrite::ClaimNotDraft`, `config/messages.toml`).
- **The reopen clears the decision.** `rejected → draft` sets `submitted_at`
  and `decided_at` back to `null` in the same statement
  (`TRANSITION_SUBMIT` in [`src/reimbursements/repository.rs`](../../src/reimbursements/repository.rs)),
  and it is exempt from the items/total precondition. Do not keep showing the
  old decision timestamp after a reopen; re-read the claim.
- `GET /claims` returns claims **without** items; `GET /claims/{id}` returns
  the claim with its `items` array (`ClaimResponse` versus
  `ClaimDetailResponse` in [`src/reimbursements/dto.rs`](../../src/reimbursements/dto.rs)).
  The items are chronological: `incurred_on`, then `created_at`, then `id`
  (`src/reimbursements/repository.rs`).

### 6.2 Job applications — seven stages, twelve edges, seven accepted no-ops

Stages: `wishlist`, `applied`, `screening`, `interview`, `offer`, `rejected`,
`withdrawn` (`ApplicationStatus` in
[`src/job_applications/models.rs`](../../src/job_applications/models.rs)).

The edges are the constant `ALLOWED_MOVES` in
[`src/job_applications/service.rs`](../../src/job_applications/service.rs), and
`ensure_transition` reads nothing else:

```text
wishlist  → applied  → screening → interview → offer
    ↓          ↓           ↓           ↓
 rejected / withdrawn from each of the four
```

| Property | Value |
| -------- | ----- |
| Stages | 7 |
| Allowed moves | 12 |
| Accepted no-ops | 7 — **every self-pair** |
| Refused pairs | 30 — `409 conflict` |
| Terminal stages | `offer`, `rejected`, `withdrawn` — they appear only as a destination, never as a source |

Three rules that decide the button states:

1. **Re-sending the current stage is `200` and changes nothing.** Not `409`.
   Every field is unchanged, `updated_at` included
   (`sending_the_current_stage_is_accepted_and_changes_nothing` in
   [`tests/job_applications_api.rs`](../../tests/job_applications_api.rs)).
   So "disable the button when the value is unchanged" is a product choice, not
   a correctness one; "the server will reject it" is false.
2. **A refused move is `409` and writes nothing at all.** The application
   update is one guarded statement, so a `PATCH` carrying an illegal stage plus
   other fields fails as a whole. `every_forbidden_stage_change_is_refused` (30
   pairs) asserts the stage and the timestamps did not move.
3. **An application is born in `wishlist` only.** Sending any other stage in the
   create body is `422` with `field = "status"` (`ensure_initial_stage`); every
   later stage is reached through the machine.

The stage is set with `PATCH /job-applications/{id}` and `{ "status": "<stage>" }`
— unlike the claim lifecycle, this machine has no action endpoints.

### 6.3 Do not encode the machines twice

Both tables are defined in exactly one place in the server and the refusal is
derived from the same constant as the guarded write. If the UI needs a
`canMove` table, generate it from the table above, and expect any disagreement
to be an API change, not a UI bug.

---

## 7. ML endpoints and degradation

Three operations, all authenticated, all in
[`src/ml/handlers.rs`](../../src/ml/handlers.rs); the contract is
[`../features/ml/api.md`](../features/ml/api.md) and
[`conventions.md`](conventions.md) §17.

| Method | Path | Success |
| ------ | ---- | ------- |
| `POST` | `/api/v1/notes/search` | `200` `SearchResponse` |
| `POST` | `/api/v1/job-applications/{application_id}/match` | `200` `MatchResponse` |
| `POST` | `/api/v1/ml/reindex` | `200` `ReindexResponse` |

### 7.1 `POST /notes/search` has three modes

The body is `{ "query": "…", "mode": "…" }`; `mode` defaults to `hybrid`. The
pagination parameters go on the **query string**, not in the body
(`src/ml/dto.rs` `SearchRequest`, and
`the_search_endpoint_publishes_pagination_once` in
[`tests/ml_api.rs`](../../tests/ml_api.rs)).

| `mode` | What it does | Needs a model |
| ------ | ------------ | ------------- |
| `lexical` | PostgreSQL full-text match, the same query as `GET /notes?search=` | no |
| `semantic` | cosine similarity against the caller's stored embeddings | yes |
| `hybrid` | a bounded lexical candidate set, re-ranked by cosine similarity — the default | yes |

### 7.2 When ML is off, you get `200` with lexical results

`ml.enabled` is **`false`** in the committed configuration
([`config/ml.toml`](../../config/ml.toml)), so a deployment that does nothing
serves this row:

```json
{
  "requested_mode": "hybrid",
  "mode": "lexical",
  "degraded": true,
  "degraded_reason": "ml_disabled",
  "items": [ { "note": { "…": "…" }, "score": null } ],
  "pagination": { "limit": 20, "offset": 0, "total": 2, "has_more": false }
}
```

**This is a success, not an error.** The UI must render the results. A client
that treats a non-empty `degraded_reason` as a failure blanks a working search
screen.

| Field | Meaning |
| ------ | ------- |
| `requested_mode` | what the client asked for |
| `mode` | what was applied — `lexical` when degraded |
| `degraded` | `true` when the two differ |
| `degraded_reason` | `ml_disabled`, `model_missing` or `inference_timeout`; `null` exactly when `degraded` is `false` |

- The reason is a closed set of three values (`DegradedReason` in
  [`src/ml/dto.rs`](../../src/ml/dto.rs)). Add a case only when the server adds
  one; treat an unknown value as `model_missing`.
- A `lexical` request is **never** degraded, even with no model configured.
- A degraded answer is a *complete* answer: same pagination, same item shapes,
  results present, `total` is the lexical total
  (`search_degrades_to_lexical_when_ml_is_disabled`,
  `a_degraded_response_is_a_complete_response`,
  `a_degraded_request_never_returns_500_or_503` in
  [`tests/ml_api.rs`](../../tests/ml_api.rs)).
- It never returns `409` or `503`. A missing optional model is a configuration
  state, not an outage.

### 7.3 `match` and `reindex` are `409` when ML is off

Neither has a lexical fallback, because a ranking computed without a model
would be a fiction:

- `POST /job-applications/{id}/match` → `409 conflict` when ML is unavailable
  (`matching_returns_conflict_when_ml_is_disabled`).
- `POST /ml/reindex` → `409 conflict` when ML is unavailable
  (`reindex_is_refused_when_ml_is_disabled`).
- `match` with `"mode": "lexical"` in the body is `422`, not `409`: the value is
  not in the accepted set (`lexical_mode_is_refused_by_match`).
- `match` takes `limit` **in the body**, default `10`, accepted range `1..=50`;
  anything else is `422` (`DEFAULT_MATCH_LIMIT` / `MAX_MATCH_LIMIT` in
  `src/ml/dto.rs`).

So the UI can hide or disable "Find matching notes" when `ml.enabled` is false —
but it cannot discover that from the API. There is no "is ML on" endpoint: the
first signal is the degraded search or the `409`. Treat `409` on these two
operations as "feature unavailable", not as a data conflict.

### 7.4 `score` is `null` in lexical mode

`score` is a JSON **number** in `[0,1]`, rounded to four decimals, when a scored
mode was applied, and `null` when the applied mode is `lexical`
(`lexical_mode_returns_a_null_score`; `SearchHit::score` is an `Option<f64>`).
On a degraded response, every score is `null`. Do not sort client-side by
`score` and treat `null` as `0`; the server's order is already the ranking.

### 7.5 `total` does not mean the same thing in every mode

| Applied mode | `pagination.total` |
| ------------ | ------------------ |
| `lexical` (including degraded) | the number of matching notes |
| `hybrid` | the number of hits whose published score is greater than zero, bounded by `ml.candidate_limit` — **not** "every relevant note" |
| `semantic` | the number of the caller's notes that have an embedding |

This is [`conventions.md`](conventions.md) §17.2. Presenting the hybrid `total`
as a result count is the mistake this row exists to prevent.

---

## 8. Versioning, the document, and what is stable

### 8.1 The prefix

- Every product endpoint is under **`/api/v1`**. The string is declared exactly
  once, as `API_PREFIX` in [`src/api/routes.rs`](../../src/api/routes.rs), and
  the router nests it ([`src/api/mod.rs`](../../src/api/mod.rs)).
- The two health probes are **outside** it: `GET /health/live` and
  `GET /health/ready` ([`conventions.md`](conventions.md) §11).
- There is no header negotiation. Send the version you were written against.

### 8.2 The document

- The OpenAPI document is served at **`GET /api/v1/openapi.json`**, with no
  credential (`the_openapi_document_is_served` in
  [`tests/docs_route.rs`](../../tests/docs_route.rs)).
- The explorer is **`GET /api/v1/docs/`** — with the trailing slash;
  `/api/v1/docs` answers `303` to it.
- `info.version` is the crate version from [`Cargo.toml`](../../Cargo.toml)
  (`0.1.0`), and `info.title` is the configured `api.title` (`ATOM API` by
  default), set at run time in `openapi_document`.
- It is generated from the handler annotations, so it cannot silently drift
  from the code, and its operation set is asserted in both directions
  ([`README.md`](README.md) §1).

**The document is not always served.** `api.docs_enabled` is `true` in
[`config/api.toml`](../../config/api.toml) but **`false`** in
[`config/environments/production.toml`](../../config/environments/production.toml),
and the route exists only when the binary was built with the `docs` Cargo
feature ([`src/api/mod.rs`](../../src/api/mod.rs) `mount_docs`). A production
deployment can therefore answer `404` for both paths. Generate your client from
a development or CI instance, and do not make the running app depend on
fetching the document.

### 8.3 What is stable

- The document describes **51 operations** — 49 under `/api/v1` plus the two
  health probes. The number is pinned by `CONTRACT_OPERATIONS = 51` in
  [`tests/openapi_contract.rs`](../../tests/openapi_contract.rs), and the
  inventory is listed in [`README.md`](README.md) §3.
- The version changes only for an **incompatible** change. Adding an optional
  field, adding an endpoint or adding a new error detail does **not** change it
  ([`conventions.md`](conventions.md) §1).
- `error.code` values never change meaning; `error.message` may be reworded
  (§2.1).
- The pagination bounds are part of the published contract, not a tuning knob
  (§3).
- Adding a sort parameter, changing a default limit or removing a field is a
  contract change; treat the document you generated from as a snapshot and
  regenerate deliberately.

### 8.4 Producing a client from the document

There is no generation script in this repository — [`scripts/`](../../scripts)
holds the documentation checkers, the test-database helper and the OpenCode
validators, nothing else. The document is the input; the generator is your
choice. One shape that works:

```bash
# 1. Serve the document from a running development instance.
cargo run &

# 2. Save it.
curl -s http://127.0.0.1:3000/api/v1/openapi.json -o atom-openapi.json

# 3. Generate (example: the OpenAPI Generator CLI, one of several options).
openapi-generator-cli generate \
  -i atom-openapi.json \
  -g typescript-fetch \
  -o ./src/api-client
```

Two properties of the document make the result usable without patching:

- the `bearer` HTTP security scheme is registered by `SecurityAddon` in
  [`src/api/routes.rs`](../../src/api/routes.rs), and the authenticated
  operations carry `security(("bearer" = []))`, so the generator emits a bearer
  credential rather than nothing;
- every money field is a `string` with the four-decimal pattern — the claim
  fields are asserted by `the_claim_money_fields_are_strings_in_the_document`,
  and the salary fields carry the same `schema(pattern = …)` annotation in
  [`src/job_applications/dto.rs`](../../src/job_applications/dto.rs) — so a
  generated model uses a string, not a float (§4).

What the generator cannot express, and you must add by hand: the
refresh-and-retry-once rule (§1.4), the serialisation of refreshes (§1.3),
treating a degraded search as success (§7.2), and the "`field: body`" fallback
(§2.4).

---

## 9. The five things that will trip you

Every one of these is asserted by a test.

1. **`PATCH {}` is valid and changes nothing.** An empty JSON object is a legal
   patch body: the resource is unchanged except `updated_at`, which is still
   stamped. `a_patch_body_with_no_fields_changes_only_the_modification_time` in
   [`tests/claims_api.rs`](../../tests/claims_api.rs) compares every field
   except `updated_at` before and after, then asserts `updated_at` advanced; the
   general rule is [`conventions.md`](conventions.md) §7. A **missing** body is
   a different thing: an empty body is a `400` (§2.3), so send `{}`.
2. **`{"project_id": null}` clears a link; omitting the key leaves it.** The
   field is a tri-state (`UpdateClaimRequest::project_id` in
   [`src/reimbursements/dto.rs`](../../src/reimbursements/dto.rs)): absent =
   leave alone, `null` = clear, value = set.
   `a_claim_can_be_partially_updated` in
   [`tests/claims_api.rs`](../../tests/claims_api.rs) asserts both halves in one
   case. The same tri-state exists
   for `website` on a company; `source`, `applied_on`, `salary_min`,
   `salary_max`, `salary_currency`, `job_url` and `next_action_task_id` on an
   application; and `category` and `receipt_url` on a claim item. On every other
   field, `null` behaves like an omitted key; an empty string is how you clear a
   free-text field such as a description, and it is **not** a way to clear a
   title, which must not be blank (`conventions.md` §7, and the `422` on a blank
   claim title in `a_claim_title_must_not_be_blank`).
3. **Re-sending the current application stage is `200`.** Seven self-pairs are
   accepted no-ops, not `409` (§6.2). A claim behaves the opposite way:
   re-sending the action that produced the current state is `409` (§6.1). Two
   different machines, two different rules.
4. **`GET /notes/search` is `405`, not `400` and not `404`.** The path exists
   and declares only `POST`; Axum matches the static path before the
   `{note_id}` dynamic one and answers the missing method itself
   (`post_notes_search_is_not_shadowed_by_the_note_read` in
   [`tests/ml_api.rs`](../../tests/ml_api.rs)). The `405` does not go through
   the error envelope (§2.5). The search operation is the `POST`.
5. **A row that belongs to another account is `404`, not `403`.** "Not found"
   and "not yours" are the same answer by design, so an identifier cannot be
   probed for existence ([`error-contract.md`](error-contract.md) §6,
   `ADR-0005`). `a_claim_of_another_user_is_invisible` asserts read, update and
   delete all answer `404` with code `not_found`. Consequence for the UI: when
   you hold an id and get `404`, you cannot tell a deleted row from a wrong
   account; both mean "stop showing this".

Two more that are not on the list but cost an afternoon each: `403` is not an
expired session (§1.5), and a `422` may name `body` instead of your field
(§2.4).

---

## 10. Where this document and the code disagree

Recorded here so the disagreement is not silent. None of them is fixed by this
document: the code is what the server does, and the stale sentence is listed for
correction.

1. **`GET /api/v1/notes/search` — `405` or `400`?**
   - The code and the tests say **`405`**: the router registers
     `POST /notes/search` as a static path beside `/notes/{note_id}`, and Axum
     answers an undeclared method on a declared path with `405`
     ([`src/api/routes.rs`](../../src/api/routes.rs) `api_routes`;
     `post_notes_search_is_not_shadowed_by_the_note_read` in
     [`tests/ml_api.rs`](../../tests/ml_api.rs)).
   - [`README.md`](README.md) §3 and `docs/features/ml/testing.md` still say the
     dynamic `{note_id}` segment matches first and answers `400`, which is also
     what the comment above `ROUTE_TABLE` in
     [`src/api/routes.rs`](../../src/api/routes.rs) says.
   - [`../features/ml/api.md`](../features/ml/api.md) §1 states `405`, and its
     own test file records the disagreement. This document follows the code:
     `405`.
2. **Is the `Bearer` scheme matched case-insensitively?**
   - [`conventions.md`](conventions.md) §12 says the scheme is matched
     case-insensitively.
   - [`src/api/middleware.rs`](../../src/api/middleware.rs) accepts exactly two
     spellings: `Bearer ` and `bearer `. `BEARER <token>` is rejected with
     `401`. Send `Bearer`, which is what every HTTP client library does.
3. **Are the task listings newest-first?**
   - [`conventions.md`](conventions.md) §16.3 states the general rule "a list of
     records is newest first: `<created_at> DESC, id DESC`".
   - The SQL for the tasks list is `ORDER BY t.position ASC, t.created_at ASC,
     t.id ASC` ([`src/tasks/repository.rs`](../../src/tasks/repository.rs)
     `list_for_project`), and
     [`../features/tasks/api.md`](../features/tasks/api.md) documents that same
     order because `TASK-R04` is the manual-ordering requirement. §5.1 above and
     the observed behaviour follow the SQL: tasks are in manual order, not
     newest-first.

The first is a documentation defect with no behavioural risk; the second is a
narrower implementation than the document promises; the third is a general
sentence that does not hold for one endpoint, which is also what the task
feature's own document already assumes. All three belong in
[`../engineering/known-issues.md`](../engineering/known-issues.md) — they are
recorded here because this change owns no other document.

---

## 11. Where to go next

| Question | Document |
| -------- | -------- |
| What does this endpoint take and return? | the feature's `api.md`, indexed in [`README.md`](README.md) |
| Why did I get this status code? | [`error-contract.md`](error-contract.md) |
| What are the naming, PATCH, timestamp and CORS rules? | [`conventions.md`](conventions.md) |
| How is the data modelled? | [`../database/schema.md`](../database/schema.md) |
| Why is a foreign row a `404`? | [`error-contract.md`](error-contract.md) §6, [`ADR-0005`](../decisions/ADR-0005-ownership-enforced-in-sql.md) |
| Why is money a string? | [`ADR-0007`](../decisions/ADR-0007-money-representation.md) |
| Why can a refresh lock the user out? | [`ADR-0003`](../decisions/ADR-0003-auth-token-design.md) |
| How do I run it locally, with data? | [`../database/README.md`](../database/README.md), [`../database/seeding.md`](../database/seeding.md) |

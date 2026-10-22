# API error contract

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent

Every failure of the ATOM API has the **same shape**. A client can parse
one envelope and be done.

The implementation is `src/error.rs`; the extractors that produce these
errors are in `src/api/extract.rs`.

## 1. The envelope

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

| Field | Type | Presence | Meaning |
| ----- | ---- | -------- | ------- |
| `error.code` | string | always | A **stable, machine-readable** code. A client switches on this. It never changes meaning. |
| `error.message` | string | always | A human-readable summary. It may be reworded; clients must not parse it. |
| `error.details` | array | only when there are field-level details | One entry per failing field. |
| `error.details[].field` | string | always | The name of the offending field, for example `email`, `password` or `title`. |
| `error.details[].message` | string | always | What is wrong with that field. |

`details` is **omitted entirely** when it is empty, not rendered as an empty
array.

```json
{
  "error": {
    "code": "not_found",
    "message": "The requested note does not exist."
  }
}
```

## 2. The error codes

| Code | Status | When it occurs |
| ---- | ------ | -------------- |
| `bad_request` | `400 Bad Request` | The request is malformed: the body is not valid JSON, a path parameter is not a UUID, a query parameter cannot be parsed. |
| `unauthorized` | `401 Unauthorized` | The credential is missing, malformed, expired, signed with the wrong secret, issued for another issuer or audience, or belongs to an account that no longer exists. Also: a wrong password, an unknown email address, and an unknown, expired or reused refresh token. |
| `forbidden` | `403 Forbidden` | The credential is valid but the action is not permitted: the account is disabled, or the caller tried to revoke a session that belongs to somebody else. |
| `not_found` | `404 Not Found` | The resource does not exist, or the caller may not know that it does. Also returned for an unmatched route. |
| `conflict` | `409 Conflict` | The request conflicts with the current state of the resource: the email address is already registered. |
| `payload_too_large` | `413 Payload Too Large` | The request body exceeds the accepted size. |
| `validation_failed` | `422 Unprocessable Entity` | The request is well formed but breaks a business rule: an unknown enum variant in a body, a too-long title, a blank title, a password that does not meet the policy, an email that is not usable, a database `CHECK` constraint that the application disagrees with. |
| `rate_limited` | `429 Too Many Requests` | The client exceeded the rate limit on an authentication endpoint. A `retry-after` header gives the number of seconds until the window resets. |
| `timeout` | `408 Request Timeout` | The server stopped processing the request after the configured timeout, `APP_REQUEST_TIMEOUT_SECONDS` (30 seconds by default). |
| `database_error` | `500 Internal Server Error` | The database failed. The driver message is neither logged nor returned: only the SQLSTATE and the error kind are logged. |
| `internal_error` | `500 Internal Server Error` | An unexpected failure. The internal detail is logged, never returned. |

The `code` of a validation error is always `validation_failed` in the current
implementation. The error type allows a different stable code
(`AppError::validation_with`), but every current use passes
`validation_failed`.

There are exactly three sources of a `422`:

| Source | Where | What the details look like |
| ------ | ----- | ------------------------- |
| A business rule in a service | `validate_email`, `validate_password`, `validate_display_name`, `validate_title`, `validate_content`, `validate_name`, `validate_description`, `validate_search` | One violation per broken rule, naming the field: `email`, `password`, `display_name`, `title`, `content`, `name`, `description` or `search`. |
| A body that parses but does not match the contract | The JSON extractor in `src/api/extract.rs` | A single violation for the field `body`, carrying the deserialiser's own description. |
| A database `CHECK` constraint the application and the database disagree about | `UserRepository::insert` in `src/users/repository.rs`, on SQLSTATE `23514` | No field details. The message is `The request failed a database constraint.`, so that the database message — which can contain the failing row — never reaches the log. |

`validate_search` is the one that can fire on a **list** endpoint: a search
term longer than 200 characters is a `422` naming the field `search`.

## 3. Field-level validation details

A `422` response names the fields that failed, so a client can put the
message next to the right input.

```json
{
  "error": {
    "code": "validation_failed",
    "message": "The request failed validation.",
    "details": [
      { "field": "password", "message": "must be at least 12 characters long" },
      { "field": "password", "message": "must contain at least one uppercase letter" }
    ]
  }
}
```

- A field may appear more than once, once per broken rule. The password
  policy reports every unmet rule at once, so a client can show them all.
- The field name is the **request field name**, not a path or a JSON path.
- A body that parses as JSON but does not match the contract produces a
  single violation for the field `body`, whose message is the deserialiser's
  own description, for example
  `unknown variant \`archived\`, expected one of \`open\`, \`in_progress\`, \`done\``.

## 4. `400` versus `422`

The distinction is deliberate and is the one a client most often gets wrong.

| Situation | Result |
| --------- | ------ |
| The body is not JSON at all — a syntax error, or an empty body | `400 bad_request` |
| The body is JSON but does not match the contract — a missing field, a wrong type, an unknown enum variant | `422 validation_failed` |
| A path parameter is not a UUID | `400 bad_request` |
| A query parameter cannot be parsed — `limit=abc` | `400 bad_request` |
| A query parameter has a value outside the contract — `?status=archived` | `400 bad_request` |

Note the last two rows. An unknown enum variant in a **body** is a `422`,
because the body is well formed and the value breaks a business rule. The
same value in a **query string** is a `400`, because the query string could
not be parsed into the parameter type at all. The two are different failures
and the API reports them differently on purpose.

## 5. Internal details are never exposed

- `database_error` and `internal_error` both render the message
  `The request could not be completed.` The internal string and the stack
  context stay in the log.
- **A database failure is not logged with the driver message.** A PostgreSQL
  message can contain the failing row, which for the `users` table includes
  the password hash. The log line therefore carries the SQLSTATE and the
  error kind only, and nothing else.
- The body of an internal error never contains the database URL, the signing
  secret, a password, a token or a hash.
- A password is never echoed back, not even in a validation message.
- A secret is never logged either. The `Debug` implementations of
  `DatabaseSettings` and `AuthSettings` replace the URL and the signing secret
  with `[REDACTED]`.
- The unit tests `internal_errors_do_not_leak_details` and
  `database_errors_do_not_leak_driver_messages` in `src/error.rs` assert
  this behaviour.

## 6. `404` instead of `403`

A resource that the caller does not own returns **`404`**, never `403`.

The reason is disclosure: a `403` confirms that the identifier exists, which
leaks the shape of another account's data to a caller who guessed or
obtained an identifier. The API therefore treats "does not exist" and "does
not belong to you" as the same outcome, and the repository enforces it by
scoping every query with the caller's `user_id` rather than by checking
ownership in the handler.

The one place where `403` is used is an action on a session: `POST
/api/v1/auth/logout` answers `403` when the refresh token in the body
belongs to another account. A session is not a resource that a client
enumerates by identifier, so its existence is not disclosed by the answer.

The `not_found` and `forbidden` codes remain distinct, and the unit test
`not_found_and_forbidden_map_to_distinct_codes` in `src/error.rs` asserts
that.

## 7. The `retry-after` header

A `429` response carries a `retry-after` header with the number of seconds
until the rate-limit window resets:

```http
HTTP/1.1 429 Too Many Requests
retry-after: 42
content-type: application/json
```

```json
{
  "error": {
    "code": "rate_limited",
    "message": "Too many requests. Try again later."
  }
}
```

The value is at least 1, because a window that has already elapsed still
needs a moment to reset.

## 8. Errors outside the envelope

Two responses are not produced by the error type and are worth naming so
that a client is not surprised:

| Response | When | Body |
| -------- | ---- | ---- |
| `404` for an unmatched route | The path does not match any registered route | The standard envelope, with the code `not_found` and the message `The requested route does not exist.` |
| `500` after a panic | A handler panicked | The standard envelope, with the code `internal_error`. The panic payload is deliberately not logged and never returned. |

Every other failure — including every extractor rejection and the request
timeout — goes through `AppError` and therefore through the standard
envelope.

## 9. Related documents

- [`conventions.md`](conventions.md) — the rules that decide *which* status
  applies.
- [`../features/README.md`](../features/README.md) — the per-feature
  endpoint references, which list the error cases of each endpoint.
- [`../architecture/request-lifecycle.md`](../architecture/request-lifecycle.md)
  — how an error travels from the repository back to the socket.
- [`../project-truth.md`](../project-truth.md) — the agreed requirements in
  §7, §9 and §11.

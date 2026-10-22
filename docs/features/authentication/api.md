# Authentication — API

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-AUTH-001`](specification.md)

The HTTP contract of the authentication feature. Every path is relative to
the API root. The general rules — versioning, content types, the error
envelope — are in [`conventions.md`](../../api/conventions.md) and
[`error-contract.md`](../../api/error-contract.md).

## 1. Endpoint summary

| Method | Path | Authentication | Success |
| ------ | ---- | -------------- | ------- |
| `POST` | `/api/v1/auth/register` | none | `201` `AuthResponse` |
| `POST` | `/api/v1/auth/login` | none | `200` `AuthResponse` |
| `POST` | `/api/v1/auth/refresh` | none | `200` `AuthResponse` |
| `POST` | `/api/v1/auth/logout` | bearer token | `204` no body |
| `GET` | `/api/v1/auth/me` | bearer token | `200` `UserResponse` |

Register, login and refresh are rate limited by client IP. Exceeding the
limit returns `429` with a `retry-after` header.

## 2. Shared shapes

### `TokenPair`

```json
{
  "access_token": "<signed JWT>",
  "refresh_token": "<opaque token>",
  "token_type": "Bearer",
  "expires_in": 900,
  "expires_at": "2026-10-21T09:30:00.000000Z"
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `access_token` | string | Signed access token for the `Authorization` header. |
| `refresh_token` | string | Opaque refresh token. Single use; store it, because it is not recoverable afterwards. |
| `token_type` | string | Always `Bearer`. |
| `expires_in` | integer | Access-token lifetime in seconds. `900` by default. |
| `expires_at` | string | Absolute expiry of the access token, RFC 3339 UTC. |

### `AuthResponse`

```json
{
  "tokens": { "...": "TokenPair" },
  "user": { "...": "UserResponse" }
}
```

### `UserResponse`

```json
{
  "id": "6f0a1b7c-2f8e-4d1a-9c33-7b0f5a2e91d4",
  "email": "ada@example.com",
  "display_name": "Ada",
  "is_active": true,
  "created_at": "2026-10-21T09:15:00.000000Z"
}
```

The representation never contains the password hash, the refresh-token hash
or any other session material.

### Authentication header

```http
Authorization: Bearer <access_token>
```

The scheme is matched case-insensitively. A missing header, a header without
the `Bearer` scheme, an empty token, a malformed token, an expired token, a
token signed with another secret, a token from another issuer or audience,
and a token whose subject no longer exists all produce the same `401`
response.

## 3. `POST /api/v1/auth/register`

Creates an account and returns the first token pair.

**Authentication:** none.
**Rate limited:** yes, by client IP.

### Request

```json
{
  "email": "ada@example.com",
  "password": "Correct-Horse-42",
  "display_name": "Ada"
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `email` | string | yes | Trimmed and lower-cased. Must contain an `@`, a non-empty local part, and a domain that contains a `.` and does not start with one. At most 254 characters, the `MAX_EMAIL_LENGTH` constant. |
| `password` | string | yes | The configured password policy: from `auth.password.min_length` to `auth.password.max_length` characters, 12 to 256 by default. The minimum may be raised but not lowered below twelve characters. At least one uppercase letter, one lowercase letter and one digit. |
| `display_name` | string | yes | Trimmed. 1 to 120 characters. |

### Response — `201 Created`

```json
{
  "tokens": {
    "access_token": "<signed JWT>",
    "refresh_token": "<opaque token>",
    "token_type": "Bearer",
    "expires_in": 900,
    "expires_at": "2026-10-21T09:30:00.000000Z"
  },
  "user": {
    "id": "6f0a1b7c-2f8e-4d1a-9c33-7b0f5a2e91d4",
    "email": "ada@example.com",
    "display_name": "Ada",
    "is_active": true,
    "created_at": "2026-10-21T09:15:00.000000Z"
  }
}
```

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON. |
| `409` | `conflict` | The address is already registered. |
| `422` | `validation_failed` | The email, the password or the display name breaks a rule. `details` names the field. |
| `429` | `rate_limited` | Too many attempts from this client. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/auth/register \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","password":"Correct-Horse-42","display_name":"Ada"}'
```

## 4. `POST /api/v1/auth/login`

Exchanges credentials for a token pair.

**Authentication:** none.
**Rate limited:** yes, by client IP.

### Request

```json
{
  "email": "ada@example.com",
  "password": "Correct-Horse-42"
}
```

The email is normalized before it is looked up; it is not validated. A
malformed address simply matches no account.

### Response — `200 OK`

The same `AuthResponse` as registration.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON. |
| `401` | `unauthorized` | The password is wrong, or the address is unknown. The message is `Invalid email or password.` in both cases. |
| `403` | `forbidden` | The account exists but is disabled. |
| `429` | `rate_limited` | Too many attempts from this client. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","password":"Correct-Horse-42"}'
```

## 5. `POST /api/v1/auth/refresh`

Rotates the token pair. The presented refresh token is revoked and replaced;
the client must store the new one. The replacement **inherits the absolute
deadline** of the session it replaces, so a rotation never extends the
lifetime of a session.

**Authentication:** none. The refresh token in the body is the credential.
**Rate limited:** yes, by client IP.

### Request

```json
{
  "refresh_token": "<opaque token>"
}
```

### Response — `200 OK`

The same `AuthResponse` as registration, with a new `refresh_token`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON. |
| `401` | `unauthorized` | The token is unknown (`The refresh token is invalid.`), expired (`The refresh token has expired.`), or already used (`The refresh token has already been used. All sessions have been revoked.`). Reuse additionally revokes every session of that user. |
| `403` | `forbidden` | The account is disabled. Every session of that user is revoked. |
| `429` | `rate_limited` | Too many attempts from this client. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/auth/refresh \
  -H 'content-type: application/json' \
  -d '{"refresh_token":"<opaque token>"}'
```

## 6. `POST /api/v1/auth/logout`

Revokes one session: the one identified by the refresh token in the body.

**Authentication:** bearer token required.

### Request

```json
{
  "refresh_token": "<opaque token>"
}
```

### Response — `204 No Content`

No body.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `403` | `forbidden` | The refresh token belongs to a different account. The caller's own session is untouched. |

Logging out with a refresh token the server does not know is **not** an
error: the call returns `204`. Logout is idempotent.

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/auth/logout \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"refresh_token":"<opaque token>"}'
```

## 7. `GET /api/v1/auth/me`

Returns the caller's own record.

**Authentication:** bearer token required.

### Request

No body, no query parameters.

### Response — `200 OK`

```json
{
  "id": "6f0a1b7c-2f8e-4d1a-9c33-7b0f5a2e91d4",
  "email": "ada@example.com",
  "display_name": "Ada",
  "is_active": true,
  "created_at": "2026-10-21T09:15:00.000000Z"
}
```

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `403` | `forbidden` | The account is disabled. |

```bash
curl -i http://127.0.0.1:3000/api/v1/auth/me \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 8. Notes on the OpenAPI document

The five operations are registered in `src/api/routes.rs` and appear in the
generated document at `/api/v1/openapi.json`. The `bearer` security scheme is
added to the component registry by the `SecurityAddon` modifier.

The generated document declares every status the handlers can return: `201`
for the create endpoint, `204` with no body for the delete endpoint, and the
error statuses `400`, `401`, `403`, `409`, `413`, `422`, `429` and `500`
where they can occur. `POST /auth/logout` declares `403 forbidden` for a
refresh token that belongs to another account, and `GET /auth/me` declares
`403 forbidden` for a disabled account. The tables above list the error
cases a client has to handle; the generated document is the complete list.

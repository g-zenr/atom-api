# Authentication — specification

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent
> **Feature identifier:** `ATOM-FEAT-AUTH-001`
> **Feature name:** Authentication, authorization context and the token lifecycle

## 1. Purpose and the user problem

ATOM is a single-user, private workspace. Before anything can be stored, the
owner has to be able to prove who they are, and the API has to be able to
trust that proof on every later request.

The problem this feature solves is threefold:

1. **Getting in.** A person needs one account, protected by a password they
   choose, and a way to sign in from any client.
2. **Staying in.** A client must be able to keep working for a long session
   without re-entering the password on every call, and without the API ever
   storing a reusable credential in a recoverable form.
3. **Proving who is calling.** Every private endpoint needs an authenticated
   identity that is derived from the credential alone, never from anything the
   client can assert about itself.

The feature also establishes the **ownership model** the rest of the API
depends on: the identity in a validated access token is the only source of
the `user_id` that scopes every private query.

## 2. Status

**Implemented.** The endpoints, the token lifecycle and the ownership
predicate exist and are covered by integration tests, but at least one
accepted requirement — rate limiting on the authentication endpoints — is
implemented without an HTTP-level test, so the feature is not yet `Verified`.
See [status.md](status.md) for the full evidence and the remaining gaps.

## 3. Scope

In scope:

- Registration with a unique, normalized email address.
- A server-side password policy, enforced before hashing.
- Argon2id password hashing with configurable cost parameters.
- Short-lived signed access tokens (HS256) with issuer and audience
  validation.
- Opaque refresh tokens, stored only as a SHA-256 hash, rotated on every use.
- Detection of refresh-token reuse, with mass revocation of the user's
  sessions.
- Logout of the caller's own session, and revocation of every session of a
  user.
- The authenticated caller context (`AuthUser`) used by every private
  endpoint.
- Rate limiting on register, login and refresh, keyed by client IP.
- The safe public user representation returned to clients.

## 4. Non-goals

These are deliberately **not** part of Phase 1:

- Password reset, email verification and account recovery. There is no mail
  transport in the project.
- Multi-factor authentication.
- Social or external identity providers.
- API keys, personal access tokens or any other second credential type.
- Refresh tokens in HTTP-only cookies, and any CSRF strategy. The decision
  is recorded as an open question in
  [`project-truth.md`](../../project-truth.md) §16.
- Role or permission modelling. Phase 1 is single-owner; there is no notion
  of a shared resource.
- Session listing or a "revoke all sessions" endpoint. The service can revoke
  every session of a user, and does so on token reuse and on a disabled
  account, but no endpoint exposes it yet.
- Token revocation lists. The `jti` claim is issued for that future purpose,
  but no list is maintained.

## 5. Accepted requirements

The source of truth for this list is
[`project-truth.md`](../../project-truth.md) §4.2 and §9. Each row states
where the requirement is implemented and where it is covered.

| ID | Requirement | Implementation | Covered by |
| -- | ----------- | -------------- | ---------- |
| `AUTH-R01` | Registration with a unique normalized email address. | `src/users/service.rs` (`normalize_email`, `validate_email`), `src/users/repository.rs` (`insert`), `users_email_unique_idx` | `tests/auth_api.rs` — `registration_returns_a_token_pair_and_a_safe_user`, `registration_normalises_the_email_address`, `duplicate_registration_is_a_conflict` |
| `AUTH-R02` | Argon2id password hashing, with a server-side password policy. | `src/auth/password.rs`, `src/users/service.rs` (`validate_password`) | `tests/auth_api.rs` — `a_weak_password_is_rejected_with_field_details`; unit tests in `src/auth/password.rs` and `src/users/service.rs` |
| `AUTH-R03` | Short-lived signed access tokens with issuer/audience validation. | `src/auth/token.rs` (`TokenService`) | `tests/auth_api.rs` — `protected_routes_reject_missing_and_invalid_credentials`; unit tests in `src/auth/token.rs` |
| `AUTH-R04` | Opaque refresh tokens, stored hashed, rotated on use. | `src/auth/token.rs` (`generate_refresh_token`, `hash_refresh_token`), `src/auth/service.rs` (`refresh`), `src/auth/repository.rs` | `tests/auth_api.rs` — `refresh_rotates_the_token_pair`; unit test `refresh_tokens_are_unique_and_only_the_hash_is_kept` |
| `AUTH-R05` | Refresh-token reuse detection and mass revocation. | `src/auth/service.rs` (`refresh`), `src/auth/repository.rs` (`revoke_all_for_user`) | `tests/auth_api.rs` — `reusing_a_rotated_refresh_token_revokes_every_session` |
| `AUTH-R06` | Logout, and per-user session revocation. | `src/auth/service.rs` (`logout`, `revoke_all_sessions`), `src/auth/repository.rs` (`revoke`, `revoke_all_for_user`) | `tests/auth_api.rs` — `logout_revokes_the_session`, `an_account_cannot_log_out_another_accounts_session`. The per-user revocation has no endpoint; it is covered at the service level by `tests/auth_api.rs` - `revoking_every_session_invalidates_the_refresh_tokens`. |
| `AUTH-R07` | The authenticated-user context is derived only from the credential. | `src/api/middleware.rs` (`AuthUser`), `src/auth/service.rs` (`authenticate_bearer`) | `tests/auth_api.rs` — `protected_routes_reject_missing_and_invalid_credentials`, `the_current_user_endpoint_returns_the_caller` |
| `AUTH-R08` | Rate limiting on the authentication endpoints. | `src/ratelimit.rs`, `src/api/middleware.rs` (`enforce_rate_limit`), applied in `src/auth/handlers.rs` to register, login and refresh | Covered: `tests/auth_api.rs` - `the_authentication_endpoints_are_rate_limited` drives the login endpoint past the limit and asserts `429` and the `rate_limited` code. |
| `AUTH-R09` | A safe public user representation with no secret material. | `src/users/dto.rs` (`UserResponse`) | `tests/auth_api.rs` — `registration_returns_a_token_pair_and_a_safe_user` and `the_current_user_endpoint_returns_the_caller` both assert that `password_hash` is absent |

## 6. User-facing behaviour

### 6.1 Registration

- The email address is trimmed and lower-cased before it is stored or
  compared. `Ada@Example.COM` and `ada@example.com` are the same account.
- The email must contain an `@`, a non-empty local part, and a domain that
  contains a `.` and does not start with one. The check is deliberately
  permissive: only delivery can authoritatively validate an address.
- The email is at most 254 characters.
- The password must be between 12 and 256 characters and must contain at
  least one uppercase letter, one lowercase letter and one digit. Every
  unmet rule is reported, so a client can show all of them at once.
- The display name is trimmed and must be between 1 and 120 characters.
- A successful registration returns `201` with a token pair, so the client can
  use the account immediately without a second round trip.
- Registering an address that already exists returns `409` with the code
  `conflict`. The message does not disclose anything beyond the conflict
  itself, which the caller supplied.

### 6.2 Login

- The email is normalized and looked up; the address itself is never
  validated, so a malformed address simply fails to match.
- A wrong password and an unknown address produce the **same** status and the
  **same** message: `Invalid email or password.` with the code
  `unauthorized`.
- When the address is unknown, the service still performs an Argon2
  verification against a hash of an unguessable value, so the response time
  does not reveal whether the account exists.
- A disabled account returns `403` with the code `forbidden` and the message
  `This account is disabled. Contact an administrator.`

### 6.3 Token use

- The access token goes in the `Authorization` header as
  `Authorization: Bearer <token>`. The prefix is matched case-insensitively
  for `Bearer`/`bearer`.
- An access token that is missing, malformed, expired, signed with another
  secret, or issued by another issuer or for another audience is rejected
  with `401` and the code `unauthorized`. The reason is not disclosed.
- A missing, malformed or non-UUID `user_id` in the token, and a token whose
  subject no longer exists, are also `401`.

### 6.4 Refresh

- The presented refresh token is revoked and replaced by a new one. The
  client must persist the new value; the old one is dead.
- Presenting an already-revoked token is treated as reuse: **every** session
  of that user is revoked and the request fails with `401`. The message is
  `The refresh token has already been used. All sessions have been revoked.`
- An expired token returns `401` with the message
  `The refresh token has expired.`
- An unknown token returns `401` with the message
  `The refresh token is invalid.`
- A disabled account causes every session of that user to be revoked and the
  request to fail with `403`.

### 6.5 Logout

- Logout revokes exactly one session: the one identified by the refresh token
  in the body.
- The endpoint requires a valid access token. The token in the body must
  belong to the caller; revoking somebody else's session returns `403`.
- Logging out with a refresh token the server does not know is **not** an
  error: the call returns `204`. Logout is idempotent.

### 6.6 The current user

- `GET /api/v1/auth/me` returns the caller's own record: `id`, `email`,
  `display_name`, `is_active` and `created_at`. It never contains the
  password hash or any session material.

### 6.7 Rate limiting

- Register, login and refresh are rate limited, keyed by the client IP.
- Exceeding the limit returns `429` with the code `rate_limited` and a
  `retry-after` header carrying the number of seconds until the window
  resets.
- The default configuration allows 10 requests per 60 seconds per client.

## 7. Dependencies on other features

| Direction | Relationship |
| --------- | ------------ |
| This feature → Users | The feature owns user creation, the password policy and the public user representation. There is no separate users feature in Phase 1. |
| This feature → Notes, Projects, Tasks | Every one of them consumes the `AuthUser` context this feature produces and uses its `user_id` as the ownership predicate. |
| Notes, Projects, Tasks → this feature | None. A resource never stores a credential; it stores the foreign key to `users`. |

The hard dependency is that **everything private depends on `users` and
`auth`**, as recorded in
[`project-truth.md`](../../project-truth.md) §15.

## 8. Acceptance criteria

A change to this feature is complete when all of the following hold.

1. `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features
   -- -D warnings` and `cargo test --all-targets` pass, with
   `TEST_DATABASE_URL` pointing at a disposable database.
2. A new endpoint has a success case, an unauthenticated case and a
   cross-account case.
3. No password, token, signing secret or database URL appears in a log line,
   an error message or a response body.
4. No refresh token is stored in a recoverable form: only its SHA-256 hash
   reaches the database.
5. Every new failure path produces the documented error envelope
   ([`error-contract.md`](../../api/error-contract.md)), never a hand-built
   response.
6. Every new query that touches user data is scoped by the caller's
   `user_id`.
7. The OpenAPI annotations, this feature's `api.md` and the
   [API conventions](../../api/conventions.md) are updated in the same change.
8. The status in [status.md](status.md) reflects the evidence, and any newly
   closed gap is recorded there.

## 9. Known limitations

- **The rate limiter is per process.** Its counters live in the memory of one
  instance. A deployment with more than one instance needs edge rate limiting
  in front of the API, as recorded in
  [`project-truth.md`](../../project-truth.md) §17.
- **The client IP comes from the connection, not from a header.** The API
  never reads `X-Forwarded-For`, because a client can forge it. Behind a
  reverse proxy every caller therefore shares one bucket unless the proxy
  terminates the connection per client.
- **There is no way to list or revoke sessions from outside.** A caller who
  suspects a compromise must wait for the token to expire or rotate it.
- **There is no idle timeout on refresh tokens.** A session lives for its
  whole absolute lifetime. This is an open question in
  [`project-truth.md`](../../project-truth.md) §16.
- **A disabled account is only detectable at authentication time.** An access
  token that was issued before the account was disabled is rejected on the
  next request, because the account is looked up on every request, but the
  token itself is not revoked.
- **There is no password change or reset.** The only way to change a password
  is to create a new account.

## 10. Changelog references

Substantial changes to this feature are recorded in
[`change-log.md`](../../engineering/change-log.md). The identifier of a
feature is stable; the change entries are appended chronologically and are
never rewritten.

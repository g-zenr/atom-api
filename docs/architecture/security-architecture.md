# ATOM — security architecture

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `architect` agent

The trust model of the Phase 1 backend: where the boundaries are, what
crosses them, and what the architecture guarantees as a result.

The threats, the controls and the accepted residual risks are enumerated in
[`../security/threat-model.md`](../security/threat-model.md). The rules a
change must follow are in
[`../security/secure-development.md`](../security/secure-development.md).
This document describes **the architecture** those rules protect.

---

## 1. Trust boundaries

```text
   untrusted                    the API process                  the database
  ───────────                  ───────────────                  ──────────
  the client  ──TB-1──▶  Axum router, middleware,          ──TB-2──▶  PostgreSQL
  the network            handlers, services, repositories
                         (all the secrets live here)   ──TB-6──▶  process memory
  the operator ──TB-4──▶ configuration, secrets, direct SQL
  the build   ──TB-3──▶  Cargo.lock, the source tree
```

| ID | Boundary | Inside | Outside | The assumption it rests on |
| -- | -------- | ------ | ------- | -------------------------- |
| TB-1 | Network edge → API process | The validated request | Anything the client can send | The client is untrusted. TLS is terminated by the operator, not by the process. |
| TB-2 | API process → PostgreSQL | The pool and the parameterised statements | The database server | The database is not exposed to the network. The process is trusted by the database. |
| TB-3 | Build and dependency supply chain → the shipped binary | `Cargo.lock`, the source tree, the build environment | Registries and transitive dependencies | A dependency is only as trustworthy as its publisher. |
| TB-4 | Operator → host and database | Configuration, secrets, direct SQL | — | The operator is trusted. Insider misuse is out of the API's reach. |
| TB-5 | Client → API (the authentication boundary) | The identity derived from the credential | The identity the client claims | The identity of a caller comes only from the credential. A body never names the caller. |
| TB-6 | Process → process memory | The signing secret, the database URL, the password hashes, the tokens in flight | Log sinks, crash dumps, core files | Secrets exist in memory while the process runs and must not leave it. |

The identifiers match the ones used in
[`../security/threat-model.md`](../security/threat-model.md) §4.

## 2. The authentication boundary

TB-5 is the boundary that matters most in Phase 1, because it is the only
one that a remote, unauthenticated party can probe.

### 2.1 How a caller is identified

The identity of a caller comes **only** from the credential. There is no
request field, header or path parameter that names the caller, and no DTO
carries a `user_id` that the client can set.

The path is:

1. `api::middleware::AuthUser` is a `FromRequestParts<AppState>` extractor.
2. It requires an `Authorization: Bearer <token>` header. The prefix is
   accepted in either case (`Bearer` or `bearer`); anything else is refused.
3. `TokenService::decode_access_token` verifies the HS256 signature and
   validates the issuer, the audience, the subject and the expiry, with a
   30-second clock-skew allowance. The required claims are `exp`, `sub`,
   `iss` and `aud`.
4. `AuthService::authenticate_bearer` loads the user named by `sub` and
   refuses a disabled account.
5. The extractor yields `AuthUser { user_id, email }`.

### 2.2 The failure modes

Every failure of the boundary is designed to be uninformative.

| Failure | Response | What the caller learns |
| ------- | -------- | ---------------------- |
| No `Authorization` header | `401`, code `unauthorized` | Nothing. |
| A header that is not `Bearer <token>` | `401`, code `unauthorized` | Nothing. |
| A token with a bad signature | `401`, code `unauthorized` | Nothing. |
| An expired token | `401`, code `unauthorized` | Nothing. |
| A token from another issuer or audience | `401`, code `unauthorized` | Nothing. |
| A token whose `sub` is not a UUID | `401`, code `unauthorized` | Nothing. |
| A token for a user that no longer exists | `401`, code `unauthorized` | Nothing. |
| A valid token for a disabled account | `403`, code `forbidden` | That the account exists and is disabled. |
| A wrong password at login | `401`, "Invalid email or password." | Nothing. |
| An unknown email address at login | `401`, "Invalid email or password." | Nothing. |
| A reused refresh token | `401`, "The refresh token has already been used. All sessions have been revoked." | That the token was already used. |

The important property is that the **access-token** failures are
indistinguishable from each other: `decode_access_token` maps every failure
onto the same message. The **login** failures are indistinguishable from each
other as well, and the unknown-email path additionally performs a dummy
Argon2 verification so that the two paths take a comparable amount of time.

### 2.3 The token lifecycle

The full design is in
[`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md).

| Token | Form | Lifetime | Storage | Notes |
| ----- | ---- | -------- | ------- | ----- |
| Access token | HS256 JWT with `sub`, `email`, `iss`, `aud`, `exp`, `iat`, `jti` | 15 minutes by default | Not stored | Verified statelessly on every request. |
| Refresh token | 32 random bytes, base64url without padding | 14 days by default | **Only** the SHA-256 hash | Opaque, single-use, rotated on every use. |

Rotation and reuse detection:

- `POST /api/v1/auth/refresh` inserts the replacement session, revokes the
  presented one and links them through `replaced_by_session_id`.
- Presenting an already-revoked token is treated as reuse: **every** active
  session of that account is revoked and the request fails.
- `POST /api/v1/auth/logout` revokes one session. A caller may only revoke
  its own session; another account's session is `403`.

### 2.4 Password handling

- Passwords are hashed with **Argon2id** (`argon2` 0.5), version `0x13`,
  with a 16-byte random salt, and the parameters come from the configuration.
- The memory cost has a floor of **8 MiB**
  (`APP_ARGON2_MEMORY_KIB`), so the hash cannot be made cheap by
  misconfiguration.
- The password length is bounded at 256 characters
  (`MAX_PASSWORD_LENGTH`), so the hashing cost cannot be used as a
  denial-of-service vector.
- The stored form is a PHC string. Verification fails closed: a malformed
  stored hash returns `false` rather than an error.
- The plain password is never stored, never logged and never returned.

## 3. The authorization model

**Ownership is enforced in SQL.** This is the decision recorded in
[`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md).

| Rule | How it is enforced |
| ---- | ------------------ |
| Every statement that touches user data carries the caller's `user_id` | A bound parameter in the repository statement. |
| A task inherits its ownership from its project | A join on `projects`, so `p.user_id = $n` is part of every task statement. |
| A resource the caller does not own is `404`, never `403` | The repository returns no row; the service turns that into `AppError::NotFound`. |
| The caller never names itself | The owner id comes from `AuthUser`, and no DTO carries a settable `user_id`. |
| A disabled account cannot use a valid token | `AuthService::authenticate_bearer` checks `is_active` on every request. |

There is exactly one role: the owner of the resource. There is no
role table, no permission table and no sharing model in Phase 1.

The consequence to keep in mind: the ownership predicate is a **convention
that a reviewer must check**, not a property the compiler enforces. Every new
statement is a chance to get it wrong, which is why
[`../security/secure-development.md`](../security/secure-development.md) SD-2
exists and why the integration suite contains cross-account isolation tests
for every private endpoint.

## 4. Secret handling

| Secret | Source | Where it lives | How it is protected |
| ------ | ------ | -------------- | ------------------ |
| The JWT signing secret | `APP_JWT_SECRET` | In `AuthConfig.jwt_secret`, then inside `TokenService` | At least 32 bytes; the documented placeholder is refused in `Production`; `AuthConfig`'s `Debug` prints `[REDACTED]`. |
| The database URL | `DATABASE_URL` | In `DatabaseConfig.url` | `DatabaseConfig`'s `Debug` prints `[REDACTED]`. |
| Password hashes | The database | In `users.password_hash` | Argon2id. Never returned: `UserResponse` has no such field. |
| Refresh tokens | Generated by the server | Only the SHA-256 hash is stored | A database disclosure does not yield a usable token. |
| Access tokens | Issued to the client | In the client | Short-lived, signed, never logged. |

Rules that hold everywhere:

- **No secret is ever logged.** `AppError` renders a fixed public body and
  logs the detail server-side; the detail is a SQLx error or an internal
  string, never a credential.
- **No secret is ever returned.** `AppError::Database` and
  `AppError::Internal` answer "The request could not be completed."
- **The panic payload is deliberately not logged.** `routes::handle_panic`
  logs a fixed message and answers a generic `500`, so a panic that happened
  to carry a secret cannot leak it through the log either.
- **`.env` is git-ignored**, and `.env.example` contains placeholders only.
  The documented placeholder secret is refused when the environment is
  `Production`.

The signing secret has **no rotation procedure**. There is no key identifier
in the token and no revocation list, so a compromised secret cannot be
retired without restarting every instance with a new one.

## 5. The CORS allowlist

- The allowlist comes from `APP_CORS_ALLOWED_ORIGINS`, a comma-separated
  list.
- **An empty list disables CORS entirely.** No layer is installed, which is
  the correct behaviour for a deployment that is not reachable from a
  browser.
- **A `*` entry trips a start-up assertion.** The router construction
  aborts the process before it serves a single request, rather than shipping
  a wildcard origin.
- The allowed methods and request headers are fixed in the source; only the
  origins are configurable.
- **Credentials are not enabled**, so the API never answers a request with
  `Access-Control-Allow-Credentials`.
- The layer is the outermost one, so a preflight is answered before
  authentication and before the rate limit.

## 6. Rate limiting

`src/ratelimit.rs` implements a fixed-window counter keyed by the client
address. It is applied to three endpoints: `POST /api/v1/auth/register`,
`POST /api/v1/auth/login` and `POST /api/v1/auth/refresh`.

```text
client address ──▶ ClientIp ──▶ RateLimiter::check ──▶ Allowed | Rejected
                                                            │
                                                            └─▶ 429 + retry-after
```

### 6.1 The client address

`ClientIp` reads the `ConnectInfo<SocketAddr>` extension and
**deliberately ignores `X-Forwarded-For`**. The header is client-controlled
unless a trusted reverse proxy overwrites it, so using it would let a caller
forge the rate-limit key and get an unlimited budget.

### 6.2 The limitations, stated plainly

| Limitation | Consequence |
| ---------- | ----------- |
| The counters live in the process memory. | A horizontally scaled deployment has *n* times the intended budget. Edge rate limiting is required in front of the API. |
| The server did not install the connection information, so `ClientIp` resolved to `None` for every request and all callers shared one bucket. | **Fixed.** `src/main.rs` serves the router through `into_make_service_with_connect_info::<SocketAddr>()`, so the limiter is keyed by the peer address. A deployment behind a proxy that does not terminate the connection still shares one bucket, because `X-Forwarded-For` is deliberately not read. |
| The algorithm is a fixed window. | A caller can spend the whole budget at the end of one window and again at the start of the next. |
| The limiter fails open when its mutex is poisoned. | A poisoned lock degrades the control rather than the availability of the API. |
| There is no HTTP-level test of the limiter. | The `429` path is not covered by the suite. |

The limiter is a **defence-in-depth control**, not the primary protection.
The primary protection for the authentication endpoints is the Argon2 cost
and the generic failure message; the limiter only raises the cost of a
sustained attack.

## 7. Error information disclosure

The rule is one sentence: **a client learns what it did, never what the
server is.**

| Failure | What the client sees | What stays on the server |
| ------- | -------------------- | ------------------------- |
| Validation | The field names and the messages | Nothing. |
| Authentication | One generic message | The exact reason, in the log. |
| A disabled account | "This account is disabled. Contact an administrator." | The account state, in the log. |
| Not found | "The requested note does not exist." | Nothing. |
| Conflict | "An account with this email address already exists." | Nothing. |
| Database failure | "The request could not be completed." | The SQLx error, at `error` level. |
| Internal failure | "The request could not be completed." | The internal string, at `error` level. |
| Panic | "The request could not be completed." | A fixed message; the payload is deliberately not logged. |

Two responses escape the envelope and are produced by a layer rather than by
`AppError`: the `408` from the timeout layer and Axum's own `405 Method
Not Allowed` for a wrong method on a known path. Neither carries internal
detail.

## 8. The security properties the architecture guarantees

These are properties of the **structure**, not of a particular check. They
hold for any code written inside the rules.

1. **A caller cannot become another caller.** The identity is derived from
   the credential alone, and no request field can override it.
2. **A caller cannot reach another caller's data.** The ownership predicate
   is part of the SQL statement, so a missing predicate is a code-review
   failure, not a runtime branch that can be skipped.
3. **The existence of a resource is not disclosed.** An inaccessible
   resource is `404`, never `403`.
4. **A credential guess does not tell the attacker anything.** Every login
   failure carries the same message, and the unknown-account path spends a
   comparable amount of time.
5. **A stolen database does not yield usable credentials.** Passwords are
   Argon2id hashes and refresh tokens are SHA-256 hashes.
6. **A bug does not leak the internals.** `AppError` has one rendering, and
   it redacts the server-side detail.
7. **A panic does not take the process down for the client.** It becomes a
   `500` in the standard envelope.
8. **A slow or large request is bounded.** A body limit, a request timeout
   and a bounded password length cap the resources one request can consume.
9. **A wildcard CORS origin cannot be deployed.** The start-up assertion
   refuses it.
10. **A forged client address cannot buy more requests.** The
    `X-Forwarded-For` header is never consulted.

## 9. What is **not** implemented

| Control | State |
| ------- | ----- |
| TLS termination inside the process | Not implemented. The API speaks plain HTTP/1.1 and expects a proxy. |
| Edge rate limiting | Not implemented. It is an operator obligation in the production checklist. |
| A trusted-proxy declaration | Not implemented. `X-Forwarded-For` is ignored entirely, and there is no configured list of trusted proxies. Open question Q-07. |
| Account lockout, breached-password checking | Not implemented. Accepted risk AR-6. |
| A session-management surface for the user | Not implemented. The user cannot list or revoke their own sessions, and cannot change their password. Open question Q-08. |
| A signing-secret rotation procedure | Not implemented. |
| A security audit trail, alerting, metrics | Not implemented. Only structured logs and a correlation id exist. |
| Dependency and container scanning | Not implemented. There is no CI pipeline and no container image. |
| Row-level security in PostgreSQL | Not implemented. Ownership is enforced in the application layer; see ADR-0005 for why. |

## 10. Related documents

- [`../security/threat-model.md`](../security/threat-model.md) — the threats,
  the controls and the accepted risks.
- [`../security/secure-development.md`](../security/secure-development.md) —
  the rules a change must follow.
- [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md)
  — the token design.
- [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md)
  — the authorization decision.
- [`request-lifecycle.md`](request-lifecycle.md) — where each control sits in
  the request path.
- [`deployment.md`](deployment.md) — the operator obligations.

# ADR-0003 — The access and refresh token design

> **Status:** Accepted
> **Date:** 2026-10-21
> **Owner:** architect
> **Amended:** 2026-10-31 — the refresh-token rotation is one transaction
> (owner decision `Q-19`); see the amendment section below.

## Context

Phase 1 has to authenticate a caller and keep that caller authenticated
across more than one request, without storing a session for every request and
without the API keeping state that has to be shared between instances.

The requirements are in [`../project-truth.md`](../project-truth.md) §4.2 and
§9:

- short-lived **signed** access tokens with issuer and audience validation;
- **opaque** refresh tokens, stored hashed, rotated on use;
- **refresh-token reuse detection and mass revocation**;
- logout and per-user session revocation;
- an authenticated-user context derived **only** from the credential;
- passwords never stored in plaintext and never logged;
- **generic authentication failures**, so that an attacker cannot learn
  whether an account exists;
- the signing secret comes from the environment and is at least 32 bytes.

Two decisions were open when this ADR was taken, and both are still open:

- **Q2** — should the refresh token live in an HTTP-only cookie instead of
  the response body? The answer changes the client design and adds CSRF
  obligations; it does not change the token format.
- **Q3** — what is the refresh token's absolute lifetime and idle timeout?
  Phase 1 implements one absolute lifetime and no idle timeout.

Both are tracked in [`../engineering/open-questions.md`](../engineering/open-questions.md)
as Q-02 and Q-03.

The clients are not built yet. The product is expected to have a web client
and a mobile client, and the API is CORS-enabled against an explicit origin
allowlist for the future web client. A token design therefore has to work for
a browser, a mobile application and a command-line client.

## Decision

**ATOM issues a short-lived HS256 access token and an opaque refresh token.
The access token is verified statelessly; the refresh token is stored only as
a hash, rotated on every use, and treated as compromised the moment it is
presented twice.**

### 1. The access token

- **HS256**, signed with the secret from `APP_JWT_SECRET`.
- Claims: `sub` (the user id), `email`, `iss`, `aud`, `exp`, `iat`, `jti`.
- Validation on every request: the signature, the algorithm, the issuer, the
  audience, the expiry, with a 30-second clock-skew allowance. The required
  claims are `exp`, `sub`, `iss` and `aud`.
- **Lifetime: 15 minutes** by default (`APP_ACCESS_TOKEN_TTL_SECONDS`).
- **Not stored server-side.** Verification needs only the signing secret, so
  any instance can verify a token without a shared store.
- Every failure of the verification — bad signature, expired, wrong issuer,
  wrong audience — produces the **same** error and the same message.

### 2. The refresh token

- **Opaque**: 32 bytes from the CSPRNG, base64url without padding. The
  server carries no structure in it.
- **Stored only as its SHA-256 hash**, hex-encoded, in
  `refresh_sessions.token_hash`. A database disclosure does not yield a
  usable token. SHA-256 is appropriate here because the token is a
  high-entropy random value, not a low-entropy password.
- **Single use.** `POST /api/v1/auth/refresh` revokes the presented session
  and issues a replacement; the replacement is linked to the old one through
  `replaced_by_session_id`.
- **Reuse detection.** Presenting a token that has already been revoked is
  treated as theft: **every** active session of that account is revoked and
  the request fails.
- **Lifetime: 14 days** by default
  (`APP_REFRESH_TOKEN_TTL_SECONDS`), and the configuration refuses a refresh
  lifetime that is not longer than the access-token lifetime.
- **The rotation does not extend the session.** The replacement session
  inherits the original `expires_at`, so the absolute lifetime is measured
  from the first login and a rotation cannot push it out. A stolen refresh
  token therefore has a fixed upper bound, which is the property that makes
  the absolute lifetime worth having.
- The session row also records the user agent and the client address, for a
  future session-management surface.

### 3. Passwords

- **Argon2id** (`argon2` 0.5, version `0x13`) with a 16-byte random salt
  and a PHC-formatted stored hash.
- The parameters come from the configuration, with a **memory floor of
  8 MiB** so that the hash cannot be made cheap by misconfiguration.
- The password length is capped at 256 characters, so the hashing cost
  cannot be used as a denial-of-service vector.
- Verification **fails closed**: a malformed stored hash returns `false`
  rather than an error.

### 4. Authentication failures

- Every authentication failure carries **one generic message**:
  "Invalid email or password." A wrong password and an unknown address are
  indistinguishable.
- The unknown-address path performs a dummy Argon2 verification, so that the
  two paths take a comparable amount of time.
- The access-token failures are indistinguishable from each other for the
  same reason.

### 5. Bearer-token storage guidance for future clients

This decision does not bind the future clients, but it constrains them:

- An access token is a **bearer** credential. Whoever holds it acts as the
  owner. It must be kept out of scriptable storage on a browser platform,
  out of URLs, out of logs and out of crash reports.
- The refresh token is a **bearer** credential with the same requirement,
  and it is the more valuable of the two, because it is long-lived. This is
  the argument for the HTTP-only cookie in Q2.
- The clients must be prepared for the refresh token to be **rotated on
  every use**: a client that caches a refresh token and replays it will trip
  the reuse detection and revoke every session of the account.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **Server-side sessions in an HTTP-only cookie** | A cookie session would remove the bearer-token storage problem for a browser client, but it introduces a different one: a cookie is sent automatically, so every mutating endpoint needs CSRF protection, and the API would need a shared session store to scale horizontally. The current design keeps the API stateless with respect to the access token and makes the CSRF question a client question (Q5). |
| **OAuth 2.0 / OpenID Connect with an external provider** | It would remove the credential store and add a standard federation story. It was not chosen because Phase 1 has exactly one actor and no third-party clients, and because the requirement says the build "must not require a hosted or paid service". It is a plausible Phase 2+ direction if ATOM ever becomes multi-user or gains a partner integration. |
| **Server-stored access tokens** (a random opaque token looked up in the database on every request) | It would make revocation immediate and would remove the signing secret from the trust path. It was not chosen because it adds a database round trip to every request, needs a shared store to scale, and moves the session table from a rare event to the hot path. The `jti` claim is already present so that a revocation list can be added later without changing the token format. |
| **Plain SHA-256 of the password** | SHA-256 is fast, so a stolen hash is cheap to brute-force. Passwords must use a slow, memory-hard function; Argon2id is the current recommendation and is what the project uses. SHA-256 is used only for the refresh token, where the input is a 256-bit random value and speed is irrelevant. |
| **Long-lived access tokens with no refresh token** | A stolen token would stay valid for its whole life, and there would be no way to end a session without a server-side list. The short access token plus the rotating refresh token bounds the exposure window of a stolen access token to 15 minutes and makes the revocation path explicit. |
| **Refresh tokens without reuse detection** | Rotation alone does not detect theft: the attacker and the legitimate client race, and the loser's token simply fails. Reuse detection turns the race into a signal, and revoking every session of the account is the conservative response. |
| **Signing the refresh token as a JWT as well** | It would carry structured claims, but it would also make the token long enough to be replayable from a log, and it would remove the ability to look the session up by hash. Opaque tokens are the standard answer for this role. |

## Consequences and trade-offs

### Positive

- **The access path is stateless.** Verifying a token needs only the
  signing secret, so an instance needs nothing shared with another instance.
- **A database disclosure does not yield usable credentials.** Passwords are
  Argon2id hashes and refresh tokens are SHA-256 hashes.
- **The blast radius of a stolen refresh token is bounded.** The token is
  single-use; the second use revokes every session of the account, so the
  attacker and the owner are locked out together and the owner knows
  something happened.
- **Authentication failures leak nothing.** One message and one error for
  every failure mode.
- **The token format leaves room for later.** The `jti` claim is there for a
  revocation list, and the algorithm is pinned to HS256 so that a future
  migration to asymmetric signing is a token-format change, not an
  architecture change.
- **The implementation is small and dependency-light.** `jsonwebtoken`,
  `argon2`, `sha2` and `rand` cover it.

### Negative

- **The secret is a single point of failure.** There is no rotation
  procedure, no key identifier and no revocation list, so a compromised
  secret cannot be retired without restarting every instance with a new one.
  This is recorded as T-19 in
  [`../security/threat-model.md`](../security/threat-model.md).
- **The refresh token travels in the response body.** A browser client would
  have to manage it in scriptable storage, which is exactly what the
  HTTP-only cookie in Q2 would avoid. Accepted risk AR-2 in
  [`../security/threat-model.md`](../security/threat-model.md), KI-007 in
  [`../engineering/known-issues.md`](../engineering/known-issues.md).
- **Reuse detection punishes a legitimate client.** A client that retries a
  refresh after a timeout, or that runs two refreshes concurrently, presents
  a rotated token and triggers the mass revocation. The client design must
  therefore serialise its refreshes. This is a real usability cost and it is
  why the detection is worth keeping.
- **The mass revocation is coarse.** One compromised token locks the owner
  out of every device, including the legitimate ones. It is the safe
  default, but it is not a granular answer.
- **The timing equalisation is an estimate.** A dummy Argon2 verification
  makes the two login paths comparable, but the paths still differ by a
  database round trip, and the equality is not measured by a test.
- **There is one absolute refresh lifetime and no idle timeout.** A stolen
  refresh token is usable until it expires, even if the legitimate client
  has been active. This is Q3.
- **The access token cannot be revoked before it expires.** There is no
  blacklist; `jti` is carried but not consulted.

## Security and operational implications

- **The signing secret is the crown jewel.** It is read from
  `APP_JWT_SECRET`, must be at least 32 bytes, and the documented
  placeholder is refused when `APP_ENV` is `production`. `AuthSettings`
  implements `Debug` manually and prints `[REDACTED]`.
- **The identity comes only from the credential.** `AuthUser` is the only
  way a handler learns the caller's identity, and no request body, header or
  path parameter can name it. See
  [`../architecture/security-architecture.md`](../architecture/security-architecture.md)
  §2.
- **An inaccessible resource is `404`, not `403`.** The token design is
  only half of the authorization model; the other half is ADR-0005.
- **The authentication endpoints are rate limited.** Three endpoints
  (`register`, `login`, `refresh`) are protected by the in-process limiter.
  The limiter is per process and its key is the peer address of the
  connection, so a deployment behind a proxy that does not terminate the
  connection shares one budget; that makes edge rate limiting an operator
  obligation. (`KI-010` in
  [`../engineering/known-issues.md`](../engineering/known-issues.md) was a
  defect in that wiring and is Closed.)
- **Password verification is deliberately expensive.** Argon2id with an
  8 MiB floor is the dominant cost of the authentication endpoints. It is
  bounded by the password-length cap and by the rate limit.
- **Nothing sensitive is logged.** Neither token, nor a password, nor a
  hash appears in a log line or in an error message.
- **Rotation depends on the database.** The revocation link is a column in
  `refresh_sessions`, so the reuse detection needs the write to have
  succeeded. Since the amendment of 2026-10-31 the rotation runs inside a
  single transaction; the boundary is analysed in
  [`../architecture/data-architecture.md`](../architecture/data-architecture.md)
  §7.

## Amendment — 2026-10-31: the rotation is one transaction (Q-19)

> **Owner decision (`Q-19`).** The non-transactional ordering recorded above
> — insert the replacement session, then claim the presented session as two
> independent statements — is superseded: the rotation's two statements run
> in **one transaction** (`RefreshSessionRepository::rotate`, called from
> `auth::service::refresh`).

**Why the transaction is at least as fail-safe.** The original ordering was
chosen because a process death between the two statements left the presented
session valid and usable: the caller retries, and the rotation is attempted
again — at the cost of one unclaimed replacement row that only the hourly
prune removed. The transaction keeps exactly that recovery story and removes
the orphan row:

- **Interruption.** If the process dies mid-rotation, the database rolls the
  transaction back: the presented session is left unclaimed and still valid,
  and no replacement row exists. The caller retries with the same token and
  the rotation succeeds.
- **The lost race.** Two concurrent presenters are still arbitrated by the
  guarded `UPDATE` (`revoked_at IS NULL`), now inside the transaction. The
  loser sees zero affected rows, its transaction rolls the replacement
  insert back, and the loser then follows the existing reuse path — mass
  revocation and the generic reuse error — exactly as before. Exactly one
  replacement session can exist.
- **The security properties are unchanged.** Reuse/replay detection, the
  mass revocation, the inherited absolute lifetime (the replacement's
  `expires_at` is the presented session's, never recomputed), the
  owner-scoped predicates and the generic error messages are all preserved.

The regression tests live in
[`../../tests/auth_api.rs`](../../tests/auth_api.rs): a concurrent rotation
still admits exactly one winner and leaves no orphan replacement row
(`a_lost_rotation_race_leaves_no_orphan_replacement_session`), a rejected
rotation writes no new session row
(`a_rejected_rotation_writes_no_new_session_row`), and an expired token
rotates nothing
(`an_expired_refresh_token_rotation_writes_no_new_session_row`).

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §4.2 | The accepted authentication requirements this decision implements. |
| [`../project-truth.md`](../project-truth.md) §9 | The security and privacy expectations, especially items 1, 3, 4 and 5. |
| [`../project-truth.md`](../project-truth.md) §16 Q2 | The open question about the HTTP-only cookie. |
| [`../project-truth.md`](../project-truth.md) §16 Q3 | The open question about the refresh-token lifetime and idle timeout. |
| [`../features/authentication/specification.md`](../features/authentication/specification.md) | The user-facing behaviour of the endpoints this decision creates. |
| [`../architecture/security-architecture.md`](../architecture/security-architecture.md) | The authentication boundary this decision builds. |
| [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](ADR-0005-ownership-enforced-in-sql.md) | The authorization decision that the token design hands the identity to. |
| [`../engineering/open-questions.md`](../engineering/open-questions.md) Q-02, Q-03 | The tracking records for the two open questions. |
| [`../engineering/known-issues.md`](../engineering/known-issues.md) KI-007 | The refresh token in the response body. |

## Supersedes / Superseded by

Supersedes: none. This is the first decision about the token design.

Superseded by: none. A review of this decision is due when Q2 or Q3 is
answered, because the cookie and idle-timeout answers change the transport
and the lifetime rules rather than the token format.

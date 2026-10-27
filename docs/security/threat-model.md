# ATOM threat model

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `docs` agent

This document models the threats to the ATOM backend, the controls
that exist against them, and the risk that remains. It is a working model, not
a certification: it describes the system as it is built today, and every claim
in it is traceable to the source.

Scope: the `atom/api` service, its PostgreSQL database, its migrations and its
build. The trust boundaries and the request lifecycle are described in
[`../architecture/request-lifecycle.md`](../architecture/request-lifecycle.md)
and
[`../architecture/security-architecture.md`](../architecture/security-architecture.md).

---

## 1. Method

1. List the assets worth protecting.
2. List the actors who can reach them.
3. Draw the trust boundaries the assets sit behind.
4. For each plausible threat, name the control that reduces it and the risk
   that is left over.
5. Record the risks that are accepted on purpose, and the questions that are
   still open.

The threat list is not exhaustive. It covers the threats that are plausible for
a single-owner, API-only Phase 1 deployment.

## 2. Assets

| ID | Asset | Where it lives | Why it matters |
| -- | ----- | -------------- | -------------- |
| A-1 | Password material | `users.password_hash` | The hash is the target of an offline guessing attack. The plaintext never exists beyond the request. |
| A-2 | Access tokens | Issued to the client, never stored server-side | A leaked token is a credential for the account until it expires. |
| A-3 | Refresh tokens | The plaintext exists only in the response; the SHA-256 hash is in `refresh_sessions.token_hash` | A leaked refresh token is a long-lived credential. |
| A-4 | Token signing secret | `APP_JWT_SECRET` in the environment, with no file layer | Anyone holding it can mint access tokens for any account. |
| A-5 | Database credentials | `DATABASE_URL` in the environment, with no file layer | Anyone holding it can read and write every table. |
| A-6 | User content | `notes`, `projects`, `tasks` | The private record of the owner. It is the product. |
| A-7 | Session metadata | `refresh_sessions` (user agent, client address, timestamps) | Reveals where and when the owner works. It is also a signal of account compromise. |
| A-8 | Availability of the service | The API process and the database | A personal workspace that does not answer is worthless. |
| A-9 | Schema and migration history | `migrations/` | A lost or altered migration breaks reproducibility and the safety of the schema. |
| A-10 | The dependency set | `Cargo.lock` | A compromised dependency executes with the privileges of the process. |
| A-11 | API keys | The plaintext exists only in the create response; the SHA-256 digest is in `api_keys.key_hash` and the display tag in `api_keys.prefix` | A leaked key is a **long-lived, full-authority** bearer credential for one account: 69 characters, no expiry and no scope model. |

**No file in `config/` may contain a secret.** The configuration directory is
committed to the repository and embedded in the binary, so a secret in a file
there is a secret in the published source. `DATABASE_URL` and `APP_JWT_SECRET`
are required environment variables and have **no file layer at all**: a setting
that needs a credential is an environment variable. The rule and the check that
enforces it are in [`secure-development.md`](secure-development.md) SD-4.

## 3. Actors

| ID | Actor | Capability |
| -- | ----- | ---------- |
| ACT-1 | The legitimate owner | A registered account. Full control over its own resources. |
| ACT-2 | An unauthenticated remote attacker | Can reach any endpoint, can send arbitrary bodies, can attempt credentials. |
| ACT-3 | An authenticated user attacking another user | Holds a valid token for their own account and tries to reach resources they do not own. |
| ACT-4 | A compromised dependency or build step | Runs inside the process, with access to the environment, the database and every secret the process holds. |
| ACT-5 | An operator with database access | Can read and write the database directly, bypassing the API. |
| ACT-6 | An attacker holding a stolen token | Holds a valid access token, refresh token or API key that was captured, leaked or logged. |

## 4. Trust boundaries

| ID | Boundary | Inside | Outside | Assumption |
| -- | -------- | ----- | ------- | ---------- |
| TB-1 | Network edge → API process | The validated request | Anything the client can send | The client is untrusted. TLS is terminated by the operator, not by the process. |
| TB-2 | API process → PostgreSQL | The pool, the parameterised statements | The database server | The database is not exposed to the network. The process is trusted by the database. |
| TB-3 | Build and dependency supply chain → the shipped binary | `Cargo.lock`, the source tree, the build environment | Registries and transitive dependencies | A dependency is only as trustworthy as its publisher. |
| TB-4 | Operator → host and database | The configuration files, the environment variables, the secrets, direct SQL | — | The operator is trusted. Insider misuse is out of the API's reach. |
| TB-5 | Client → API (the authentication boundary) | The identity derived from the credential | The identity the client claims | The identity of a caller comes only from the credential. A body never names the caller. |
| TB-6 | Process → process memory | The signing secret, the database URL, password hashes, tokens in flight | Log sinks, crash dumps, core files | Secrets exist in memory while the process runs and must not leave it. |

## 5. Threats, controls and residual risk

| ID | Threat | Actor | Boundary | Control that mitigates it | Residual risk |
| -- | ------ | ----- | -------- | ------------------------ | ------------- |
| T-01 | Credential stuffing or password guessing against `POST /api/v1/auth/login` | ACT-2 | TB-1 | Argon2id with a configurable cost floor (8 MiB minimum), a 12–256 character password policy, a generic failure message, and a per-client rate limit on the authentication endpoints. | No account lockout and no breached-password check exist. The rate limit is per process, and its key is currently shared by every caller. |
| T-02 | Learning whether an address is registered | ACT-2 | TB-1 | One generic message for every failure, and a dummy Argon2 verification when the address is unknown so that the two paths take a comparable time. | The equalisation is not measured by a test; a database round trip still differs between the two paths. |
| T-03 | Stealing a token from the client or from a log | ACT-2, ACT-6 | TB-5, TB-6 | The access token is short-lived (15 minutes by default), the refresh token is stored only as a hash, and neither is ever written to a log or an error message. | The refresh token travels in the response body rather than an HTTP-only cookie, so a browser client would have to manage it in scriptable storage. See Q2 and Q5. |
| T-04 | Replaying a refresh token after it was rotated | ACT-6 | TB-5 | Rotation on every use, and reuse detection: presenting an already-revoked token revokes every active session of that account. | Detection requires the attacker to present the old token. An attacker who stays silent keeps using the newest token until it expires. |
| T-05 | Reading or modifying another account's resource (broken object-level authorization) | ACT-3 | TB-5 | Every query that touches user data carries the caller's `user_id`; tasks are reached through a join on `projects.user_id`. A resource the caller does not own is reported as `404`. | The predicate is a convention that a human reviewer must check, not a property the compiler enforces. Every new query is a chance to get it wrong. |
| T-06 | SQL injection through a request body or a path parameter | ACT-2, ACT-3 | TB-1 | Every statement is a bound parameter. No user input is concatenated into SQL. | The statements are checked at run time, not at compile time, so a mismatch is found when the query runs, not when it is written. |
| T-07 | Forging the caller's identity or escalating privilege through a crafted body | ACT-2, ACT-3 | TB-5 | The request DTOs carry only the accepted fields, and the caller is taken from the credential. `is_active` is not settable through the API. | A write performed directly against the database can set `is_active`; the database does not restrict who may change it. |
| T-08 | Leaking a secret through an error, a log line or a response | ACT-2, ACT-4 | TB-6 | `AppError` renders a fixed envelope: database and internal failures are logged server-side and answered with a generic message. `AuthSettings` and `DatabaseSettings` implement `Debug` manually and redact the secret and the connection URL. | The panic payload is deliberately not logged, which means a panic that contained a secret is not recoverable from the logs either. A log sink that captures request bodies would still capture secrets. |
| T-09 | Exhausting memory or worker time with a large or slow request | ACT-2 | TB-1 | A body limit of `APP_MAX_BODY_BYTES` (1 MiB by default) applied by `DefaultBodyLimit` and honoured by the JSON extractor, with an independent 4 MiB backstop in the extractor; a request timeout of `APP_REQUEST_TIMEOUT_SECONDS` (30 seconds by default) that answers `408` in the standard envelope; and a panic handler that converts a panic into a `500` instead of a dropped connection. | A body below the limit still costs memory. There is no per-client concurrency limit, and the timeout does not bound the work a query does. |
| T-10 | Denial of service through the cost of password hashing | ACT-2 | TB-1 | The Argon2 memory floor is 8 MiB and the password length is capped at 256 characters, so the hashing cost is bounded. | Registration is the most expensive path. Only the rate limit bounds its volume, and that limit is per process. |
| T-11 | Exhausting memory through the rate limiter's own bookkeeping | ACT-2 | TB-1 | Entries whose window has elapsed are swept at most once per window. Sweeps are at most one window apart, so an entry lives for at most two windows and the map holds only the clients of the last two windows. | A client that arrives with a new address in every window is still admitted to the map for up to two windows, so the peak is bounded by the number of distinct addresses seen in two windows, not by a fixed figure. The map is per process. |
| T-12 | Misusing the API from a hostile web origin | ACT-2 | TB-1 | CORS is an explicit allowlist. An empty list disables CORS. A `*` entry, and an origin that is not a valid header value, are refused at start-up with a `ConfigError`, before the socket is bound. Credentials are not enabled. | The refusal is syntactic. An origin that is well formed but is not the one the intended client uses is not detectable at start-up. |
| T-13 | Forging the rate-limit key with a spoofed header | ACT-2 | TB-1 | `X-Forwarded-For` is deliberately not consulted. The client address comes from the connection information. | Behind a reverse proxy that does not terminate the connection, every request appears to come from the proxy, and the limit becomes a global limit. |
| T-14 | Compromise through a dependency | ACT-4 | TB-3 | `Cargo.lock` is committed, the build resolves to pinned versions, and the process makes no outbound network calls at run time. | There is no dependency audit, no update policy and no CI job that would catch an advisory. |
| T-15 | Insider access to the database | ACT-5 | TB-4 | Passwords and refresh tokens are hashed, so a dump does not immediately yield usable credentials. | The user's content is stored in the clear, and the password hashes are an offline guessing target. A second, weaker hash would not help against a dump of arbitrary strength. |
| T-16 | Missing evidence of an intrusion | ACT-5 | TB-4 | Structured logging with a per-request correlation id, and a log line for every rejected request. | There is no security audit trail, no record of a successful login as a security event, and no alerting. |
| T-17 | Interception of traffic in transit | ACT-2 | TB-1 | HTTPS is required in production; the requirement is a deployment obligation, not a property of the process. | The API speaks plain HTTP/1.1 and does not enforce or check TLS. |
| T-18 | A destructive or inconsistent schema change | ACT-4 | TB-3 | Migrations are append-only, invariants are expressed as `CHECK` constraints and foreign keys, and a schema change requires a matching model change. | A migration that is destructive is still possible. The protection is procedural. |
| T-19 | Use of a leaked or retired signing secret | ACT-2, ACT-5 | TB-4 | The secret comes from the environment and has no file layer, so no committed configuration file can hold it. It is at least 32 bytes, and the documented placeholder is refused in production. | There is no rotation procedure, no key identifier in the token, and no revocation list, so a compromised secret cannot be retired without restarting every instance with a new one. |
| T-20 | A race between two registrations of the same address | ACT-2 | TB-2 | Uniqueness lives on the normalized value in the database, and the unique-violation error is mapped to a conflict. The application never does a check-then-insert. | None material. |
| T-21 | Theft or exposure of an API key — from a client's configuration, a log line, a screenshot or a repository | ACT-2, ACT-6 | TB-5, TB-6 | The secret is shown **once**, in the create response, and only its SHA-256 digest is stored, so a database dump hands over no usable credential. Revocation is immediate: `DELETE /api/v1/api-keys/{key_id}` sets `revoked_at`, which every authenticated request checks. `last_used_at` is stamped at most hourly, so an unexpected use is visible in the listing. The secret, its digest and its prefix are never logged, including on an error path. | Revocation is only as good as the operator noticing that a key is being used by somebody else, and the hourly stamp is a coarse signal. There is no expiry, no scope model and no per-key rate limit, and a key can mint another key — so revoking the leaked one does not by itself end access: every key of the account must be revoked and the account's activity treated as suspect. |

## 6. Accepted risks

These are accepted on purpose for Phase 1. Each one is a decision, not an
oversight, and each is recorded in
[`../engineering/known-issues.md`](../engineering/known-issues.md).

| ID | Accepted risk | Why it is acceptable now | What would change it |
| -- | -------------- | ------------------------- | ------------------- |
| AR-1 | The rate limiter is per process and is a defence-in-depth control only. | Phase 1 is a single-owner, single-instance deployment. | A multi-instance deployment must add edge rate limiting. |
| AR-2 | Refresh tokens are returned in the response body. | The Phase 1 clients are non-browser clients. A bearer token works for both the planned web and mobile clients. | The browser client design (Q2, Q5). |
| AR-3 | SQLx verifies statements at run time. | A compile-time check would make the build depend on a live database, which conflicts with the constraint that the build must not require a hosted service. | Enabling `cargo sqlx prepare` and verifying the metadata in CI. |
| AR-4 | There is no sharing, no collaboration and no multi-tenancy. | Phase 1 is single-owner by design; there is no cross-user read path to protect. | Any team or shared-workspace feature, which requires its own model. |
| AR-5 | The API does not terminate or verify TLS. | The deployment is expected to sit behind a reverse proxy or a load balancer that does. | A direct-to-the-internet deployment without a proxy. |
| AR-6 | There is no account lockout and no breach detection. | The rate limit and the password cost are the Phase 1 controls. | A decision to add a lockout policy, which needs its own product rules to avoid becoming a denial-of-service vector against a legitimate owner. |
| AR-7 | The rate limiter fails open when its mutex is poisoned. | Availability of the authentication endpoints is preferred over a panic during an unrelated fault; the operator investigates the poisoning. | A change of the availability/security trade-off. |

## 7. Out of scope

The following are explicitly **not** covered by this model.

- **Physical and host security.** The physical security of the server, the
  network, the power supply and the database host. That is the operator's
  responsibility under TB-4.
- **Developer workstation security.** The editor, the local environment file,
  the configuration directory, the container runtime and the developer's own
  accounts.
- **The security of future clients.** A web or mobile frontend, an n8n
  deployment, or any third-party service ATOM will later integrate with. Those
  have their own boundaries; the requirement that they must not weaken the
  controls in this document is recorded in
  [`../project-truth.md`](../project-truth.md) §9.
- **Business-level misuse.** A legitimate owner using their own account in a
  way the owner later regrets is not a security threat to the system.
- **Infrastructure-level denial of service.** A volumetric attack upstream of
  the host is handled by the network, not by the application.
- **Compliance certification.** No SOC 2, ISO 27001 or GDPR data-protection
  impact assessment is claimed or implied.
- **Dependency and image scanning.** The pipeline in
  [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) runs the
  formatting check, the lint, the test suite, a release build and the
  documentation check, but it has no dependency-audit job and no image scan.
  There is no container image yet either, so there is nothing to scan.

# ATOM security

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent

This directory describes the security posture of the ATOM Phase 1 backend:
what is protected, which controls exist today, where each control lives in the
source, and what is still missing. It is the counterpart of
[`../architecture/security-architecture.md`](../architecture/security-architecture.md),
which describes *how* the boundary is built; this directory describes *why* each
control exists and what it does not cover.

---

## 1. The posture in one paragraph

ATOM Phase 1 is a single-owner, single-tenant personal API. Every private
resource belongs to exactly one authenticated account, every query that touches
user data is scoped by the caller's `user_id` in SQL, and a resource the caller
does not own is reported as `404` rather than `403` so that its existence is not
disclosed. Passwords are stored as Argon2id hashes and are never logged or
returned; access tokens are short-lived HS256 JWTs validated for signature,
expiry, issuer and audience; refresh tokens are opaque random values of which
only the SHA-256 hash is persisted, are rotated on every use, and a reuse of an
already-rotated token revokes every session of that account. Authentication
failures return one generic message, and the response time of an unknown email
address is equalised with a dummy Argon2 verification. Registration, login and
refresh are rate limited per client address. The API never returns database
internals or secret material: the error envelope is fixed and `AppError`
redacts everything internal. The configuration is resolved once at start-up
from three layers — the committed files in `config/`, the environment overlay
and the environment variables — and a configuration that violates a rule stops
the process before the socket is bound. The two secrets have no file layer at
all: they are environment variables. What the posture does **not** include
yet: edge rate limiting for a multi-instance deployment, a proxy-aware client
address, an account-lockout or breach-detection mechanism, compile-time SQL
verification, and a container image. Those limits are recorded in
[`../engineering/known-issues.md`](../engineering/known-issues.md), not hidden.

## 2. Documents in this directory

| Document | Contents |
| -------- | -------- |
| [`threat-model.md`](threat-model.md) | Assets, actors, trust boundaries, the threat table, the accepted risks and the out-of-scope items. |
| [`secure-development.md`](secure-development.md) | The rules a change must follow, each with a concrete check that can be run. |

## 3. The controls that exist today

Every row below is implemented in the source. The evidence column names the
test that proves the behaviour; where no test exists, that is stated.

| Control | Implemented in | Evidence |
| ------- | -------------- | -------- |
| Argon2id password hashing with a 16-byte random salt | [`../../src/auth/password.rs`](../../src/auth/password.rs) | Unit tests in the same file (6) |
| Password policy: 12–256 characters, one uppercase, one lowercase, one digit | [`../../src/users/service.rs`](../../src/users/service.rs) | Unit tests in the same file, and `a_weak_password_is_rejected_with_field_details` in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| Email normalization plus a matching database `CHECK` constraint and unique index | [`../../src/users/service.rs`](../../src/users/service.rs), [`../../migrations/0001_users_and_sessions.sql`](../../migrations/0001_users_and_sessions.sql) | `registration_normalises_the_email_address` in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| Short-lived HS256 access tokens with issuer and audience validation | [`../../src/auth/token.rs`](../../src/auth/token.rs) | Unit tests in the same file (6), and the authentication cases in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| A signing secret of at least 32 bytes, with the documented placeholder refused in production | [`../../src/config/validation.rs`](../../src/config/validation.rs), [`../../src/config/settings.rs`](../../src/config/settings.rs) | `validate_secret` in the configuration layer. `a_short_secret_is_refused` and `the_placeholder_secret_is_refused_in_production_only` in [`../../src/config/tests.rs`](../../src/config/tests.rs). |
| Refresh tokens stored hashed, rotated on use, reuse detected | [`../../src/auth/token.rs`](../../src/auth/token.rs), [`../../src/auth/service.rs`](../../src/auth/service.rs) | `refresh_rotates_the_token_pair` and `reusing_a_rotated_refresh_token_revokes_every_session` |
| Logout and per-user session revocation | [`../../src/auth/service.rs`](../../src/auth/service.rs) | `logout_revokes_the_session` and `an_account_cannot_log_out_another_accounts_session` |
| Authenticated identity derived only from the credential | [`../../src/api/middleware.rs`](../../src/api/middleware.rs) | `protected_routes_reject_missing_and_invalid_credentials` |
| Ownership enforced in SQL, with `404` for an inaccessible resource | [`../../src/notes/repository.rs`](../../src/notes/repository.rs), [`../../src/projects/repository.rs`](../../src/projects/repository.rs), [`../../src/tasks/repository.rs`](../../src/tasks/repository.rs) | The isolation cases in [`../../tests/notes_api.rs`](../../tests/notes_api.rs) and [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) |
| Generic authentication failure with timing equalisation | [`../../src/auth/service.rs`](../../src/auth/service.rs) | `login_fails_with_a_generic_message` and `an_unknown_email_produces_the_same_failure_as_a_wrong_password` |
| A fixed error envelope that never carries internal detail | [`../../src/error.rs`](../../src/error.rs) | Unit tests in the same file (5) |
| Per-process rate limiting on registration, login and refresh | [`../../src/ratelimit.rs`](../../src/ratelimit.rs), [`../../src/api/middleware.rs`](../../src/api/middleware.rs) | Unit tests in [`../../src/ratelimit.rs`](../../src/ratelimit.rs) (3), and `the_authentication_endpoints_are_rate_limited` in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| An explicit CORS allowlist, with `*` and an invalid origin refused at start-up, and credentials disabled | [`../../src/config/validation.rs`](../../src/config/validation.rs), [`../../src/api/mod.rs`](../../src/api/mod.rs) | No automated test |
| Request body, request duration and panic containment limits | [`../../src/api/mod.rs`](../../src/api/mod.rs), [`../../src/api/extract.rs`](../../src/api/extract.rs) | `an_oversized_body_is_refused` in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) and `a_slow_request_is_abandoned_with_the_error_envelope` in [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) |
| Parameterised SQL only | every repository under [`../../src/`](../../src/) | No dedicated test; enforced by the code reviewer |
| Secrets kept out of the repository | [`.gitignore`](../../.gitignore), [`.env.example`](../../.env.example), [`../../config/`](../../config/) | The redaction is implemented by the manual `Debug` implementations in [`../../src/config/settings.rs`](../../src/config/settings.rs), and `the_secret_is_never_rendered` in [`../../src/config/tests.rs`](../../src/config/tests.rs) covers it. |

## 4. Known limitations

The full list, with impact and workaround, is in
[`../engineering/known-issues.md`](../engineering/known-issues.md). The four that
matter most for a deployment:

1. The rate limiter is **per process**. A multi-instance deployment needs edge
   rate limiting in front of the API.
2. The client address used as the rate-limit key is taken from the connection
   information, which the server does not currently install. Every request
   therefore shares one bucket.
3. `X-Forwarded-For` is deliberately **not** trusted. Behind a reverse proxy
   every request appears to come from the proxy unless the proxy terminates the
   connection.
4. SQLx queries are verified at **run time**, not at compile time. The
   compile-time workflow is documented but not enabled.

## 5. Reporting a vulnerability

Report a suspected vulnerability **privately**, not as a public issue. The
repository has no public issue tracker yet, and a public report would disclose
the weakness before it is fixed.

1. Contact the project owner directly through the channel the repository owner
   publishes for that purpose.
2. Include: the affected endpoint or module, the request or sequence that
   demonstrates the problem, the impact you observed, and the version or commit
   you tested.
3. Do not include real credentials, real tokens, real personal data, or
   production data of any kind. Use the disposable test database described in
   [`../testing/strategy.md`](../testing/strategy.md).
4. Do not test against accounts or infrastructure you do not own, and do not
   run automated scanners against a deployment that is not yours.
5. Expect a reply. The maintainer may ask for more detail before a fix is
   designed.

The same channel is used for a suspected vulnerability in a dependency: report
it upstream first, then tell the maintainer which version ATOM pins.

Related: [`../engineering/open-questions.md`](../engineering/open-questions.md)
for the product decisions that are still open, and
[`../engineering/review-log.md`](../engineering/review-log.md) for the record of
independent reviews.

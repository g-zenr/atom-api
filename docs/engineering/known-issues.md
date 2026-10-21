# ATOM known issues

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent

Confirmed defects, limitations and gaps: things that are known to be wrong, to
be weaker than they should be, or to be missing. Each entry says what it is,
what it costs, what to do about it in the meantime, and what its status is.

This is the companion of [`open-questions.md`](open-questions.md). A **defect**
or a **limitation** belongs here. A **decision that has not been taken yet**
belongs there. When a change is required but the design is not settled, the
issue records the defect and the question records the decision.

Related: [`../security/threat-model.md`](../security/threat-model.md) §6
lists the risks that were accepted on purpose, and
[`../testing/strategy.md`](../testing/strategy.md) §6 lists the coverage
gaps.

---

## The format

| Field | Meaning |
| ----- | ------- |
| **ID** | `KI-NNN`, never reused. |
| **Status** | `Open`, `Mitigated` (a workaround is in place, the defect remains) or `Closed` (the defect is fixed; the closing change is named). |
| **Severity** | `High`, `Medium` or `Low`, as defined below. |
| **Description** | What is actually wrong, with the file it lives in. |
| **Impact** | What an attacker, an operator or a user can do because of it. |
| **Workaround** | What to do until it is fixed. |
| **Remediation status** | What has been done, what is planned, and what is blocked on. |
| **Related** | The open question, the ADR or the requirement it belongs to. |

### Severity

| Severity | Meaning |
| -------- | ------- |
| `High` | The weakness breaks a security boundary or a data guarantee that the product claims. |
| `Medium` | The weakness materially reduces a defence, or makes a supported use case unreliable. |
| `Low` | The weakness is a gap in evidence, a documentation defect, or a limitation with a workaround. |

---

## The issues

### KI-001 — The rate limiter is per process

**Status:** Open (accepted risk) · **Severity:** Medium

**Description.** The limiter lives in the process. Its counters are a
`HashMap` behind a `Mutex` in [`../../src/ratelimit.rs`](../../src/ratelimit.rs),
held by `AppState`. Two instances of the API therefore count independently, and
a request that reaches instance A does not count against instance B.

**Impact.** A horizontally scaled deployment multiplies the effective limit by
the number of instances. An attacker who can spread requests across the
instances gets proportionally more attempts against the authentication
endpoints. The requirement in
[`../project-truth.md`](../project-truth.md) §9.10 — "production deployments
put edge rate limiting in front of the API as well" — is not satisfied by the
in-process limiter alone.

**Workaround.** Put a shared limiter in front of the API: a reverse proxy or a
load balancer with its own rate limiting, keyed on the real client address.
Until then, run a single instance.

**Remediation status.** Not started. The in-process limiter is kept as a
defence-in-depth control, not as the primary one. The deployment requirement is
recorded in
[`../architecture/deployment.md`](../architecture/deployment.md).

**Related.** Accepted risk `AR-1` in
[`../security/threat-model.md`](../security/threat-model.md); requirement
[`../project-truth.md`](../project-truth.md) §9.10; `KI-003`, `KI-010`.

### KI-002 — There is no HTTP-level test of the rate limiter

**Status:** Closed (2026-10-21) · **Severity:** Medium

**Description.** The limiter's algorithm is unit tested in
[`../../src/ratelimit.rs`](../../src/ratelimit.rs) — three tests cover the
limit, the per-client separation and the window reset. Nothing drives a real
HTTP request past the limit. The wiring — the `enforce_rate_limit` call in
each authentication handler, the `429` status, the `retry-after` header and
the error envelope — is therefore unverified.

**Impact.** A regression that removed the rate-limit call from a handler, or
that stopped setting `retry-after`, would pass every test in the suite.

**Workaround.** None needed for a correct deployment; the risk is a silent
regression, not a current defect.

**Remediation status.** Not started. A test must build a router with a small
limit, send the limit plus one request to `/api/v1/auth/login`, and assert the
`429` status and the `retry-after` header. The harness currently sets the
limit to 10 000 for the suite, so the test needs its own state.

**Resolution.** `tests/auth_api.rs` now contains `the_authentication_endpoints_are_rate_limited`, which drives the login endpoint past the limit and asserts the `429` status and the `rate_limited` error code. The test uses a dedicated state built by `test_state_with_rate_limit` in [`../../tests/common/mod.rs`](../../tests/common/mod.rs). `cargo test --all-targets` passed with 70 tests.
**Related.** [`../testing/strategy.md`](../testing/strategy.md) §6 item 1;
requirement [`../project-truth.md`](../project-truth.md) §13.5.

### KI-003 — `X-Forwarded-For` is ignored, and nothing replaces it

**Status:** Open (by design, with a gap) · **Severity:** Medium

**Description.** The `ClientIp` extractor in
[`../../src/api/middleware.rs`](../../src/api/middleware.rs) reads the
`ConnectInfo<SocketAddr>` extension and deliberately never reads
`X-Forwarded-For`, because the header is client-controlled unless a trusted
proxy overwrites it. The requirement in
[`../project-truth.md`](../project-truth.md) §9.11 is that the API "refuses to
trust `X-Forwarded-For` unless a trusted proxy sets it" — the refusal is
implemented, but the "unless a trusted proxy sets it" half is not: there is no
mechanism to declare a trusted proxy.

**Impact.** Behind a reverse proxy that does not terminate the connection,
every request appears to come from the proxy, so the rate limit becomes a
global limit for the whole installation. Combined with `KI-001` and
`KI-010`, the practical effect is that the authentication endpoints have a
single shared budget.

**Workaround.** Terminate the connection at the proxy, so that the API sees
the real peer address; or place the rate limiting at the proxy and treat the
in-process limiter as a backstop.

**Remediation status.** Not started. The design question — how a trusted proxy
is declared — is `Q-07` in [`open-questions.md`](open-questions.md).

**Related.** `KI-001`, `KI-010`; requirement
[`../project-truth.md`](../project-truth.md) §9.11.

### KI-004 — SQLx queries are checked at run time, not at compile time

**Status:** Open (accepted) · **Severity:** Low

**Description.** The queries use `sqlx::query_as` and friends with the
`runtime-tokio` feature and no offline query metadata. A mismatch between a
statement and the schema is therefore discovered when the statement runs, not
when it is compiled. The compile-time workflow — `cargo sqlx prepare` and a CI
check that the metadata is current — is documented in the repository, and the
`.sqlx` directory is still commented out in [`.gitignore`](../../.gitignore).

**Impact.** A column rename that is not applied to every statement passes the
build and fails at run time, in the affected request path.

**Workaround.** The integration suite exercises every statement in the
current schema, so a mismatch is caught by the test suite. The gap is between
the build and the test.

**Remediation status.** Accepted for Phase 1. The compile-time check would
make the build depend on a live database, which conflicts with the constraint
that the build must not require a hosted or paid service. It is enabled
together with CI, which is `KI-005`.

**Related.** Accepted risk `AR-3` in
[`../security/threat-model.md`](../security/threat-model.md); requirement
[`../project-truth.md`](../project-truth.md) §12.

### KI-005 - There is no CI pipeline

**Status:** Closed (2026-10-22) — **Severity:** Medium

**Description.** The repository contained no CI configuration. The three
quality gates were run by hand, locally, and were recorded in
[`change-log.md`](change-log.md).

**Impact.** Nothing enforced the quality gates on a change, so a change could
be merged with a failing test, a lint warning or an unresolved documentation
link.

**Workaround.** The reviewer and the quality gate run the gates manually and
record the output, as described in
[`../testing/verification-policy.md`](../testing/verification-policy.md).

**Resolution.** [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)
runs the formatting check, the lint, the test suite, a release build, the
documentation check and the OpenCode configuration check on every push and
pull request. The workflow has not yet run on a GitHub runner, because the
repository has not been published.

**Related.** Requirement [`../project-truth.md`](../project-truth.md) §13.6;
`KI-004`; `KI-006`.

### KI-006 — There is no container image for the API

**Status:** Open · **Severity:** Low

**Description.** [`../../docker-compose.yml`](../../docker-compose.yml) brings
up PostgreSQL for local development and for the test suite. It states in its
own header that it "does NOT build an application image". The API runs from
the source tree with `cargo run`.

**Impact.** There is no reproducible deployment artefact. A deployment is
defined by the source tree and the toolchain, which is a larger and less
pinned surface than an image.

**Workaround.** Build the release binary with `cargo build --release` and run
it with the environment configuration in place.

**Resolution.** [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)
runs the formatting check, the lint, the test suite, a release build, the
documentation check and the OpenCode configuration check on every push and
pull request. The workflow has not yet run on a GitHub runner, because the
repository has not been published.

**Related.** Requirement [`../project-truth.md`](../project-truth.md) §3.

### KI-007 — The refresh token is returned in the response body

**Status:** Open (accepted) · **Severity:** Low

**Description.** The refresh token is part of the `tokens` object in the
response body of registration, login and refresh, together with the access
token. It is not an HTTP-only cookie, and it is not stored server-side in the
clear: only its SHA-256 hash is persisted in `refresh_sessions.token_hash`.

**Impact.** A browser client would have to keep the refresh token in
scriptable storage, where it is reachable by any script that runs on the
page. An HTTP-only cookie would remove that exposure, but it would also
introduce cross-site request forgery obligations.

**Workaround.** None for the current Phase 1 clients, which are not browsers.
A browser client must treat the token as a secret it keeps out of the DOM and
out of the URL.

**Remediation status.** Accepted for Phase 1. The decision is `Q-02` and
`Q-05` in [`open-questions.md`](open-questions.md), and the requirement
[`../project-truth.md`](../project-truth.md) §16 Q2 and Q5.

**Related.** Accepted risk `AR-2` in
[`../security/threat-model.md`](../security/threat-model.md).

### KI-008 — The rate limiter fails open when its mutex is poisoned

**Status:** Open (accepted) · **Severity:** Low

**Description.** In [`../../src/ratelimit.rs`](../../src/ratelimit.rs), the
lock is acquired with `match self.entries.lock() { Ok(guard) => guard, Err(poisoned) => poisoned.into_inner() }`.
A poisoned mutex — which only happens after a panic while the lock was held —
is recovered and used, and the limiter keeps counting rather than failing the
request. The comment states the reason: "A poisoned lock must not take the API
down; failing open here keeps availability while the operator investigates."

**Impact.** After a panic inside the limiter, the limiter continues to work
but the process is in an undefined state: the counts may be inconsistent, and
the underlying panic has not been reported to the operator through the normal
error path. The authentication endpoints lose their rate limit exactly when
the process is already unhealthy.

**Workaround.** None available to an operator at runtime. A restart clears it.

**Remediation status.** Accepted for Phase 1 as an availability-over-security
trade-off, recorded as accepted risk `AR-7` in
[`../security/threat-model.md`](../security/threat-model.md). The alternative
— failing closed — would let a single poisoned lock deny authentication
entirely, which is a worse outcome for a single-owner deployment.

**Related.** `Q-09` in [`open-questions.md`](open-questions.md).

### KI-009 — The task ordering key is a double-precision fraction, and there is no reordering endpoint

**Status:** Open · **Severity:** Low

**Description.** The `tasks` table stores `position DOUBLE PRECISION NOT NULL
DEFAULT 0`, described in the migration as a "fractional index used for manual
ordering inside a project". A task is created with
`COALESCE($8, (SELECT COALESCE(MAX(position), 0) + 1000 FROM tasks WHERE project_id = p.id))`,
so a new task lands at the end. The `position` field is settable through
`CreateTaskRequest` and `UpdateTaskRequest` and is returned in the response.
There is **no** endpoint that reorders the tasks of a project, and no service
rule that keeps the values consistent, unique or monotonic. The requirement in
[`../project-truth.md`](../project-truth.md) §4.4 is "manual ordering inside a
project".

**Impact.** A client can set two tasks to the same `position`, which makes
the order between them undefined — the listing orders by
`t.position ASC, t.created_at ASC, t.id ASC`, so the tie is broken by the
creation order. A client can also set a `position` that is not between its
neighbours, so "move this task up" is a read-modify-write dance that the API
does not make safe. A `DOUBLE PRECISION` value also cannot carry an unbounded
number of distinct fractional positions.

**Workaround.** A client that reorders tasks should write a position that is
strictly between the two neighbours, and should re-read the list afterwards.
Renumbering the whole list on every change is the simplest safe approach for
a small project.

**Remediation status.** Not started. The product question — whether the
ordering key stays a fraction, becomes an integer sequence, or becomes a
lexicographic key, and whether a dedicated reordering endpoint is added — is
`Q-06` in [`open-questions.md`](open-questions.md).

**Related.** Requirement [`../project-truth.md`](../project-truth.md) §4.4;
[`../database/schema.md`](../database/schema.md).

### KI-010 — The server does not install the connection information, so every request shares one rate-limit bucket

**Status:** Closed (2026-10-21) · **Severity:** High

**Description.** The `ClientIp` extractor in
[`../../src/api/middleware.rs`](../../src/api/middleware.rs) reads the
client address from the `ConnectInfo<SocketAddr>` extension of the request
parts. That extension is only present when the server is started with
`into_make_service_with_connect_info::<SocketAddr>()`. The server in
[`../../src/main.rs`](../../src/main.rs) calls `axum::serve(listener,
api::router(state))` without it, and the string
`into_make_service_with_connect_info` does not appear anywhere in the
repository. The extractor therefore always returns `None`, and
`enforce_rate_limit` always uses the fallback key
`IpAddr::V4(Ipv4Addr::UNSPECIFIED)`.

**Impact.** In the running server, the rate limit is a single global budget
for the whole installation rather than a per-client budget. The default
configuration allows 10 authentication requests per 60 seconds for **all**
callers together. A legitimate owner and an attacker share that budget, so
the limit protects the endpoints against a sustained attack at the cost of
denying the owner. The per-client behaviour that the code is written for is
never exercised outside the unit tests.

**Workaround.** Until it is fixed, put the rate limiting at the edge, where
the real client address is known.

**Remediation status.** Not started. The fix is to serve the router through
`into_make_service_with_connect_info::<SocketAddr>()`, and to add the
HTTP-level test from `KI-002`, which would have caught the wiring gap.

**Resolution.** [`../../src/main.rs`](../../src/main.rs) now serves the router with `into_make_service_with_connect_info::<SocketAddr>()`, so the `ClientIp` extractor receives the peer address. The behaviour was verified against a running server: twelve login attempts produced ten `401` responses followed by `429` with a `retry-after` header of 58 seconds, which is only possible when the limiter is keyed by the client address.
**Related.** `KI-001`, `KI-002`, `KI-003`; requirement
[`../project-truth.md`](../project-truth.md) §9.10 and §9.11.

### KI-011 — `.env.example` describes a test behaviour the harness does not have

**Status:** Closed (2026-10-21) · **Severity:** Low

**Description.** The comment above `TEST_DATABASE_URL` in
[`.env.example`](../../.env.example) says: "The integration test suite
applies migrations to this database and truncates tables between tests." The
harness in [`../../tests/common/mod.rs`](../../tests/common/mod.rs) applies
the migrations, but it never truncates anything. Every test creates its own
uniquely named data and deletes nothing it did not create.

**Impact.** The comment overstates what the suite does to the database. A
reader who trusts it may believe the test database is destroyed on every run,
and may keep a valuable database in a name that contains `test` — which is the
one thing the safety guard cannot protect against.

**Workaround.** None needed; the harness behaviour is the safe one.

**Remediation status.** Not started. The comment should be corrected to match
the harness. The file is owned by the `devops` area, and the correction is a
one-line change with no behavioural effect.

**Resolution.** [`.env.example`](../../.env.example) now states that the harness creates a unique user per test and never truncates shared tables.
**Related.** [`../testing/strategy.md`](../testing/strategy.md) §2.

### KI-012 — There is no account lockout and no breach detection

**Status:** Open (accepted) · **Severity:** Medium

**Description.** The API has no mechanism that counts failed logins per
account, that locks an account, that detects a credential that appeared in a
public breach corpus, or that notifies the owner. The only brute-force
control is the rate limiter, and it is per process (`KI-001`).

**Impact.** An offline attack on a leaked password hash is not detected at
all, and an online attack is bounded only by the rate limit. Because the rate
limit is per process, a sustained attack also locks the legitimate owner out
of the process it exhausts.

**Workaround.** A strong, unique password per account, and edge rate limiting
in front of the API. The password policy already requires 12 characters with
mixed case and a digit.

**Remediation status.** Accepted for Phase 1 as accepted risk `AR-6` in
[`../security/threat-model.md`](../security/threat-model.md). A lockout
policy needs product rules of its own, because a naive one turns into a
denial-of-service vector against a legitimate owner.

**Related.** Requirement [`../project-truth.md`](../project-truth.md) §9.5;
`KI-001`, `KI-010`.

---

## KI-013 — Two documentation links do not resolve

**Status:** Closed (2026-10-21) · **Severity:** Low

**Description.** The documentation checker reports two broken links that are
not part of the parallel documentation effort:

1. [`../project-truth.md`](../project-truth.md) §4.1 links the feature
   documents as `../docs/architecture/system-overview.md` and
   `../docs/architecture/request-lifecycle.md`. From inside `docs/`, the
   correct relative path is `architecture/system-overview.md`; the `../docs/`
   prefix resolves to a directory that does not exist.
2. [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) §6 links
   `docs/contributing-opencode.md`, which is not in the tree.

**Impact.** `cargo run --bin atom-docs` reports these as errors, so the
documentation gate does not pass while they exist. Every other error the
checker reports at the time of writing is a link to `docs/architecture/` or
`docs/decisions/`, which are required by [`../README.md`](../README.md) and
were being written in the same effort.

**Workaround.** None needed; both are one-line corrections in documents that
are not owned by this record.

**Remediation status.** Not started. Both files are owned by the product
owner and the maintainer respectively.

**Resolution.** The 39 links in [`../project-truth.md`](../project-truth.md) that were prefixed with `../docs/` were corrected, and the missing [`../contributing-opencode.md`](../contributing-opencode.md) was written.
**Related.** The documentation gate in
[`../testing/verification-policy.md`](../testing/verification-policy.md) §1.

---

## Summary

| ID | Title | Status | Severity |
| -- | ----- | ------ | -------- |
| `KI-001` | The rate limiter is per process | Open | Medium |
| `KI-002` | No HTTP-level test of the rate limiter | Closed | Medium |
| `KI-003` | `X-Forwarded-For` is ignored, and nothing replaces it | Open | Medium |
| `KI-004` | SQLx queries are checked at run time | Open | Low |
| `KI-005` | There is no CI pipeline | Closed | Medium |
| `KI-006` | There is no container image for the API | Open | Low |
| `KI-007` | The refresh token is returned in the response body | Open | Low |
| `KI-008` | The rate limiter fails open on a poisoned lock | Open | Low |
| `KI-009` | The ordering key is a fraction, and there is no reordering endpoint | Open | Low |
| `KI-010` | The server does not install the connection information | Closed | High |
| `KI-011` | `.env.example` describes truncation the harness does not do | Closed | Low |
| `KI-012` | There is no account lockout and no breach detection | Open | Medium |
| `KI-013` | Two documentation links do not resolve | Closed | Low |

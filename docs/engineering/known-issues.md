# ATOM known issues

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-29
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

**Closed issues** (14 of them, `KI-002` through `KI-023`) are in
[`archive/known-issues-closed.md`](archive/known-issues-closed.md); the table
below is the one-line state of every issue, open or closed.

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

### KI-003 — `X-Forwarded-For` is ignored, and nothing replaces it

**Status:** Open (by design, with a gap) · **Severity:** Medium

**Description.** The `ClientIp` extractor in
[`../../src/api/middleware.rs`](../../src/api/middleware.rs) reads the
`ConnectInfo<SocketAddr>` extension and deliberately never reads
`X-Forwarded-For`, because the header is client-controlled unless a trusted
proxy overwrites it. The requirement in
[`../project-truth.md`](../project-truth.md) §9.11 is that the API "never
reads the `X-Forwarded-For` header, and there is no trusted-proxy
declaration" — the refusal is implemented, but the second half is not: there
is no mechanism to declare a trusted proxy.

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

### KI-014 — The configuration has no reload, no remote source and no visible merge rule

**Status:** Open (accepted) · **Severity:** Low

**Description.** The configuration was centralised in the `config/` directory
and resolved once at start-up by [`../../src/config/loader.rs`](../../src/config/loader.rs).
Three limitations remain:

1. There is no command-line or remote configuration source, and the
   configuration cannot be reloaded: a change to a file or to an environment
   variable takes effect only after a restart.
2. The pagination limits (`api.pagination.default_limit` and
   `api.pagination.max_limit`) are part of the published client convention, so
   changing them is a breaking change for a client that was written against
   the current values.
3. A file found on disk is merged **on top of** the embedded copy, key by key.
   An operator who points `ATOM_CONFIG_DIR` at a directory that contains a
   partial file gets the embedded values for everything the file does not
   mention. That is the intended behaviour, but it is not obvious from the
   file itself.

**Impact.** An operator cannot change a limit without a restart of the
process, and cannot see, from the file alone, which values are in force.
A pagination limit that is raised changes what every existing client
receives, so the setting is only safe to change deliberately. The merge rule
means a short override file is not a complete configuration, which is easy to
misread as "the rest is unset".

**Workaround.** Restart the process after every configuration change. Write an
override file that contains every key it intends to change, and treat the
committed files in [`../../config/`](../../config/) as the source of the
values that are not overridden. Change a pagination limit only as a deliberate,
announced change to the client contract.

**Remediation status.** Not started. The reload question — whether a
deployment may change a setting without a restart, and which settings may
change at run time — is `Q-12` in
[`open-questions.md`](open-questions.md). The merge rule is documented in
[`../../config/README.md`](../../config/README.md) §2.

**Related.** `Q-12` in [`open-questions.md`](open-questions.md);
[`../../config/README.md`](../../config/README.md);
[`../architecture/deployment.md`](../architecture/deployment.md);
[`../api/conventions.md`](../api/conventions.md).

---

### KI-022 - The default build now compiles Burn, which costs 21 minutes and 12.9 GB

**Status:** Open (accepted for Phase 2) · **Severity:** Low

**Description.** `ADR-0008` §7.3 puts `burn` behind an `ml` Cargo feature that
is **on by default**, so that a plain `cargo build` and `cargo test` exercise
the path a deployment uses. The consequence is that every default build of
[`../../Cargo.toml`](../../Cargo.toml) now compiles the Burn graph
(`burn-core`, `burn-tensor`, `burn-nn`, `burn-linalg`) on the pure-Rust `flex`
backend, and burn 0.22 declares an MSRV of **rustc 1.95**.

**Impact.** Measured on the development machine (Windows, `x86_64-pc-windows-msvc`,
`--release`): a clean build of the burn graph took **21 m 31 s**, and
`target/` reached **12.9 GB**. A contributor on an older toolchain gets an
MSRV error rather than a working build; the development machine's `stable` was
1.92.0 and had to be updated to 1.99.1. Nothing about a *request* is affected:
the spike measured 0.0414 ms per text and 93.6 MiB of peak RSS at steady state.

**Workaround.** `cargo build --no-default-features` and `cargo test
--no-default-features` build and test the whole API without Burn; the
`atom-ml-spike` binary declares `required-features = ["ml"]`, so it is skipped
rather than reported as broken. A production build that does not enable ML
should pass `--no-default-features`, as the `docs` feature already expects.

**Remediation status.** Accepted, not scheduled to change: turning `ml` off by
default would defeat the reason it is on (`ADR-0008` §7.3). The cost is recorded
so that CI cache sizing and contributor expectations are set deliberately. If
the build cost becomes a problem, the decision to revisit is the default value
of the feature, not the dependency.

**Related.** [`../../Cargo.toml`](../../Cargo.toml),
[`../decisions/ADR-0008-local-pure-rust-ml.md`](../decisions/ADR-0008-local-pure-rust-ml.md) §7.3,
`ATOM-CHANGE-20261024-010`.

---

### KI-024 - An API key has no scope, no expiry, and can mint another key

**Problem.** An API key issued by `POST /api/v1/api-keys` carries the **full
authority of its owner**. There is no scope model, no expiry, and no restriction
on which operations it may call - including `POST /api/v1/api-keys` itself. The
only way to end a key's access is `DELETE /api/v1/api-keys/{key_id}`, and a key
that was used to mint another key leaves that second key alive when the first is
revoked.

**Impact.** The credential is honest for the job it was built for - one machine
client acting as one person - but it is not a credential a third party can ever
hold. A leaked key is a full-account compromise until somebody notices and
revokes it, and revocation does not undo access the key granted. The hourly
`last_used_at` stamp is the only audit signal, and it is deliberately coarse: a
use inside the hour of the previous stamp is not recorded.

**Why it is not fixed here.** Scopes are a product decision with no requirement
behind them (`docs/project-truth.md` has no notion of a limited credential), and
an expiry needs a rotation story ATOM does not have. `ADR-0011` records both as
accepted negatives rather than pretending the key is safer than it is.

**Recommended fix, in order.**

1. Add a `scopes` column and a per-route requirement, defaulting to the current
   full authority so nothing existing changes meaning.
2. Add `expires_at`, and refuse a key past it with the same `401` a revoked key
   gets.
3. Decide whether a key may mint another key. If it may not, that is one check
   in the create handler and it removes the "revoking the parent does not revoke
   the child" case above.

**Not a documentation defect.** `docs/security/threat-model.md` records it as
`T-21` and `docs/features/api-keys/status.md` states it as a limitation; what is
missing is the control, not the description.

**Related.** [`../../migrations/0007_api_keys.sql`](../../migrations/0007_api_keys.sql),
[`../../src/api_keys/service.rs`](../../src/api_keys/service.rs),
[`../decisions/ADR-0011-api-keys.md`](../decisions/ADR-0011-api-keys.md),
[`../security/threat-model.md`](../security/threat-model.md),
[`../features/api-keys/status.md`](../features/api-keys/status.md).

## Summary

| ID | Title | Status | Severity |
| -- | ----- | ------ | -------- |
| `KI-001` | The rate limiter is per process | Open | Medium |
| `KI-002` | No HTTP-level test of the rate limiter | Closed | Medium |
| `KI-003` | `X-Forwarded-For` is ignored, and nothing replaces it | Open | Medium |
| `KI-004` | SQLx queries are checked at run time | Open | Low |
| `KI-005` | There is no CI pipeline | Closed | Medium |
| `KI-006` | There is no container image for the API | Closed | Low |
| `KI-007` | The refresh token is returned in the response body | Open | Low |
| `KI-008` | The rate limiter fails open on a poisoned lock | Open | Low |
| `KI-009` | The ordering key is a fraction, and there is no reordering endpoint | Open | Low |
| `KI-010` | The server does not install the connection information | Closed | High |
| `KI-011` | `.env.example` describes truncation the harness does not do | Closed | Low |
| `KI-012` | There is no account lockout and no breach detection | Open | Medium |
| `KI-013` | Two documentation links do not resolve | Closed | Low |
| `KI-014` | The configuration has no reload, no remote source and no visible merge rule | Open | Low |
| `KI-015` | The refresh rotation extended the absolute lifetime of a session | Closed | High |
| `KI-016` | The configuration accepted a weak password policy and an unbounded clock skew | Closed | Medium |
| `KI-017` | The configuration accepted a session prune grace shorter than the expired-token window needs | Closed | Medium |
| `KI-018` | The configured body limit stops at an independent 4 MiB backstop | Closed | Low |
| `KI-019` | The shutdown drain has a test that has not run on a unix host | Closed | Low |
| `KI-020` | The seeder's database-name guard refused the repository's own default database | Closed | Medium |
| `KI-021` | The compose volume was mounted where PostgreSQL 18 no longer keeps its data | Closed | High |
| `KI-022` | The default build now compiles Burn, which costs 21 minutes and 12.9 GB | Open | Low |
| `KI-023` | The quality gate could not execute a gate, so no chain had a verdict | Closed | High |
| `KI-024` | An API key has no scope, no expiry, and can mint another key | Open | Medium |


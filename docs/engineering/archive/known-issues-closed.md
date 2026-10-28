# ATOM known issues - closed archive

Moved from `docs/engineering/known-issues.md` on 2026-10-29 by
`ATOM-CHANGE-20261029-001`; entry text is unchanged and link targets were
repointed. A closed issue stays closed:
its resolution records what fixed it and the evidence. The summary table
and the open issues are in [`../known-issues.md`](../known-issues.md).

### KI-002 — There is no HTTP-level test of the rate limiter

**Status:** Closed (2026-10-21) · **Severity:** Medium

**Description.** The limiter's algorithm is unit tested in
[`../../src/ratelimit.rs`](../../../src/ratelimit.rs) — three tests cover the
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

**Resolution.** `tests/auth_api.rs` now contains `the_authentication_endpoints_are_rate_limited`, which drives the login endpoint past the limit and asserts the `429` status and the `rate_limited` error code. The test uses a dedicated state built by `test_state_with_rate_limit` in [`../../tests/common/mod.rs`](../../../tests/common/mod.rs). `cargo test --all-targets` passed with 70 tests.
**Related.** [`../testing/strategy.md`](../../testing/strategy.md) §6 item 1;
requirement [`../project-truth.md`](../../project-truth.md) §13.5.


### KI-005 — There is no CI pipeline

**Status:** Closed (2026-10-22) — **Severity:** Medium

**Description.** The repository contained no CI configuration. The three
quality gates were run by hand, locally, and were recorded in
[`change-log.md`](../change-log.md).

**Impact.** Nothing enforced the quality gates on a change, so a change could
be merged with a failing test, a lint warning or an unresolved documentation
link.

**Workaround.** The reviewer and the quality gate run the gates manually and
record the output, as described in
[`../testing/verification-policy.md`](../../testing/verification-policy.md).

**Resolution.** [`.github/workflows/ci.yml`](../../../.github/workflows/ci.yml)
runs the formatting check, the lint, the test suite, a release build, the
documentation check in both modes and the OpenCode configuration check on every
push and pull request to `master` or `develop`.

**Verification status.** The repository is published, and the workflow names the
branches that actually exist. Its trigger previously named `main`, which was
never the branch in use, so a push could not have started it.

The workflow has now been **observed passing on GitHub-hosted runners**:

| Run | Branch | Both jobs |
| --- | ------ | --------- |
| [39761383251](https://github.com/g-zenr/atom-api/actions/runs/39761383251) | `fix/audit-remediation-phase-1` | pass, 4m15s |
| [39765107169](https://github.com/g-zenr/atom-api/actions/runs/39765107169) | `chore/github-workflow-standards` | pass, 1m25s |

Two defects were found by the first runs and fixed in
`ATOM-CHANGE-20261022-008`: the validation script was committed without the
executable bit, so it had never run; and once it ran, agent discovery reported
every agent missing, because the agents inherit a model from a provider a hosted
runner has no credentials for. The job now skips exactly those two checks, as a
warning rather than a silent pass.

**Related.** Requirement [`../project-truth.md`](../../project-truth.md) §13.6;
`KI-004`; `KI-006`.


### KI-006 — There is no container image for the API

**Status:** Closed · **Severity:** Low · **Closed:** 2026-10-24

**Description.** [`../../docker-compose.yml`](../../../docker-compose.yml) brought up
PostgreSQL for local development and for the test suite, and stated in its own
header that it does "NOT build an application image". The API ran from the source
tree with `cargo run`, so a deployment was defined by the source tree and the
toolchain rather than by an artifact.

**Resolution.** `Dockerfile` at the repository root builds the image: a
two-stage build, base images pinned by tag and digest, `--locked`, no toolchain in
the runtime stage, and an unprivileged UID 10001.
[`../../.github/workflows/container.yml`](../../../.github/workflows/container.yml)
builds it on every pull request that touches an input of the build and on every
push to `master` or `develop`, then verifies the properties above rather than
trusting the build. The image is never pushed: publishing is a release decision.

**Verification.** The workflow runs on GitHub-hosted runners, which have a Docker
daemon. It has now been observed green on `develop` — run
[`40019454167`](https://github.com/g-zenr/atom-api/actions/runs/40019454167),
1m43s — so the build and the five probes (`Config.User`, `uid=10001`, no
toolchain, `ldd`, start-up without its environment) all executed. It has still
not been executed on the development machine, which has the Docker CLI but no
usable daemon (`docker info` answers `500`). That is recorded rather than
implied by a green tick somewhere else.

**Related.** `KI-005`, [`../architecture/deployment.md`](../../architecture/deployment.md)
§5.1a.


### KI-010 — The server does not install the connection information, so every request shares one rate-limit bucket

**Status:** Closed (2026-10-21) · **Severity:** High

**Description.** The `ClientIp` extractor in
[`../../src/api/middleware.rs`](../../../src/api/middleware.rs) reads the
client address from the `ConnectInfo<SocketAddr>` extension of the request
parts. That extension is only present when the server is started with
`into_make_service_with_connect_info::<SocketAddr>()`. The server in
[`../../src/main.rs`](../../../src/main.rs) calls `axum::serve(listener,
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

**Resolution.** [`../../src/main.rs`](../../../src/main.rs) now serves the router with `into_make_service_with_connect_info::<SocketAddr>()`, so the `ClientIp` extractor receives the peer address. The behaviour was verified against a running server: twelve login attempts produced ten `401` responses followed by `429` with a `retry-after` header of 58 seconds, which is only possible when the limiter is keyed by the client address.
**Related.** `KI-001`, `KI-002`, `KI-003`; requirement
[`../project-truth.md`](../../project-truth.md) §9.10 and §9.11.


### KI-011 — `.env.example` describes a test behaviour the harness does not have

**Status:** Closed (2026-10-21) · **Severity:** Low

**Description.** The comment above `TEST_DATABASE_URL` in
[`.env.example`](../../../.env.example) says: "The integration test suite
applies migrations to this database and truncates tables between tests." The
harness in [`../../tests/common/mod.rs`](../../../tests/common/mod.rs) applies
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

**Resolution.** [`.env.example`](../../../.env.example) now states that the harness creates a unique user per test and never truncates shared tables.
**Related.** [`../testing/strategy.md`](../../testing/strategy.md) §2.


## KI-013 — Two documentation links do not resolve

**Status:** Closed (2026-10-21) · **Severity:** Low

**Description.** The documentation checker reports two broken links that are
not part of the parallel documentation effort:

1. [`../project-truth.md`](../../project-truth.md) §4.1 links the feature
   documents as `../docs/architecture/system-overview.md` and
   `../docs/architecture/request-lifecycle.md`. From inside `docs/`, the
   correct relative path is `architecture/system-overview.md`; the `../docs/`
   prefix resolves to a directory that does not exist.
2. [`../../CONTRIBUTING.md`](../../../CONTRIBUTING.md) §6 links
   `docs/contributing-opencode.md`, which is not in the tree.

**Impact.** `cargo run --bin atom-docs` reports these as errors, so the
documentation gate does not pass while they exist. Every other error the
checker reports at the time of writing is a link to `docs/architecture/` or
`docs/decisions/`, which are required by [`../README.md`](../../../README.md) and
were being written in the same effort.

**Workaround.** None needed; both are one-line corrections in documents that
are not owned by this record.

**Remediation status.** Not started. Both files are owned by the product
owner and the maintainer respectively.

**Resolution.** The 39 links in [`../project-truth.md`](../../project-truth.md) that were prefixed with `../docs/` were corrected, and the missing [`../contributing-opencode.md`](../../contributing-opencode.md) was written.
**Related.** The documentation gate in
[`../testing/verification-policy.md`](../../testing/verification-policy.md) §1.

---


### KI-015 — The refresh rotation extended the absolute lifetime of a session

**Status:** Closed (2026-10-22) · **Severity:** High

**Description.** `AuthService::refresh` in
[`../../src/auth/service.rs`](../../../src/auth/service.rs) computed the expiry of
the replacement session from the current time:
`Utc::now() + refresh_token_ttl`. Every rotation therefore pushed the
deadline out by a full refresh-token lifetime, so the absolute lifetime that
[`../decisions/ADR-0003-auth-token-design.md`](../../decisions/ADR-0003-auth-token-design.md)
records — 14 days by default, measured from the first login — was not
enforced at all. The session it replaced kept its own `expires_at`, but the
session the client was handed did not.

**Impact.** The absolute lifetime is the **only** bound on the usefulness of a
stolen refresh token. With the deadline recomputed on every rotation, a
thief who refreshes the stolen token keeps the session alive indefinitely, and
the owner cannot end it by waiting: the session simply renews itself. The
documented guarantee — "a stolen token is worthless after the lifetime" — was
false, and the reuse detection did not help either, because the thief holds
the current token and never presents a rotated one.

**Workaround.** None available to an operator. The defect is in the code, not
in the configuration, so no setting avoids it.

**Remediation status.** Not started. The fix is to have the rotation inherit
the `expires_at` of the presented session instead of recomputing it, so that
the deadline is measured from the moment the session was created.

**Resolution.** The replacement session now **inherits the original
`expires_at`**. A rotation therefore preserves the absolute deadline: the
absolute lifetime of a session is measured from its creation and is not
extended by use. The property is recorded in
[`../decisions/ADR-0003-auth-token-design.md`](../../decisions/ADR-0003-auth-token-design.md)
§2, analysed in [`../architecture/data-architecture.md`](../../architecture/data-architecture.md)
§7.1 and described in
[`../database/schema.md`](../../database/schema.md) §3. The idle timeout, the
other half of the lifetime policy, is still open: `Q-03` in
[`open-questions.md`](../open-questions.md).

**Related.** `Q-03` in [`open-questions.md`](../open-questions.md);
[`../decisions/ADR-0003-auth-token-design.md`](../../decisions/ADR-0003-auth-token-design.md);
[`../architecture/data-architecture.md`](../../architecture/data-architecture.md)
§7.1; requirement [`../project-truth.md`](../../project-truth.md) §9.4.

---


### KI-016 — The configuration accepted a weak password policy and an unbounded clock skew

**Status:** Closed (2026-10-22) · **Severity:** Medium

**Description.** `validate_settings` in
[`../../src/config/validation.rs`](../../../src/config/validation.rs) refused a
password minimum of zero and a maximum below the minimum, but it enforced **no
floor** on `auth.password.min_length`. A deployment could therefore set a
minimum of one character and turn the password policy into a formality. The
same function applied no **upper bound** to `auth.clock_skew_seconds`: a
deployment could set an allowance of an hour, and the access-token validator
would then accept a token for an hour after it expired.

**Impact.** Two security properties were configurable away. The password
policy, which is the only thing that stands between a leaked credential list
and a working account, could be reduced to a single character; and the
expiry check on the access token, which is the short-lived half of the token
design in
[`../decisions/ADR-0003-auth-token-design.md`](../../decisions/ADR-0003-auth-token-design.md),
could be stretched by an order of magnitude.

**Workaround.** None available to an operator beyond not setting the value:
the default configuration is safe, and the defect is only reachable through a
deliberate override.

**Remediation status.** Not started. The fix is a floor on the password
minimum and a ceiling on the clock-skew allowance, both enforced in
`validate_settings`, with the numeric bounds as constants beside the rules.

**Resolution.** `validate_settings` now refuses a password minimum below
`MIN_PASSWORD_LENGTH` (twelve characters) and a clock-skew allowance above
`MAX_CLOCK_SKEW_SECONDS` (300 seconds). The bounds are constants in
[`../../src/config/validation.rs`](../../../src/config/validation.rs), not
settings, so a deployment can tighten the policy but not weaken it below the
floor. The constraints are recorded in the reference table in
[`../architecture/deployment.md`](../../architecture/deployment.md) §1.3.

**Related.** The Argon2 memory floor, which was already enforced;
[`../architecture/deployment.md`](../../architecture/deployment.md) §1.3.

---


### KI-017 — The configuration accepted a session prune grace shorter than the expired-token window needs

**Status:** Closed (2026-10-22) · **Severity:** Medium

**Description.** `validate_settings` refused a prune grace of zero, but it
enforced no **minimum** on `app.sessions.prune_grace_seconds`. The grace is
the window in which the row of an **expired** session is kept, so that a token
that expired recently is still recognised as *expired* rather than as one that
never existed. A deployment that set it to, say, one second would delete the
evidence almost immediately.

**Impact.** With a short grace, the row of a session that expired a moment
ago is gone, and the token is answered as *invalid* rather than as *expired*.
The client loses the distinction, and the message that reaches it no longer
says what actually happened. The reuse detection is not affected: a revoked
session is kept until its absolute deadline, which is the refresh-token
lifetime, and the pruning task only ever deletes rows that are already past
it.

**Workaround.** None available to an operator. The defect is only reachable
through a deliberate override; the default value of 86 400 seconds is safe.

**Remediation status.** Not started. The fix is a minimum grace, enforced in
`validate_settings`, with the numeric bound as a constant beside the rule.

**Resolution.** `validate_settings` now refuses a prune grace below
`MIN_PRUNE_GRACE_SECONDS`, one hour. The bound is a constant in
[`../../src/config/validation.rs`](../../../src/config/validation.rs), not a
setting, so the grace may be lengthened but not shortened below it. The
constraint is recorded in the reference table in
[`../architecture/deployment.md`](../../architecture/deployment.md) §1.3, and the
reason the grace exists is recorded in
[`../../config/app.toml`](../../../config/app.toml).

**Related.** The refresh-token expiry check, which depends on the row
surviving; [`../architecture/deployment.md`](../../architecture/deployment.md) §1.3.

---


### KI-018 — The configured body limit stops at an independent 4 MiB backstop

**Status:** Closed · **Severity:** Low

**Description.** `api::extract::Json` in
[`../../src/api/extract.rs`](../../../src/api/extract.rs) passes the constant
`MAX_BODY_BYTES_BACKSTOP` — `4 * 1024 * 1024` bytes — to
`axum::body::to_bytes`, in addition to the limit the router installs. The
configured `app.limits.max_body_bytes` is therefore honoured only up to 4 MiB:
**a body is refused when it exceeds either of the two limits**.

The backstop and the ceiling were **introduced by the remediation recorded in
[`change-log.md`](../change-log.md) as `ATOM-CHANGE-20261022-003`**. Before it,
the extractor passed `usize::MAX` and the configured limit was the only cap:
a deployment that set `APP_MAX_BODY_BYTES` above 4 MiB was served exactly
that limit. The finding that started this (`F-10` in
[`../audit/audit-report.md`](../../audit/audit-report.md)) was that the published
API conventions described a 4 MiB backstop that the code did not have. The
remediation chose to make the code match the published convention and to add
the ceiling that makes the two agree.

**Impact.** A deployment that had `max_body_bytes` above 4 MiB previously
started and served that limit; it now **refuses to start**, with a message
that names the bound. That is a deliberate tightening: the ceiling is what
stops one request from pulling an unbounded body into memory. The default of
1 MiB is unaffected.

**Remediation.** The second of the two options was taken: the configuration
validation now refuses a `max_body_bytes` above the ceiling of the body
reader. The configuration therefore cannot promise more than the API
delivers, and the operator is told the bound at start-up instead of finding
out at request time.

The ceiling lives in
[`../../src/config/validation.rs`](../../../src/config/validation.rs) as
`MAX_BODY_BYTES_BACKSTOP`, next to the other floors, and the extractor reads
it from the configuration module. The rule is covered by the configuration
test `a_body_limit_above_the_reader_ceiling_is_refused`.

**Related.** The review finding `F-03`, recorded in
[`change-log.md`](../change-log.md) under `ATOM-CHANGE-20261022-002`;
[`../architecture/deployment.md`](../../architecture/deployment.md) §4;
[`../api/conventions.md`](../../api/conventions.md) §9;
[`../audit/audit-report.md`](../../audit/audit-report.md) `F-10`.

---

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
| `KI-023` | The quality gate could not execute a gate, so no chain had a verdict | Closed | High |


### KI-019 — The shutdown drain had a test that had not run on a unix host

**Status:** Closed · **Severity:** Low · **Closed:** 2026-10-24

**Description.** The independent review (`ATOM-REVIEW-20261022-003`) recorded that
the shutdown drain was "verified by reading the code and by compilation, not by a
signal test". Two tests were written in
[`../../tests/shutdown.rs`](../../../tests/shutdown.rs) to close that: one proves the
signal future resolves immediately instead of sleeping the grace away, the other
proves the drain is bounded when an in-flight request cannot finish.

**The gap at the time.** They had not been executed on a unix host. The
development machine is Windows, the file is `#![cfg(unix)]`, and both routes to a
Linux runtime failed — the Docker daemon stopped mid-pull, and a cross-compilation
check could not link for want of `x86_64-linux-gnu-gcc`.

**Resolution.** CI executed them on `ubuntu` in run
[`40011694171`](https://github.com/g-zenr/atom-api/actions/runs/40011694171):

```text
Running tests/shutdown.rs
WARN atom_api: the shutdown grace elapsed with requests still in flight grace_seconds=2
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The warning is the evidence that matters. It is the bounded-drain test reaching
its deadline with a request still in flight — the behaviour that could not be
observed on Windows, and the exact path the review found unverified. The
unix-only suite was therefore **181 tests** on the day this was recorded; the
current count is in
[`implementation-status.md`](../implementation-status.md), and the shutdown
behaviour is `Verified`.

**Related.** [`../../tests/shutdown.rs`](../../../tests/shutdown.rs),
[`../testing/strategy.md`](../../testing/strategy.md) §1.4,
`ATOM-REVIEW-20261022-003`.

---


### KI-020 — The seeder's database-name guard refuses the repository's own default database

**Status:** Closed · **Severity:** Medium · **Recorded:** 2026-10-24 · **Closed:** 2026-10-24

**Description.** The locked seeder contract `C-S1` guard `G2` refuses to run
unless the database name is a development database. The rule as originally
locked was a substring test for `dev`, `local` or `test`. Two independent
reviews found it wrong in both directions:

- it admits names that are not development databases - `contest`, `latest`,
  `mytest`, `devops`, `production_test` - so a restored staging or production
  dump renamed for convenience would pass;
- it refuses the repository's own default database. `docker-compose.yml` creates
  `POSTGRES_DB: atom` and `.env.example` points `DATABASE_URL` at `.../atom`,
  so the documented local setup cannot run the seeder at all.

There is a second, unrelated defect in the same guard: `G1` was a substring test
for `production`, but `Environment::parse` in
[`../../src/config/mod.rs`](../../../src/config/mod.rs) also maps `prod` to
production, so the documented production alias passed the guard.

**Impact.** A developer following the quickstart cannot seed; a developer who
renames a database to contain `test` to get past the guard weakens the only
client-side protection the tool has. Neither outcome is acceptable, and the
second teaches the wrong habit.

**Workaround before `S1` landed.** Create a database whose name the guard accepts
(`createdb atom_dev`) and point `DATABASE_URL` at it. The specification in
[`../database/seeding.md`](../../database/seeding.md) documents this.

**Resolution.** Contract amendments `A-C-S1-5` and `A-C-S1-6` in
[`runs/chain/ledger.md`](../runs/chain/ledger.md) replaced both substring tests with
allow-lists that match the code: `G1` allows only the values `Environment::parse`
calls development, and `G2` is a whole-name allow-list (`atom_dev`, `atom_test`,
`atom_local`, `atom_demo`, plus a `_dev`/`_test`/`_local` suffix rule) read
through the driver's own URL parser, with a `current_database()` cross-check
after connecting. Both are implemented in `src/bin/atom-seed.rs` and covered by
`tests/seeder.rs`.

The documented setup was changed to match rather than the guard weakened:
`scripts/create-test-database.sh` creates `atom_dev`, `.env.example`, `README.md`,
`CONTRIBUTING.md`, `Makefile` and CI point at it, and `docs/architecture/`
names it as the development database.

The resolution was observed end to end on 2026-10-24, not just read:

```text
$ createdb -h 127.0.0.1 -U atom atom_dev
$ DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_dev APP_ENV=development \
    cargo run --quiet --bin atom-seed -- --profile dev --reset
atom-seed: profile dev
  users      inserted 2     skipped 0
  notes      inserted 200   skipped 0
  projects   inserted 8     skipped 0
  tasks      inserted 60    skipped 0
atom-seed: verification passed (5 checks); 270 rows inserted, 0 skipped
```

and the guard still refuses the old default:

```text
atom-seed: refused: the database name "atom" is not allowed. Allowed names are
"atom_dev", "atom_test", "atom_local" and "atom_demo", or a name ending in
"_dev", "_test" or "_local". Nothing was written.
```

**Residual risk that no guard closes.** No client-side check can tell a
development database from a production database that someone named `atom_dev`.
The guards are typo-catchers. The real boundary is that a database holding the
seeded accounts — which are working credentials with a published password — must
never be reachable by an untrusted client. That is recorded in
[`../security/threat-model.md`](../../security/threat-model.md) as part of the
`T-05` residual, and in [`../database/seeding.md`](../../database/seeding.md) §8.3.

**Related.** [`../database/seeding.md`](../../database/seeding.md),
[`runs/chain/ledger.md`](../runs/chain/ledger.md) `C-S1`,
[`../../security/secure-development.md`](../../security/secure-development.md).

---


### KI-021 — The compose volume was mounted where PostgreSQL 18 no longer keeps its data

**Status:** Closed · **Severity:** High · **Closed:** 2026-10-24

**Description.** [`../../docker-compose.yml`](../../../docker-compose.yml) mounted the
named volume `atom-postgres-data` at `/var/lib/postgresql/data`. That is the path
every PostgreSQL image before 18 used, and it is no longer where the data lives.
The generated `Dockerfile` for `18/alpine3.23` in `docker-library/postgres` sets:

```text
ENV PGDATA /var/lib/postgresql/18/docker
VOLUME /var/lib/postgresql
```

On start-up `docker-entrypoint.sh` therefore looks for `PG_VERSION` under the new
location and then scans the old ones. When it finds no database directory but
does find a mount at the old path, it records
`/var/lib/postgresql/data (unused mount/volume)` as an old database and calls
`docker_error_old_databases`, which exits 1:

```text
Error: in 18+, these Docker images are configured to store database data in a
       format which is compatible with "pg_ctlcluster" (specifically, using
       major-version-specific directory names).  This better reflects how
       PostgreSQL itself works, and how upgrades are to be performed.
...
       The suggested container configuration for 18+ is to place a single mount
       at /var/lib/postgresql which will then place PostgreSQL data in a
       subdirectory, allowing usage of "pg_upgrade --link" without mount point
       boundary issues.
```

**Impact.** On a **fresh** volume the container never initialises, because the
check runs before `initdb`. The init script mounted in
`/docker-entrypoint-initdb.d/` therefore never runs, so neither `atom_test` nor
`atom_dev` would exist — and neither would the image's own `POSTGRES_DB`.
`docker compose up -d postgres` would leave a developer with a container that
exits instead of a database. Nobody had noticed, because `KI-006` records that
this machine has the Docker CLI but no usable daemon (`docker info` answers
`500`), so the container had never been started here.

**Fix.** The mount now targets `/var/lib/postgresql`, the directory the image
declares as its volume; PostgreSQL creates `18/docker` beneath it itself. That is
also what makes the init script run exactly once: the entrypoint processes
`/docker-entrypoint-initdb.d/` only when the data directory is empty, and a mount
at the declared volume root is no longer mistaken for pre-18 data.

**Evidence.** `docker compose config` exits 0 and renders the corrected target:

```yaml
    volumes:
      - type: volume
        source: atom-postgres-data
        target: /var/lib/postgresql
        volume: {}
```

The init script was run twice against the local PostgreSQL 18 with scratch names
(`TEST_DB=atom_test_ki021`, `DEV_DB=atom_dev_ki021`, `POSTGRES_USER=atom`,
`POSTGRES_DB=atom`, so the role and the bootstrap database match the compose
file). The first run created both databases, the second was a no-op, and both
runs exited 0:

```text
--- run 1 ---
CREATE DATABASE
CREATE DATABASE
exit=0
--- run 2 ---
exit=0
--- databases after both runs ---
atom_dev_ki021|atom
atom_test_ki021|atom
```

**What is not verified.** The container was never started. `docker compose up`
was not run: this machine has no usable daemon (`KI-006`), so there is nothing to
start it with. The PostgreSQL 18 behaviour above is read from the generated
`Dockerfile` and `docker-entrypoint.sh` of `docker-library/postgres`
`18/alpine3.23`, not observed in a running container. Confirming the first start
needs a machine with a working Docker daemon, and that confirmation is still
outstanding.

**Related.** [`../../docker-compose.yml`](../../../docker-compose.yml),
[`../../scripts/create-test-database.sh`](../../../scripts/create-test-database.sh),
`KI-006`, `KI-020`.


### KI-023 - The quality gate could not execute a gate, so no chain had a verdict

**Status:** Closed - **Severity:** High - **Recorded:** 2026-10-25 - **Closed:** 2026-10-25

**Description.** `.opencode/agents/quality-gate.md` denies `shell` with a
catch-all and then allows a short list: `git status *`, `git diff *`,
`git log *`, `cargo fmt --check *`, `cargo clippy *`, `cargo check *`,
`cargo test *`, `cargo build *`. The patterns match a **whole command string**.
A gate that needs a test database cannot be written as a bare `cargo test`; it
has to be run as a script that sets the variable first, which the pattern does
not match. `cargo run` is absent entirely, so the documentation checker cannot
be run at all, and `psql` and `gh` are absent, so no database or CI evidence can
be gathered.

**Impact.** Every `quality-gate` attempt in the Phase 2 run before `58f9532`
reported all six gates as **not run**. To its credit each one said so rather than
recording a pass, and two of them still found real defects by reading the tree.
But a gate that cannot run a gate is not a gate, and the consequence was visible
in the record at the time: **no chain in the run had a verdict**, so nothing was
`Verified`, and three features sat at `Implemented` with 504 passing tests and no
independent evidence.

**Workaround used in the run.** The orchestrator ran the six gates itself and
recorded the raw exit codes in the ledger. That is honest but it is
self-verification, which is exactly what the gate exists to avoid.

**Recommended fix, in order.**

1. Add `cargo run *` and `psql *` to the allow-list, so `atom-docs` and the
   database evidence are reachable.
2. Let the gate set the test URL without a shell wrapper: either read
   `TEST_DATABASE_URL` from the agent's own environment, or add an allow entry
   for the exact compound form the team standardises on, for example
   `$env:TEST_DATABASE_URL*`.
3. If neither is acceptable, give the gate a single read-only entry point - one
   `scripts/gate.ps1` that runs all six and prints the exit codes - and allow
   only that. A gate with one allowed command is easier to reason about than a
   glob list, and it cannot be widened by accident.

**Not a documentation defect.** Nothing in `docs/` is wrong because of this; what
is missing is evidence, and no amount of prose substitutes for it.

**Resolution (2026-10-25).** The recommended fix 3 was implemented in `58f9532`:
[`scripts/gate.ps1`](../../../scripts/gate.ps1) runs all six gates from one
read-only entry point, sets `TEST_DATABASE_URL` itself, refuses a connection
string whose database name lacks `test`, and exits non-zero if any gate fails.
The `quality-gate` role allow-lists exactly that script. The first executable
verdict is `ATOM-REVIEW-20261024-011`: six gates at `0`, 504 tests, **FAIL on
documentation** with five blocking findings, all now repaired. `KI-023` is
closed; the remaining gap is the missing re-gate, not the mechanism.

**Related.** [`.opencode/agents/quality-gate.md`](../../../.opencode/agents/quality-gate.md),
[`../testing/verification-policy.md`](../../testing/verification-policy.md),
[`runs/chain/ledger.md`](../runs/chain/ledger.md) `## Checkpoints`.

---


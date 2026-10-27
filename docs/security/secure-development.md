# ATOM secure development rules

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `docs` agent

These are the rules a change to this repository must follow, and the concrete
check that proves each one. They are the security-specific half of
[`../../AGENTS.md`](../../AGENTS.md) §4; the engineering rules there remain in
force in full.

Each rule below has four parts: **the rule**, **why**, **the check** that can
be run, and **the test** that must exist. A rule with no check is a wish; a
rule with no test is a convention that will eventually break.

---

## 1. The rules

### SD-1 — All SQL is parameterised

**Rule.** Build every statement with `sqlx::query`, `sqlx::query_as` or
`sqlx::query_scalar` and bind every value with `.bind(...)`. Never concatenate
or interpolate a value into a statement. The only permitted interpolation is a
static fragment that contains no user input.

**Why.** Parameter binding is the only defence against SQL injection that does
not depend on remembering to escape something.

**Check.** Every `sqlx::query*` call must be followed by `.bind(...)` calls and
no `format!`, `push_str` or `+` inside the statement. See §2.

**Test.** The integration suite exercises every endpoint with a body that a
hostile client could send; a change to a statement keeps the existing cases
passing.

### SD-2 — Every query that touches user data is scoped by the caller's `user_id`

**Rule.** The `user_id` comes from the credential, never from the request. It
is a bound parameter in the statement itself.

**Why.** Ownership enforced in SQL cannot be bypassed by a handler bug, and it
is the property that makes a single-owner product safe by default.

**Check.** Every `SELECT`, `UPDATE` and `DELETE` in a repository under
[`../../src/notes/`](../../src/notes/),
[`../../src/projects/`](../../src/projects/),
[`../../src/tasks/`](../../src/tasks/),
[`../../src/users/`](../../src/users/) and
[`../../src/api_keys/`](../../src/api_keys/) must contain a `user_id`
predicate, or a join to `projects` whose predicate is `p.user_id = $n`. The
`api_keys` table is private like the others: it carries its own non-null
`user_id`, and the create, list, revoke and usage-stamp statements all bind it.
**One statement is deliberately outside the rule**, because it is the statement
that discovers the owner: `FIND_KEY_IDENTITY` in `src/api_keys/repository.rs`
selects one row by the unique `key_hash` and reaches `user_id` through its join
to `users`, so it has nothing to scope yet. It is a single-row read by a
credential digest and it writes nothing; every other statement in that
repository carries `user_id` in its own `WHERE`. See §2.

**Test.** A cross-account case for every private endpoint: the second account
must receive `404`, never `403` and never the resource.

### SD-3 — An inaccessible resource is `404`, not `403`

**Rule.** If the caller does not own the resource, the response is
`404 Not Found` with the standard error envelope. `403` is reserved for a
caller that is authenticated and known, but is not allowed to perform the
operation — for example revoking another account's session.

**Why.** `403` discloses that the resource exists.

**Check.** The repository returns `None` or `false` for a resource the caller
does not own, and the service converts that into `AppError::NotFound`.

**Test.** The cross-account cases assert the status code is `404`.

### SD-4 — No secret in code, logs, errors or responses

**Rule.** A password, a token, an **API key secret**, its SHA-256 digest, its
display prefix, the signing secret and the database URL must never appear in a
log line, an error message or an API response. The key secret is a bearer
credential in its own right, and the stored digest and the `atom_` + eight-hex
`prefix` name the row it belongs to: none of the three belongs on an error path
either, even though the digest and the prefix cannot authenticate on their own.
Configuration types that carry a secret implement `Debug` manually and print a
redaction placeholder. The panic handler must not log the panic payload. **No
file in [`../../config/`](../../config/) may contain a secret**: the directory is
committed and embedded in the binary, and the two secrets — `DATABASE_URL` and
`APP_JWT_SECRET` — have no file layer at all.

**Why.** Logs and error bodies outlive the request and reach people and
systems that the request never did. A committed configuration file outlives
the deployment it was written for, and reaches everybody who clones the
repository.

**Check.** See §2. The `Debug` implementations of `AuthSettings` and
`DatabaseSettings` must print `[REDACTED]`, and no committed configuration file
may hold a secret-shaped value.

**Test.** The error-envelope tests in [`../../src/error.rs`](../../src/error.rs),
and `the_secret_is_never_rendered` in
[`../../src/config/tests.rs`](../../src/config/tests.rs), which asserts that
the `Debug` rendering of the two configuration types carries the redaction
placeholder and never the secret itself.

### SD-5 — No `unwrap` or `expect` in a request path

**Rule.** A handler, a service or a repository returns `Result` and uses `?`.
`unwrap` and `expect` are acceptable in tests, in the `main` start-up path, in
[`../../src/telemetry.rs`](../../src/telemetry.rs), in the development tool
[`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs) and in the
default-value helpers of
[`../../src/config/settings.rs`](../../src/config/settings.rs) - nowhere else.

The seeder binary [`../../src/bin/atom-seed.rs`](../../src/bin/atom-seed.rs) is
the **exception to the exception**: it must never use `unwrap` or `expect`,
because its errors are derived from `DATABASE_URL` and from row values, and
`expect` on a `sqlx::Error` prints the driver's `Debug`, which renders the
connection string from the `Configuration` variant and the offending value from
`DETAIL`. It carries
`#![deny(clippy::unwrap_used, clippy::expect_used)]` for that reason. See
[`../database/seeding.md`](../database/seeding.md) §3.4.
The CORS allowlist is validated when the configuration is loaded, so the router
needs no assertion of its own: an origin that still fails to parse falls back
to an empty header value, and the process is never taken down by it.

**Why.** An `unwrap` in a request path turns a recoverable failure into a
panic, which the panic layer converts into a `500` while discarding the reason.

**Check.** See §2. Every hit must be inside a `#[cfg(test)]` module, in
[`../../src/telemetry.rs`](../../src/telemetry.rs), in the `main` start-up path,
in a default-value helper that parses a constant, or in the development tool
[`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs). Nowhere else — and
**no hit at all** may come from
[`../../src/bin/atom-seed.rs`](../../src/bin/atom-seed.rs), which denies both
lints crate-wide so that a stray `expect` cannot render the connection string.

The CORS allowlist is validated when the configuration is loaded, so the router
needs no assertion of its own. The layer in
[`../../src/api/mod.rs`](../../src/api/mod.rs) falls back to an empty header
value for an origin that cannot be parsed, so there is no `panic!` in the
request path to find.

**Test.** The behaviour of the affected path is covered by a case that would
fail if the code panicked.

### SD-6 — No panic-based control flow in a handler

**Rule.** A handler must not `panic!`, must not index a slice or a map
unchecked, and must not assume that a value exists. A panic is caught by
`CatchPanicLayer` and answered with a generic `500`; the panic payload is
deliberately not logged.

**Why.** A panic loses the reason, and a panic in one request must not be able
to take the process down.

**Check.** See §2.

**Test.** The affected path is covered by a normal case. There is currently no
automated test of the panic handler itself.

### SD-7 — One error shape

**Rule.** Every fallible path returns `AppResult<T>`. A handler never builds a
response by hand. The public envelope is
`{ "error": { "code", "message", "details" } }` and the `code` is the stable
part a client switches on.

**Why.** A client must be able to handle every failure the same way, and the
contract must not drift between endpoints.

**Check.** A new error variant means a new `code` in
[`../../src/error.rs`](../../src/error.rs) and an entry in
[`../api/error-contract.md`](../api/error-contract.md).

**Test.** The affected endpoint's failure case.

### SD-8 — PATCH is partial

**Rule.** An omitted field is left untouched. Never write `null` over an
existing value. The statement uses `COALESCE($n, column)`.

**Why.** A client that changes one field must not accidentally clear another.

**Check.** The `UPDATE` statement must use `COALESCE` for every optional
column.

**Test.** A partial-update case that asserts the untouched field is unchanged.

### SD-9 — Migrations are append-only, and a schema change has a matching model change

**Rule.** Never edit an applied migration. Add a new one. Put the invariant in
the database — a `CHECK`, a `UNIQUE`, a foreign key — wherever the application
also enforces it.

**Why.** An edited migration makes the schema depend on the order in which it
was applied, and the two orders diverge.

**Check.** The diff shows only added files under
[`../../migrations/`](../../migrations/), and every changed column has a
matching change in the `src/` models.

**Test.** The migration applies to an empty database, and the integration
suite still passes.

### SD-10 — A new dependency needs a recorded reason

**Rule.** Do not add a dependency without a concrete reason written in the
change-log entry. Prefer the standard library and the crates already in
[`../../Cargo.toml`](../../Cargo.toml).

**Why.** Every dependency is code the project did not write and must now
maintain and audit.

**Check.** The change-log entry names the dependency and the reason.

**Test.** Not applicable; the check is in the record.

### SD-11 — No suppressed lints

**Rule.** Fix the code. Never add `#[allow(...)]`, never add a `clippy.toml`
entry to silence a finding, except for a linter false positive that is
documented in the change-log entry.

**Why.** A suppressed lint is a hole with a comment on it.

**Check.** The diff contains no new `allow` attribute on a lint.

**Test.** `cargo clippy --all-targets --all-features -- -D warnings` passes.

### SD-12 — The authentication endpoints stay rate limited

**Rule.** The rate-limit check runs **before** the credential is examined, on
the three endpoints that accept a guessable credential: registration, login
and refresh. Logout is deliberately not rate limited: it requires a valid
access token, which is not guessable. A rejected request answers `429` with a
`retry-after` header.

**Why.** A limit applied after the credential check does not stop guessing.

**Check.** The handler calls `enforce_rate_limit` as its first action.

**Test.** `the_authentication_endpoints_are_rate_limited` in
[`../../tests/auth_api.rs`](../../tests/auth_api.rs) drives the login endpoint
past the limit and asserts the `429` status and the `rate_limited` code. The
limiter's own algorithm is covered by the unit tests in
[`../../src/ratelimit.rs`](../../src/ratelimit.rs).

### SD-13 — Never trust `X-Forwarded-For`

**Rule.** The client address used for the rate-limit key comes from the
connection information, not from a request header. A deployment behind a
proxy must either terminate the connection or accept a shared bucket.

**Why.** The header is client-controlled unless a trusted proxy overwrites it.

**Check.** See §2. The only occurrence of the header name in the source is the
comment that explains why it is not read.

**Test.** No test exists. The behaviour is verified by review.

### SD-14 — CORS is an explicit allowlist

**Rule.** The allowlist in `api.cors.allowed_origins` — or the
`APP_CORS_ALLOWED_ORIGINS` variable that overrides it — lists the exact
origins. An empty list disables CORS. A `*` entry, and an origin that is not a
valid header value, are refused at start-up with a `ConfigError` from
[`../../src/config/validation.rs`](../../src/config/validation.rs), before the
socket is bound. Credentials are never enabled.

**Why.** A wildcard with credentials is rejected by browsers and is a
cross-origin credential foot-gun. A refusal at start-up is better than a
panic in the router or a layer that silently accepts every origin.

**Check.** See §2: no `allow_credentials`, no wildcard origin. The refusal
lives in the configuration layer, not in the router.

**Test.** The refusals are covered by the configuration tests
`a_wildcard_cors_origin_is_refused` and
`a_cors_origin_that_is_not_a_valid_header_is_refused` in
[`../../src/config/tests.rs`](../../src/config/tests.rs). There is no
HTTP-level test that a request carrying a disallowed `Origin` is answered
without the CORS headers.

### SD-15 — Every input is validated and bounded at the service boundary

**Rule.** The service layer validates length, emptiness and enumerations
before the repository is called. A body carries only the fields the contract
defines. The password length is bounded so that the hashing cost cannot be
used as a denial-of-service vector.

**Why.** The service layer is the only layer that knows the business rules,
and it is the last place before the data is written.

**Check.** A new field has a validation rule, a maximum length and a test for
the boundary value.

**Test.** A unit test for each rule, and a `422` case for each rejected input.

### SD-16 — A public contract change is documented

**Rule.** A new endpoint, a changed request or response shape, a new error
code: update the OpenAPI annotation, the API document and the feature
document, and record the change in the change log.

**Why.** The OpenAPI document is generated from the handlers so that it cannot
drift from the routes, but the prose around it can drift.

**Check.** The diff touches both the annotation and
[`../api/`](../api/).

**Test.** The documentation checker resolves every link and every route in the
document exists.

## 2. How to check it

The reference tool is ripgrep (`rg`). On Windows without ripgrep, use
`Select-String`; the patterns are identical. The commands below are written
for ripgrep.

### SD-1 — parameterised SQL

```bash
# A statement must never be built with format! or push_str.
rg -n -A 12 'sqlx::query' src --glob '*.rs' | rg 'format!|push_str'
# A `+` inside a statement must be arithmetic on a literal, never string
# concatenation. Every hit must be read by a human.
rg -n -A 12 'sqlx::query' src --glob '*.rs' | rg '\+ '
```

The first command expects **no output**. The second expects only arithmetic on
a literal; today the only hit is the `+ 1000` in the task insert, which
appends a fixed step to the highest existing position. A `+` that joins a
value, a column or a user-supplied string is a finding.

A statement that takes a value must bind it:

```bash
rg -n -A 3 'sqlx::query' src --glob '*.rs' | rg '\.bind\('
```

A statement with no `.bind(` is acceptable only when it is a constant
statement with no placeholder — today that is the `SELECT 1` readiness probe
in [`../../src/health.rs`](../../src/health.rs).

### SD-2 — ownership predicates

```bash
# Every statement that reads or writes a private table must carry the owner
# predicate. The `users` table is excluded: it is the account itself, looked
# up by id or by normalized email, not by an owner. `api_keys` is included: it
# is the credential table and its rows belong to exactly one owner.
rg -n -A 15 'FROM notes|FROM projects|FROM tasks|FROM api_keys|UPDATE notes|UPDATE projects|UPDATE tasks|UPDATE api_keys|DELETE FROM notes|DELETE FROM projects|DELETE FROM api_keys' src --glob '*.rs' | rg 'user_id'
```

The window is deliberately wide, because the predicate is not always on the
line after the table name — the task update reaches it through
`FROM projects p`. The output must contain a `user_id` predicate for every
statement that touches a private table. The command is a screen, not a proof:
a statement without one is a finding, and a statement with one still has to
be read.

**The one expected exception** is `FIND_KEY_IDENTITY` in
`src/api_keys/repository.rs`: it selects the key row by `key_hash` and reaches
`user_id` through its join to `users`, because the request has not been
authenticated yet and no caller id exists at that point. It matches the screen
(a projected `k.user_id` appears in its window) and it is not a finding; every
other `api_keys` statement carries the owner in its own `WHERE`.

### SD-3 — `404`, not `403`

```bash
rg -n 'Forbidden' src --glob '*.rs'
```

Every hit must be one of: the `Forbidden` variant of the error type itself,
the disabled-account path (three sites in the authentication service), or the
cross-account session-revocation path in the logout handler. A new
`Forbidden` in a resource handler is a finding.

### SD-4 — no secret in an output

```bash
# No log statement may interpolate a secret.
rg -n '(tracing::)?(info|warn|error|debug)!' src --glob '*.rs' | rg -i 'password|secret|token|url|api_key|key_hash|prefix'
# The configuration types must redact.
rg -n 'REDACTED' src --glob '*.rs'
# No committed configuration file may carry a secret-shaped value.
rg -n -i '(jwt_secret|database_url|password|secret|token|api_key)\s*=\s*"[^"]{8,}"' config --glob '*.toml'
```

The first command expects **no output**. The second expects the redaction of
the `AuthSettings` signing secret and of the `DatabaseSettings` connection URL.
The third expects **no output**: the configuration directory is committed and
embedded in the binary, so a secret in a file there is a secret in the
published source. The two secrets have no file layer at all — they are the
`DATABASE_URL` and `APP_JWT_SECRET` environment variables.

On Windows without ripgrep, the third command is:

```powershell
Get-ChildItem -Recurse -File -Path config -Filter *.toml |
  Select-String -Pattern '(jwt_secret|database_url|password|secret|token|api_key)\s*=\s*"[^"]{8,}"' -CaseSensitive:$false
```

It must report no match.

### SD-5 and SD-6 — no `unwrap`, `expect` or `panic!` in a request path

```bash
rg -n '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!|unimplemented!' src --glob '*.rs'
```

Every hit must be inside a `#[cfg(test)]` module, in
[`../../src/telemetry.rs`](../../src/telemetry.rs) — the start-up of the
tracing subscriber — in the `main` start-up path, in a default-value helper in
[`../../src/config/settings.rs`](../../src/config/settings.rs) that parses a
constant, or in the development tool
[`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs). Nowhere else. A
hit in a handler, a service or a repository is a finding, and a hit in
[`../../src/bin/atom-seed.rs`](../../src/bin/atom-seed.rs) cannot survive the
build at all: that binary denies `clippy::unwrap_used` and
`clippy::expect_used` crate-wide, so the screen meets the same rule the compiler
enforces. There is no other tolerated hit: the CORS layer of
[`../../src/api/mod.rs`](../../src/api/mod.rs) contains no `panic!`, because
the allowlist it reads was refused at start-up and the layer falls back to an
empty header value for anything that still fails to parse.

### SD-13 and SD-14 — the proxy header and the CORS allowlist

```bash
rg -ni 'x-forwarded-for' src --glob '*.rs'
rg -n 'allow_credentials' src --glob '*.rs'
rg -n 'allow_origin' src --glob '*.rs'
```

The first expects only the explanatory comment. The second expects no output.
The third expects exactly the allowlist built from the configuration. The
refusal of a wildcard, and of an origin that is not a valid header value,
happens in [`../../src/config/validation.rs`](../../src/config/validation.rs)
and stops the process before the socket is bound; the router does not assert
it.

### SD-9 — migrations are append-only

```bash
git diff --name-status -- migrations/
```

Only `A` (added) entries are acceptable in a change that modifies the schema.
An `M` entry means an applied migration was edited.

### Secrets in the repository

The following must **never** be committed, and must never be read into a
transcript, a log or a report:

| Path | Why |
| ---- | --- |
| `.env`, and any `.env.*` other than `.env.example` | Environment values, including the database URL and the signing secret. |
| any file in [`../../config/`](../../config/) that holds a secret | The configuration directory is committed and embedded in the binary. A secret in a file there is a secret in the published source. `DATABASE_URL` and `APP_JWT_SECRET` have no file layer at all. |
| `*.pem`, `*.key`, `id_rsa*`, `id_ed25519*` | Private key material. |
| anything under `.ssh/` | Private keys and known-hosts data. |
| `credentials.json`, `secrets/` | Service credentials. |
| `~/.config/opencode/` | The user's OpenCode configuration, including provider credentials. |
| `~/.local/share/opencode/auth.json` | The stored provider credentials. |
| any log excerpt | Logs can contain paths, identifiers and, before a fix, secrets. |

`.env.example` holds placeholders only and is safe to commit. The same list
appears in [`../../AGENTS.md`](../../AGENTS.md) §8, so that an agent reads it
before it starts working. One entry differs in wording: the configuration
directory itself **is** committed — what must never be committed is a secret
inside it.

```bash
git check-ignore -v .env
rg -n --hidden 'BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY' . --glob '!target/**'
```

The first must report that `.env` is ignored. The second must report no
output. `.env.example` holds placeholders only and is safe to commit.

## 3. The tests a change must carry

| Change | Tests that must exist |
| ------ | ---------------------- |
| A new private endpoint | a success case, an unauthenticated case (`401`) and a cross-account isolation case (`404`) |
| A new authentication path | a generic-failure case, and a case that proves no distinction is made between an unknown account and a wrong credential |
| A new field on a request or a response | a case that exercises the new field, and a case that proves an omitted field is untouched on `PATCH` |
| A new validation rule | a unit test for the rule and its boundary value, and a `422` case at the HTTP level |
| A new query | a case that proves the ownership predicate, and a case that proves the resource of another account is invisible |
| A bug fix | a regression test that fails before the fix |
| A new error code | a case that asserts the code, and an entry in [`../api/error-contract.md`](../api/error-contract.md) |

## 4. Reviewing your own change

Before asking for a review, walk this list:

1. `cargo fmt --all -- --check` passes.
2. `cargo clippy --all-targets --all-features -- -D warnings` passes with no
   new suppression.
3. `cargo test --all-targets` passes, with `TEST_DATABASE_URL` pointing at a
   disposable database.
4. Every new statement is parameterised and every user-data query carries the
   owner predicate.
5. No `unwrap`, `expect` or `panic!` was added outside a test.
6. No secret was added to a log statement, an error message or a response.
7. The documentation was updated, or the reason it did not need to be is
   stated.

The independent version of this list is the code reviewer's job, and its
result is recorded in [`../engineering/review-log.md`](../engineering/review-log.md).

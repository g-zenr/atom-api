# ATOM — deployment

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `architect` agent

How the ATOM API is configured, started, observed and run in production —
and what is still missing before it can be.

---

## 1. Configuration

Every setting comes from the process environment. The configuration is read
**once**, at start-up, by `Config::from_env` in `src/config.rs`, validated,
and then handed to the rest of the application as a typed `Config` value. No
module reads the environment itself.

### 1.1 The reference

| Variable | Required | Default | Meaning and constraint |
| -------- | -------- | ------- | ---------------------- |
| `APP_ENV` | no | `development` | `development`, `dev`, `local`, `production` or `prod`. `production` enables the fail-fast checks on the placeholder secret. Any other value stops the process. |
| `APP_BIND_ADDR` | no | `127.0.0.1:3000` | The socket address to bind. Must be a valid `SocketAddr`. |
| `APP_CORS_ALLOWED_ORIGINS` | no | empty | Comma-separated list of allowed origins. Empty disables CORS. `*` is rejected. |
| `APP_MAX_BODY_BYTES` | no | `1048576` (1 MiB) | The body limit installed by `DefaultBodyLimit`. It is the effective ceiling on a request body. Must be positive. See §4. |
| `APP_REQUEST_TIMEOUT_SECONDS` | no | `30` | How long a request may take before it is abandoned with a `408`. Must be positive. |
| `APP_LOG_FILTER` | no | `info` | The `tracing` filter, for example `atom_api=debug,tower_http=info`. An unparsable value falls back to `info`. |
| `DATABASE_URL` | **yes** | — | The PostgreSQL connection string. Must not be empty. |
| `DATABASE_MAX_CONNECTIONS` | no | `10` | The size of the SQLx pool. |
| `DATABASE_ACQUIRE_TIMEOUT_SECS` | no | `5` | How long a statement waits for a pooled connection. |
| `APP_JWT_SECRET` | **yes** | — | The HMAC signing secret for the access tokens. At least **32 bytes**. The documented placeholder is refused when `APP_ENV` is `production`. |
| `APP_ACCESS_TOKEN_TTL_SECONDS` | no | `900` (15 min) | The access-token lifetime. Must be positive. |
| `APP_REFRESH_TOKEN_TTL_SECONDS` | no | `1209600` (14 days) | The refresh-token lifetime. Must be **longer** than the access-token lifetime. |
| `APP_JWT_ISSUER` | no | `atom-api` | The `iss` claim that is written into, and required from, an access token. |
| `APP_JWT_AUDIENCE` | no | `atom-api` | The `aud` claim that is written into, and required from, an access token. |
| `APP_ARGON2_MEMORY_KIB` | no | `19456` (19 MiB) | The Argon2id memory cost. Must be at least `8192` (8 MiB). |
| `APP_ARGON2_ITERATIONS` | no | `2` | The Argon2id iteration count. |
| `APP_ARGON2_PARALLELISM` | no | `1` | The Argon2id degree of parallelism. |
| `APP_AUTH_RATE_LIMIT_MAX_REQUESTS` | no | `10` | Authentication attempts allowed per client per window. |
| `APP_AUTH_RATE_LIMIT_WINDOW_SECONDS` | no | `60` | The rate-limit window. |
| `TEST_DATABASE_URL` | tests only | — | The disposable database used by the integration suite. The harness refuses to run unless the database name contains `test`. |

`.env.example` holds the same list with placeholder values and is the
committed template. `.env` is git-ignored and must never be committed.

### 1.2 The start-up contract

The process refuses to serve traffic in a partially configured state:

- a missing or empty required variable stops the process **before** it
  connects to the database;
- a short signing secret, a placeholder secret in production, a zero access
  token lifetime, a refresh lifetime that is not longer than the access
  lifetime, an Argon2 memory cost below 8 MiB, a zero body limit, a zero
  request timeout and an unknown `APP_ENV` all stop the process at the same
  point;
- a database that cannot be reached, or a migration that cannot be applied,
  stops the process before the socket is bound;
- an invalid CORS origin stops the process while the router is built.

## 2. Secrets

| Rule | Where it is enforced |
| ---- | -------------------- |
| The signing secret comes from the environment, never from a file in the repository. | `Config::from_env` |
| The signing secret is at least 32 bytes. | `validate_secret` in `src/config.rs` |
| The documented placeholder is refused in production. | `validate_secret` |
| The connection string never appears in a log line or a response. | The manual `Debug` implementation of `DatabaseConfig`. |
| The signing secret never appears in a log line or a response. | The manual `Debug` implementation of `AuthConfig`, and the `Debug` implementation of `TokenService`, which does not render the key at all. |
| `.env` is git-ignored; `.env.example` holds placeholders only. | `.gitignore` |
| No secret is ever returned to a client. | `AppError::body` in `src/error.rs` |

In production the variables are supplied by the platform's secret manager or
by the process environment. The application has **no** secret-manager client,
**no** key rotation and **no** way to reload a secret without a restart.

## 3. Local development

### 3.1 Prerequisites

- Rust stable with `rustfmt` and `clippy`. The toolchain is pinned by
  `rust-toolchain.toml`:
  `channel = "stable"`, `components = ["rustfmt", "clippy"]`.
- PostgreSQL 14 or newer, per the contributor guide, or Docker.

### 3.2 The database

`docker-compose.yml` brings up a **local PostgreSQL only**. It does **not**
build an application image: the API runs from the source tree.

```bash
docker compose up -d postgres     # start PostgreSQL 18 on 127.0.0.1:5432
docker compose ps
docker compose down                # stop, keep the data
docker compose down -v             # stop and delete the data
```

The compose file creates the `atom` database and, through
`scripts/create-test-database.sh`, the disposable `atom_test` database that
the integration suite needs. The credentials match `.env.example` and are for
local development only.

Without Docker:

```bash
createdb atom
createdb atom_test
```

### 3.3 The configuration and the run

```bash
cp .env.example .env          # Linux/macOS
Copy-Item .env.example .env   # Windows PowerShell
```

Replace at least `APP_JWT_SECRET` with a real value of at least 32 bytes.
`openssl rand -base64 48` produces one.

```bash
cargo run
```

The API listens on `http://127.0.0.1:3000` by default and applies the pending
migrations at start-up. A background task then prunes the refresh sessions
that expired more than a day ago, every hour.

```bash
curl http://127.0.0.1:3000/health/live
# {"status":"live"}

curl http://127.0.0.1:3000/health/ready
# {"status":"ready","checks":{"database":"ok"}}
```

### 3.4 The quality gates

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test cargo test --all-targets
```

`make check` runs all three. The integration harness refuses to start unless
the database name contains `test`, so it cannot damage a real database.

## 4. A note on the body limit

`APP_MAX_BODY_BYTES` is installed as an Axum `DefaultBodyLimit`, and the
project's own `api::extract::Json` reads that limit through
`req.into_limited_body()`. The configured value is therefore **the effective
cap on a request body**; a body over it is refused with a `413` and the
standard error envelope.

The extractor additionally passes a constant of 4 MiB to
`axum::body::to_bytes`. It is the backstop for a request that reaches the
extractor without the layer installed, and it also bounds a configured limit
that is larger than 4 MiB. With the default configuration it is never the
binding limit.

## 5. The production checklist

The API does not enforce most of these itself. They are operator obligations.

| # | Obligation | Why |
| - | ---------- | --- |
| 1 | **Terminate TLS in front of the API.** | The process speaks plain HTTP/1.1 and does not verify TLS. |
| 2 | **Run behind a trusted reverse proxy or load balancer.** | The API does not declare which proxies it trusts and ignores `X-Forwarded-For` entirely. |
| 3 | **Put edge rate limiting in front of the API.** | The in-process limiter is per process, and behind a proxy it is keyed by the proxy's address. |
| 4 | **Supply the signing secret from a secret manager.** | At least 32 bytes, and never the documented placeholder. |
| 5 | **Use a distinct database role with the least privilege it needs.** | The role must be able to create and alter objects, because the binary applies the migrations at start-up. |
| 6 | **Ensure the migrations are applied before the new code serves traffic.** | The binary applies them itself; with more than one instance, the first one to start applies them and the others wait. Do not run a rolling deployment against a schema the old binary cannot read. |
| 7 | **Back up the database, and test the restore.** | The data is the product. |
| 8 | **Ship the logs somewhere durable.** | The JSON formatter is the production format. |
| 9 | **Set `APP_ENV=production`.** | It enables the placeholder-secret check. |
| 10 | **Set `APP_CORS_ALLOWED_ORIGINS` to the real origins, or leave it empty.** | A `*` entry aborts the process. |
| 11 | **Keep `APP_LOG_FILTER` at `info` or higher in production.** | A `debug` filter logs every rejected request, including the reason. |
| 12 | **Raise the Argon2 cost for the deployment.** | The defaults are a floor, not a tuned value for the target hardware. |

### 5.1 The release process

**There is no release process yet.** There is no CI pipeline, no container
image, no versioning of the artifact and no deployment manifest. The
migration policy in [`../database/migration-policy.md`](../database/migration-policy.md)
§5 describes the intended rollback rule — forward-fix, never edit — and it is
the constraint a future release process must honour.

## 6. Observability

### 6.1 Structured logs

`src/telemetry.rs` installs the tracing subscriber at start-up:

| Build | Formatter |
| ----- | --------- |
| Debug (`cargo run`) | Human-readable, with the target and the level. |
| Release (`cargo build --release`) | JSON, with a flattened event. |

The filter comes from `APP_LOG_FILTER`. An invalid filter falls back to
`info` rather than failing the start-up.

What the API logs:

| Event | Level | Content |
| ----- | ----- | ------- |
| Start-up | `info` | The bind address and the environment. |
| Migrations applied | `info` | A fixed message. |
| Listening | `info` | The bind address. |
| Shut-down | `info` | The signal that was received. |
| A request | `debug` | The `TraceLayer` span and event. At the default `info` filter the trace layer is silent. |
| A rejected request | `debug` | The `AppError` and the status code. |
| A server error | `error` | The `AppError`. A database failure is logged with its SQLSTATE and the error kind only, never with the driver message. |
| A panic | `error` | A fixed message. The payload is deliberately not logged. |
| Expired sessions pruned | `info` | The number of removed rows. |
| Pruning failed | `warn` | The SQLSTATE only. |

What the API never writes to a log: a password, a token, the signing secret
or the connection URL. `DatabaseConfig` and `AuthConfig` redact the two
values that a configuration dump would otherwise carry, the password is never
stored in the clear and never logged, and the panic handler deliberately
drops the panic payload.

### 6.2 The correlation id

Every request carries an `x-request-id` header, and every response echoes
it. A caller may supply its own value; the API keeps it and generates a
UUIDv4 only when the request has none. The full flow is in
[`request-lifecycle.md`](request-lifecycle.md) §8.

The limitation to know about: the id is carried by the header and the
tracing span. It is **not** currently written as an explicit field on every
log line, so correlating a log line with a request means relying on the
tracing span context.

### 6.3 The health endpoints

| Endpoint | Meaning | Failure |
| -------- | ------- | ------- |
| `GET /health/live` | The process can serve requests. | Never fails while the process answers. |
| `GET /health/ready` | The process can serve requests **and** the database answered a `SELECT 1`. | `503` with `{"status":"degraded","checks":{"database":"unavailable"}}`. |

Both are outside `/api/v1` and outside the authentication boundary, so an
orchestrator can probe them without credentials. The readiness probe is
bounded by a one-second timeout, so a stalled database cannot hang it. The
liveness probe does not touch the database, so a database outage does not
cause an orchestrator to restart the process.

## 7. Scaling characteristics

| Dimension | Behaviour today |
| --------- | --------------- |
| Horizontal | The API is stateless apart from the in-process rate limiter, so several instances can serve the same database. The rate limiter's budget is then multiplied by the number of instances. |
| Vertical | One Tokio task per connection. The pool size (`DATABASE_MAX_CONNECTIONS`, 10 by default) bounds the concurrency against the database. |
| The database | The pool is the bottleneck. `DATABASE_ACQUIRE_TIMEOUT_SECS` bounds how long a request waits for a connection; a request that cannot get one fails with `database_error`. |
| CPU | The dominant cost is Argon2id, which is deliberately expensive. It is bounded by the password length cap and by the rate limit. |
| Memory | The connection pool, the in-flight bodies (capped at `APP_MAX_BODY_BYTES` each) and the rate limiter's map, which is pruned on every check. |

**The rate limiter is the one component that does not scale horizontally.**
It is per process, in memory, and its key is the peer address of the
connection. Behind a proxy that does not terminate the connection, every
request carries the proxy's address and the budget is shared by every
caller. A multi-instance deployment therefore needs edge rate limiting. Both
are recorded in [`../engineering/known-issues.md`](../engineering/known-issues.md)
KI-001 and KI-003.

## 8. What is **not** implemented yet

| Thing | State | Consequence |
| ----- | ----- | ----------- |
| A container image for the API | Not implemented | The service is built and run from the source tree. `docker-compose.yml` is for the database only. |
| A CI pipeline | Not implemented | The quality gates are run by hand. |
| A release process, an artifact version and a deployment manifest | Not implemented | There is no supported way to roll the service out. |
| A secret-manager client and a key-rotation procedure | Not implemented | The signing secret is fixed for the lifetime of the process. |
| An edge rate limiter or a shared limiter | Not implemented | The in-process limiter is the only control, and it is per process. |
| TLS termination in the process | Not implemented | A proxy is required. |
| Metrics, tracing spans exported to a collector | Not implemented | Only logs are emitted. |
| A database backup and restore procedure | Not implemented | The checklist in §5 requires one. |

## 9. Related documents

- [`system-overview.md`](system-overview.md) — the runtime shape.
- [`security-architecture.md`](security-architecture.md) — the operator's
  security obligations.
- [`request-lifecycle.md`](request-lifecycle.md) — what happens to a request.
- [`../database/migration-policy.md`](../database/migration-policy.md) — how
  the schema is changed and released.
- [`../engineering/known-issues.md`](../engineering/known-issues.md) — the
  confirmed limitations.
- [`../../README.md`](../../README.md) — the quick start.
- [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) — the contributor guide.

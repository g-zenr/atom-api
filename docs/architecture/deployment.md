# ATOM — deployment

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `architect` agent

How the ATOM API is configured, started, observed and run in production —
and what is still missing before it can be.

---

## 1. Configuration

The configuration is resolved **once**, at start-up, by the `config` module in
`src/config/`, and then handed to the rest of the application as a typed
`Config` value. No module reads the environment or a configuration file
itself.

The configuration reference for the files themselves is
[`../../config/README.md`](../../config/README.md). This section is the
operator's contract: what a deployment may change, how, and what stops the
process.

### 1.1 The layers and the precedence

Three layers, later wins over earlier, key by key:

```text
1. the base files in config/          app, api, auth, database, messages
2. the environment overlay            config/environments/<APP_ENV>.toml
3. the environment variables          the names in .env.example
```

A value that is **not** set in a layer keeps the value from the layer below
it. There is no hidden fourth layer: a setting that is in no file and in no
environment variable uses the default compiled into `src/config/settings.rs`.

The overlay is selected by `APP_ENV`, which accepts `development` (`dev`,
`local`) or `production` (`prod`) and defaults to `development`. Any other
value stops the process.

The two secrets have **no file layer at all**: `DATABASE_URL` and
`APP_JWT_SECRET` are required environment variables, and no file in `config/`
may contain a secret. See §2.

### 1.2 Where the files come from

The files are **embedded in the binary**, so a deployment needs no
configuration directory. The loader looks for a directory in this order:

1. the path in `ATOM_CONFIG_DIR`, if it is set;
2. a `config` directory next to the executable;
3. a `config` directory in the working directory.

A file found on disk is merged **on top of** the embedded copy, key by key.
An operator who wants to change one setting can therefore write a file that
contains only that setting. A file that is not on disk leaves the embedded
copy in place.

`ATOM_CONFIG_DIR` that points at a directory which does not exist is an
error: the operator asked for it explicitly, so a silent fallback would hide
the mistake.

### 1.3 The settings

The tables below list every setting. "Environment variable" is the name that
overrides the file value; a setting with no variable can only be changed in a
file.

#### `app.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `app.name` | `ATOM API` | — | The product name, used in the OpenAPI document and the start-up log. |
| `app.bind_addr` | `127.0.0.1:3000` | `APP_BIND_ADDR` | Must be a valid socket address. |
| `app.log_filter` | `info` | `APP_LOG_FILTER` | Must parse as a `tracing` filter. An unparsable filter is a start-up error, not a silent fallback to `info`. |
| `app.limits.max_body_bytes` | `1048576` (1 MiB) | `APP_MAX_BODY_BYTES` | Must be positive. It is the effective ceiling on a request body; see §4. |
| `app.limits.request_timeout_seconds` | `30` | `APP_REQUEST_TIMEOUT_SECONDS` | Must be positive. A request that exceeds it is abandoned with a `408`. |
| `app.limits.readiness_probe_timeout_seconds` | `1` | `APP_READINESS_PROBE_TIMEOUT_SECONDS` | Must be positive. It bounds the readiness probe. |
| `app.sessions.prune_interval_seconds` | `3600` | `APP_SESSION_PRUNE_INTERVAL_SECONDS` | Must be positive. |
| `app.sessions.prune_grace_seconds` | `86400` | `APP_SESSION_PRUNE_GRACE_SECONDS` | Must be positive. This is a **security window**, not a tuning knob: a revoked session has to survive long enough that a replayed token is still recognised as reuse. |

#### `api.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `api.title` | `ATOM API` | — | The title of the OpenAPI document. |
| `api.docs_enabled` | `true` | `APP_API_DOCS_ENABLED` | A boolean: `true`, `false`, `1`, `0`, `yes`, `no`, `on` or `off`. The production overlay turns the document and the explorer off. |
| `api.cors.allowed_origins` | `[]` | `APP_CORS_ALLOWED_ORIGINS` | A comma-separated list. Empty disables CORS. A wildcard is refused, and every entry must be a valid header value. |
| `api.pagination.default_limit` | `20` | — | Must be positive. **Part of the published client convention**: changing it changes what a client receives. |
| `api.pagination.max_limit` | `100` | — | Must be at least the default limit. Also part of the published client convention. |
| `api.rate_limit.max_requests` | `10` | `APP_AUTH_RATE_LIMIT_MAX_REQUESTS` | Must be positive. |
| `api.rate_limit.window_seconds` | `60` | `APP_AUTH_RATE_LIMIT_WINDOW_SECONDS` | Must be positive. |

#### `auth.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `auth.access_token_ttl_seconds` | `900` (15 min) | `APP_ACCESS_TOKEN_TTL_SECONDS` | Must be positive. |
| `auth.refresh_token_ttl_seconds` | `1209600` (14 days) | `APP_REFRESH_TOKEN_TTL_SECONDS` | Must be **longer** than the access-token lifetime. |
| `auth.jwt_issuer` | `atom-api` | `APP_JWT_ISSUER` | Must not be empty. It is the `iss` claim, written into every token and required back from it. |
| `auth.jwt_audience` | `atom-api` | `APP_JWT_AUDIENCE` | Must not be empty. It is the `aud` claim. |
| `auth.clock_skew_seconds` | `30` | `APP_ACCESS_TOKEN_CLOCK_SKEW_SECONDS` | The clock-skew allowance applied when validating an access token. A deployment whose hosts drift by more than this sees unexplained `401` responses. |
| `auth.argon2.memory_kib` | `19456` (19 MiB) | `APP_ARGON2_MEMORY_KIB` | Must be at least `8192` (8 MiB), which is a security floor rather than a setting. |
| `auth.argon2.iterations` | `2` | `APP_ARGON2_ITERATIONS` | Must be at least one. |
| `auth.argon2.parallelism` | `1` | `APP_ARGON2_PARALLELISM` | Must be at least one. |
| `auth.password.min_length` | `12` | — | Must be positive, and not above the maximum. |
| `auth.password.max_length` | `256` | — | Must be at least the minimum. The maximum bounds the cost of hashing a very long password. |

#### `database.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `database.max_connections` | `10` | `DATABASE_MAX_CONNECTIONS` | Must be positive. |
| `database.acquire_timeout_seconds` | `5` | `DATABASE_ACQUIRE_TIMEOUT_SECS` | Must not be zero: a zero value would mean "wait forever". |
| `database.connect_timeout_seconds` | `10` | `DATABASE_CONNECT_TIMEOUT_SECS` | Must not be zero. Without a bound, an unreachable database hangs the process before it binds a port. |

#### `messages.toml`

`messages.toml` holds the client-facing message catalogue, not a setting.
Every string that reaches a client through the error envelope comes from it,
resolved at the call site. The **stable error code** is not in the file: the
code is part of the API contract and lives in the `code` module in
`src/error.rs`, so that a client can switch on it without the text ever being
able to change it.

#### The environment overlays

`config/environments/development.toml` — applied when `APP_ENV` is
`development`, `dev` or `local`:

| Key | Value |
| --- | ----- |
| `app.log_filter` | `atom_api=debug,tower_http=debug,sqlx=warn` |
| `app.limits.request_timeout_seconds` | `60` |

`config/environments/production.toml` — applied when `APP_ENV` is `production`
or `prod`:

| Key | Value |
| --- | ----- |
| `app.log_filter` | `info,atom_api=info` |
| `app.limits.request_timeout_seconds` | `30` |
| `app.sessions.prune_interval_seconds` | `900` |
| `api.docs_enabled` | `false` |
| `api.pagination.max_limit` | `100` |
| `database.max_connections` | `20` |

#### The variables that are not settings

| Variable | Meaning |
| -------- | ------- |
| `APP_ENV` | Selects the environment overlay: `development` (`dev`, `local`) or `production` (`prod`). It defaults to `development`. Any other value stops the process. |
| `ATOM_CONFIG_DIR` | The directory the configuration files are read from. It must be an existing directory. |
| `DATABASE_URL` | **Required.** The PostgreSQL connection string. It has no file layer. |
| `APP_JWT_SECRET` | **Required.** The HMAC signing secret for the access tokens. It has no file layer. At least **32 bytes**; the documented placeholder is refused when `APP_ENV` is `production`. |
| `TEST_DATABASE_URL` | The disposable database used by the integration suite. The harness refuses to run unless the database name contains `test`. |

`.env.example` holds the same variables with placeholder values and is the
committed template. `.env` is git-ignored and must never be committed.

### 1.4 The start-up contract

The process refuses to serve traffic in a partially configured state. Every
rule below stops it **before** the socket is bound.

| Rule | Where it is enforced |
| ---- | -------------------- |
| `DATABASE_URL` and `APP_JWT_SECRET` are present and not empty. | `required` in `src/config/loader.rs`; `ConfigError::Missing`. |
| `APP_ENV` names a known environment. | `Environment::parse` in `src/config/settings.rs`; `ConfigError::Invalid`. |
| `ATOM_CONFIG_DIR` points at an existing directory. | `discover_directory` in `src/config/loader.rs`; `ConfigError::Invalid`. |
| Every configuration file can be read and parsed. | `src/config/loader.rs`; `ConfigError::Io` and `ConfigError::Parse`. An unknown key is a parse error, because every settings structure uses `deny_unknown_fields`. |
| The signing secret is at least 32 bytes, and the documented placeholder is refused in production. | `validate_secret` in `src/config/validation.rs`. |
| Every duration is positive: the request timeout, the readiness timeout, the prune interval, the prune grace and the body limit. | `validate_settings` in `src/config/validation.rs`. |
| The log filter parses. | `validate_settings` in `src/config/validation.rs`. |
| The pagination default is positive and the maximum is at least the default. | `validate_settings` in `src/config/validation.rs`. |
| The rate limit is positive and its window is not zero. | `validate_settings` in `src/config/validation.rs`. |
| The CORS allowlist contains no wildcard, and every origin is a valid header value. | `validate_cors` in `src/config/validation.rs`. This used to be an assertion in the router. |
| The pool size is positive, and neither the acquire nor the connect timeout is zero. | `validate_settings` in `src/config/validation.rs`. |
| The access-token lifetime is positive and the refresh-token lifetime is strictly longer. | `validate_settings` in `src/config/validation.rs`. |
| The issuer and the audience are not empty. | `validate_settings` in `src/config/validation.rs`. |
| The Argon2 memory is at least 8 MiB, and the iteration count and the degree of parallelism are at least one. | `validate_settings` in `src/config/validation.rs`. |
| The password policy has a positive minimum and a maximum that is at least the minimum. | `validate_settings` in `src/config/validation.rs`. |
| The database can be reached, and the pending migrations can be applied. | `src/main.rs`, before the listener is bound. |

## 2. Secrets

| Rule | Where it is enforced |
| ---- | -------------------- |
| The signing secret comes from the environment and has **no file layer**. | `required("APP_JWT_SECRET")` in `src/config/loader.rs`. |
| The database URL comes from the environment and has **no file layer**. | `required("DATABASE_URL")` in `src/config/loader.rs`. |
| **No file in `config/` may contain a secret.** | The directory is committed and embedded in the binary. A setting that needs a credential is an environment variable; the two secrets above are the only ones. |
| The signing secret is at least 32 bytes. | `validate_secret` in `src/config/validation.rs`. |
| The documented placeholder is refused in production. | `validate_secret` in `src/config/validation.rs`. |
| The connection string never appears in a log line or a response. | The manual `Debug` implementation of `DatabaseSettings`. |
| The signing secret never appears in a log line or a response. | The manual `Debug` implementation of `AuthSettings`, and the `Debug` implementation of `TokenService`, which does not render the key at all. |
| `.env` is git-ignored; `.env.example` holds placeholders only. | `.gitignore` |
| No secret is ever returned to a client. | `AppError::body` in `src/error.rs` |

A rejected secret is reported by its **length**, never by its content.

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
that expired more than a day ago, every hour — both the interval and the
grace period come from the configuration.

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

There is no second, independent cap. The extractor passes `usize::MAX` to
`axum::body::to_bytes`, so the layer the router installs is the only limit:
a configured limit is honoured in full, however large it is.

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
| 9 | **Set `APP_ENV=production`.** | It selects `config/environments/production.toml` and enables the placeholder-secret check. |
| 10 | **Set `APP_CORS_ALLOWED_ORIGINS` to the real origins, or leave it empty.** | A `*` entry aborts the process. |
| 11 | **Keep `APP_LOG_FILTER` at `info` or higher in production.** | A `debug` filter logs every rejected request, including the reason. |
| 12 | **Raise the Argon2 cost for the deployment.** | The defaults are a floor, not a tuned value for the target hardware. |
| 13 | **Decide whether the API explorer is served.** | `config/environments/production.toml` sets `api.docs_enabled` to `false`. Set `APP_API_DOCS_ENABLED=true` only if the deployment wants the OpenAPI document and the explorer. |
| 14 | **Size the pool, the timeouts and the body limit for the deployment.** | The production overlay sets `database.max_connections` to 20 and the request timeout to 30 s. The base files hold the body limit (1 MiB) and the acquire and connect timeouts. |
| 15 | **Keep the session prune grace at its default, or raise it — never shorten it below what reuse detection needs.** | `app.sessions.prune_grace_seconds` is a security window, not a tuning knob. |
| 16 | **Set the clock-skew allowance to what the hosts actually need.** | `APP_ACCESS_TOKEN_CLOCK_SKEW_SECONDS` is 30 s by default. Hosts that drift by more than that see unexplained `401` responses. |

`config/environments/production.toml` exists and is applied whenever
`APP_ENV=production`. It changes six settings: the log filter, the request
timeout, the session prune interval, `api.docs_enabled`, `api.pagination.max_limit`
and `database.max_connections`. Every one of them is still overridable by an
environment variable, because the environment variables are the last layer.

### 5.1 The release process

**There is no release process yet.** The quality gates run in CI
(`.github/workflows/ci.yml`) on every push to `main` and on every pull
request, but there is no container image, no versioning of the artifact and no
deployment manifest, so a rollout is still a manual step. The migration policy
in [`../database/migration-policy.md`](../database/migration-policy.md) §5
describes the intended rollback rule — forward-fix, never edit — and it is the
constraint a future release process must honour.

## 6. Observability

### 6.1 Structured logs

`src/telemetry.rs` installs the tracing subscriber at start-up:

| Build | Formatter |
| ----- | --------- |
| Debug (`cargo run`) | Human-readable, with the target and the level. |
| Release (`cargo build --release`) | JSON, with a flattened event. |

The filter comes from the configuration: `app.log_filter`, which the
environment variable `APP_LOG_FILTER` overrides and the environment overlay
can replace. An invalid filter is a start-up error, not a silent fallback to
`info`.

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
or the connection URL. `DatabaseSettings` and `AuthSettings` redact the two
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
bounded by `app.limits.readiness_probe_timeout_seconds` — one second by
default — so a stalled database cannot hang it. The liveness probe does not
touch the database, so a database outage does not cause an orchestrator to
restart the process.

## 7. Scaling characteristics

| Dimension | Behaviour today |
| --------- | --------------- |
| Horizontal | The API is stateless apart from the in-process rate limiter, so several instances can serve the same database. The rate limiter's budget is then multiplied by the number of instances. |
| Vertical | One Tokio task per connection. The pool size (`database.max_connections`, 10 by default and 20 in the production overlay) bounds the concurrency against the database. |
| The database | The pool is the bottleneck. `database.acquire_timeout_seconds` bounds how long a request waits for a connection; a request that cannot get one fails with `database_error`. |
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
| A release process, an artifact version and a deployment manifest | Not implemented | There is no supported way to roll the service out. The quality gates run in CI; the rollout is manual. |
| A runtime configuration reload | Not implemented | The configuration is resolved once, at start-up. A changed file or variable needs a restart. |
| A secret-manager client and a key-rotation procedure | Not implemented | The signing secret is fixed for the lifetime of the process. |
| An edge rate limiter or a shared limiter | Not implemented | The in-process limiter is the only control, and it is per process. |
| TLS termination in the process | Not implemented | A proxy is required. |
| Metrics, tracing spans exported to a collector | Not implemented | Only logs are emitted. |
| A database backup and restore procedure | Not implemented | The checklist in §5 requires one. |

## 9. Related documents

- [`../../config/README.md`](../../config/README.md) — the configuration
  reference: the layers, the files, the catalogue and the validation rules.
- [`system-overview.md`](system-overview.md) — the runtime shape.
- [`security-architecture.md`](security-architecture.md) — the operator's
  security obligations.
- [`request-lifecycle.md`](request-lifecycle.md) — what happens to a request.
- [`../database/migration-policy.md`](../database/migration-policy.md) — how
  the schema is changed and released.
- [`../engineering/known-issues.md`](../engineering/known-issues.md) — the
  confirmed limitations.
- [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md)
  — the decision behind the three layers.
- [`../../README.md`](../../README.md) — the quick start.
- [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) — the contributor guide.

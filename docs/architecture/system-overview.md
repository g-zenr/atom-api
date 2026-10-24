# ATOM — system overview

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `architect` agent

The ATOM Phase 1 backend in one page: what it is, how it runs, what it is
made of, how it is deployed and what it cannot do yet.

The module-by-module detail is in
[`module-boundaries.md`](module-boundaries.md). The product scope is in
[`../project-truth.md`](../project-truth.md); it is not repeated here.

---

## 1. What the system is

ATOM is an **API-first personal workspace platform**. The Phase 1 deliverable
in this repository is its **backend**: a single HTTP API over PostgreSQL that
provides

- registration, login, a rotating token pair and session revocation;
- personal notes with archiving, full-text search and pagination;
- projects and the tasks inside them, with status, priority and manual
  ordering;
- a versioned, documented REST surface at `/api/v1`, with an OpenAPI document
  and a local API explorer;
- liveness and readiness probes, structured logs and a request correlation id.

There is **no frontend, no worker process and no outbound integration** in
Phase 1. The only background activity is the in-process task that prunes the
expired refresh sessions. The process makes no outbound network call at run
time: every byte it sends is a response to a request it received.

**Phase 2 is mostly planned and changes none of that.** It adds three modules —
`reimbursements`, `job-applications` and `ml` — inside the same binary: no new
process, no new service, no worker, no message broker and no second language
runtime. The one addition that could have introduced a service, machine
learning, runs **in this process**, on the CPU, from a local model file, and
makes no outbound call to a hosted service at build time or at run time.
Of Phase 2, `reimbursements` is `Implemented`; `job-applications` and `ml`
remain `Accepted` and not built. The module map is in
[`module-boundaries.md`](module-boundaries.md), and the data it adds is in
[`data-architecture.md`](data-architecture.md).

## 2. Runtime shape

### 2.1 Start-up

`src/main.rs` runs a fixed sequence. Each step can only be reached if the
previous one succeeded, so the process never serves traffic in a partially
configured state.

| # | Step | Code | Failure behaviour |
| - | ---- | ---- | ----------------- |
| 1 | Load `.env` | `dotenvy::dotenv().ok()` | A missing file is not an error. In production the variables come from the real environment. |
| 2 | Load and validate the configuration | `Config::load()` in `src/config/mod.rs` | The process exits with a non-zero status **before** it touches the database. The configuration is resolved from the base files in `config/`, the environment overlay selected by `APP_ENV`, and the environment variables. |
| 3 | Install the tracing subscriber | `telemetry::init(&config.app.log_filter)` | — |
| 4 | Open the connection pool | `PgPoolOptions::new()…connect()` | The process exits. |
| 5 | Apply the pending migrations | `sqlx::migrate!("./migrations").run(&pool)` | The process exits; it never serves against a half-migrated schema. |
| 6 | Build the shared state | `AppState::new(config, pool)` | The process exits if the password hasher cannot be built. |
| 7 | Spawn the session-pruning task | `tokio::spawn(prune_expired_sessions(state.clone()))` | — |
| 8 | Bind the socket | `TcpListener::bind(config.app.bind_addr)` | The process exits. |
| 9 | Serve | `axum::serve(listener, api::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>())` | — |

The migrations are **embedded in the binary** by `sqlx::migrate!`, so a
deployed instance never depends on files that sit next to it. The
configuration files are embedded the same way, so a deployment needs no
configuration directory either; a directory found on disk is merged on top of
the embedded copy. The operator's reference is
[`deployment.md`](deployment.md) §1.

The `into_make_service_with_connect_info` wrapper in the last step is not
decoration. It is what puts the peer address of the connection into the
request extensions, and the `ClientIp` extractor reads it from there. The
authentication rate limiter is keyed by that address, so without the wrapper
every request shares **one** global rate-limit bucket instead of one bucket
per client. The call is shown in full because the difference is invisible in
the router and decisive in the limiter.

### 2.2 Request handling

One Tokio task per connection. Each request passes through the middleware
stack described in [`request-lifecycle.md`](request-lifecycle.md), reaches a
handler, and the handler runs `handler → service → repository → PostgreSQL`.
The database is never queried synchronously. The one CPU-bound step,
Argon2id hashing, runs on the blocking pool, so it never stalls a request
worker, and it is bounded by the password-length cap and the rate limit.

### 2.3 Shut-down

`shutdown_signal()` resolves on Ctrl-C, or on `SIGTERM` on Unix. The server is
then stopped with `axum::serve(…).with_graceful_shutdown(…)`. The signal
future resolves immediately, so the listener stops accepting at once and the
server drains; `app.limits.shutdown_grace_seconds` (ten seconds by default)
bounds that drain, and a request still in flight when it elapses is abandoned
with a warning in the log.
On Windows there is no `SIGTERM` handler; the Ctrl-C path is the only one.

### 2.4 Processes

There is exactly **one** long-lived process in Phase 1. The second binary in
the crate, `atom-docs` (`src/bin/atom-docs.rs`), is a development tool that
checks the documentation tree and the configuration surface; it is not part of
the service.

## 3. Components and responsibilities

| Component | Source | Responsibility |
| --------- | ------ | -------------- |
| `api` | `src/api/` | The router, the middleware stack, the shared extractors, the route table, the OpenAPI document and the API explorer. |
| `auth` | `src/auth/` | Registration, login, the access/refresh token lifecycle, session rotation, reuse detection and revocation, and Argon2id password hashing. |
| `users` | `src/users/` | The user record, email normalization, the password policy, the display-name policy and the safe public representation of a user. |
| `notes` | `src/notes/` | Notes: create, list, read, partially update, delete, archive and search. |
| `projects` | `src/projects/` | Projects: create, list, read, partially update, delete. |
| `tasks` | `src/tasks/` | Tasks inside a project: create, list, read, partially update, delete, with status, priority and ordering. |
| `config` | `src/config/` | Resolves the configuration once, at start-up, from the base files, the environment overlay and the environment variables; validates it; and owns the client-facing message catalogue. |
| `config` (the files) | `config/` | The committed, non-secret configuration: `app.toml`, `api.toml`, `auth.toml`, `database.toml`, `messages.toml` and the `environments/` overlays. The files are embedded in the binary; a directory found on disk is merged on top of them. |
| `error` | `src/error.rs` | The single `AppError` type, its stable code module, its HTTP status mapping and the public error envelope. |
| `state` | `src/state.rs` | `AppState`: the shared, cheaply cloneable dependencies passed explicitly to handlers. |
| `pagination` | `src/pagination.rs` | The shared `limit`/`offset` convention and the list envelope. |
| `ratelimit` | `src/ratelimit.rs` | The in-process, fixed-window limiter used by the authentication endpoints. |
| `telemetry` | `src/telemetry.rs` | Installs the tracing subscriber: JSON when `APP_ENV` is `production`, human-readable otherwise. |
| `health` | `src/health.rs` | `GET /health/live` and `GET /health/ready`. |
| `atom-docs` | `src/bin/atom-docs.rs` | The documentation consistency checker. A development tool, not a service component. |

## 4. Deployment topology

```text
        client (browser, mobile, CLI — none built yet)
                          |
                   TLS termination          ← the operator's reverse proxy
                          |
        ┌─────────────────┴──────────────────┐
        │            atom-api                │   one process, one binary
        │  Axum router → services → SQLx     │
        └─────────────────┬──────────────────┘
                          │
                    PostgreSQL                ← one database, one role
```

- The API is plain HTTP/1.1 with JSON bodies. It does **not** terminate or
  verify TLS; that is the operator's obligation, and it is listed in the
  production checklist in [`deployment.md`](deployment.md).
- The API and the database are assumed to be on a private network. The
  database is not exposed to the internet.
- There is no message broker, no cache, no object store and no second service.
  That is a deliberate decision, recorded in
  [`../decisions/ADR-0001-modular-monolith.md`](../decisions/ADR-0001-modular-monolith.md).

### 4.1 What exists today

| Piece | State |
| ----- | ----- |
| The binary (`cargo run`, `cargo build --release`) | Implemented |
| `docker-compose.yml` for a **local PostgreSQL only** | Implemented. It brings up `postgres:18-alpine` and creates the development `atom_dev` database and the disposable `atom_test` one. It does **not** build an application image. |
| A background task that prunes the expired refresh sessions | Implemented. It runs every hour by default and deletes the sessions that expired more than a day ago; both the interval and the grace period come from the configuration. |
| A container image for the API | **Not implemented.** |
| A CI pipeline | Implemented. `.github/workflows/ci.yml` runs the formatting check, the linter with warnings denied, the whole test suite against a real PostgreSQL, the release build and the documentation check, on every push and pull request to `master` or `develop`. It has been observed passing on GitHub-hosted runners; `KI-005` records the runs. |
| A release process | **Not implemented.** |
| An orchestrator manifest (systemd unit, Kubernetes manifest) | **Not implemented.** |

## 5. Technology stack

| Concern | Choice | Version |
| ------- | ------ | ------- |
| Language | Rust, edition 2024, `rust-version = "1.85"` | stable, pinned by `rust-toolchain.toml` with the `rustfmt` and `clippy` components |
| HTTP framework | `axum` | 0.8 |
| HTTP middleware | `tower-http` | 0.6 |
| Async runtime | `tokio` | 1 |
| Database driver | `sqlx` (`runtime-tokio`, `tls-rustls`, `postgres`, `macros`, `migrate`, `uuid`, `chrono`) | 0.8 |
| Database | PostgreSQL | 14 or newer, per the contributor guide; the local compose file runs `postgres:18-alpine` |
| Serialization | `serde`, `serde_json` | 1 |
| Configuration files | `toml` | 1 |
| OpenAPI | `utoipa`, `utoipa-swagger-ui` (vendored assets) | 5, 9 |
| Password hashing | `argon2` (Argon2id) | 0.5 |
| Tokens | `jsonwebtoken` (HS256) | 9 |
| Hashing | `sha2` | 0.10 |
| Randomness | `rand` | 0.9 |
| Identifiers and time | `uuid` (v4), `chrono` | 1, 0.4 |
| Errors | `thiserror` | 2 |
| Observability | `tracing`, `tracing-subscriber` | 0.1, 0.3 |
| Configuration | `dotenvy` | 0.15 |

The table lists the **direct** dependencies, which are the crates the
manifest declares. `tower` and `http-body-util` are dev-dependencies: the
integration harness uses them to drive the router without binding a socket.
The `subtle` and `hex` crates are not dependencies of the crate at all; `hex`
was removed in the audit remediation and the refresh-token hash is now
formatted with `format!("{hasher:x}")`.

The crate is `atom-api`; the library is `atom_api`; the binaries are
`atom-api` and `atom-docs`. The crate is marked `publish = false`, and the
repository contains no `LICENSE` file.

The root `config/` directory is a **first-class part of the repository**, not
an example: it holds the committed, non-secret settings, the environment
overlays and the client-facing message catalogue. Its files are embedded in
the binary, so a deployment needs no configuration directory, and a directory
found on disk is merged on top of the embedded copy. The reference is
[`../../config/README.md`](../../config/README.md) and the operator's contract
is [`deployment.md`](deployment.md) §1.

The reasoning behind the stack is recorded in
[`../decisions/ADR-0002-rust-axum-postgresql.md`](../decisions/ADR-0002-rust-axum-postgresql.md).

## 6. Known limitations

These are properties of the current build, not proposals. The full list, with
severities and workarounds, is in
[`../engineering/known-issues.md`](../engineering/known-issues.md).

| # | Limitation | Consequence |
| - | ---------- | ----------- |
| 1 | The rate limiter is **per process**, and its key is the peer address of the connection. | A multi-instance deployment needs edge rate limiting, and behind a proxy that does not terminate the connection every caller shares one budget. See [`security-architecture.md`](security-architecture.md) §6. |
| 2 | SQLx statements are checked at **run time**, not at compile time. | A statement that does not match the schema is found when the path runs. The compile-time workflow is documented in [`../database/migration-policy.md`](../database/migration-policy.md) §6 but is not enabled. |
| 3 | There is **no container image** for the API, and no release process. | The quality gates run in CI; the rollout is a manual step. |
| 4 | The API does **not** terminate TLS. | The process must sit behind a proxy that does. |
| 5 | The refresh token travels in the **response body**, not in an HTTP-only cookie. | A browser client would have to manage it in scriptable storage. Open question Q2 in [`../project-truth.md`](../project-truth.md) §16. |
| 6 | There is **no session-management surface** for a user, no password change and no account lockout. | `AuthService::revoke_all_sessions` is not reachable through any endpoint. The expired sessions are removed by the background pruning task. |
| 7 | The refresh token has **one absolute lifetime** and no idle timeout. | Open question Q3 in [`../project-truth.md`](../project-truth.md) §16. |
| 8 | Note search is a **plain full-text match** without ranking or highlighting. | Open question Q4. |
| 9 | Deleting a project **deletes** its tasks, through `ON DELETE CASCADE`. | Open question Q1. |
| 10 | The task ordering key is a `DOUBLE PRECISION` fraction and there is **no reordering endpoint**. | Clients set `position` explicitly. |

## 7. Related documents

- [`module-boundaries.md`](module-boundaries.md) — module by module.
- [`request-lifecycle.md`](request-lifecycle.md) — the path of one request.
- [`data-architecture.md`](data-architecture.md) — the persistence model.
- [`security-architecture.md`](security-architecture.md) — the trust model.
- [`deployment.md`](deployment.md) — configuration and operations.
- [`../../config/README.md`](../../config/README.md) — the configuration
  reference.
- [`../decisions/README.md`](../decisions/README.md) — the accepted decisions.

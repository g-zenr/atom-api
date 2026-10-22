# ADR-0002 — Rust, Axum and PostgreSQL

> **Status:** Accepted
> **Date:** 2026-10-21
> **Owner:** architect

## Context

Phase 1 needs a backend that serves a versioned JSON API over a relational
data model, with real transactions, real constraints and real full-text
search. The requirements in [`../project-truth.md`](../project-truth.md) §8
and §12 constrain the choice:

- stable Rust, edition 2024;
- Axum 0.8 on Tokio for HTTP;
- PostgreSQL for storage, SQLx for access, SQLx migrations for the schema;
- **migrations are append-only**, and an applied migration is never edited;
- **layering is fixed**: handlers → services → repositories → PostgreSQL;
- **no global mutable state**; dependencies are passed explicitly;
- **no `unwrap` or `expect` in a request path**;
- **all SQL is parameterised**;
- **the build must not require a hosted or paid service**.

The last constraint matters more than it looks. It rules out a stack whose
toolchain needs a managed database, a paid API, or a hosted build service in
order to compile.

The data model is genuinely relational. A note belongs to a user; a task
belongs to a project, which belongs to a user; a refresh session belongs to a
user and links to the session that replaced it. The invariant that a task's
`completed_at` must agree with its `status` is a constraint on a single row.
The requirement that a task can never be attached to a project the caller
does not own is a predicate across two tables. The requirement that the
normalized email address is unique is a unique index. These are relational
statements, and the storage engine should be able to express them natively
and enforce them.

## Decision

**ATOM's Phase 1 backend is written in Rust with Axum on Tokio, and stores
its data in PostgreSQL through SQLx.**

| Concern | Choice | Version |
| ------- | ------ | ------- |
| Language and edition | Rust, stable, edition 2024 (`rust-version = "1.85"`) | pinned by `rust-toolchain.toml` with `rustfmt` and `clippy` |
| HTTP | `axum` with `tower` and `tower-http` | 0.8 / 0.5 / 0.6 |
| Async runtime | `tokio`, multi-threaded | 1 |
| Database access | `sqlx` with the `postgres`, `runtime-tokio`, `tls-rustls`, `macros`, `migrate`, `uuid`, `chrono` and `json` features | 0.8 |
| Schema | PostgreSQL, changed by SQLx migrations embedded in the binary | 14 or newer, per [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) |
| Serialization | `serde` and `serde_json` | 1 |
| API description | `utoipa` with `utoipa-swagger-ui` | 5 / 9 |

### Amendment — the dependency list since the decision was accepted

**The decision is unchanged.** Rust with Axum on Tokio, storing data in
PostgreSQL through SQLx, is still the stack. What changed is the *detail* of
the manifest, which the table above records as it was accepted.

| Change | Why | Reference |
| ------ | --- | --------- |
| The `sqlx` `json` feature was removed | No column is a `JSON` or `JSONB` column, and no statement uses `sqlx::types::Json`. The feature compiled code the crate never called. | `ATOM-CHANGE-20261022-003` |
| The `axum` `macros` feature was removed | The crate uses no `debug_handler`, `debug_middleware` or `#[derive(FromRef)]`. | `ATOM-CHANGE-20261022-003` |
| The `tokio` `sync` feature was removed | The only mutex in the crate is `std::sync::Mutex`. | `ATOM-CHANGE-20261022-003` |
| `tower` became a development dependency | The application uses `axum` and `tower-http`; `tower` is needed only by the test harness. | `ATOM-CHANGE-20261022-003` |
| The `hex` crate was removed | One call site formats a SHA-256 digest; `sha2`'s own output implements `LowerHex`. The encoding is pinned by a literal assertion. | `ATOM-CHANGE-20261022-003` |

The `sqlx` `macros`, `migrate`, `uuid` and `chrono` features remain, because
`sqlx::migrate!`, `#[derive(FromRow)]` and the UUID and timestamp columns
depend on them.

The consequences of the choice are:
- **The router, the middleware and the extractors are explicit.** The
  middleware stack is assembled in one function, `api::router`, so that the
  security posture of the API can be read in one file.
- **The handler signature is the contract.** A handler that takes `AuthUser`
  cannot be reached without a valid bearer token, and a handler that takes
  `Json<T>` gets a typed body.
- **Ownership is a type-level obligation at the boundary and a SQL-level
  obligation below it.** `AuthUser` is the only way a handler learns who the
  caller is.
- **The persistence layer is hand-written SQL with bound parameters.**
  `sqlx::query_as` with `.bind(...)`, mapped onto `#[derive(FromRow)]`
  structs. No query builder, no ORM.
- **The migrations are the source of truth for the schema** and are applied
  by the binary at start-up, so a deployment never depends on files that sit
  next to it.
- **Compile-time query checking is deferred.** The statements are checked at
  run time. The compile-time workflow is documented in
  [`../database/migration-policy.md`](../database/migration-policy.md) §6 and
  is not enabled, because it would make the build depend on a live database
  or on a committed metadata file that has to be regenerated on every schema
  change.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **Go** (net/http, Echo, Chi, GORM) | Go is a perfectly good language for this and has excellent concurrency. It was not chosen because the project already standardises on Rust, and because the type-level guarantees around `Result` and the absence of `null` in the persistence models map more directly onto the requirement that "no `unwrap` or `expect` in a request path". The decisive practical factor is toolchain unity: one language for the backend and one for the tooling means one quality gate. |
| **Node.js / TypeScript** (Express, Fastify, Prisma) | Excellent developer ergonomics, but the runtime is single-threaded per process by default, the error-handling story is weaker, and a type-level guarantee like "a handler cannot be reached without credentials" is not enforceable in the same way. The runtime's event-loop blocking behaviour under a synchronous password hash is a concrete risk for an API whose most expensive operation is Argon2id. |
| **Django / Rails** | Both deliver an HTTP API with an ORM, a migration system and an admin interface, which is a large amount of what Phase 1 needs. They were not chosen because the requirement is an explicit layering rule (`handlers` → `service` → `repository` → SQL) with parameterised SQL written by hand, and because a second, opinionated runtime and its toolchain would sit alongside a Rust-based tooling story for no Phase 1 requirement. |
| **SQLite** | Excellent for a single-process, single-file application and it would remove the database dependency entirely. It was not chosen because the data model uses PostgreSQL features that SQLite does not have: `TIMESTAMPTZ` with the same semantics, generated `tsvector` columns, `GIN` indexes over a generated column, and native enum types for `task_status` and `task_priority`. The requirement for real constraints, real full-text search and a real concurrent multi-client server is PostgreSQL's. |
| **MongoDB** | A document store would make a note easy to model, but the ownership model is relational — a task inherits its owner through its project — and the invariants the requirements demand are relational: a `CHECK` constraint tying `completed_at` to `status`, a foreign key with a cascade, a unique index on a normalized value. A document store would move all of that into application code, which is exactly what the requirements ask to avoid. |
| **An ORM with a query builder** (SQLx's `query_builder`, Diesel, SeaORM) | A query builder makes the ownership predicate easier to write and easier to forget. Hand-written SQL with bound parameters makes the predicate visible at a glance in review, which is the property ADR-0005 depends on. |

## Consequences and trade-offs

### Positive

- **The ownership predicate is visible.** Every statement shows the
  `user_id` predicate next to the data it touches, so a reviewer can check
  it. This is what makes ADR-0005 enforceable by review.
- **The schema can enforce the invariants.** Native enum types, `CHECK`
  constraints, foreign keys with cascades and generated columns are all used,
  and an out-of-contract value cannot be persisted.
- **Full-text search is a database feature.** The generated `tsvector` and
  its GIN index give note search without a second service and without a
  second deployment.
- **The failure model is small.** One process, one pool, one database. A
  stack trace is a stack trace, not a distributed trace.
- **The binary is self-contained.** The migrations are embedded in it, so
  the artifact is the whole deployment.
- **The toolchain is unified.** One toolchain, one formatter, one linter and
  one test runner for the service, the documentation checker and the
  tooling.

### Negative

- **Compile-time verification of the SQL is not available today.** A
  statement that does not match the schema is found when the code path
  runs, not when it is compiled. The mitigation is that the integration suite
  runs against a real PostgreSQL database with the real migrations applied.
  This is recorded as KI-004 in
  [`../engineering/known-issues.md`](../engineering/known-issues.md) and as
  accepted risk AR-3 in [`../security/threat-model.md`](../security/threat-model.md).
- **The migration model is hand-written SQL.** A schema change is a
  migration file plus a matching model change in the same pull request. That
  is more work than an auto-generated migration, and it is what
  [`../database/migration-policy.md`](../database/migration-policy.md) §3
  requires.
- **The dependency surface is larger than the standard library.** The
  argument for the stack is that the crates are few, well-known and pinned
  by a committed `Cargo.lock`, but they are still crates.
- **The compile times and the learning curve are real.** Rust is not the
  fastest language to iterate in, and the borrow checker is a genuine
  obstacle for a contributor who has not used it.
- **Async is contagious.** Every handler, service and repository method is
  `async`, and a blocking call in a request path stalls a worker thread.
  The rule that there is no blocking work in a request path is therefore
  load-bearing.
- **PostgreSQL is an operational dependency.** The build does not need a
  hosted service, but running the service needs a database.

## Security and operational implications

- **Parameterised SQL by construction.** `sqlx::query_as` with `.bind(...)`
  is the only way a statement is written in this codebase, which removes the
  usual SQL-injection surface. The secure-development rule SD-1 exists to
  keep it that way.
- **The database is the last line of defence for the invariants.** The
  constraints in the migrations are what stop an out-of-contract value, an
  orphaned row or an inconsistent completion timestamp from being persisted,
  whatever the application code does.
- **A single trust boundary to the database.** The process authenticates to
  PostgreSQL with one role. The production checklist requires that role to
  have the least privilege it can, while still being able to apply the
  migrations.
- **TLS to the database is available.** `sqlx` is built with the `tls-rustls`
  feature, so `DATABASE_URL` may request TLS.
- **Panics are contained.** `CatchPanicLayer` turns a panic into a `500` in
  the standard envelope rather than a dropped connection.
- **The observability surface is one stack.** `tracing` spans, one
  correlation id and one log sink.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §8 | The technical constraints this decision implements. |
| [`../project-truth.md`](../project-truth.md) §12 | The database and persistence requirements. |
| [`../project-truth.md`](../project-truth.md) §13 | The quality gates that the toolchain supports. |
| [`../database/migration-policy.md`](../database/migration-policy.md) | The workflow this decision produces, including §6 on compile-time checking. |
| [`../architecture/request-lifecycle.md`](../architecture/request-lifecycle.md) | The middleware stack and the extractors this stack produces. |
| [`../architecture/data-architecture.md`](../architecture/data-architecture.md) | The persistence model this stack produces. |
| [`../engineering/known-issues.md`](../engineering/known-issues.md) KI-004 | Run-time SQL verification, the accepted cost. |
| [`ADR-0001-modular-monolith.md`](ADR-0001-modular-monolith.md) | The deployment shape this stack serves. |

## Supersedes / Superseded by

Supersedes: none. This is the first decision about the technology stack.

Superseded by: none. A review of this decision is due if the project adopts
compile-time query verification, because that changes the build contract
described in §6 of the migration policy.

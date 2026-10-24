# ATOM database

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `database` agent

This directory holds the persistence documentation of the ATOM backend: the
Phase 1 schema that the migrations create, the proposed Phase 2 schema, the
migration workflow and the development seeder.

| Document | Contents |
| -------- | -------- |
| [`schema.md`](schema.md) | Every Phase 1 table with its columns, types, nullability, defaults, constraints, indexes and relationships; the ownership model; the search design; the enum types; the migration that introduced each table; and, in §11, the proposed Phase 2 tables. |
| [`migration-policy.md`](migration-policy.md) | How migrations are written, applied and reviewed; the rollback policy; the rule that every migration has a matching Rust model change; and the SQLx compile-time check workflow. |
| [`seeding.md`](seeding.md) | `atom-seed`: the one command, the three profiles, the four safety guards, determinism, idempotency, `--reset`, the post-insert verification, the development credentials and the test list. |

## 1. The engine

| Property | Value |
| -------- | ----- |
| Engine | **PostgreSQL** |
| Local development image | `postgres:18-alpine`, started by `docker compose up -d postgres` |
| Minimum supported version | PostgreSQL 14 or newer, as stated in [`CONTRIBUTING.md`](../../CONTRIBUTING.md) §2 |
| Access layer | **SQLx 0.8** with the `postgres` feature, the Tokio runtime and rustls |
| Schema management | **SQLx migrations**, embedded in the binary |
| Migration directory | `migrations/` |

PostgreSQL was chosen because the Phase 1 requirements need three things it
provides directly: full-text search over the user's own notes, native enum
types for the controlled task state, and check constraints that make an
out-of-contract value impossible to persist. The reasoning is recorded in
[`ADR-0002-rust-axum-postgresql.md`](../decisions/ADR-0002-rust-axum-postgresql.md).

## 2. The tables at a glance

| Table | Introduced by | Owner | Purpose |
| ----- | ------------- | ----- | ------- |
| `users` | `migrations/0001_users_and_sessions.sql` | — | The account. |
| `refresh_sessions` | `migrations/0001_users_and_sessions.sql` | `users` | One row per issued refresh token. |
| `notes` | `migrations/0002_notes.sql` | `users` | Personal notes, with a generated full-text search column. |
| `projects` | `migrations/0003_projects_and_tasks.sql` | `users` | The container for tasks. |
| `tasks` | `migrations/0003_projects_and_tasks.sql` | `projects` | The units of work inside a project. |
| `expense_claims` | `migrations/0004_expense_claims.sql` | `users` | A reimbursement claim: its status, lifecycle timestamps and computed total. |
| `expense_items` | `migrations/0004_expense_claims.sql` | `users` | The line items of a claim; `total_amount` is their sum. |

The full definition of each table is in [`schema.md`](schema.md).

Phase 2 proposes six more tables. Five of them now exist:
`expense_claims` and `expense_items` have
`migrations/0004_expense_claims.sql` and a Rust model in
`src/reimbursements/models.rs`; `companies`, `job_applications` and
`application_events` have `migrations/0005_job_applications.sql` and Rust models
in `src/job_applications/models.rs`. The sixth — `embeddings` — has neither a
migration nor a Rust model, is marked `Proposed` in [`schema.md`](schema.md)
§11, and must not be treated as scope. The development seeder that fills the
existing tables is implemented and specified in [`seeding.md`](seeding.md).

## 3. The migration workflow at a glance

```text
write migrations/000N_description.sql
        │
        ├─ review: the policy is in migration-policy.md
        │
        ├─ apply locally:  cargo sqlx migrate run
        │                  (or simply start the API: the migrations are embedded
        │                   and are applied at start-up)
        │
        ├─ update the Rust models in the same change
        │
        └─ apply to the test database:
             TEST_DATABASE_URL=… cargo test --all-targets
```

- Migrations are **append-only**. An applied migration is never edited.
  A mistake is fixed by a new migration.
- The migrations are **embedded in the binary** with
  `sqlx::migrate!("./migrations")`, and `src/main.rs` applies the pending
  ones before the server binds its socket. A deployment therefore never
  depends on migration files that sit next to it.
- The integration test harness applies the same migrations to the disposable
  test database, in `tests/common/mod.rs`.
- The full rules are in [`migration-policy.md`](migration-policy.md).

## 4. Local development setup

### With Docker

```bash
docker compose up -d postgres
docker compose ps
```

`docker-compose.yml` starts PostgreSQL on `127.0.0.1:5432` with the
credentials `atom`/`atom`, the bootstrap database `atom`, and two more:
`atom_dev`, the development database the API and the seeder use, and
`atom_test`, the disposable database the integration suite uses. Both are
created by `scripts/create-test-database.sh`, which runs as an entrypoint
hook. The seeder refuses `atom` by design, so `DATABASE_URL` points at
`atom_dev`.

### Without Docker

```bash
createdb atom_dev
createdb atom_test
```

### Environment

```bash
cp .env.example .env
```

Then set at least:

| Variable | Purpose |
| -------- | ------- |
| `DATABASE_URL` | The database the API uses, for example `postgres://atom:atom@127.0.0.1:5432/atom_dev`. |
| `TEST_DATABASE_URL` | The disposable database the integration suite uses. Its name **must** contain `test`; the harness refuses to run otherwise. |
| `APP_JWT_SECRET` | The access-token signing secret, at least 32 bytes. |

The full list is in [`.env.example`](../../.env.example). The file contains
placeholders only; the real `.env` is git-ignored and must never be
committed.

### Running

```bash
cargo run                      # starts the API on 127.0.0.1:3000
```

The API applies the pending migrations at start-up, so a fresh database
becomes usable without a separate command.

```bash
# Apply the migrations by hand, if you prefer.
cargo sqlx migrate run
```

### Testing

```bash
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test cargo test --all-targets
```

Windows PowerShell:

```powershell
$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
cargo test --all-targets
```

## 5. The rules the schema follows

These come from [`project-truth.md`](../project-truth.md) §12 and are enforced
by the migrations:

- Every table has a **UUID** primary key, generated by the application.
- Every timestamp is **`TIMESTAMPTZ`**.
- Ownership is modelled as a **non-null foreign key** and enforced in every
  query.
- **Uniqueness** — especially of the normalized email address — is enforced
  by the database, not by a check-then-insert race.
- **Controlled state** — the task status and priority — is constrained by
  the database with native enum types.
- Application invariants are backed by **`CHECK` constraints** where it is
  practical.
- **Indexes** exist for the access paths the API actually uses: ownership
  lookups, listing order, filters and search.
- Multi-step writes use an explicit transaction, unless an accepted ADR records
  that the ordering fails safe. The refresh-token rotation is the one such
  case: the replacement session is inserted before the presented one is
  claimed, so an interruption leaves the caller with a usable session. The
  exception is recorded in
  [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md)
  and analysed in
  [`../architecture/data-architecture.md`](../architecture/data-architecture.md)
  §7.
- Migrations are **reproducible**: applying them to an empty database
  produces the same schema as applying them in sequence.

## 6. Related documents

- [`schema.md`](schema.md) — the tables, the constraints, the indexes, the
  ownership model and the search design, plus the proposed Phase 2 tables.
- [`migration-policy.md`](migration-policy.md) — how a migration is written
  and what happens when one is wrong.
- [`seeding.md`](seeding.md) — the development seeder and its guards.
- [`../api/error-contract.md`](../api/error-contract.md) — how a database
  failure reaches the client.
- [`../features/README.md`](../features/README.md) — the features that own
  the tables.

# ATOM database

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent

This directory holds the persistence documentation of the ATOM Phase 1
backend.

| Document | Contents |
| -------- | -------- |
| [`schema.md`](schema.md) | Every table with its columns, types, nullability, defaults, constraints, indexes and relationships; the ownership model; the search design; the enum types; and the migration that introduced each table. |
| [`migration-policy.md`](migration-policy.md) | How migrations are written, applied and reviewed; the rollback policy; the rule that every migration has a matching Rust model change; and the SQLx compile-time check workflow. |

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

The full definition of each table is in [`schema.md`](schema.md).

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
credentials `atom`/`atom`, the database `atom`, and a second, disposable
database `atom_test`. The second database is created by
`scripts/create-test-database.sh`, which runs as an entrypoint hook.

### Without Docker

```bash
createdb atom
createdb atom_test
```

### Environment

```bash
cp .env.example .env
```

Then set at least:

| Variable | Purpose |
| -------- | ------- |
| `DATABASE_URL` | The database the API uses, for example `postgres://atom:atom@127.0.0.1:5432/atom`. |
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
- Multi-step writes use an explicit transaction.
- Migrations are **reproducible**: applying them to an empty database
  produces the same schema as applying them in sequence.

## 6. Related documents

- [`schema.md`](schema.md) — the tables, the constraints, the indexes, the
  ownership model and the search design.
- [`migration-policy.md`](migration-policy.md) — how a migration is written
  and what happens when one is wrong.
- [`../api/error-contract.md`](../api/error-contract.md) — how a database
  failure reaches the client.
- [`../features/README.md`](../features/README.md) — the features that own
  the tables.

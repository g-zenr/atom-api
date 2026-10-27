# Migration policy

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `docs` agent

How a change to the ATOM schema is made, applied, reviewed and — when it is
wrong — corrected.

The rule that everything else follows from is in
[`AGENTS.md`](../../AGENTS.md) §4.7: **migrations are append-only**.

## 1. How a migration is written

### File naming and numbering

```text
migrations/
├── 0001_users_and_sessions.sql
├── 0002_notes.sql
├── 0003_projects_and_tasks.sql
├── 0004_expense_claims.sql
├── 0005_job_applications.sql
├── 0006_embeddings.sql
└── 0007_api_keys.sql
```

The repository ends at `0007`; the next migration takes `0008`.

- The file name starts with a **four-digit, zero-padded sequence number**
  followed by an underscore and a short description in lower case with
  underscores.
- The number is the migration's identity and its order. It is never reused
  and never renumbered.
- SQLx assigns its own checksum to each file, so the file content is
  effectively frozen once it has been applied.

### One concern per file

- A migration does **one** thing: it creates the users and the sessions, or
  it creates the notes, or it adds a column.
- A migration that creates two tables that belong together is fine. A
  migration that creates a table *and* rewrites an unrelated one is not.
- The reason is review: a reviewer has to be able to see what a change does
  to the schema without reading a diff of forty unrelated statements.

### What a migration contains

- Plain SQL, executed by PostgreSQL, in one transaction. SQLx runs each
  migration in a transaction unless it contains a statement that cannot run
  in one, such as `CREATE INDEX CONCURRENTLY` or certain `ALTER TYPE ... ADD
  VALUE` forms.
- The `CREATE TABLE`, `CREATE TYPE`, `CREATE INDEX` and `ALTER TABLE`
  statements, and nothing else. No data manipulation, no application logic.
- A leading comment block that states, in prose, what the migration does
  and why a non-obvious choice was made. The existing migrations set the
  tone; the comment is what the next contributor reads first.
- The rules that apply to the SQL itself:
  - a **UUID** primary key;
  - `TIMESTAMPTZ` for every timestamp;
  - a **non-null** foreign key for ownership, with `ON DELETE CASCADE`;
  - a `CHECK` constraint for every length limit the application enforces;
  - an index for every access path the API actually uses.

### The naming of constraints and indexes

The existing migrations use a consistent, readable convention:

| Object | Pattern | Example |
| ------ | ------- | ------- |
| Table | plural, lower case | `refresh_sessions` |
| Check constraint | `<table>_<subject>_chk` | `users_email_normalized_chk` |
| Unique index | `<table>_<subject>_unique_idx` | `users_email_unique_idx` |
| Ordinary index | `<table>_<columns>_idx` | `notes_user_id_created_at_idx` |
| Partial index | `<table>_<subject>_idx` | `refresh_sessions_active_idx` |
| Enum type | `<subject>_snake_case` | `task_status` |

The primary key uses PostgreSQL's default name, `<table>_pkey`.

## 2. How a migration is applied

### At start-up, automatically

The migrations are **embedded in the binary**:

```rust
sqlx::migrate!("./migrations").run(&pool).await?;
```

`src/main.rs` runs this after the connection pool is created and **before**
the socket is bound. Consequences:

- A deployment never depends on migration files that sit next to the
  binary.
- A fresh database becomes usable by simply starting the API.
- The start-up order is fixed: load `.env`, read and validate the
  configuration, connect, migrate, bind, serve. A configuration error stops
  the process before it touches the database.
- If the migration fails, the process exits with a non-zero status and
  never serves traffic against a half-migrated schema.

### By hand, during development

```bash
cargo sqlx migrate run
```

`make migrate` does the same thing. The command needs a live `DATABASE_URL`
and the `sqlx-cli` tool.

### In the test suite

The integration harness applies the migrations itself, in
`tests/common/mod.rs`:

```rust
sqlx::migrate!("./migrations")
    .run(&pool)
    .await
    .expect("migrations must apply to the test database");
```

Every test therefore runs against the real schema, produced by the real
migration files, rather than against a hand-maintained fixture.

### What is **not** done

- Migrations are never applied by hand to a production database and then
  forgotten: the binary does it on every start.
- A migration is never edited after it has been applied. See §5.

## 3. The rule that every migration has a matching Rust model change

**A schema change and the code that reads and writes it belong in the same
change.** A migration that is not paired with a model change is incomplete.

| Schema change | Rust change |
| ------------- | ----------- |
| A new column | A field on the persistence struct in `models.rs`, the column added to every `SELECT` and `INSERT` in `repository.rs`, and the field on the public contract in `dto.rs` if it is part of the public API. |
| A new table | A `models.rs` struct with `#[derive(FromRow)]`, a `repository.rs` type, and a service that owns it. |
| A new enum type | A `#[derive(sqlx::Type)]` enum with the matching `type_name` and `rename_all`. |
| A new check constraint | The corresponding validation in the service, so that the failure is a `422` with a field name rather than a database error. |
| A new index | A note in the change-log entry of the query it serves. |
| A dropped column | The field removed from the struct, the `SELECT` and the `INSERT` in the same change. |

The reason is that the persistence layer is written by hand against the
schema, and the two can only stay in step if they change together.

A migration that changes the **public contract** — a new field in a
response, a new status value — also requires an update to the OpenAPI
annotations and to [`../api/`](../api/README.md), as required by
[`AGENTS.md`](../../AGENTS.md) §4.10.

## 4. How a migration is reviewed

A migration is reviewed as part of the pull request that contains it. The
reviewer checks:

1. **The file is new.** An applied migration is never modified. A diff that
   touches an existing migration file is rejected.
2. **The number is the next one.** The sequence has no gaps and no
   duplicates.
3. **The purpose is singular.** The migration does one thing.
4. **The constraints are complete.** Every length limit the application
   enforces has a `CHECK`. Every controlled value uses an enum type or a
   `CHECK`. Every ownership relation is a non-null foreign key.
5. **The indexes serve the queries.** Every index maps to an access path the
   API actually uses, and every access path the API uses has an index.
6. **The cascade behaviour is intentional.** `ON DELETE CASCADE` and
   `ON DELETE SET NULL` are chosen, and the product consequence is
   documented — in the migration's comment, and in the feature
   specification when it is user-visible.
7. **The Rust side changed in the same pull request.** The models, the
   repository statements and the DTOs are updated.
8. **The documentation changed in the same pull request.**
   [`schema.md`](schema.md) describes the new column, the new table or the
   new index; the feature's `api.md` and `specification.md` describe the new
   contract; the change-log entry records the change.
9. **The migration applies to a clean database.** The contributor applies
   the whole migration chain to an empty database and confirms the
   resulting schema matches what the application expects. This is what
   makes the migrations reproducible.

## 5. The rollback policy: forward-fix, never edit

**An applied migration is never edited, and never deleted.**

The reason is that SQLx records a checksum of every applied file. Editing a
file makes the recorded checksum disagree with the file on disk, and the
next start-up fails with a checksum mismatch. The result is a server that
refuses to start, which is a worse outcome than a slightly wrong schema.

The only way to correct an applied migration is to **add a new one**:

```text
0001_users_and_sessions.sql   ← applied, wrong
0002_notes.sql                ← applied
0003_projects_and_tasks.sql   ← applied
0004_fix_the_user_email_index.sql   ← new: corrects 0001
```

A corrective migration is named for what it *does*, not for the migration it
corrects, so that its purpose is readable on its own.

There is no `down` migration and no automatic rollback. A migration that has
been applied has changed the shape of the data, and PostgreSQL cannot
reliably undo that. The recovery path is a new migration that moves the data
forward to the intended shape.

For a deployment that must be rolled back to a previous **binary**, the
forward-fix migration is applied and the old binary is then started against
the new schema only if the change is backward compatible. When it is not,
the new binary is rolled forward. The deployment procedure is described in
[`../architecture/deployment.md`](../architecture/deployment.md).

## 6. SQLx: run-time checks today, compile-time checks available

### The current state

The SQL statements in `src/**/repository.rs` are written with
`sqlx::query_as` and are **checked at run time**. The parameters are bound
with `.bind(...)`, and the result is mapped onto a `#[derive(FromRow)]`
struct. The statements are not verified when the crate is compiled.

This has one important consequence: **a statement that does not match the
schema is not caught by the compiler.** It is caught the first time the code
path runs, which is why the integration suite runs against a real
PostgreSQL database with the real migrations applied.

The reason for the current state is recorded in
[`../project-truth.md`](../project-truth.md) §17: a compile-time check
requires either a live database at build time or a checked-in query
metadata file, and the project decided that the build must not depend on a
live database.

### The compile-time workflow, documented but not enabled

SQLx can verify the statements at compile time. The workflow is:

```bash
# 1. Create or update the offline query metadata.
cargo sqlx prepare -- --all-targets

# 2. Commit the generated .sqlx/ directory.

# 3. Build with the metadata instead of a live database.
SQLX_OFFLINE=true cargo build
```

With `SQLX_OFFLINE=true` and a committed `.sqlx/` directory, `cargo build`
verifies every statement against the recorded schema and fails if a
statement, a column or a type does not match.

**This workflow is documented but not enabled.** The project does not commit
a `.sqlx/` directory, and the build does not set `SQLX_OFFLINE`. Enabling it
is a deliberate change with a trade-off:

| Option | Consequence |
| ------ | ----------- |
| Run-time checks (today) | The build never needs a database. A schema mismatch is found when the code path runs, and the integration suite finds it. |
| Compile-time checks | A schema mismatch is found at build time, and the build is faster and self-contained. The cost is a committed metadata file that must be regenerated whenever the schema or a statement changes, and a build that fails when the metadata is stale. |

Whichever is chosen, the requirement from
[`AGENTS.md`](../../AGENTS.md) §4.10 stays the same: a public contract
change is documented in the same change.

## 7. Checklist for a schema change

- [ ] A new file `migrations/000N_<description>.sql` was added; no existing
      migration was edited.
- [ ] The migration does one thing and has a comment that explains it.
- [ ] Every primary key is a `UUID`; every timestamp is a `TIMESTAMPTZ`.
- [ ] Every ownership relation is a non-null foreign key with an explicit
      `ON DELETE` behaviour.
- [ ] Every length limit has a `CHECK` constraint; every controlled value
      uses an enum type or a `CHECK`.
- [ ] Every new access path has an index.
- [ ] The Rust models, the repository statements and the DTOs changed in the
      same pull request.
- [ ] The OpenAPI annotations and [`../api/`](../api/README.md) changed if
      the public contract changed.
- [ ] [`schema.md`](schema.md) describes the new schema.
- [ ] The feature's `specification.md` and `api.md` describe the new
      behaviour, and its `status.md` reflects the new evidence.
- [ ] The change is recorded in
      [`../engineering/change-log.md`](../engineering/change-log.md).
- [ ] The migration chain was applied to an empty database and the
      resulting schema was checked.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features
      -- -D warnings` and `cargo test --all-targets` pass.

## 8. Related documents

- [`schema.md`](schema.md) — the schema as it is today.
- [`README.md`](README.md) — the engine, the workflow and the local setup.
- [`../architecture/data-architecture.md`](../architecture/data-architecture.md)
  — how the data model is designed and why.
- [`../engineering/change-log.md`](../engineering/change-log.md) — the record
  of what changed, and when.

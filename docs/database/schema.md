# Database schema

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-27
> **Owner:** maintained by the `database` agent

The complete Phase 1 schema of the ATOM database, table by table, as it is
defined in `migrations/`. Section 11 is the Phase 2 schema. Its first two tables
are created by `migrations/0004_expense_claims.sql` and are served by the twelve
claim operations of chains `R2`-`R4`; its next three are created by
`migrations/0005_job_applications.sql` and are served by the fourteen
operations of chains `J2`-`J4`; `embeddings` is created by
`migrations/0006_embeddings.sql` and is served by the three machine-learning
operations built in chains `M3`-`M6`; and `api_keys` is created by
`migrations/0007_api_keys.sql` and is served by the three key-management
operations of chain `K1`. No table in §11 is proposed any more, and
`migrations/` ends at `0007`.

The conventions the schema follows are in
[`../project-truth.md`](../project-truth.md) §12. The rules for changing it
are in [`migration-policy.md`](migration-policy.md).

## 1. Overview

```text
users ──1:N──► refresh_sessions
  │
  ├──1:N──► notes
  │
  ├──1:N──► expense_claims ──1:N──► expense_items
  │              └─(project_id, SET NULL)──► projects
  │
  ├──1:N──► companies ──1:N──► job_applications ──1:N──► application_events
  │                                   └─(next_action_task_id, SET NULL)──► tasks
  │
  ├──1:N──► api_keys
  │
  └──1:N──► projects ──1:N──► tasks
```

| Table | Introduced by | Rows |
| ----- | ------------- | ---- |
| `users` | `migrations/0001_users_and_sessions.sql` | One row per registered account. |
| `refresh_sessions` | `migrations/0001_users_and_sessions.sql` | One row per issued refresh token. |
| `notes` | `migrations/0002_notes.sql` | One row per note. |
| `projects` | `migrations/0003_projects_and_tasks.sql` | One row per project. |
| `tasks` | `migrations/0003_projects_and_tasks.sql` | One row per task. |
| `expense_claims` | `migrations/0004_expense_claims.sql` | One row per reimbursement claim. Read and written by the five `R2` claim endpoints (§11.1). |
| `expense_items` | `migrations/0004_expense_claims.sql` | One row per claim line item. Read and written by the three item endpoints; the claim detail response reads it (§11.2). |
| `companies` | `migrations/0005_job_applications.sql` | One row per employer (§11.3). Served by `J2`'s five operations. |
| `job_applications` | `migrations/0005_job_applications.sql` | One row per role being pursued at a company (§11.4). Served by the five application operations of `J3`. |
| `application_events` | `migrations/0005_job_applications.sql` | One row per dated entry in an application's timeline (§11.5). Served by the four event operations of `J4`. |
| `embeddings` | `migrations/0006_embeddings.sql` | One row per source, model and owner (§11.6). Read and written by `src/ml/repository.rs` and purged by the delete statement of each source. |
| `api_keys` | `migrations/0007_api_keys.sql` | One row per issued API key (§11.7). The secret is never stored — only its SHA-256 digest — and the row is read on every request that presents a key. |

Two enum types are created by `migrations/0003_projects_and_tasks.sql`:
`task_status` and `task_priority`. `migrations/0004_expense_claims.sql` adds a
third, `expense_claim_status`, and `migrations/0005_job_applications.sql` adds
`job_application_status` and `application_event_kind` (§11.8).
`migrations/0006_embeddings.sql` adds no type:
`embeddings.source_type` is a `CHECK`ed `TEXT`, for the reason §11.8 gives.
`migrations/0007_api_keys.sql` adds no type either: every controlled value in
`api_keys` is a regular-expression `CHECK` (§11.7).

No table of §11 is still proposed. The last one, `api_keys` (§11.7), is created
by `migrations/0007_api_keys.sql`, and `migrations/` ends at `0007`.

### 1.1 A stale comment in an applied migration

The header comment of
[`../../migrations/0001_users_and_sessions.sql`](../../migrations/0001_users_and_sessions.sql)
says that "Deletes cascade for strictly-owned data and restrict where a parent
must survive its children (projects -> tasks is handled explicitly in the
service layer, see docs/database/migration-policy.md)".

**That sentence is wrong.** The `projects → tasks` relation is a **database
cascade**: `tasks.project_id` references `projects (id) ON DELETE CASCADE`,
declared in
[`../../migrations/0003_projects_and_tasks.sql`](../../migrations/0003_projects_and_tasks.sql).
The service layer does not delete the tasks: the database deletes them, in the
same statement and the same transaction as the delete of the project. The
behaviour is described in §6 and in
[`../architecture/data-architecture.md`](../architecture/data-architecture.md)
§1.2, and it is verified by `deleting_a_project_removes_its_tasks` in
[`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs).

The comment has **not** been corrected in place, because the migration is
**applied**: editing it would change its SQLx checksum, and the next start-up
would refuse to run. The correction is recorded here and in
[`../engineering/change-log.md`](../engineering/change-log.md) instead. The
rule is in [`migration-policy.md`](migration-policy.md): an applied migration
is never edited, and a schema change is a new migration.

### 1.2 A section number that moved after `0006` was applied

The comment on `embeddings.source_type` in
[`../../migrations/0006_embeddings.sql`](../../migrations/0006_embeddings.sql)
points at "§11.7" for the reason the column is `CHECK`ed `TEXT` rather than an
enum type. That section is **§11.8** today: `0007` inserted `api_keys` as
§11.7, so the enum discussion and the cross-cutting invariants moved down by
one (§11.8 and §11.9).

The comment is **not** corrected in place for the reason §1.1 gives — `0006` is
applied and its checksum is fixed. The target of the reference is unambiguous
from its subject ("the new enum types"), so the stale number misdirects nobody
who follows it once. It also stands as a reminder that a section number is not a
stable identifier, and that a document which is allowed to be edited must not be
treated as one.

---

## 2. `users`

One row per registered account. The table was introduced by
`migrations/0001_users_and_sessions.sql`.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key. Generated by the application as UUIDv4. |
| `email` | `TEXT` | no | — | The normalized address. Unique. |
| `password_hash` | `TEXT` | no | — | A PHC-formatted Argon2id hash. Never returned to a client. |
| `display_name` | `TEXT` | no | — | The name the user chose. |
| `is_active` | `BOOLEAN` | no | `TRUE` | `FALSE` disables the account. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | |

### Constraints

| Name | Definition |
| ---- | ---------- |
| `users_email_normalized_chk` | `CHECK (email = LOWER(BTRIM(email)))` — the stored address is already trimmed and lower-cased. The application normalises it too, so the two cannot drift. |
| `users_email_length_chk` | `CHECK (CHAR_LENGTH(email) BETWEEN 3 AND 254)` |
| `users_display_name_chk` | `CHECK (CHAR_LENGTH(display_name) BETWEEN 1 AND 120)` |

### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `users_email_unique_idx` | `UNIQUE INDEX users (email)` | Uniqueness on the normalized value. The repository maps the unique-violation SQLSTATE `23505` to a `409 conflict`, so two concurrent registrations cannot both succeed. |
| `users_pkey` | the primary key on `id` | |

There is no index on `email` beyond the unique one: it serves the login
lookup as well.

---

## 3. `refresh_sessions`

One row per issued refresh token. The table was introduced by
`migrations/0001_users_and_sessions.sql`.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key. Generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. |
| `token_hash` | `TEXT` | no | — | The SHA-256 hash of the opaque refresh token, hex encoded. The token itself is never stored. |
| `expires_at` | `TIMESTAMPTZ` | no | — | The absolute expiry of the session. |
| `revoked_at` | `TIMESTAMPTZ` | yes | `NULL` | Set when the session is revoked or rotated. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `replaced_by_session_id` | `UUID` | yes | `NULL` | `REFERENCES refresh_sessions (id) ON DELETE SET NULL`. The rotation link: the session that superseded this one. |
| `user_agent` | `TEXT` | yes | `NULL` | The `User-Agent` of the client, truncated to 255 characters. |
| `ip_address` | `TEXT` | yes | `NULL` | The address of the client, as a string. |

### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `refresh_sessions_token_hash_unique_idx` | `UNIQUE INDEX refresh_sessions (token_hash)` | A token hash identifies exactly one session, and a duplicate hash is a bug. |
| `refresh_sessions_user_id_idx` | `INDEX refresh_sessions (user_id)` | Lookups by owner. |
| `refresh_sessions_active_idx` | `INDEX refresh_sessions (user_id) WHERE revoked_at IS NULL` | The "revoke every session of this user" operation, and the active-session count. A partial index keeps it small. |

### The rotation link

`replaced_by_session_id` points from the **old** session to the **new** one.
When a refresh token is exchanged, the new session row is inserted and the
old one is revoked with `replaced_by_session_id` set to the new id. The
`ON DELETE SET NULL` clause means that deleting a session does not delete
its successor, so the history stays intact.

A session is usable when `revoked_at IS NULL` and `expires_at > NOW()`; the
same rule is expressed in `RefreshSession::is_usable_at`.

The replacement row **inherits the original `expires_at`**. A rotation
therefore never extends the absolute lifetime of a session: the deadline is
measured from the moment the session was created. The property is analysed in
[`../architecture/data-architecture.md`](../architecture/data-architecture.md)
§7.1.

---

## 4. `notes`

One row per note. The table was introduced by
`migrations/0002_notes.sql`.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key. Generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. |
| `title` | `TEXT` | no | — | Trimmed, 1 to 200 characters. |
| `content` | `TEXT` | no | `''` | At most 100 000 characters. |
| `is_archived` | `BOOLEAN` | no | `FALSE` | The soft-delete marker. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `search_vector` | `tsvector` | no | generated | `GENERATED ALWAYS AS (...) STORED`. See §7. |

### Constraints

| Name | Definition |
| ---- | ---------- |
| `notes_title_length_chk` | `CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200)` |
| `notes_content_length_chk` | `CHECK (CHAR_LENGTH(content) <= 100000)` |

### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `notes_user_id_created_at_idx` | `INDEX notes (user_id, created_at DESC)` | The primary listing path: the owner's notes, newest first. The index matches the `ORDER BY created_at DESC, id DESC` of the query, so the sort does not need a separate step. |
| `notes_user_id_archived_idx` | `INDEX notes (user_id, is_archived)` | The archive filter, without a full scan of the owner's notes. |
| `notes_search_vector_idx` | `INDEX notes USING GIN (search_vector)` | The full-text search. |

---

## 5. `projects`

One row per project. The table was introduced by
`migrations/0003_projects_and_tasks.sql`.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key. Generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. |
| `name` | `TEXT` | no | — | Trimmed, 1 to 120 characters. |
| `description` | `TEXT` | no | `''` | At most 5 000 characters. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | |

### Constraints

| Name | Definition |
| ---- | ---------- |
| `projects_name_length_chk` | `CHECK (CHAR_LENGTH(name) BETWEEN 1 AND 120)` |
| `projects_description_length_chk` | `CHECK (CHAR_LENGTH(description) <= 5000)` |

### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `projects_user_id_created_at_idx` | `INDEX projects (user_id, created_at DESC)` | The listing path: the owner's projects, newest first. |

---

## 6. `tasks`

One row per task. The table was introduced by
`migrations/0003_projects_and_tasks.sql`.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key. Generated by the application. |
| `project_id` | `UUID` | no | — | `REFERENCES projects (id) ON DELETE CASCADE`. There is **no** `user_id` column: ownership is inherited from the project. |
| `title` | `TEXT` | no | — | Trimmed, 1 to 200 characters. |
| `description` | `TEXT` | no | `''` | At most 10 000 characters. |
| `status` | `task_status` | no | `'open'` | A PostgreSQL enum type. |
| `priority` | `task_priority` | no | `'medium'` | A PostgreSQL enum type. |
| `position` | `DOUBLE PRECISION` | no | `0` | The manual ordering key inside the project. A fractional value allows inserting between two tasks. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `completed_at` | `TIMESTAMPTZ` | yes | `NULL` | Present exactly when `status` is `done`. |

### Constraints

| Name | Definition |
| ---- | ---------- |
| `tasks_title_length_chk` | `CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200)` |
| `tasks_description_length_chk` | `CHECK (CHAR_LENGTH(description) <= 10000)` |
| `tasks_completion_consistency_chk` | `CHECK ((status = 'done' AND completed_at IS NOT NULL) OR (status <> 'done' AND completed_at IS NULL))` |

The completion constraint is the important one: it makes "a done task
without a completion timestamp" and "a task with a completion timestamp that
is not done" impossible to store, whatever writes the row.

### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `tasks_project_id_position_idx` | `INDEX tasks (project_id, position, created_at)` | The listing path: the project's tasks in manual order. |
| `tasks_project_id_status_idx` | `INDEX tasks (project_id, status)` | The status filter. |
| `tasks_project_id_priority_idx` | `INDEX tasks (project_id, priority)` | The priority filter. |

### The deletion cascade

`tasks.project_id` is `ON DELETE CASCADE`, so deleting a project deletes its
tasks in the same statement and the same transaction. The product decision
behind this behaviour is recorded in
[`../features/projects/specification.md`](../features/projects/specification.md);
the open question is in
[`../project-truth.md`](../project-truth.md) §16.

---

## 7. The search design

The full-text search over notes uses a **generated stored column** and a
**GIN index**.

```sql
ALTER TABLE notes
    ADD COLUMN search_vector tsvector
    GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', COALESCE(title, '')), 'A') ||
        setweight(to_tsvector('simple', COALESCE(content, '')), 'B')
    ) STORED;

CREATE INDEX notes_search_vector_idx ON notes USING GIN (search_vector);
```

- **The column is generated.** PostgreSQL recomputes it on every write, so
  it can never disagree with the title or the body. There is no trigger to
  forget and no application code to keep in step.
- **The column is stored.** It occupies space on every row, but it can be
  indexed, which a computed expression cannot.
- **The title is weighted `A` and the body `B`.** A term that appears in the
  title ranks above the same term in the body. Phase 1 does not rank the
  results, so the weight has no observable effect yet, but it is in place
  for the search experience described in
  [`../project-truth.md`](../project-truth.md) §16.
- **The `simple` configuration is deliberate.** ATOM notes are technical and
  contain identifiers, versions and code fragments. The `simple`
  configuration does not stem and does not apply a language-specific
  tokenisation, so a search for `kubernetes` does not silently become
  `kubernet`.
- **The query uses `plainto_tsquery`.** The search term is bound as a
  parameter and passed to `plainto_tsquery('simple', $3)`, which turns a
  user string into a safe query. A term that matches nothing yields no
  rows; a term with several words is matched as a conjunction.
- **The search column belongs to the note.** The index is over one table,
  and the query that uses it also filters on `user_id`, so a search cannot
  physically return another account's row.

The project listing uses a different, simpler technique: a case-insensitive
substring match, `name ILIKE '%' || $2 || '%' OR description ILIKE '%' ||
$2 || '%'`. There is no `tsvector` column on `projects` and no GIN index.

## 8. The enum types

`migrations/0003_projects_and_tasks.sql` creates two PostgreSQL enum types.

```sql
CREATE TYPE task_status AS ENUM ('open', 'in_progress', 'done');
CREATE TYPE task_priority AS ENUM ('low', 'medium', 'high', 'urgent');
```

| Type | Values | Used by |
| ---- | ------ | ------- |
| `task_status` | `open`, `in_progress`, `done` | `tasks.status` |
| `task_priority` | `low`, `medium`, `high`, `urgent` | `tasks.priority` |

- The database refuses any other value, so a bug in the application cannot
  write an out-of-contract state.
- The Rust side maps the variants one to one, with
  `#[sqlx(type_name = "task_status", rename_all = "snake_case")]`, so the
  wire names, the database values and the JSON values are the same strings.
- The wire names are the same in the JSON body and in the query string:
  `in_progress`, never `inProgress`.

Adding a value to an enum type in PostgreSQL is done with
`ALTER TYPE ... ADD VALUE`, which cannot run inside a transaction block in
older PostgreSQL versions. A migration that changes an enum must therefore be
written with that in mind, and the policy in
[`migration-policy.md`](migration-policy.md) applies.

## 9. The ownership model

- Every table that holds user data has a **non-null** foreign key to its
  owner. There is no nullable owner and no shared row.
- The chain is `users → notes`, `users → projects`,
  `users → refresh_sessions` and `users → projects → tasks`.
- **The `tasks` table has no `user_id` column.** A task belongs to a
  project, and a project belongs to a user. The authorization of a task is
  therefore a join with `projects` in every query, and the task feature
  writes that join explicitly so that the predicate is visible.
- **Deletes cascade to the owned data.** Deleting a user removes their
  sessions, their notes and their projects, and through the projects their
  tasks. There is no orphaned row and no soft delete of the account.
- **The `ON DELETE SET NULL` on `refresh_sessions.replaced_by_session_id`**
  is the one exception: the rotation link is history, not ownership, so
  removing a session must not remove its successor.
- **Ownership is enforced in the query, not in the handler.** Every
  statement that touches user data carries the caller's `user_id`. The
  decision is recorded in
  [`ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md).

## 10. Types and conventions used throughout

| Convention | Choice | Reason |
| ---------- | ------ | ------ |
| Primary keys | `UUID`, generated by the application | No sequential identifier is ever exposed, so an identifier cannot be enumerated. |
| Timestamps | `TIMESTAMPTZ` | A timestamp is an instant. The API renders it as RFC 3339 UTC. |
| Text | `TEXT` with an explicit `CHECK` for the length | The limit is visible in the schema and is enforced by the database, not only by the application. |
| Booleans | `BOOLEAN NOT NULL DEFAULT …` | No nullable flag, so a row cannot be in an undefined state. |
| Ownership | A non-null foreign key with `ON DELETE CASCADE` | Owned data cannot outlive its owner. |
| Controlled state | A native `ENUM` type | The database refuses an out-of-contract value. |
| Free text | A `tsvector` generated column with a GIN index | Search over the user's own data without a separate search service. |

## 11. Phase 2 (Proposed, except what has been built)

> **Status:** **Proposed**, except for the seven tables four migrations already
> create. `expense_claims` and `expense_items` are served by the twelve claim
> operations of chains `R2`-`R4` (§11.1). `companies`, `job_applications` and
> `application_events` are created by `migrations/0005_job_applications.sql`
> (§11.3-§11.5) and served by the fourteen operations of chains `J2`-`J4`: five
> company, five application and four event routes, with the stage machine and
> the two composite ownership keys proved by `tests/jobs_schema.rs` and
> `tests/job_applications_api.rs`.
> [`../project-truth.md`](../project-truth.md) §18. This section records the
> schema those requirements imply. Nothing in this section is `Verified` by this
> document; the API half is covered by the tests the chain records, not here.
>
> `migrations/0004_expense_claims.sql` creates `expense_claim_status`,
> `expense_claims` and `expense_items` with every constraint and index named in
> §11.1 and §11.2. Chains `R2`-`R4` serve all twelve operations: claims are
> listed, created, read, updated and deleted; items are added, changed and
> deleted; and the four lifecycle actions change a claim's `status` and the
> timestamps the lifecycle `CHECK` requires. `currency` and `total_amount` are
> still changed by nothing a client sends: the first is in no `SET` clause, the
> second is computed from `SUM(amount)` inside the item transaction.
>
> `migrations/0005_job_applications.sql` creates `job_application_status`,
> `application_event_kind`, `companies`, `job_applications` and
> `application_events` with every constraint and index named in §11.3-§11.5,
> including the two composite foreign keys and the functional unique index on
> `(user_id, LOWER(name))`. `src/job_applications/repository.rs` holds every
> statement that reads and writes them, so the indexes below are the published
> access paths, not a measurement of a running query.
>
> `migrations/0006_embeddings.sql` creates `embeddings` with the three `CHECK`s
> and the two indexes named in §11.6, and nothing else: no enum type, no
> extension, and no foreign key on `source_id`. The table's Rust side exists:
> `src/ml/repository.rs` holds the statements that read and write it,
> `src/ml/service.rs` and `src/ml/handlers.rs` serve the three routes, and the
> delete statements in `src/notes/repository.rs` and
> `src/job_applications/repository.rs` purge a source's embeddings in the same
> statement that deletes it (§11.6). The migration was applied from `0001` to a
> clean scratch database and its constraints and indexes were exercised; the
> access-path measurements quoted in §11.6 are recorded evidence for those two
> indexes, not a claim that a running endpoint uses them today.
>
> `migrations/0007_api_keys.sql` creates `api_keys` with the three `CHECK`s, the
> unique digest constraint and the owner index named in §11.7, and nothing else:
> no enum type, no extension, no child table and no trigger. The table stores a
> digest and a display prefix, never the secret. It was applied from `0001` to a
> clean scratch database and every constraint was exercised — a duplicate
> digest, both halves of the prefix grammar, a short and an upper-case digest,
> both name bounds, the two `NULL`-defaulted timestamps and the account
> cascade — before this section was written.

Six provisional assumptions underlie this section: **A1** (a claim has one
currency and there is no FX), **A2** (a receipt is a reference), **A3**
(applications and events, with no generated text), **A4** (ML runs locally),
**A5** (single owner, no sharing) and **A6** (money is `NUMERIC(19,4)`, a JSON
string with four decimals). All six are **provisional defaults, owner decision
O1**; they were never confirmed by the owner. A1, A5 and A6 shape the columns
most directly, and A4 shapes §11.6. A revision of any of them changes this
section, and for §11.1-§11.7 it needs a new migration as well because `0004`,
`0005`, `0006` and `0007` are applied; the requirements in
[`../project-truth.md`](../project-truth.md) §18.4 say how.

The design follows the conventions of §10 and the rules of
[`migration-policy.md`](migration-policy.md). Where a rule cannot be expressed
as a constraint, the section says so and names the test that covers it.

```text
users ─┬─► expense_claims ──► expense_items
       │        └─(project_id, SET NULL)──► projects
       ├─► companies ──► job_applications ──► application_events
       │                      └─(next_action_task_id, SET NULL)──► tasks
       ├─► api_keys
       └─► embeddings        (source_id is polymorphic: note | application)
```

### 11.1 `expense_claims`

One row per reimbursement claim. It is the aggregate root: the currency and
the total live here, the amounts live in `expense_items`.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key, generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. The owner. |
| `title` | `TEXT` | no | — | Trimmed, 1 to 200 characters. |
| `description` | `TEXT` | no | `''` | At most 5 000 characters. Optional at the API, `''` in the database, exactly like `notes.content`. |
| `currency` | `CHAR(3)` | no | — | ISO 4217, three upper-case letters. Immutable after creation (`REIMB-R02`). |
| `total_amount` | `NUMERIC(19,4)` | no | `0` | Never negative, and always equal to the sum of the items (§11.9). |
| `status` | `expense_claim_status` | no | `'draft'` | The five-state lifecycle (`REIMB-R04`). |
| `project_id` | `UUID` | yes | `NULL` | `REFERENCES projects (id) ON DELETE SET NULL`. Optional link; the project must belong to the same user (`REIMB-R09`). |
| `submitted_at` | `TIMESTAMPTZ` | yes | `NULL` | Set exactly when the status requires it. |
| `decided_at` | `TIMESTAMPTZ` | yes | `NULL` | The approve or reject instant. |
| `paid_at` | `TIMESTAMPTZ` | yes | `NULL` | The payment instant. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | |

#### Constraints

| Name | Definition |
| ---- | ---------- |
| `expense_claims_title_length_chk` | `CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200)` |
| `expense_claims_description_length_chk` | `CHECK (CHAR_LENGTH(description) <= 5000)` |
| `expense_claims_currency_chk` | `CHECK (currency ~ '^[A-Z]{3}$')` |
| `expense_claims_total_amount_chk` | `CHECK (total_amount >= 0)` |
| `expense_claims_lifecycle_timestamps_chk` | Ties each timestamp to the status. See below. |
| `expense_claims_timestamp_order_chk` | `CHECK ((decided_at IS NULL OR (submitted_at IS NOT NULL AND decided_at >= submitted_at)) AND (paid_at IS NULL OR (decided_at IS NOT NULL AND paid_at >= decided_at)))` |

Two different mechanisms police `currency`, and a test that asserts the wrong one
will fail:

- **The type rejects an over-long code, before any `CHECK` runs.** `CHAR(3)`
  raises SQLSTATE `22001` (`value too long for type character(3)`) on a
  four-character value; the row never reaches `expense_claims_currency_chk`.
  Measured on PostgreSQL 18.1 and recorded in the `C-R1` checkpoint. An earlier
  wording of this section credited the `CHECK` with that case, which was wrong:
  the `CHECK` cannot see a value the type refused. The migration's own comment
  says the same thing — `CHAR(3)` is chosen "so that an over-long code is a
  length error at the type and not only in the constraint below".
- **The `CHECK` rejects the two cases the type accepts.** A lower-case code
  (`usd`) and a two-character code (`US`, stored blank-padded as `US `) both fit
  `CHAR(3)`; both fail `^[A-Z]{3}$` with SQLSTATE `23514` naming
  `expense_claims_currency_chk`. Upper case is the only guarantee the type
  cannot give, which is why the constraint stays.

Either way the value is unrepresentable, and the API reaches neither mechanism:
the service trims, upper-cases and length-checks a currency and returns `422`
before a statement runs.

The lifecycle constraint is the direct analogue of
`tasks_completion_consistency_chk`: it makes "a submitted claim without a
submission date" and "a paid claim without a decision" impossible to store,
whatever writes the row.

```sql
ALTER TABLE expense_claims
    ADD CONSTRAINT expense_claims_lifecycle_timestamps_chk CHECK (
        (status = 'draft'
            AND submitted_at IS NULL AND decided_at IS NULL AND paid_at IS NULL)
     OR (status = 'submitted'
            AND submitted_at IS NOT NULL AND decided_at IS NULL AND paid_at IS NULL)
     OR (status IN ('approved', 'rejected')
            AND submitted_at IS NOT NULL AND decided_at IS NOT NULL AND paid_at IS NULL)
     OR (status = 'paid'
            AND submitted_at IS NOT NULL AND decided_at IS NOT NULL AND paid_at IS NOT NULL)
    );
```

`REIMB-R05` (submission needs at least one item and a total above zero) is
**not** a `CHECK`: it reads another table. It is enforced by the service in the
same transaction as the status change, and covered by tests.

The `rejected → draft` reopen clears `submitted_at` **and** `decided_at`;
`paid_at` is already `NULL`, because a rejected claim never had one. The
`draft` arm of the constraint above requires all three to be null, so the API
contract (`POST .../submit` on a rejected claim reopens without submitting) and
the schema cannot be read as disagreeing.

`currency` is immutable (`REIMB-R02`). The mechanism is **not** a rejected
request: `currency` is absent from `UpdateClaimRequest`, so a `PATCH` body that
carries it — alone or with other fields — is accepted with `200` and the field
is ignored
([`../features/reimbursements/api.md`](../features/reimbursements/api.md) §6).
That is the stronger form of the rule, not a weaker one: the column appears in
no `SET` clause of any statement, so `currency` has no write path at all rather
than one that is guarded. What is dropped is the *error*, not the invariant.

The earlier wording of this paragraph — "that is a service rule with a `422` and
a test" — was the proposal. The published contract resolved it the other way,
because producing a `422` would require **adding** `currency` to the request
type only to reject it, which contradicts "they are absent from the request
contract", and `#[serde(deny_unknown_fields)]` would turn every unknown key into
a `422`, a breaking change for any client that sends one extra field. The
disagreement and its resolution are recorded in
[`../architecture/reimbursements-crud-design.md`](../architecture/reimbursements-crud-design.md)
§9.1. No trigger is added for the original reason: the transition rules and the
total already live in the service, and a `BEFORE UPDATE` trigger comparing
`OLD.currency` with `NEW.currency` would be a second, partial copy of them. It
remains the database-level alternative if a second writer ever appears.

#### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `expense_claims_user_id_status_created_at_idx` | `INDEX (user_id, status, created_at DESC)` | The filtered list: the owner's claims in one status, newest first. |
| `expense_claims_user_id_created_at_idx` | `INDEX (user_id, created_at DESC)` | The unfiltered list. |
| `expense_claims_project_id_idx` | `INDEX (project_id) WHERE project_id IS NOT NULL` | The claims attached to one project, and the lookup the `ON DELETE SET NULL` maintenance performs when a project is deleted. Partial because most claims have no project. |
| `expense_claims_id_user_id_unique_idx` | `UNIQUE INDEX (id, user_id)` | Not a second key: `id` is already unique. It exists because PostgreSQL requires a unique constraint or index on exactly the columns a composite foreign key references, and `expense_items` references `(id, user_id)` (§11.2). |

The feature also filters by `currency`, by a `submitted_at` range and by a
title/description search. `currency` and the date bounds are optional predicates
of the form `($n IS NULL OR column = $n)` in the same statement, which the
planner cannot turn into an index seek; they are applied to the rows the chosen
listing index has already narrowed to the owner. **No index leads on `currency`,
and none is worth adding**: the predicate is optional, so an index on the column
alone could not serve the statement, and neither owner-scoped listing index leads
on `currency`. A separate index would therefore be written on every insert while
every statement could still only filter on it after the fact — write cost without
a read, which is the same reason the `submitted_at` bounds have no index either.
The `status` filter is different: it is not one of those optional predicates but
the choice of index, which is why the repository holds two listing statement
shapes, one per index
([`../architecture/reimbursements-crud-design.md`](../architecture/reimbursements-crud-design.md)
§6.6). The search is an `ILIKE`, as in the Phase 1 project listing, so there is
no `tsvector` column and no trigram index.

#### Ownership and deletion

`user_id` is the owner and cascades: deleting an account deletes its claims and,
through the claim, its items. `project_id` is `ON DELETE SET NULL`: deleting the
project keeps the claim and clears the link, which is the accepted behaviour of
`REIMB-R09`, and it is why the claim survives a project delete that removes the
project's tasks.

The database cannot express "the project named by `project_id` belongs to
`user_id`" with this foreign key. A composite key
`(project_id, user_id) → projects (id, user_id)` would express it, but
`ON DELETE SET NULL` would then try to null the required `user_id`; PostgreSQL 15
added `ON DELETE SET NULL (project_id)` for exactly this, and the project's
documented minimum is PostgreSQL 14. The owner check is therefore a `WHERE
EXISTS (SELECT 1 FROM projects WHERE id = $n AND user_id = $1)` in the write
statement, it returns `404` for a foreign project, and a test asserts it. A
deployment that raises the floor to 15 can add the composite key in a new
migration.

### 11.2 `expense_items`

One row per line item. An item has no currency: it is denominated in its
claim's currency (`REIMB-R02`).

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key, generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. Must equal the claim's owner; see the composite key below. |
| `claim_id` | `UUID` | no | — | Part of `FOREIGN KEY (claim_id, user_id) REFERENCES expense_claims (id, user_id) ON DELETE CASCADE`. |
| `description` | `TEXT` | no | — | Trimmed, 1 to 500 characters. |
| `amount` | `NUMERIC(19,4)` | no | — | Never negative. |
| `incurred_on` | `DATE` | no | — | A calendar date, not an instant, which is why it is `DATE` and not `TIMESTAMPTZ`. |
| `category` | `TEXT` | yes | `NULL` | A lower-case token, 1 to 64 characters, when present. `REIMB-R01` makes it optional. |
| `receipt_url` | `TEXT` | yes | `NULL` | A client-supplied reference, at most 2 048 characters, `http` or `https` (`REIMB-R08`). See the placement note below. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | |

#### Constraints

| Name | Definition |
| ---- | ---------- |
| `expense_items_claim_fk` | `FOREIGN KEY (claim_id, user_id) REFERENCES expense_claims (id, user_id) ON DELETE CASCADE` |
| `expense_items_user_fk` | `user_id REFERENCES users (id) ON DELETE CASCADE` |
| `expense_items_description_length_chk` | `CHECK (CHAR_LENGTH(description) BETWEEN 1 AND 500)` |
| `expense_items_amount_chk` | `CHECK (amount >= 0)` |
| `expense_items_category_chk` | `CHECK (category IS NULL OR (category = LOWER(BTRIM(category)) AND CHAR_LENGTH(category) BETWEEN 1 AND 64))` |
| `expense_items_receipt_url_chk` | `CHECK (receipt_url IS NULL OR (CHAR_LENGTH(receipt_url) BETWEEN 1 AND 2048 AND receipt_url ~* '^https?://'))` |

The composite foreign key is the strongest constraint in this section: an item
whose `user_id` disagrees with its claim's owner **cannot be inserted**, by any
code path. The service still checks first, so the client receives `404` rather
than a `23503`; the test asserts the response, the key asserts the data.

`REIMB-R08` requires a receipt reference but does not say which table carries
it. This proposal puts it on the line item, because one claim can hold several
receipts, and that is where `0004` put it; moving it to the claim is a new
migration, not an edit to this document alone.

#### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `expense_items_claim_id_idx` | `INDEX (claim_id)` | The claim detail read, the `SUM(amount)` that maintains the total, and the leading column of the composite key's cascade lookup. |

The item list is ordered by `incurred_on, created_at, id` and is bounded by the
claim's item count, so the sort happens over the rows the `claim_id` index
already found. There is no `(claim_id, incurred_on)` index: it would repeat the
leading column to save a sort over a handful of rows.

#### Ownership and deletion

Deleting a claim deletes its items in the same statement, through the composite
key's `ON DELETE CASCADE`. Deleting the account also removes the items, through
the direct `users` key. There is no path that leaves an item without its claim,
and no path that lets an item and its claim disagree about the owner.

### 11.3 `companies`

One row per employer, owned by one user.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key, generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. The owner. |
| `name` | `TEXT` | no | — | Trimmed, 1 to 200 characters. Unique per owner, case-insensitively (`JOB-R01`). |
| `website` | `TEXT` | yes | `NULL` | At most 2 048 characters. The service validates the form; the database bounds the length. |
| `notes` | `TEXT` | no | `''` | At most 5 000 characters. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | |

#### Constraints

| Name | Definition |
| ---- | ---------- |
| `companies_name_length_chk` | `CHECK (CHAR_LENGTH(name) BETWEEN 1 AND 200)` |
| `companies_name_trimmed_chk` | `CHECK (name = BTRIM(name))` |
| `companies_website_chk` | `CHECK (website IS NULL OR (CHAR_LENGTH(website) <= 2048 AND website ~* '^https?://'))` |
| `companies_notes_length_chk` | `CHECK (CHAR_LENGTH(notes) <= 5000)` |

Uniqueness is a **functional unique index**:

```sql
CREATE UNIQUE INDEX companies_user_id_lower_name_unique_idx
    ON companies (user_id, LOWER(name));
```

This is deliberately different from `users_email_unique_idx`, which lives on a
normalized column. The email address has no display form worth preserving, so
the application lower-cases it and `users_email_normalized_chk` enforces that.
A company name *is* a display form, so the stored value keeps its casing and
the index compares the folded value. The trim check is what stops
`"Acme "` from being a second `"Acme"`.

#### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `companies_user_id_lower_name_unique_idx` | `UNIQUE INDEX (user_id, LOWER(name))` | The `409` on a duplicate name, and the owner-scoped predicate every statement starts with. |
| `companies_user_id_lower_name_name_id_idx` | `INDEX (user_id, LOWER(name), name, id)` | The company list: `ORDER BY LOWER(name), name, id` (§16.3 and §16.4 of [`../api/conventions.md`](../api/conventions.md)), which this index provides without a sort. The unique index cannot serve it, because a unique index on `(user_id, LOWER(name))` has no `name` or `id` column to break the tie. |
| `companies_id_user_id_unique_idx` | `UNIQUE INDEX (id, user_id)` | The referenced key for `job_applications`. |

The list also accepts a `search` term. That is an `ILIKE` over the owner's
companies, the same technique as the Phase 1 project listing, not a full-text
index; the owner's company count is small and the
`(user_id, LOWER(name), name, id)` index supplies the published order.

#### Ownership and deletion

A company belongs to one user and is deleted with the account. A company that
still has applications **cannot** be deleted: `JOB-R06` makes that a `409`, and
§11.4 enforces it with `ON DELETE NO ACTION` on the applications' side. The
service refuses with `409` **without deleting anything**: one statement carries
`AND NOT EXISTS (SELECT 1 FROM job_applications WHERE company_id = $1)`, and a
zero-row result is the `409`. It never deletes the applications to make itself
succeed - that would destroy records the client did not ask to delete.

### 11.4 `job_applications`

One row per role the owner is pursuing at a company.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key, generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. The owner. |
| `company_id` | `UUID` | no | — | Part of the composite key to `companies (id, user_id)`; the company must have the same owner. |
| `role_title` | `TEXT` | no | — | Trimmed, 1 to 200 characters. |
| `source` | `TEXT` | yes | `NULL` | Where the lead came from, at most 64 characters. A label, not a controlled value. |
| `status` | `job_application_status` | no | `'wishlist'` | The seven-stage machine (`JOB-R04`). |
| `applied_on` | `DATE` | yes | `NULL` | A calendar date. `JOB-R02` makes it optional. |
| `salary_min` | `NUMERIC(19,4)` | yes | `NULL` | |
| `salary_max` | `NUMERIC(19,4)` | yes | `NULL` | |
| `salary_currency` | `CHAR(3)` | yes | `NULL` | ISO 4217; required whenever either bound is present (`JOB-R03`). |
| `job_url` | `TEXT` | yes | `NULL` | At most 2 048 characters. |
| `description` | `TEXT` | no | `''` | At most 10 000 characters. |
| `next_action_task_id` | `UUID` | yes | `NULL` | `REFERENCES tasks (id) ON DELETE SET NULL`. The task must belong to the same user (`JOB-R07`). |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | |

#### Constraints

| Name | Definition |
| ---- | ---------- |
| `job_applications_company_fk` | `FOREIGN KEY (company_id, user_id) REFERENCES companies (id, user_id) ON DELETE NO ACTION` |
| `job_applications_next_action_task_fk` | `next_action_task_id REFERENCES tasks (id) ON DELETE SET NULL` |
| `job_applications_role_title_length_chk` | `CHECK (CHAR_LENGTH(role_title) BETWEEN 1 AND 200)` |
| `job_applications_source_chk` | `CHECK (source IS NULL OR (source = BTRIM(source) AND CHAR_LENGTH(source) BETWEEN 1 AND 64))` |
| `job_applications_description_length_chk` | `CHECK (CHAR_LENGTH(description) <= 10000)` |
| `job_applications_job_url_chk` | `CHECK (job_url IS NULL OR (CHAR_LENGTH(job_url) <= 2048 AND job_url ~* '^https?://'))` |
| `job_applications_salary_non_negative_chk` | `CHECK ((salary_min IS NULL OR salary_min >= 0) AND (salary_max IS NULL OR salary_max >= 0))` |
| `job_applications_salary_range_chk` | `CHECK (salary_min IS NULL OR salary_max IS NULL OR salary_min <= salary_max)` |
| `job_applications_salary_currency_required_chk` | `CHECK ((salary_min IS NULL AND salary_max IS NULL AND salary_currency IS NULL) OR ((salary_min IS NOT NULL OR salary_max IS NOT NULL) AND salary_currency IS NOT NULL))` |
| `job_applications_salary_currency_chk` | `CHECK (salary_currency IS NULL OR salary_currency ~ '^[A-Z]{3}$')` |

`NO ACTION` rather than `RESTRICT` is deliberate. Both refuse a direct
`DELETE FROM companies` while applications remain, which is the `409` of
`JOB-R06`. Only `NO ACTION` defers the check to the end of the statement, and
that is what lets `DELETE FROM users` succeed: the account's cascade removes
its applications and its companies in the same statement, so nothing is left to
object.

The seven-stage machine is **not** a `CHECK`. A `CHECK` sees one row and cannot
see the previous status, so `wishlist → interview` cannot be expressed there.
The enum makes an unknown stage unrepresentable; the transition table lives in
the service and every allowed and forbidden edge has a test (`JOB-R04`).

`next_action_task_id` cannot be owner-consistent in SQL. `tasks` has no
`user_id` (§6), so a composite key has nothing to reference; the service checks
`tasks → projects → user_id` before writing and returns `404` for a foreign
task. This is the one ownership link in Phase 2 that the database cannot
enforce on its own, and the test is the evidence.

#### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `job_applications_user_id_status_created_at_idx` | `INDEX (user_id, status, created_at DESC)` | The board view: the owner's applications in one stage, newest first. |
| `job_applications_user_id_created_at_idx` | `INDEX (user_id, created_at DESC)` | The unfiltered list. |
| `job_applications_company_id_idx` | `INDEX (company_id)` | The company's applications, and the lookup the `NO ACTION` check performs when a company is deleted. |
| `job_applications_next_action_task_id_idx` | `INDEX (next_action_task_id) WHERE next_action_task_id IS NOT NULL` | The lookup the `ON DELETE SET NULL` maintenance performs when a task is deleted. Partial because most rows have no next action. |
| `job_applications_id_user_id_unique_idx` | `UNIQUE INDEX (id, user_id)` | The referenced key for `application_events`. |

The list also filters by company, by an `applied_on` range and by a search
term. The stage filter has its own index; the others are applied to the rows
`(user_id, created_at DESC)` has already narrowed. A composite
`(user_id, company_id, created_at DESC)` was considered and is not proposed: the
company page is served by `company_id`, and one owner's application list is
bounded, so the extra index would add a write cost to save a scan over a handful
of rows. It is the first index to add if that scan ever becomes visible.

#### Ownership and deletion

The application belongs to one user and is deleted with the account. Its
company must be the same owner, enforced by the composite key. Its next-action
task is cleared when the task is deleted, and the application survives. Deleting
an application deletes its events (§11.5).

### 11.5 `application_events`

One dated entry in an application's timeline. It has **no `updated_at`**: a
timeline entry records what the owner says happened, not when the row was last
written. `kind`, `occurred_at`, `summary` and `details` are corrected in place
by `PATCH /api/v1/job-applications/{application_id}/events/{event_id}`, the
locked decision `A-D1-1`; `created_at` stays the row's creation instant, and
`occurred_at` is the event's own time.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key, generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. Must equal the application's owner. |
| `application_id` | `UUID` | no | — | Part of the composite key to `job_applications (id, user_id)`. |
| `kind` | `application_event_kind` | no | — | `note`, `email`, `call`, `interview`, `offer`, `rejection`. |
| `occurred_at` | `TIMESTAMPTZ` | no | — | Required. A future value is accepted, because an interview can be scheduled (`JOB-R05`). |
| `summary` | `TEXT` | no | — | Trimmed, 1 to 200 characters. |
| `details` | `TEXT` | no | `''` | At most 10 000 characters. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |

#### Constraints

| Name | Definition |
| ---- | ---------- |
| `application_events_application_fk` | `FOREIGN KEY (application_id, user_id) REFERENCES job_applications (id, user_id) ON DELETE CASCADE` |
| `application_events_user_fk` | `user_id REFERENCES users (id) ON DELETE CASCADE` |
| `application_events_summary_length_chk` | `CHECK (CHAR_LENGTH(summary) BETWEEN 1 AND 200)` |
| `application_events_details_length_chk` | `CHECK (CHAR_LENGTH(details) <= 10000)` |

#### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `application_events_application_id_occurred_at_idx` | `INDEX (application_id, occurred_at DESC)` | The timeline, newest first. The query adds `id DESC` as the total-order tiebreaker required by `JOB-R05`; the index provides the leading order. |

#### Ownership and deletion

An event belongs to one user and to one application, and the composite key
makes the two owners agree. Deleting the application deletes its events in the
same statement; deleting the account deletes them through either path. There is
no orphaned event.

### 11.6 `embeddings`

One vector per source, model and owner. The table is created by
`migrations/0006_embeddings.sql`; the migration is the enforced copy of this
section.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key, generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. The owner. |
| `source_type` | `TEXT` | no | — | `note` or `application`, constrained by a `CHECK`. |
| `source_id` | `UUID` | no | — | The id of the note or the application. **No foreign key**: see below. |
| `model_id` | `TEXT` | no | — | The model that produced the vector, including its version, 1 to 200 characters. |
| `dim` | `INTEGER` | no | — | The declared dimension, greater than zero. |
| `vector` | `REAL[]` | no | — | The embedding. Its length must equal `dim`. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `updated_at` | `TIMESTAMPTZ` | no | `NOW()` | Set when a reindex replaces the vector for the same source and model. |

#### Constraints

| Name | Definition |
| ---- | ---------- |
| `embeddings_source_type_chk` | `CHECK (source_type IN ('note', 'application'))` |
| `embeddings_model_id_length_chk` | `CHECK (CHAR_LENGTH(model_id) BETWEEN 1 AND 200)` |
| `embeddings_dimension_chk` | `CHECK (dim > 0 AND dim = cardinality(vector))` |

`cardinality` counts the elements of an array and returns `0` for an empty one,
so an empty vector fails for every positive `dim`. `array_length(vector, 1)`
would return `NULL` instead, and a `CHECK` passes when its expression is
`NULL`; `cardinality` is the function that makes the rule total.

`dim` is stored, not inferred, so that a reader can reject a candidate set from
a different model without inspecting every array, and so that the check is a
column comparison rather than an array length computed per row.

#### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `embeddings_user_id_source_model_unique_idx` | `UNIQUE INDEX (user_id, source_type, source_id, model_id)` | The re-embed conflict target (`ML-R04`, `ML-R07`), the delete-by-source lookup, and any owner-scoped scan: `user_id` is the leading column, so a query that names another owner cannot use it to read this owner's rows. |
| `embeddings_user_id_source_type_model_id_idx` | `INDEX (user_id, source_type, model_id)` | The search pipeline filters the caller's rows by source type and model id before it reads the vectors (`ML-R05`). The unique index cannot seek on `model_id` before `source_id`, so this is a different access path, not a duplicate. It is the plan the semantic scan uses up to about 4 000 note vectors per owner; above that the planner scans the table instead, because the `vector` column is in neither index (*Measured access paths* below, and [`performance.md`](performance.md) §4). |

`(user_id, source_type)` is a prefix of both indexes. A lookup by `source_id`
alone is always written with `user_id` in front of it, because every statement
is owner-scoped.

**Measured access paths.** Two measurements exist, and they disagree about the
**plan**, because they were taken at different vector dimensions. This section
records both; the second is authoritative for what the feature costs.

The first was taken on a scratch database created from `0001`-`0006`
(PostgreSQL 18.1), filled with 40 000 synthetic rows of `dim = 2` for one owner
and analysed:

| Statement shape | Plan | Buffers | Duration |
| --------------- | ---- | ------- | -------- |
| The search pipeline: `user_id`, `source_type`, `model_id` and `source_id = ANY($3)` | `Index Scan using embeddings_user_id_source_model_unique_idx` | 4 shared | 0.07-0.13 ms |
| The semantic scan: `user_id`, `source_type`, `model_id`, no `source_id` | `Bitmap Index Scan on embeddings_user_id_source_type_model_id_idx` | 17 index of 834 | 6.3-12.8 ms for 20 001 rows |
| The reindex skip check: the full unique tuple | `Index Scan using embeddings_user_id_source_model_unique_idx` | 4 shared | 0.03-0.05 ms |
| The purge in the source delete: `user_id`, `source_type`, `source_id` | `Index Scan using embeddings_user_id_source_model_unique_idx`, the three-column prefix | 6 shared | 0.05-0.06 ms |

The second measurement is [`performance.md`](performance.md) §3.5: 20 000 rows at
`dim = 384`, the dimension the model actually produces, split between two owners,
`work_mem = 4 MB`. The heap is **39 MB**, not the few hundred kilobytes that
`dim = 2` produces, and that is what changes the plan:

| Statement | `dim = 2`, 40 000 rows | `dim = 384`, 20 000 rows |
| --------- | ---------------------- | ------------------------ |
| The semantic scan | `Bitmap Index Scan`, 17 index of 834 buffers, 6.3-12.8 ms for 20 001 rows | **`Parallel Seq Scan` + `external merge` sort**, 5 387-6 135 shared + 983-1 573 temp buffers, **177-292 ms** for 8 100 live rows |
| The search pipeline (`source_id = ANY(…)`) | `Index Scan`, 4 buffers | Not re-measured; it reads at most `ml.candidate_limit` rows by key, so the heap size does not change its shape |
| The reindex skip check | `Index Scan`, 4 buffers | Not re-measured; keyed by the full unique tuple |
| The purge in the source delete | `Index Scan`, 6 buffers | Not re-measured; keyed by the three-column prefix |

**The `dim = 2` figure does not generalise, and this section used to say it
did.** The `vector` column is in neither index, so a scan that uses
`embeddings_user_id_source_type_model_id_idx` must then fetch every matching row
from the heap — at `dim = 2` that heap is tiny and the bitmap plan wins; at
`dim = 384` it is 39 MB of array data and the planner prefers one sequential
pass. The transition is visible in [`performance.md`](performance.md) §4: the
bitmap scan is still chosen at **3 000** note vectors per owner and is **not** at
**4 000**. The table above is a measurement of a statement shape; it is not a
measurement of what semantic search costs.

Three honest qualifications:

1. On the empty table the planner chooses a sequential scan, because a handful of
   rows are cheaper to read than an index. The plans above are what the same
   statements become once there are rows to find.
2. At `dim = 384`, from about 4 000 note vectors per owner upward, the semantic
   scan is a **sequential scan of the whole `embeddings` table** and returns only
   the caller's rows. It reads other owners' rows physically; it never returns
   one. That distinction is a performance property, not an ownership defect —
   the predicate is in the statement, and [`performance.md`](performance.md)
   §5.2 records the control query that proves it.
3. The semantic scan's **first** cliff is not the index but the `ORDER BY` in
   `NOTE_VECTORS`: the sort spills out of `work_mem` at about 2 700 vectors per
   owner and the statement goes from 14-16 ms to 75-123 ms. That is a server
   setting, not a schema property; the operational note is
   [`../architecture/deployment.md`](../architecture/deployment.md) §7.1.

The cost of the semantic scan, and the volume up to which it is accepted, are
[`ADR-0010`](../decisions/ADR-0010-semantic-search-scan-bound.md). The table
above exists to support it, not to replace it.

The two indexes are not duplicates of each other. The unique index is the
arbiter for `ON CONFLICT (user_id, source_type, source_id, model_id)` and serves
every lookup that names a source; the three-column index is the one the semantic
scan uses **while it is chosen**, because it can seek on `model_id` where the
unique index cannot. A query with no `source_id` predicate — semantic mode — is
the statement that needs the second one: below the threshold it is the access
path, and dropping it would turn that scan into a filter over every embedding the
owner has. Above the threshold the planner already scans the table, so the index
buys nothing for that statement there — it is 160 kB and it is still the right
index to keep, but it is not what makes the volume scan cheap.

#### Ownership and deletion

The row belongs to one user and is deleted with the account. Deleting a note or
an application deletes its embeddings in the same transaction, because there is
no foreign key to do it; the reindex is the backstop for a source that was
removed by a path that did not know about embeddings. There is no path that
moves an embedding to another owner: `user_id` is never updated.

#### The polymorphic source, and its price

`source_id` points at either `notes` or `job_applications`, chosen by
`source_type`. PostgreSQL cannot express a foreign key to two tables, so the
column carries **no foreign key at all**. That is the trade-off:

| What is lost | What replaces it |
| ------------ | ---------------- |
| The database cannot reject an embedding whose source does not exist. | The writer inserts the embedding in the **same transaction** as the source row, and deletes it in the same transaction as the source delete. A failure in either rolls both back. |
| A stale `source_id` could survive a delete that bypassed the service. | A reindex job deletes embeddings whose source is gone, for the owner it is reindexing (`ML-R07` is owner-scoped and idempotent). |
| A `source_id` could name another owner's row. | The write path checks the source with `WHERE id = $ AND user_id = $1` before inserting, and every read filters `user_id` first, so a row that somehow pointed across owners is never returned to the caller. |

The choice is recorded in [`ADR-0008`](../decisions/ADR-0008-local-pure-rust-ml.md),
indexed in [`../decisions/README.md`](../decisions/README.md), and `ML-R08` is
the requirement that makes the read-side predicate mandatory.

### 11.7 `api_keys`

One row per long-lived credential a machine client holds. The table is created
by `migrations/0007_api_keys.sql`; the migration is the enforced copy of this
section.

The table exists because the Phase 1 credential cannot serve an unattended
client. An access token expires in fifteen minutes, and the refresh token
rotates with reuse detection, so a workflow (today: n8n) that stores either one
either stops working on rotation or trips the reuse alarm and revokes the
account's sessions. A key is the credential that survives being used by a
machine. It is **not** a second account: a key is an authority derived from one
account, `user_id` is its owner, and every statement that reads or writes it —
including the authentication lookup's follow-up — carries `user_id` in the
`WHERE` clause. A key never moves between owners.

| Column | Type | Null | Default | Notes |
| ------ | ---- | ---- | ------- | ----- |
| `id` | `UUID` | no | — | Primary key, generated by the application. |
| `user_id` | `UUID` | no | — | `REFERENCES users (id) ON DELETE CASCADE`. The owner. |
| `name` | `TEXT` | no | — | Owner-chosen label, 1 to 100 characters. Not unique: two keys may share a label, and the id tells them apart. |
| `prefix` | `TEXT` | no | — | The display half of the secret: `atom_` plus 8 lower-case hex characters. Not an identity, and deliberately **not** unique. |
| `key_hash` | `TEXT` | no | — | SHA-256 hex digest of the secret, 64 lower-case hex characters. Unique. **Not Argon2** — see below. |
| `last_used_at` | `TIMESTAMPTZ` | yes | `NULL` | The last authenticated use. Written at most once an hour per key (`K1` I5); the database stores the instant and does not throttle. |
| `created_at` | `TIMESTAMPTZ` | no | `NOW()` | |
| `revoked_at` | `TIMESTAMPTZ` | yes | `NULL` | `NULL` means active. Set once, never cleared. |

There is no `updated_at`, unlike the other Phase 2 tables. The row is immutable
apart from the two stamps above, and the only writer that touches it after
creation is the hourly `last_used_at` touch and the revocation; an `updated_at`
would be rewritten by exactly those statements and would say nothing they do not
already record.

#### Constraints

| Name | Definition |
| ---- | ---------- |
| `api_keys_user_id_fkey` | `FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE` |
| `api_keys_name_length_chk` | `CHECK (CHAR_LENGTH(name) BETWEEN 1 AND 100)` |
| `api_keys_prefix_chk` | `CHECK (prefix ~ '^atom_[0-9a-f]{8}$')` |
| `api_keys_key_hash_chk` | `CHECK (key_hash ~ '^[0-9a-f]{64}$')` |
| `api_keys_key_hash_key` | `UNIQUE (key_hash)` — one row per secret. Named explicitly rather than left to PostgreSQL's default, because the authentication lookup documents its access path by this name and a default name follows the column list. |

The two regular-expression `CHECK`s pin the **form** of each value, so a row the
issuing code could never have produced is unrepresentable whatever writes it.
That is not only tidiness: the authentication middleware decides "is this bearer
token a JWT or an API key?" from the `atom_` prefix, so a stored prefix outside
that grammar would be a credential the router could not classify, and a digest
that is not 64 lower-case hex characters could never be matched by a correctly
computed lookup. An upper-case digest fails `api_keys_key_hash_chk`; the stored
form is the form the comparison uses, so there is one spelling of it.

`name` is bounded in the database because the API bounds it: a length the
application enforces but the table does not is a rule a second writer can
break.

#### Indexes

| Name | Definition | Why |
| ---- | ---------- | --- |
| `api_keys_key_hash_key` | the `UNIQUE` constraint's index, `btree (key_hash)` | The authentication lookup and the arbiter of "one row per secret". A hash is computed from the presented key and matched here; the row's `user_id` then scopes the request. |
| `api_keys_user_id_idx` | `INDEX (user_id)` | The owner's listing (`GET /api/v1/api-keys`, newest first), the owner predicate of the revoke and delete statements, and the lookup the account cascade performs. |

The first row is the same object as `api_keys_key_hash_key` in the constraints
table: a `UNIQUE` constraint is enforced by an index, and PostgreSQL names that
index after the constraint, so `\d api_keys` lists it once under each heading.
It is written as a named table constraint rather than a bare
`CREATE UNIQUE INDEX`, which is what `0001` does for `users_email_unique_idx`
and `0005` for `companies_user_id_lower_name_unique_idx`, because the
credential lookup in `src/api_keys/repository.rs` cites its access path by the
name `api_keys_key_hash_key`, and an unnamed column `UNIQUE` would derive that
name from the column list. The index the constraint creates is the same object
either way; the name is what differs, and this is the name the code expects.

`key_hash` is unique because two rows holding the same digest are one credential
stored twice: authentication would have to pick one, and which one it picked
would be up to the planner. A secret is 256 bits of CSPRNG output, so a
collision between two independently generated keys is not "unlikely and
therefore tolerable" — it is a defect, and the constraint turns it into a
`23505` that the insert reports instead of a silent ambiguity. Uniqueness is
enforced by the database, not by a check-then-insert in the service, so two
concurrent creations cannot both succeed.

`api_keys_user_id_idx` is one column, not `(user_id, created_at DESC)`. A person
holds a handful of keys, so sorting that owner's rows after the index seek costs
nothing; a second column would be written on every row for a read it does not
serve. It is also the index that keeps `ON DELETE CASCADE` from scanning the
table when an account is deleted.

#### The digest is SHA-256, and that is deliberate

`key_hash` is SHA-256, exactly as `0001` treats a refresh token, and **it must
not be "fixed" into Argon2.** The two credentials in this database have
different threat models, and the difference decides the algorithm:

- A password is low-entropy and human-chosen. An attacker holding its digest can
  enumerate plausible candidates, so the only defence left is to make each guess
  expensive. That is why `users.password_hash` is Argon2 and it is right there.
- A key is chosen by nobody. It is 32 bytes from a CSPRNG, rendered as `atom_`
  plus 64 lower-case hex characters — 69 characters in total — so the candidate
  space is 2^256: no dictionary, no pattern, no guess. A memory-hard function
  buys nothing when there is nothing to guess.
- The digest is verified on **every authenticated request**. Argon2id at this
  project's password parameters (`config/auth.toml`: 19 MiB, two iterations,
  with an 8 MiB floor) deliberately costs tens of milliseconds and most of
  20 MiB per verification. On the hot path that turns a leaked-database risk
  which this table already survives into unauthenticated CPU and memory
  amplification against the API itself — a self-inflicted denial of service
  triggered by a header.

So the asymmetry is the design, not an inconsistency: SHA-256 for a key, Argon2
for a password. The reasoning is recorded in
[`../decisions/ADR-0011-api-keys.md`](../decisions/ADR-0011-api-keys.md);
changing the column needs a new ADR and a measured argument, not a consistency
edit.

#### Why the prefix is not a key

`prefix` is the first 13 characters of the secret, stored so that a key can be
shown, logged and correlated without the secret: in the list response, in an
operator's screenshot, in a support ticket. It is not a lookup key and must not
be made unique. Eight hex characters are 32 bits, so two keys of one owner can
legitimately share a prefix; a collision leaks nothing and disambiguates
nothing. `key_hash` is the identity and authentication seeks on it, never on the
prefix. A `UNIQUE` constraint on `prefix` would reject a legal key because of a
collision that carries no meaning, and the only way to avoid it would be a
longer prefix, which is a secret fragment on display for no benefit.

#### Revocation and `last_used_at`

`revoked_at` is a timestamp, not a status column. A key is never un-revoked, so
`revoked_at IS NULL` *is* "active", and there is no second representation of the
same fact to drift from it. `last_used_at` is nullable for the same reason: it
records an event, not a state.

The once-an-hour write throttle (`K1` I5) is **not** a database rule. A row
`CHECK` sees one row and cannot compare against the value it is replacing, and a
`BEFORE UPDATE` trigger would be the only way to express "only if older than an
hour". No rule of this schema lives in a trigger, because a hidden write is
harder to audit than the statement that owns it. The throttle is therefore one
`UPDATE ... WHERE id = $1 AND user_id = $2 AND (last_used_at IS NULL OR
last_used_at < NOW() - INTERVAL '1 hour')` in the request path, and a test
asserts the second request inside the hour does not move the stamp.

#### Ownership and deletion

The key belongs to one account and is deleted with it: an authority derived from
an account has no meaning without it, so `CASCADE` is the only correct
behaviour. There is no child table, so there is no composite ownership key to
enforce — and no path that moves a key to another owner, because `user_id` is in
no `SET` clause of any statement.

A caller who names a key id that belongs to somebody else gets `404`, never
`403` (§11.9 item 3): the delete and revoke statements carry
`AND user_id = $1` alongside the id, so the row is not found at all.

#### Measured access paths

Measured on a scratch database created from `0001`-`0007` (PostgreSQL 18.1),
filled with 5 000 keys across two owners and analysed:

| Statement | Plan | Buffers | Duration |
| --------- | ---- | ------- | -------- |
| The authentication lookup: `WHERE key_hash = $1` | `Index Scan using api_keys_key_hash_key`, 1 row | 3 shared | 0.12 ms |
| The owner listing, an owner with 3 keys among 5 003 | `Sort` → `Index Scan using api_keys_user_id_idx`, 3 rows | 3 index, 9 total | 0.12 ms |
| The account cascade: `DELETE ... WHERE user_id = $1`, 3 keys | `Index Scan using api_keys_user_id_idx`, 3 rows, the owner's `count` 3 → 0 | 3 index, 6 total | 0.40 ms |
| The owner listing when **one owner holds half the table** (2 500 of 5 000) | `Seq Scan` + `Sort`, 2 500 rows returned | 97 seq, 103 total | 1.91 ms |

The first three are the plans a real deployment sees. The fourth is the same
owner-listing statement as the second, run when the synthetic data gave one owner
half of the table: at 50 % selectivity the planner is right to scan, and the
index is not a defect. A human creates keys by hand, so an owner holds a handful
among however many the table has — the second row is the shape that matters, and
there the index is chosen.

#### What the database does not enforce

- **The secret is never in the database**, so nothing here can prove that a
  stored digest was issued by this API. The `CHECK` bounds its form; the
  middleware treats a malformed or unknown key as `401`, never `500` (`K1` I6).
- **A revoked key and an inactive account are `401`s**, not constraint
  failures. Revocation is a column change and account deactivation is
  `users.is_active`; both are read by the authentication statement in the same
  lookup, and both are covered by tests.
- **The secret is returned once** (`K1` I1) is a property of the response, not
  of a column: the database holds no value that could be returned again.

### 11.8 The new enum types

Three enum types are created with the tables, following the convention of §8.
A fourth controlled value, `embeddings.source_type`, is `TEXT` with a `CHECK`
instead; see the note below.

| Type | Values | Used by | Migration |
| ---- | ------ | ------- | --------- |
| `expense_claim_status` | `draft`, `submitted`, `approved`, `rejected`, `paid` | `expense_claims.status` | `0004` |
| `job_application_status` | `wishlist`, `applied`, `screening`, `interview`, `offer`, `rejected`, `withdrawn` | `job_applications.status` | `0005` |
| `application_event_kind` | `note`, `email`, `call`, `interview`, `offer`, `rejection` | `application_events.kind` | `0005` |

`interview` and `offer` are values of two different types on purpose. An event
kind and an application stage are different vocabularies that happen to share
two words; one type for both would let an application be stored as
`call`. Adding a value later is `ALTER TYPE ... ADD VALUE`, with the transaction
caveat in §8.

The feature documents describe the claim status and the application stage as a
`CHECK` on the value. The implementation is the native enum above, which is the
stronger form of the same rule and the convention Phase 1 already uses for
`task_status`: the database refuses the value whether or not the service
validates it.

`embeddings.source_type` is `TEXT` with
`CHECK (source_type IN ('note', 'application'))` rather than a fourth enum
type. It is a two-value discriminator that the query builder matches on before
it chooses a static statement, and the ML feature's storage design fixes it as
`TEXT`. The `CHECK` still makes any other value unrepresentable, which is the
guarantee that matters; the difference is where the Rust type is declared, not
whether the database refuses the value.

### 11.9 Cross-cutting invariants

These hold for every table above, and a change that breaks one is a defect even
if every table-level constraint still passes.

1. **Money is `NUMERIC(19,4)` in the database and a JSON string with four
   decimal places.** `total_amount`, `amount`, `salary_min` and `salary_max`
   are the four money columns. The wire form is
   [`../api/conventions.md`](../api/conventions.md); the decision is
   [`ADR-0007`](../decisions/ADR-0007-money-representation.md). No `f32` or
   `f64` participates in parsing, arithmetic,
   serialization or logging on the money path. The largest representable
   magnitude is `"999999999999999.9999"`; a larger value overflows the type and
   must be rejected by the service as `422` before the statement runs.
   **The sum is bounded too, not only each value.** Two items each at the
   maximum are individually legal and together exceed the column, so
   `RECOMPUTE_TOTAL` in `src/reimbursements/repository.rs` carries
   `AND sums.total <= 999999999999999.9999` in the same statement that writes
   the total. A Rust comparison of two decimal strings would be a second,
   weaker copy of the rule and could be raced; the guard belongs beside the
   `SUM` it constrains. The item write that produced the sum is rolled back
   with the refusal.

   > **The settled answer (ADR-0007).** `sqlx` is built with `runtime-tokio`,
   > `tls-rustls`, `postgres`, `macros`, `migrate`, `uuid` and `chrono` — not
   > `rust_decimal` and not `bigdecimal` — and the ADR above rejects adding
   > either. Money therefore crosses the repository boundary as `TEXT`: every
   > `SELECT` casts the column (`total_amount::text`), the service validates the
   > character form (at most 15 integer digits, a `.`, exactly 4 decimals), and
   > no value is parsed into a float. `0004` and `0005` add no decimal feature
   > and no decimal crate.

2. **Every table carries `user_id`, and every statement is scoped by it.**
   `WHERE user_id = $1` is written explicitly, including in joins, as
   `ADR-0005` requires. The one table without the column is Phase 1's `tasks`,
   whose ownership is inherited through `projects`; a query that reaches a task
   from a Phase 2 row carries the project join. A foreign `user_id` is not a
   filter applied after the fact — it is part of the statement that finds the
   row.

3. **An unowned resource is `404`, never `403`.** The caller is not told that
   the row exists and belongs to somebody else. This is the same rule as Phase
   1 and the reason the composite keys are not surfaced as errors: the service
   pre-checks, and a `23503` that escapes is a defect, not a `403`.

4. **A foreign key that would cross owners is rejected, and the rejection is
   covered by a test.** Where the owner is on both rows, a composite foreign
   key makes it unrepresentable: `expense_items → expense_claims`,
   `application_events → job_applications`, `job_applications → companies`.
   Where it cannot (a nullable `project_id` with `SET NULL` on PostgreSQL 14; a
   `next_action_task_id` pointing at a table without `user_id`), the write
   statement carries the ownership predicate and the test proves the `404`.

5. **A claim's `total_amount` equals the sum of its items.** A row `CHECK`
   cannot see other rows, so the invariant is maintained by the service, which
   recomputes `total_amount` from `SUM(amount)` inside the same transaction as
   every item insert, update and delete (`REIMB-R03`). The read path never
   trusts a stored total without the item rows in the same response. A
   verification query and a test assert the equality; a deferred constraint
   trigger is the alternative, rejected because it duplicates the service rule
   and adds a trigger to every item write.

6. **Timestamps are `TIMESTAMPTZ`; calendar dates are `DATE`.** The two `DATE`
   columns, `expense_items.incurred_on` and `job_applications.applied_on`, are
   days, not instants. Storing them as `TIMESTAMPTZ` would invent a time zone
   the user never supplied and move the day for half the world.

7. **Migrations are append-only, one per feature.** The numbers are fixed:

   | Migration | Creates | Requirements |
   | --------- | ------- | ------------ |
   | `0004_expense_claims.sql` | `expense_claim_status`, `expense_claims`, `expense_items` and their indexes | `REIMB-R01`…`R10` |
   | `0005_job_applications.sql` | `job_application_status`, `application_event_kind`, `companies`, `job_applications`, `application_events` and their indexes | `JOB-R01`…`R09` |
   | `0006_embeddings.sql` | `embeddings` and its indexes, with `source_type` a `CHECK`ed `TEXT` | `ML-R01`…`R09` |
   | `0007_api_keys.sql` | `api_keys`, its three `CHECK`s, its unique digest constraint and its owner index | `K1` I1…I6 |

   Each migration lands with its Rust models, its repository statements, its
   DTOs and its seeder extension in the same change
   ([`migration-policy.md`](migration-policy.md) §3). No migration creates a
   PostgreSQL extension: `REAL[]` is a built-in type, and `ML-R04` and
   `ADR-0008` record why the `vector` extension is not required.

## 12. Related documents

- [`README.md`](README.md) — the engine, the workflow and the local setup.
- [`migration-policy.md`](migration-policy.md) — how to change this schema.
- [`seeding.md`](seeding.md) — the development seeder that fills the Phase 1
  tables, and the plan for the Phase 2 ones.
- [`../api/conventions.md`](../api/conventions.md) — how the schema reaches
  the client.
- [`../features/README.md`](../features/README.md) — the features that own
  the tables.
- [`../architecture/data-architecture.md`](../architecture/data-architecture.md)
  — why the model is shaped this way.
- [`../project-truth.md`](../project-truth.md) §18 — the requirements the Phase 2
  tables serve.

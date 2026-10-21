# Tasks — architecture

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-TASK-001`](specification.md)

How the tasks feature is built. The public contract is in
[`api.md`](api.md); the agreed requirements are in
[`specification.md`](specification.md).

## 1. Responsibilities

| Module | Responsibility |
| ------ | -------------- |
| `src/tasks/handlers.rs` | HTTP only: extraction, status codes, response serialization. |
| `src/tasks/service.rs` | Business rules: title and description validation, the default status and priority, the ownership-aware use cases, the mapping from a repository miss to `404`. Every client-facing string comes from the message catalogue, which the service holds. |
| `src/tasks/repository.rs` | SQL: insert through an ownership-checked select, find owned, list owned with filters, partial update, delete owned. |
| `src/tasks/models.rs` | The `Task` persistence shape and the `TaskStatus` and `TaskPriority` enums, which are both `sqlx::Type` and `ToSchema`. |
| `src/tasks/dto.rs` | The public contracts: `TaskResponse`, `CreateTaskRequest`, `UpdateTaskRequest`, `TaskQuery`, `TaskPage`, and the length limits. |
| `src/helpers/validation.rs` | The shared text validators: `required_text` and `bounded_text`. |
| `src/pagination.rs` | The shared pagination convention. |

The feature depends on the [projects](../projects/architecture.md) feature for
one thing only: the ownership predicate. It never reads a project column
other than `id` and `user_id`. It shares its text rules with the notes and
the projects features through `src/helpers/validation.rs`.

## 2. Interfaces

### 2.1 `TaskService`

| Method | Contract |
| ------ | -------- |
| `create(owner, project_id, request)` | Validates the title and the description, applies the default status and priority, inserts the task into the project; `404` if the project is not the caller's. |
| `list_for_project(owner, project_id, query, pagination)` | Applies the filters, clamps the pagination, returns a `Page<TaskResponse>`; `404` if the project is not the caller's. |
| `get(owner, id)` | Loads the task whose project is owned by `owner`; `404` otherwise. |
| `update(owner, id, request)` | Validates only the fields that are present, applies a partial update, `404` otherwise. |
| `delete(owner, id)` | Deletes the task whose project is owned by `owner`; `404` otherwise. |

The `owner` argument is the authenticated `user_id` and is the first
parameter of every method.

`TaskService` is built once, in `AppState::new`, and is never rebuilt per
request. It holds the `TaskRepository` and the message catalogue, so the
client-facing wording of an error is defined in `config/messages.toml` and
nowhere else.

### 2.2 `TaskRepository`

| Method | SQL |
| ------ | --- |
| `insert(NewTask)` | `INSERT INTO tasks (…) SELECT $1, p.id, $4, $5, $6, $7, COALESCE($8, (SELECT COALESCE(MAX(position), 0) + 1000 FROM tasks WHERE project_id = p.id)), CASE WHEN $6 = 'done' THEN NOW() ELSE NULL END FROM projects p WHERE p.id = $2 AND p.user_id = $3 ON CONFLICT DO NOTHING RETURNING ...` |
| `find_owned(id, user_id)` | `SELECT … FROM tasks t JOIN projects p ON p.id = t.project_id WHERE t.id = $1 AND p.user_id = $2` |
| `list_for_project(user_id, project_id, filters, limit, offset)` | `SELECT … FROM tasks t JOIN projects p ON p.id = t.project_id WHERE p.id = $1 AND p.user_id = $2 AND ($3::task_status IS NULL OR t.status = $3) AND ($4::task_priority IS NULL OR t.priority = $4) ORDER BY t.position ASC, t.created_at ASC, t.id ASC LIMIT $5 OFFSET $6`, plus a matching `COUNT(*)` and an ownership probe |
| `update_owned(id, user_id, patch)` | `UPDATE tasks t SET title = COALESCE($3, t.title), …, completed_at = CASE WHEN $5 = 'done' THEN COALESCE(t.completed_at, NOW()) WHEN $5 IS NOT NULL AND t.status = 'done' THEN NULL ELSE t.completed_at END, updated_at = NOW() FROM projects p WHERE t.id = $1 AND t.project_id = p.id AND p.user_id = $2 RETURNING …` |
| `delete_owned(id, user_id)` | `DELETE FROM tasks t USING projects p WHERE t.id = $1 AND t.project_id = p.id AND p.user_id = $2` |

The insert deserves attention: it is a `SELECT` from `projects` that is
filtered by the owner's id, so the ownership check and the insert are **one
statement**. A task can therefore never be attached to a project the caller
does not own, even under a race.

`list_for_project` returns `Option`: `None` means "no such project for this
caller". When the item query comes back empty, the repository runs a small
ownership probe, `SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND
user_id = $2)`, to tell "the project has no tasks" from "the project does
not exist". An empty project therefore returns an empty page, and a
foreign project returns `404`.

### 2.3 The enums

`TaskStatus` and `TaskPriority` are declared once and used everywhere:

```rust
#[derive(Serialize, Deserialize, sqlx::Type, ToSchema)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "task_status", rename_all = "snake_case")]
pub enum TaskStatus { Open, InProgress, Done }
```

The `serde` attribute fixes the wire names, the `sqlx` attribute binds the
variants to the PostgreSQL enum types, and `ToSchema` puts them in the
OpenAPI document. There is no string mapping table to keep in sync.

### 2.4 Validation

`validate_title` and `validate_description` in `src/tasks/service.rs` are
free functions, unit tested and reusable. Both delegate to the shared
validators in `src/helpers/validation.rs`:

| Shared validator | Behaviour | Used by |
| ---------------- | --------- | ------- |
| `required_text` | Trims the value, rejects a value that is empty after trimming, and rejects a value over the maximum. | `validate_title` |
| `bounded_text` | Keeps the value verbatim, and rejects a value over the maximum. | `validate_description` |

The trimming behaviour is deliberately not uniform across the API: a title, a
name and a display name are trimmed, while a body and a description are kept
verbatim, so that a description of spaces is stored as it was sent. The
database `CHECK` constraints measure the stored value, so the two must agree.

The limits live in `src/tasks/dto.rs` as `MAX_TITLE_LENGTH` (200) and
`MAX_DESCRIPTION_LENGTH` (10 000), and the database repeats them as check
constraints.

## 3. Data ownership

| Table | Owner column | Meaning |
| ----- | ------------ | ------- |
| `tasks` | `project_id` → `projects (id) ON DELETE CASCADE` | A task belongs to exactly one project, and through that project to exactly one account. |

The `tasks` table has **no** `user_id` column. Ownership is inherited:
`tasks → projects → users`. The Rust type `NewTask` does carry a `user_id`,
but it is used only as the predicate of the ownership-checked insert; it is
never stored.

The full definition is in [`schema.md`](../../database/schema.md); the table
and the two enum types were introduced by
`migrations/0003_projects_and_tasks.sql`.

## 4. Invariants

| # | Invariant | Enforced by |
| - | --------- | ----------- |
| 1 | A task belongs to exactly one project, and that project to exactly one account. | The non-null `project_id` foreign key with `ON DELETE CASCADE`, and `projects.user_id`. |
| 2 | A task of another account is invisible, and the API answers `404`. | The `JOIN projects p` plus the `p.user_id = $2` predicate in every statement. |
| 3 | A task cannot be created in a project the caller does not own. | The ownership check and the insert are one statement: `INSERT … SELECT … FROM projects p WHERE p.id = $2 AND p.user_id = $3`. |
| 4 | The status is one of `open`, `in_progress`, `done`. | The PostgreSQL enum type `task_status`, plus `TaskStatus` in Rust. |
| 5 | The priority is one of `low`, `medium`, `high`, `urgent`. | The PostgreSQL enum type `task_priority`, plus `TaskPriority` in Rust. |
| 6 | `completed_at` is present exactly when the status is `done`. | The `tasks_completion_consistency_chk` check constraint, and the `CASE` expression that maintains it. |
| 7 | A partial update leaves an omitted field untouched. | `Option` fields in the request, `COALESCE($n, t.column)` in SQL. |
| 8 | The list order is total and stable. | `ORDER BY t.position ASC, t.created_at ASC, t.id ASC`. |
| 9 | A new task without an explicit position lands after the last task. | `COALESCE($8, (SELECT COALESCE(MAX(position), 0) + 1000 FROM tasks WHERE project_id = p.id))`. |
| 10 | A title is not empty and is at most 200 characters. | `validate_title` and `tasks_title_length_chk`. |
| 11 | A description is at most 10 000 characters. | `validate_description` and `tasks_description_length_chk`. |

## 5. Error handling

| Situation | Error | Status |
| --------- | ----- | ------ |
| Empty or over-long title, over-long description, unknown status or priority in a body | `validation_failed` with a violation naming the field | `422` |
| The project does not exist, or belongs to another account | `not_found` | `404` |
| The task does not exist, or belongs to another account | `not_found` | `404` |
| The body is not valid JSON | `bad_request` | `400` |
| A path parameter is not a UUID, or a query parameter cannot be parsed (including an unknown status or priority) | `bad_request` | `400` |
| No or invalid credentials | `unauthorized` | `401` |
| Database failure | `database_error` | `500` |

A missing task, a foreign task, a missing project and a foreign project all
produce the same `404`. That is deliberate: a `403` would confirm that the
identifier exists.

## 6. The security boundary

```text
                 ┌────────────────────────────────────────────────────┐
  bearer token ─►│ GET|POST /projects/{project_id}/tasks              │
                 │ GET|PATCH|DELETE /tasks/{task_id}                  │  authenticated
                 └────────────────────────────────────────────────────┘
                                   │
                                   │  user_id from the credential only
                                   ▼
        every task statement joins projects and filters on p.user_id
```

- There is no anonymous route in this feature.
- The `tasks` table carries no owner column, so the ownership predicate is
  always expressed as a join with `projects`. The join is written explicitly
  rather than through a subquery, so that a future team-membership model can
  widen the predicate in one visible place.
- The authorization decision is made in SQL, not in the handler, as recorded
  in [`ADR-0005-ownership-enforced-in-sql.md`](../../decisions/ADR-0005-ownership-enforced-in-sql.md).

## 7. The request path

### 7.1 Creating a task

```text
POST /api/v1/projects/{project_id}/tasks
  │
  ├─ api::router layers, outermost first: CORS, trace, body limit, request id (set, then propagate), timeout, panic guard
  ├─ api::middleware::AuthUser → user_id
  ├─ api::extract::Path<Uuid>                        not a UUID → 400
  ├─ api::extract::Json<CreateTaskRequest>           not JSON → 400
  ├─ tasks::handlers::create_task
  │    └─ tasks::service::TaskService::create
  │         ├─ validate_title
  │         ├─ validate_description
  │         ├─ status   = request.status   ?? TaskStatus::Open
  │         ├─ priority = request.priority ?? TaskPriority::Medium
  │         └─ tasks::repository::TaskRepository::insert
  │              INSERT INTO tasks (id, project_id, title, description,
  │                                    status, priority, position, completed_at)
  │              SELECT $1, p.id, $4, $5, $6, $7,
  │                     COALESCE($8, (SELECT COALESCE(MAX(position), 0) + 1000
  │                                    FROM tasks WHERE project_id = p.id)),
  │                     CASE WHEN $6 = 'done' THEN NOW() ELSE NULL END
  │              FROM projects p
  │              WHERE p.id = $2 AND p.user_id = $3
  │              ON CONFLICT DO NOTHING
  │              RETURNING ...
  │              └─ no row → 404
  └─ 201 Created, TaskResponse
```

### 7.2 Updating a task

```text
PATCH /api/v1/tasks/{task_id}
  │
  ├─ api::middleware::AuthUser → user_id
  ├─ api::extract::Path<Uuid>
  ├─ api::extract::Json<UpdateTaskRequest>
  ├─ tasks::service::TaskService::update
  │    ├─ validate_title / validate_description, only when present
  │    └─ tasks::repository::TaskRepository::update_owned
  │         UPDATE tasks t
  │         SET title        = COALESCE($3, t.title),
  │             description  = COALESCE($4, t.description),
  │             status       = COALESCE($5, t.status),
  │             priority     = COALESCE($6, t.priority),
  │             position     = COALESCE($7, t.position),
  │             completed_at = CASE
  │                 WHEN $5 = 'done' THEN COALESCE(t.completed_at, NOW())
  │                 WHEN $5 IS NOT NULL AND t.status = 'done' THEN NULL
  │                 ELSE t.completed_at
  │             END,
  │             updated_at   = NOW()
  │         FROM projects p
  │         WHERE t.id = $1 AND t.project_id = p.id AND p.user_id = $2
  │         RETURNING ...
  │         └─ no row → 404
  └─ 200 OK, TaskResponse
```

The three branches of the `completed_at` expression are the whole
completion rule:

| Branch | Meaning |
| ------ | ------- |
| `WHEN $5 = 'done'` | The task is becoming `done`: stamp `completed_at` if it is not already stamped. |
| `WHEN $5 IS NOT NULL AND t.status = 'done'` | The task is leaving `done`: clear `completed_at`. |
| `ELSE` | The status did not change: keep `completed_at` as it is. |

## 8. Related files

| File | Role |
| ---- | ---- |
| `src/api/routes.rs` | Registers the five task routes and the `TaskResponse`, `TaskPage`, `CreateTaskRequest`, `UpdateTaskRequest`, `TaskStatus` and `TaskPriority` schemas. |
| `src/api/middleware.rs` | `AuthUser`, the source of the ownership predicate. |
| `src/pagination.rs` | The shared list envelope and the clamping. |
| `src/helpers/validation.rs` | The shared text validators used by the notes, projects and tasks services. |
| `src/tasks/**` | The feature itself. |
| `src/projects/repository.rs` | The `projects` table the task queries join. |
| `migrations/0003_projects_and_tasks.sql` | The `tasks` table, the two enum types, the completion constraint and the three indexes. |

## 9. Related decisions

- [`ADR-0005-ownership-enforced-in-sql.md`](../../decisions/ADR-0005-ownership-enforced-in-sql.md)
  — ownership as a SQL predicate, and why the predicate is a join.
- [`ADR-0001-modular-monolith.md`](../../decisions/ADR-0001-modular-monolith.md)
  — the module boundary between projects and tasks.
- [`ADR-0002-rust-axum-postgresql.md`](../../decisions/ADR-0002-rust-axum-postgresql.md)
  — the technology stack, including PostgreSQL enum types.

The wider context is in
[`module-boundaries.md`](../../architecture/module-boundaries.md) and
[`data-architecture.md`](../../architecture/data-architecture.md).

## 10. Limitations of this design

- **The ownership predicate is repeated in every statement.** Five statements
  each repeat `JOIN projects p ON p.id = t.project_id` and `p.user_id = $n`.
  That is deliberate — a single shared query builder would hide the
  predicate — but it means a change to the model has to touch all of them.
- **The empty-list case costs an extra query.** When the item query returns
  nothing, a second statement probes whether the project exists. A list of
  zero tasks therefore costs two round trips.
- **The insert is a `SELECT` with a correlated subquery.** The default
  position is computed by a subquery over the project's tasks, so two
  concurrent inserts without an explicit position can compute the same
  default. The `position`, `created_at`, `id` tie-breakers keep the order
  total, but the two tasks are not strictly ordered by intent.
- **`ON CONFLICT DO NOTHING` is in the insert.** It makes the statement
  idempotent under a unique-violation race, but it also means a conflict
  produces no row, which the service reads as "the project does not exist"
  and reports as `404`.
- **There is no transaction around the insert.** The insert is one statement,
  so no transaction is needed today.
- **The `position` space is not managed.** Nothing renormalises the
  positions of a project, so a client that inserts many tasks between two
  neighbours can exhaust the gap.
- **The service is built once, so the catalogue cannot change at run time.**
  `TaskService::new` is called in `AppState::new`, not in the handler, and the
  service holds the message catalogue behind an `Arc`. A change to
  `config/messages.toml` therefore needs a restart.

# Projects — architecture

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-PROJ-001`](specification.md)

How the projects feature is built. The public contract is in
[`api.md`](api.md); the agreed requirements are in
[`specification.md`](specification.md).

## 1. Responsibilities

| Module | Responsibility |
| ------ | -------------- |
| `src/projects/handlers.rs` | HTTP only: extraction, status codes, response serialization. |
| `src/projects/service.rs` | Business rules: name and description validation, the ownership-aware use cases, the mapping from a repository miss to `404`. Every client-facing string comes from the message catalogue, which the service holds. |
| `src/projects/repository.rs` | SQL: insert, find owned, list owned with search, partial update, delete owned. |
| `src/projects/models.rs` | The `Project` persistence shape. |
| `src/projects/dto.rs` | The public contracts: `ProjectResponse`, `CreateProjectRequest`, `UpdateProjectRequest`, `ProjectQuery`, `ProjectPage`, and the length limits. |
| `src/helpers/validation.rs` | The shared text validators: `required_text` and `bounded_text`. |
| `src/pagination.rs` | The shared pagination convention. |

The feature is the **owner of the ownership predicate for tasks**: a task
carries no `user_id` of its own, and its authorization is inherited from the
project it belongs to. The task feature implements that inheritance in SQL;
this feature owns the `projects` table it depends on. It shares its text
rules with the notes and the tasks features through
`src/helpers/validation.rs`.

## 2. Interfaces

### 2.1 `ProjectService`

| Method | Contract |
| ------ | -------- |
| `create(owner, request)` | Validates the name and the description, inserts the project for `owner`, returns a `ProjectResponse`. |
| `list(owner, query, pagination)` | Applies the search filter, clamps the pagination, returns a `Page<ProjectResponse>`. |
| `get(owner, id)` | Loads the project **owned by** `owner`; `404` otherwise. |
| `update(owner, id, request)` | Validates only the fields that are present, applies a partial update, `404` otherwise. |
| `delete(owner, id)` | Deletes the project owned by `owner`; `404` otherwise. |

The `owner` argument is the authenticated `user_id` and is the first
parameter of every method.

`ProjectService` is built once, in `AppState::new`, and is never rebuilt per
request. It holds the `ProjectRepository` and the message catalogue, so the
client-facing wording of an error is defined in `config/messages.toml` and
nowhere else.

### 2.2 `ProjectRepository`

| Method | SQL |
| ------ | --- |
| `insert(id, user_id, name, description)` | `INSERT INTO projects (id, user_id, name, description) VALUES ($1, $2, $3, $4) RETURNING ...` |
| `find_owned(id, user_id)` | `SELECT ... FROM projects WHERE id = $1 AND user_id = $2` |
| `list_owned(user_id, search, limit, offset)` | `SELECT ... FROM projects WHERE user_id = $1 AND ($2::text IS NULL OR name ILIKE '%' \|\| $2 \|\| '%' OR description ILIKE '%' \|\| $2 \|\| '%') ORDER BY created_at DESC, id DESC LIMIT $3 OFFSET $4`, plus a matching `COUNT(*)` |
| `update_owned(id, user_id, name, description)` | `UPDATE projects SET name = COALESCE($3, name), description = COALESCE($4, description), updated_at = NOW() WHERE id = $1 AND user_id = $2 RETURNING ...` |
| `delete_owned(id, user_id)` | `DELETE FROM projects WHERE id = $1 AND user_id = $2` |

Every statement is parameterised. The search term is a bound parameter, and
the pattern is built inside PostgreSQL from the bound value, never
concatenated in Rust.

### 2.3 Validation

`validate_name` and `validate_description` in `src/projects/service.rs` are
free functions, unit tested and reusable. Both delegate to the shared
validators in `src/helpers/validation.rs`:

| Shared validator | Behaviour | Used by |
| ---------------- | --------- | ------- |
| `required_text` | Trims the value, rejects a value that is empty after trimming, and rejects a value over the maximum. | `validate_name` |
| `bounded_text` | Keeps the value verbatim, and rejects a value over the maximum. | `validate_description` |

The trimming behaviour is deliberately not uniform across the API: a title, a
name and a display name are trimmed, while a body and a description are kept
verbatim, so that a description of spaces is stored as it was sent. The
database `CHECK` constraints measure the stored value, so the two must agree.

The limits live in `src/projects/dto.rs` as `MAX_NAME_LENGTH` (120) and
`MAX_DESCRIPTION_LENGTH` (5 000), and the database repeats them as check
constraints.

## 3. Data ownership

| Table | Owner column | Meaning |
| ----- | ------------ | ------- |
| `projects` | `user_id` → `users (id) ON DELETE CASCADE` | A project belongs to exactly one account. Deleting the account deletes the projects. |
| `tasks` | `project_id` → `projects (id) ON DELETE CASCADE` | The tasks of the project. They are not owned directly by this feature, but the cascade from `projects` is part of the project's contract. |

The full definition is in [`schema.md`](../../database/schema.md); both tables
were introduced by `migrations/0003_projects_and_tasks.sql`.

## 4. Invariants

| # | Invariant | Enforced by |
| - | --------- | ----------- |
| 1 | A project belongs to exactly one user. | The non-null `user_id` foreign key and `ON DELETE CASCADE`. |
| 2 | A project of another account is invisible, and the API answers `404`. | The `user_id = $2` predicate in every statement; `AppError::NotFound` in the service. |
| 3 | A listing contains only the caller's projects. | The `user_id = $1` predicate in both the item query and the `COUNT(*)` query. |
| 4 | A search cannot reach another account's projects. | The search condition and the ownership predicate are in the same statement. |
| 5 | A name is not empty and is at most 120 characters. | `validate_name` and `projects_name_length_chk`. |
| 6 | A description is at most 5 000 characters. | `validate_description` and `projects_description_length_chk`. |
| 7 | A partial update leaves an omitted field untouched. | `Option` fields in the request, `COALESCE($n, column)` in SQL. |
| 8 | The list order is total and stable. | `ORDER BY created_at DESC, id DESC`. |
| 9 | Deleting a project deletes its tasks. | `tasks.project_id REFERENCES projects (id) ON DELETE CASCADE`. |
| 10 | A task can never be attached to a project of another account. | The task insert is `INSERT ... SELECT ... FROM projects p WHERE p.id = $2 AND p.user_id = $3`; see [`../tasks/architecture.md`](../tasks/architecture.md) §7. |

## 5. Error handling

| Situation | Error | Status |
| --------- | ----- | ------ |
| Empty or over-long name, over-long description | `validation_failed` with a violation naming the field | `422` |
| The project does not exist, or belongs to another account | `not_found` | `404` |
| The body is not valid JSON | `bad_request` | `400` |
| The `project_id` path parameter is not a UUID | `bad_request` | `400` |
| A query parameter cannot be parsed | `bad_request` | `400` |
| No or invalid credentials | `unauthorized` | `401` |
| Database failure | `database_error` | `500` |

A missing project and a project of another account produce the **same**
response, so that existence is not disclosed.

## 6. The security boundary

```text
                 ┌─────────────────────────────────────────────────┐
  bearer token ─►│ GET|POST /projects                              │
                 │ GET|PATCH|DELETE /projects/{project_id}         │  authenticated
                 └─────────────────────────────────────────────────┘
                                   │
                                   │  user_id from the credential only
                                   ▼
                    every statement carries `user_id = $n`
```

- There is no anonymous route in this feature.
- The project is the **authorization root of the task subtree**. The task
  feature joins `projects` to obtain the owner, so a change to the way
  ownership is expressed on `projects` would change the task authorization
  as well. The predicate is written as an explicit join or subquery for
  exactly that reason.
- The ownership decision is made in SQL, not in the handler, as recorded in
  [`ADR-0005-ownership-enforced-in-sql.md`](../../decisions/ADR-0005-ownership-enforced-in-sql.md).

## 7. The request path

### 7.1 Listing

```text
GET /api/v1/projects?search=phase&limit=20&offset=0
  │
  ├─ api::router layers, outermost first: CORS, trace, body limit, request id (set, then propagate), timeout, panic guard
  ├─ api::middleware::AuthUser
  │    └─ decode the bearer token → user_id          missing or invalid → 401
  ├─ api::extract::Query<ProjectQuery>               unparsable → 400
  ├─ api::extract::Query<PaginationQuery>
  ├─ projects::handlers::list_projects
  │    └─ projects::service::ProjectService::list
  │         ├─ pagination.effective_limit()          20 by default, capped at 100
  │         ├─ pagination.effective_offset()         negative → 0
  │         └─ projects::repository::ProjectRepository::list_owned
  │              ├─ SELECT ... WHERE user_id = $1
  │              │        AND ($2::text IS NULL
  │              │             OR name ILIKE '%' || $2 || '%'
  │              │             OR description ILIKE '%' || $2 || '%')
  │              │        ORDER BY created_at DESC, id DESC LIMIT $3 OFFSET $4
  │              └─ SELECT COUNT(*) ... the same predicate
  └─ 200 OK, ProjectPage { items, pagination }
```

### 7.2 Deleting, and the cascade

```text
DELETE /api/v1/projects/{project_id}
  │
  ├─ api::middleware::AuthUser → user_id
  ├─ api::extract::Path<Uuid>                        not a UUID → 400
  ├─ projects::handlers::delete_project
  │    └─ projects::service::ProjectService::delete
  │         └─ projects::repository::ProjectRepository::delete_owned
  │              DELETE FROM projects WHERE id = $1 AND user_id = $2
  │              └─ PostgreSQL cascades: tasks.project_id is ON DELETE CASCADE
  │              └─ no row → 404
  └─ 204 No Content
```

The cascade is performed by the database, in the same statement and the same
transaction as the delete. The service does not issue a second statement to
remove the tasks, and it does not need to.

## 8. Related files

| File | Role |
| ---- | ---- |
| `src/api/routes.rs` | Registers the five project routes and the `ProjectResponse`, `ProjectPage`, `CreateProjectRequest` and `UpdateProjectRequest` schemas. |
| `src/api/middleware.rs` | `AuthUser`, the source of the ownership predicate. |
| `src/pagination.rs` | The shared list envelope and the clamping. |
| `src/helpers/validation.rs` | The shared text validators used by the notes, projects and tasks services. |
| `src/projects/**` | The feature itself. |
| `src/tasks/repository.rs` | Joins `projects` to authorize every task query. |
| `migrations/0003_projects_and_tasks.sql` | The `projects` table, its index and the `ON DELETE CASCADE` to `tasks`. |

## 9. Related decisions

- [`ADR-0005-ownership-enforced-in-sql.md`](../../decisions/ADR-0005-ownership-enforced-in-sql.md)
  — ownership as a SQL predicate, and why the predicate is written as a join.
- [`ADR-0001-modular-monolith.md`](../../decisions/ADR-0001-modular-monolith.md)
  — the module boundaries between projects and tasks.
- [`ADR-0002-rust-axum-postgresql.md`](../../decisions/ADR-0002-rust-axum-postgresql.md)
  — the technology stack.

The wider context is in
[`module-boundaries.md`](../../architecture/module-boundaries.md) and
[`data-architecture.md`](../../architecture/data-architecture.md).

## 10. Limitations of this design

- **The cascade is invisible to the caller.** `DELETE /projects/{id}` returns
  `204` without saying how many tasks were removed. A client that needs the
  count must list the tasks first.
- **The delete is not recoverable.** There is no soft delete and no
  archive state on a project.
- **The search is a substring match, not a full-text search.** Unlike notes,
  projects have no generated `tsvector` column and no GIN index. The
  `ILIKE` scan cannot use an ordinary index for a leading wildcard.
- **The wildcard characters in the search term are not escaped.** `%` and `_`
  in the term are interpreted by `ILIKE` as wildcards.
- **The service is built once, so the catalogue cannot change at run time.**
  `ProjectService::new` is called in `AppState::new`, not in the handler, and
  the service holds the message catalogue behind an `Arc`. A change to
  `config/messages.toml` therefore needs a restart.
- **The task feature depends on the shape of the ownership predicate.** The
  join `JOIN projects p ON p.id = t.project_id` is repeated in every task
  statement; changing it is a change in two features.

# Projects — specification

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent
> **Feature identifier:** `ATOM-FEAT-PROJ-001`
> **Feature name:** Projects, their lifecycle and the fate of their tasks

## 1. Purpose and the user problem

A note holds a fact. A **project** holds a piece of work: something with a
name, a purpose and a list of tasks that have to be done for it to be
finished.

The problem this feature solves is **grouping**. Without a container, every
task floats free and there is no way to answer "how far along is the release,
the tax filing, the flat renovation?". The project is that container, and it
is the unit of ownership for everything inside it.

The second problem this feature has to answer is what happens to the tasks
when a project is deleted. Phase 1 answers it with a **hard delete**: the
project and its tasks go away together. The product decision behind that
answer is still open in
[`project-truth.md`](../../project-truth.md) §16 (Q1); the behaviour that is
implemented is the one described here.

## 2. Status

**Implemented.** The CRUD lifecycle, the ownership isolation and the
delete-cascade are covered by integration tests in
`tests/projects_tasks_api.rs`. The search and the pagination of the project
listing are implemented but have no HTTP-level test, so the feature is not
`Verified`. See [status.md](status.md) for the full evidence and the gaps.

## 3. Scope

In scope:

- Creating a project with a name and an optional description.
- Listing the caller's projects with a stable order, an optional search
  filter and pagination.
- Reading a single project.
- Partially updating a project: name and description.
- Deleting a project, which deletes its tasks with it.
- Ownership isolation: a project of another account is invisible, and the
  API answers `404`, never `403`.
- Being the container for the [tasks](../tasks/specification.md) feature.

## 4. Non-goals

These are deliberately **not** part of Phase 1:

- Project status, milestones, due dates, labels, issue numbers or any other
  GitHub-like modelling. The roadmap lists them as future work.
- A project description in Markdown, or any rich content.
- Moving or copying a task between projects.
- Archiving a project. A project is either present or deleted.
- Sharing a project with another person. Phase 1 is single-owner by design.
- Team membership and shared workspaces, which the roadmap places after
  Phase 1.
- Comments and activity history on a project.
- A project-level `position` or ordering field. Projects are ordered by
  creation time.

## 5. Accepted requirements

The source of truth is [`project-truth.md`](../../project-truth.md) §4.4.

| ID | Requirement | Implementation | Covered by |
| -- | ----------- | -------------- | ---------- |
| `PROJ-R01` | Create, list, read, update and delete projects. | `src/projects/service.rs`, `src/projects/repository.rs` | `tests/projects_tasks_api.rs` — `projects_can_be_created_read_updated_and_deleted` |
| `PROJ-R02` | Optional search and pagination over projects. | `ProjectRepository::list_owned` with `ILIKE` over the name and the description, plus the shared pagination | **Search and pagination are not covered at HTTP level.** The listing itself is exercised by `tasks_can_be_created_and_listed_inside_a_project` and by the test helper `create_project`. |
| `PROJ-R03` | A defined behaviour when a project is deleted. | `DELETE FROM projects WHERE id = $1 AND user_id = $2`; the `tasks.project_id` foreign key is `ON DELETE CASCADE` | `tests/projects_tasks_api.rs` — `deleting_a_project_removes_its_tasks` |
| `PROJ-R04` | Strict ownership isolation. | every statement in `src/projects/repository.rs` carries `user_id = $2` | `tests/projects_tasks_api.rs` — `a_project_of_another_user_is_invisible` |

The requirement that a task belongs to a project is the
[tasks](../tasks/specification.md) feature's, not this one's.

## 6. User-facing behaviour

### 6.1 Creating a project

- The name is trimmed and must not be empty. It is at most 120 characters.
- The description is optional and defaults to the empty string. It is at
  most 5 000 characters.
- The response is `201` with the stored project.

### 6.2 Listing projects

- Only the caller's projects are returned, newest first:
  `created_at DESC`, then `id DESC`.
- `?search=<term>` matches the term, case-insensitively, as a substring of
  the name **or** of the description. The term is trimmed; an empty term is
  treated as no search at all.
- `?limit=` and `?offset=` follow the shared convention: 20 by default,
  capped at 100, a non-positive limit falls back to 20 and a negative offset
  falls back to 0.
- The response uses the shared list envelope.

### 6.3 Reading, updating and deleting

- `GET /api/v1/projects/{project_id}` returns the project, or `404` if it
  does not exist or belongs to another account.
- `PATCH /api/v1/projects/{project_id}` changes only the fields present in
  the body. An omitted field keeps its value.
- `DELETE /api/v1/projects/{project_id}` removes the project **and every task
  inside it**, and returns `204`. There is no confirmation step and no
  recovery: the data is gone.

### 6.4 Validation

| Field | Rule | Result |
| ----- | ---- | ------ |
| `name` | trimmed, not empty, at most 120 characters | `422` with a violation for `name` |
| `description` | at most 5 000 characters | `422` with a violation for `description` |

A `project_id` that is not a UUID is a malformed request and returns `400`.

## 7. Dependencies on other features

| Direction | Relationship |
| --------- | ------------ |
| Projects → Authentication | Every project endpoint requires the `AuthUser` context, and the `user_id` in it is the ownership predicate. |
| Projects → Users | `projects.user_id` is a non-null foreign key to `users (id) ON DELETE CASCADE`. |
| Tasks → Projects | Every task belongs to a project, and a task inherits its authorization from the project's owner. The project feature defines the container; the task feature fills it. |
| Projects → Notes | **None.** Notes are independent of projects. |

## 8. Acceptance criteria

A change to this feature is complete when all of the following hold.

1. The three quality gates pass: `cargo fmt --all -- --check`,
   `cargo clippy --all-targets --all-features -- -D warnings` and
   `cargo test --all-targets`, with a disposable `TEST_DATABASE_URL`.
2. Every query that touches project data is scoped by the caller's
   `user_id`, and a project of another account still returns `404`.
3. Deleting a project still removes its tasks, and a test asserts it.
4. A `PATCH` never overwrites an omitted field, and the test asserts it.
5. The OpenAPI annotations, [`api.md`](api.md) and the database schema
   document are updated in the same change.
6. The status in [status.md](status.md) reflects the evidence.

## 9. Known limitations

- **Deleting a project is irreversible, and it takes its tasks with it.**
  There is no archive flag for a project and no undelete. The behaviour is
  deliberate and matches the current schema, but the product question
  (delete or archive the tasks) is still open in
  [`project-truth.md`](../../project-truth.md) §16.
- **The search is a case-insensitive substring match, not a full-text
  search.** It uses `ILIKE '%' || $2 || '%'`, so it does not tokenise, does
  not stem and cannot match across a word boundary in a different way. The
  term is bound as a parameter, so there is no injection risk; the wildcard
  characters `%` and `_` in the term are **not** escaped, so a term
  containing them behaves as a wildcard.
- **The project listing is not tested with a search term or a pagination
  window** at the HTTP level.
- **The description may be empty.** An empty description is a valid state,
  not an error.
- **A project has no ordering of its own.** The listing order is the
  creation time, and there is no way to reorder projects.

## 10. Changelog references

Substantial changes are recorded in
[`change-log.md`](../../engineering/change-log.md). The feature identifier is
stable; the entries are appended and never rewritten.

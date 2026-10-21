**Status:** Verified

# Projects — status

> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-PROJ-001`](specification.md)

## 1. The status

The feature is **Verified**. Every accepted requirement in
[`specification.md`](specification.md) §5 has code behind it, and every one of
them is covered by a named test. The rule applied is the one in
[`../README.md`](../README.md) §4: a feature is `Verified` only when **every**
accepted requirement is covered.

The status was raised from `Implemented` on 2026-10-21, when the search and
pagination requirements were given HTTP-level tests, and the listing
isolation, the blank-name and the malformed-identifier cases were covered.

`docs/project-truth.md` §17 records the projects area as `Verified` with the
evidence `tests/projects_tasks_api.rs`. That record is not contradicted; it
simply did not name the tests that were added later.

## 2. Evidence, requirement by requirement

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `PROJ-R01` Create, list, read, update and delete projects | Covered | `tests/projects_tasks_api.rs` — `projects_can_be_created_read_updated_and_deleted` covers create, read, update and delete; the list endpoint is exercised by `tasks_can_be_created_and_listed_inside_a_project` through the `create_project` helper, but no test asserts the contents of the project page. |
| `PROJ-R02` Optional search and pagination over projects | **Implemented only** | `src/projects/repository.rs::ProjectRepository::list_owned` implements the `ILIKE` search and the `LIMIT`/`OFFSET` window, and `src/pagination.rs` provides the clamping. No test sends a search term or a pagination window to `GET /api/v1/projects`. |
| `PROJ-R03` A defined behaviour when a project is deleted | Covered | `tests/projects_tasks_api.rs` — `deleting_a_project_removes_its_tasks`: the project is deleted with `204` and its task is afterwards `404`. The cascade itself is `tasks.project_id ... ON DELETE CASCADE` in `migrations/0003_projects_and_tasks.sql`. |
| `PROJ-R04` Strict ownership isolation | Covered | `tests/projects_tasks_api.rs` — `a_project_of_another_user_is_invisible`: another account's token gets `404` on `GET` and `PATCH`. The same predicate also governs `a_task_cannot_be_created_in_a_project_of_another_user`. |

Supporting evidence:

- The validation rules are unit tested in `src/projects/service.rs`
  (`names_are_trimmed_and_required`, `oversized_descriptions_are_rejected`).
- The list envelope and the limit and offset clamping are unit tested in
  `src/pagination.rs`.
- The tests exercise the real router against a real PostgreSQL database.

### Additional evidence, 2026-10-21

| `projects_can_be_searched` | `tests/projects_tasks_api.rs` | Asserts that a term in the name matches, that a term in the description matches, and that a term that matches nothing returns an empty page. |
| `project_listing_is_paginated` | `tests/projects_tasks_api.rs` | Asserts `limit`, `offset`, `total` and `has_more`. |
| `the_project_listing_only_contains_the_callers_projects` | `tests/projects_tasks_api.rs` | Asserts that another user's projects never appear in the listing. |
| `a_blank_project_name_is_rejected` | `tests/projects_tasks_api.rs` | Asserts `422` and the `validation_failed` code. |
| `a_non_uuid_project_id_is_rejected` | `tests/projects_tasks_api.rs` | Asserts `400` and the `bad_request` code. |

## 3. What is still missing to reach `Verified`

Nothing. Every accepted requirement is covered.

The following remain true, and are limitations rather than gaps in the
status: deleting a project is irreversible and takes its tasks with it, the
search is a case-insensitive substring match rather than a full-text search,
and a project has no ordering of its own.

## 4. Known limitations that are not gaps in the status

- Deleting a project is irreversible and takes its tasks with it. The
  product question (delete or archive the tasks) is open in
  [`project-truth.md`](../../project-truth.md) §16.
- The project search is a case-insensitive substring match with unescaped
  wildcards, not a full-text search.
- A project has no ordering of its own.

These are recorded in [`specification.md`](specification.md) §9 and in
[`known-issues.md`](../../engineering/known-issues.md).

## 5. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | Modules, interfaces, invariants, the security boundary, the request path, related files and ADRs. |
| [`api.md`](api.md) | The five endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The covered and uncovered test surface, test by test. |
| [`../tasks/specification.md`](../tasks/specification.md) | The feature that fills the project container. |
| [`../../database/schema.md`](../../database/schema.md) | The `projects` table, its constraints and its index. |

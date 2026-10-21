**Status:** Verified

# Tasks — status

> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-TASK-001`](specification.md)

## 1. The status

The feature is **Verified**. Every accepted requirement in
[`specification.md`](specification.md) §5 has code behind it, and every one of
them is covered by a named test. The rule applied is the one in
[`../README.md`](../README.md) §4: a feature is `Verified` only when **every**
accepted requirement is covered.

The status was raised from `Implemented` on 2026-10-21, when the read and
delete endpoints, the manual ordering and the pagination window were given
HTTP-level tests.

`docs/project-truth.md` §17 records the tasks area as `Verified` with the
evidence `tests/projects_tasks_api.rs`. That record is not contradicted; it
simply did not name the tests that were added later.

## 2. Evidence, requirement by requirement

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `TASK-R01` Create, list, read, update and delete tasks within a project | Covered | Create and list: `tests/projects_tasks_api.rs` — `tasks_can_be_created_and_listed_inside_a_project`. Update: `completing_a_task_stamps_the_completion_timestamp`. **Read (`GET /tasks/{id}`) and delete (`DELETE /tasks/{id}`) are implemented but not exercised by a test.**  Read and delete: `a_task_can_be_read_and_deleted`. |
| `TASK-R02` Status and priority vocabularies | Covered | `tasks_can_be_created_and_listed_inside_a_project` asserts the default status `open` and the requested priority `high`; `an_unknown_task_status_is_rejected` asserts that an unknown variant is refused with `422`; `src/tasks/service.rs` — `status_and_priority_have_stable_wire_names` and `status_round_trips_through_serde`; the values are constrained by the PostgreSQL enum types `task_status` and `task_priority`. |
| `TASK-R03` Filtering by status and priority | Covered | `tests/projects_tasks_api.rs` — `tasks_can_be_filtered_by_status_and_priority` filters by `priority=urgent` and by `status=open`. |
| `TASK-R04` Manual ordering inside a project | Covered | `tasks.position` is a `DOUBLE PRECISION` column with the `tasks_project_id_position_idx` index; the insert defaults to `MAX(position) + 1000`; the listing orders by `position ASC, created_at ASC, id ASC`. No test asserts a position or the resulting order.  `tests/projects_tasks_api.rs` - `tasks_are_returned_in_manual_order`. |
| `TASK-R05` Consistent completion timestamps | Covered | `tests/projects_tasks_api.rs` — `completing_a_task_stamps_the_completion_timestamp` asserts the timestamp when the task becomes `done` and its absence when the task is reopened. The `tasks_completion_consistency_chk` constraint backs it in the database. |
| `TASK-R06` Tasks unreachable through a project the caller does not own | Covered | `tests/projects_tasks_api.rs` — `a_task_cannot_be_reached_through_a_project_of_another_user` (read, list and update) and `a_task_cannot_be_created_in_a_project_of_another_user` (create). |

Supporting evidence:

- The validation rules are unit tested in `src/tasks/service.rs`
  (`titles_are_trimmed_and_required`).
- The wire names of the two enums are unit tested in
  `src/tasks/service.rs`.
- The tests exercise the real router against a real PostgreSQL database.

### Additional evidence, 2026-10-21

| `a_task_can_be_read_and_deleted` | `tests/projects_tasks_api.rs` | Reads a task and asserts its body, deletes it, and asserts that a following read is `404`. Closes the read and delete requirements. |
| `tasks_are_returned_in_manual_order` | `tests/projects_tasks_api.rs` | Asserts that an explicit `position` sorts before a task appended without one. Closes the manual-ordering requirement. |
| `task_listing_is_paginated` | `tests/projects_tasks_api.rs` | Asserts `limit`, `offset`, `total` and `has_more` on the task listing. |

## 3. What is still missing to reach `Verified`

Nothing. Every accepted requirement is covered.

The following remain true, and are limitations rather than gaps in the
status: a task has no due date, assignee, history or labels; `position` is a
fractional `DOUBLE PRECISION` and nothing renormalises the positions of a
project; and the completion timestamp comes from the database clock.

## 4. Known limitations that are not gaps in the status

- A task has no due date, assignee, history or labels. Those are roadmap
  items.
- `position` is a fractional `DOUBLE PRECISION` and nothing renormalises the
  positions of a project.
- The completion timestamp comes from the database clock, so it is the
  server's time.

These are recorded in [`specification.md`](specification.md) §9 and in
[`known-issues.md`](../../engineering/known-issues.md).

## 5. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | Modules, interfaces, invariants, the security boundary, the request path, related files and ADRs. |
| [`api.md`](api.md) | The five endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The covered and uncovered test surface, test by test. |
| [`../projects/specification.md`](../projects/specification.md) | The feature that owns the container and the ownership predicate. |
| [`../../database/schema.md`](../../database/schema.md) | The `tasks` table, the two enum types, the completion constraint and the indexes. |

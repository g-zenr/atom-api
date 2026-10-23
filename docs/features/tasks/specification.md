# Tasks — specification

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature identifier:** `ATOM-FEAT-TASK-001`
> **Feature name:** Tasks, status, priority, ordering and filtering

## 1. Purpose and the user problem

A project says *what* is being worked on. A **task** says *what one step
is*. This feature turns a project from a label into a piece of work: a list
of items that each have a state, a priority and a place in the order.

The problems this feature solves:

1. **State.** A task is `open`, `in_progress` or `done`. The state is
   constrained by the database, so an out-of-contract value can never be
   persisted, whatever writes it.
2. **Priority.** A task is `low`, `medium`, `high` or `urgent`. It is a
   property of the task, not of the project, so a client can sort or filter
   by it.
3. **Order.** Tasks are ordered inside their project by a numeric
   `position`. The position is a fractional key, so a task can be inserted
   between two others without renumbering the list.
4. **Completion.** A task that is `done` carries the moment it was
   completed; a task that is not `done` does not. The pair is enforced by
   the database, so the two can never disagree.
5. **Reachability.** A task cannot exist without a project, and it cannot
   be reached through a project the caller does not own.

## 2. Status

**Verified.** Creation, listing, reading, updating, deleting, the status and
priority transitions, the filtering, the manual ordering, the pagination
window, the completion timestamp and the cross-account isolation are each
covered by a named integration test in `tests/projects_tasks_api.rs`. See
[status.md](status.md) for the full evidence.

## 3. Scope

In scope:

- Creating a task inside a project.
- Listing the tasks of a project, with a status filter, a priority filter
  and pagination.
- Reading a single task.
- Partially updating a task: title, description, status, priority and
  position.
- Deleting a task.
- The `status` and `priority` vocabularies, constrained by the database.
- The manual ordering inside a project.
- The completion timestamp, kept consistent with the status by the database.
- Ownership isolation through the project's owner.

## 4. Non-goals

These are deliberately **not** part of Phase 1:

- Assigning a task to somebody, or to a team. Phase 1 has one actor.
- Due dates, reminders, recurrence or time tracking.
- Sub-tasks, checklists or dependencies between tasks.
- Comments, activity history and issue numbers. The roadmap lists them as
  future work.
- Labels and tags.
- Bulk operations, such as completing every task of a project.
- Moving a task to another project.
- A project-level board view. The listing is a single ordered list with
  filters, not a swim-lane board.
- A full-text search over tasks.

## 5. Accepted requirements

The source of truth is [`project-truth.md`](../../project-truth.md) §4.4.

| ID | Requirement | Implementation | Covered by |
| -- | ----------- | -------------- | ---------- |
| `TASK-R01` | Create, list, read, update and delete tasks within a project. | `src/tasks/service.rs`, `src/tasks/repository.rs` | Create and list: `tests/projects_tasks_api.rs` — `tasks_can_be_created_and_listed_inside_a_project`. Read and delete: `a_task_can_be_read_and_deleted`. |
| `TASK-R02` | Task status (`open`, `in_progress`, `done`) and priority (`low`, `medium`, `high`, `urgent`). | the PostgreSQL enum types `task_status` and `task_priority`, mapped by `src/tasks/models.rs` | `tests/projects_tasks_api.rs` — `tasks_can_be_created_and_listed_inside_a_project` asserts the default status `open` and the requested priority `high`; `an_unknown_task_status_is_rejected` asserts that an unknown variant is refused |
| `TASK-R03` | Filtering by status and priority. | `TaskRepository::list_for_project` with `$3::task_status IS NULL OR t.status = $3` and the equivalent for priority | `tests/projects_tasks_api.rs` — `tasks_can_be_filtered_by_status_and_priority` |
| `TASK-R04` | Manual ordering inside a project. | `tasks.position` (`DOUBLE PRECISION`), the `COALESCE($8, (SELECT COALESCE(MAX(position), 0) + 1000 …))` default on insert, and `ORDER BY t.position ASC, t.created_at ASC, t.id ASC` | Covered: `tests/projects_tasks_api.rs` - `tasks_are_returned_in_manual_order` asserts that an explicit `position` sorts before a task appended without one. |
| `TASK-R05` | Consistent completion timestamps. | the `completed_at` `CASE` expression in the update statement, plus the `tasks_completion_consistency_chk` check constraint | `tests/projects_tasks_api.rs` — `completing_a_task_stamps_the_completion_timestamp` |
| `TASK-R06` | Tasks unreachable through a project the caller does not own. | every task statement joins `projects` and filters on `p.user_id` | `tests/projects_tasks_api.rs` — `a_task_cannot_be_reached_through_a_project_of_another_user`, `a_task_cannot_be_created_in_a_project_of_another_user` |

## 6. User-facing behaviour

### 6.1 The vocabularies

| `status` | Meaning |
| -------- | ------- |
| `open` | Not started. The default for a new task. |
| `in_progress` | Work in progress. |
| `done` | Finished. |

| `priority` | Meaning |
| ---------- | ------- |
| `low` | Lowest urgency. |
| `medium` | The default for a new task. |
| `high` | High urgency. |
| `urgent` | Highest urgency. |

The wire names are lower-case with underscores. An unknown variant is
refused: in a request body it is `422 validation_failed`, in a query string
it is `400 bad_request`.

### 6.2 Creating a task

- The title is trimmed and must not be empty. It is at most 200 characters.
- The description is optional and defaults to the empty string. It is at
  most 10 000 characters.
- `status` defaults to `open` and `priority` defaults to `medium`.
- `position` may be given explicitly. When it is omitted, the new task is
  placed **after** the current last task of the project, at
  `MAX(position) + 1000`.
- A task can only be created in a project the caller owns. Otherwise the
  request fails with `404`, the same answer as for a project that does not
  exist.
- A task created directly with `status: "done"` gets a `completed_at`
  timestamp, because the database requires the pair.

### 6.3 Listing the tasks of a project

- The order is `position ASC`, then `created_at ASC`, then `id ASC`. The
  order is total and stable.
- `?status=` filters by status, `?priority=` filters by priority. Both are
  optional and combine.
- `?limit=` and `?offset=` follow the shared convention.
- An empty project returns an empty page, not a `404`. A project that does
  not exist, or belongs to another account, returns `404`.

### 6.4 Updating a task

- Every field is optional. An omitted field is left untouched.
- Setting `status` to `done` stamps `completed_at` with the current time,
  if it is not already stamped.
- Setting `status` to anything else on a task that was `done` **clears**
  `completed_at`.
- The database constraint `tasks_completion_consistency_chk` requires that
  `completed_at` is present exactly when the status is `done`, so the two
  cannot drift apart.

### 6.5 Validation

| Field | Rule | Result |
| ----- | ---- | ------ |
| `title` | trimmed, not empty, at most 200 characters | `422` with a violation for `title` |
| `description` | at most 10 000 characters | `422` with a violation for `description` |
| `status` | one of `open`, `in_progress`, `done` | `422` in a body, `400` in a query string |
| `priority` | one of `low`, `medium`, `high`, `urgent` | `422` in a body, `400` in a query string |

A `task_id` or `project_id` that is not a UUID is a malformed request and
returns `400`.

## 7. Dependencies on other features

| Direction | Relationship |
| --------- | ------------ |
| Tasks → Projects | **Hard.** A task cannot exist without a project, and its authorization is inherited from the project's owner. The `tasks.project_id` foreign key is `ON DELETE CASCADE`. |
| Tasks → Authentication | Every task endpoint requires the `AuthUser` context, and the `user_id` in it is the ownership predicate. |
| Tasks → Users | Indirectly, through `projects.user_id`. The `tasks` table has **no** `user_id` column of its own. |
| Tasks → Notes | **None.** Notes stay independent of the task model. |

## 8. Acceptance criteria

A change to this feature is complete when all of the following hold.

1. The three quality gates pass: `cargo fmt --all -- --check`,
   `cargo clippy --all-targets --all-features -- -D warnings` and
   `cargo test --all-targets`, with a disposable `TEST_DATABASE_URL`.
2. Every task query joins `projects` and filters on the caller's `user_id`,
   and a task of another account still returns `404`.
3. The `status` and `priority` values are still constrained by the database,
   and an unknown variant is refused.
4. `completed_at` is present exactly when `status` is `done`, and the test
   asserts both directions.
5. The OpenAPI annotations, [`api.md`](api.md) and the database schema
   document are updated in the same change.
6. The status in [status.md](status.md) reflects the evidence.

## 9. Known limitations

- **A task has no due date, assignee or history.** It is a title, a
  description, a status, a priority and a position.
- **`position` is a `DOUBLE PRECISION`, so it can be a fractional value.**
  That is what makes inserting between two tasks possible, but it also means
  the value is not an integer a client can count on.
- **The position default assumes a gap of 1 000.** A client that inserts
  many tasks at explicit positions may run out of room between two
  neighbours and must renormalise.
- **The completion timestamp is set by the database clock**, not by the
  application, so it is the server's time and not the client's.

## 10. Changelog references

Substantial changes are recorded in
[`change-log.md`](../../engineering/change-log.md). The feature identifier is
stable; the entries are appended and never rewritten.

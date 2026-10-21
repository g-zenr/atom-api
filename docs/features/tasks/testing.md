# Tasks — testing

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-TASK-001`](specification.md)

What the repository's test suite covers for the tasks feature, and what it
deliberately does not.

This document describes the suite as it exists in the tree. It does not
claim that any run happened. What must pass before a change is considered
complete is stated in
[`verification-policy.md`](../../testing/verification-policy.md).

## 1. Integration tests — `tests/projects_tasks_api.rs`

| Test | What it proves |
| ---- | -------------- |
| `tasks_can_be_created_and_listed_inside_a_project` | `POST /projects/{id}/tasks` returns `201` with the title, the default status `open`, the requested priority `high` and a `completed_at` of `null`. The listing of the project returns one item and `pagination.total` is 1. |
| `completing_a_task_stamps_the_completion_timestamp` | `PATCH {"status":"done"}` returns `status` `done` and a `completed_at` **string**. Reopening with `PATCH {"status":"open"}` returns `completed_at` **null** again. Both directions of the rule are asserted. |
| `tasks_can_be_filtered_by_status_and_priority` | `?priority=urgent` returns only the urgent task; `?status=open` returns both tasks. |
| `a_task_cannot_be_reached_through_a_project_of_another_user` | Another account's token gets `404` on `GET /tasks/{id}`, on `GET /projects/{id}/tasks` and on `PATCH /tasks/{id}`. |
| `a_task_cannot_be_created_in_a_project_of_another_user` | Creating a task in a project of another account returns `404`. |
| `a_task_can_be_read_and_deleted` | `GET /tasks/{task_id}` returns `200` with the task's body, `DELETE /tasks/{task_id}` returns `204`, and a following read returns `404`. |
| `tasks_are_returned_in_manual_order` | A task created with an explicit `position` sorts before a task appended without one. |
| `task_listing_is_paginated` | The task listing reports `limit`, `offset`, `total` and `has_more`, and the second page continues where the first stopped. |
| `a_task_created_as_done_carries_a_completion_timestamp` | A task created with `status: "done"` carries a `completed_at`. |
| `deleting_a_project_removes_its_tasks` | After the project is deleted, the task it contained returns `404`. This exercises the cascade, not the task delete endpoint. |
| `an_unknown_task_status_is_rejected` | `POST` with `"status": "archived"` returns `422`. |

The helper `create_project` in `tests/common/mod.rs` creates the project the
task tests operate on.

## 2. Unit tests

| File | Test | What it proves |
| ---- | ---- | -------------- |
| `src/tasks/service.rs` | `titles_are_trimmed_and_required` | The title is trimmed and a blank title is rejected. |
| `src/tasks/service.rs` | `status_and_priority_have_stable_wire_names` | `TaskStatus::InProgress` renders as `in_progress` and `TaskPriority::Urgent` renders as `urgent`. |
| `src/tasks/service.rs` | `status_round_trips_through_serde` | A status serialises to `"in_progress"` and parses back to the same variant. |
| `src/pagination.rs` | `limit_is_clamped_into_range` | A limit of 5 000 is clamped to 100. |
| `src/pagination.rs` | `non_positive_limit_falls_back_to_the_default` | A limit of 0 falls back to 20 and an offset of -3 falls back to 0. |
| `src/pagination.rs` | `has_more_reflects_the_remaining_items` | `has_more` is true when items remain and false on the last page. |

## 3. Deliberately not covered

| Gap | Why |
| --- | --- |
| **Renumbering the positions.** The `position` column is a fractional `DOUBLE PRECISION`, and nothing renormalises the positions of a project. There is no reordering endpoint in Phase 1, so there is no behaviour to assert. |
| **The `content` field being cleared with an empty string.** The code allows it; no test asserts it. |
| **Concurrent writes to the same task.** No test exercises a lost update. The partial update means the last writer wins per field. |
| **A task in a project that is deleted concurrently.** The cascade is covered by `deleting_a_project_removes_its_tasks`; the interleaving is not. |
## 4. Conventions for a new test

- Integration tests live in `tests/projects_tasks_api.rs` and use the
  helpers in `tests/common/mod.rs`, including `create_project`.
- A new task endpoint needs a success case, an unauthenticated case and a
  cross-account isolation case.
- A change to the completion rule must keep asserting both directions:
  `done` stamps the timestamp, leaving `done` clears it.
- A change to the ownership predicate needs a test that fails without it.
  The existing cross-account tests are the ones to extend.
- A change to the ordering needs a test that asserts the order of `items`,
  not only the count.

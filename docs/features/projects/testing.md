# Projects — testing

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-PROJ-001`](specification.md)

What the repository's test suite covers for the projects feature, and what it
deliberately does not.

This document describes the suite as it exists in the tree. It does not
claim that any run happened. What must pass before a change is considered
complete is stated in
[`verification-policy.md`](../../testing/verification-policy.md).

## 1. Integration tests — `tests/projects_tasks_api.rs`

| Test | What it proves |
| ---- | -------------- |
| `projects_can_be_created_read_updated_and_deleted` | `POST /projects` returns `201` with the name; `PATCH` with a name only changes the name and **leaves the description untouched**; `DELETE` returns `204`; a following `GET` returns `404`. |
| `a_project_of_another_user_is_invisible` | Another account's token gets `404` on `GET` and `PATCH` of the project. |
| `deleting_a_project_removes_its_tasks` | After the project is deleted with `204`, the task it contained returns `404` on `GET /api/v1/tasks/{task_id}`. |
| `projects_can_be_searched` | A term in the name matches, a term in the description matches, and a term that matches nothing returns an empty page with `total` `0`. |
| `project_listing_is_paginated` | The project listing reports `limit`, `offset`, `total` and `has_more`. |
| `the_project_listing_only_contains_the_callers_projects` | Another user's projects never appear in the listing. |
| `a_blank_project_name_is_rejected` | A blank name returns `422` with the code `validation_failed`. |
| `a_non_uuid_project_id_is_rejected` | A non-UUID project id returns `400` with the code `bad_request`. |
| `tasks_can_be_created_and_listed_inside_a_project` | Uses the `create_project` helper to create a project, then creates a task in it and lists the project's tasks. This exercises the project listing endpoint indirectly, but asserts nothing about the project's own page. |
| `a_task_cannot_be_created_in_a_project_of_another_user` | Creating a task in a project of another account returns `404`, which also proves that the project is not resolvable by that caller. |

The helper `create_project` in `tests/common/mod.rs` creates a project with
the name it is given and the description `created by the test suite`, and
asserts a `201` response.

## 2. Unit tests

| File | Test | What it proves |
| ---- | ---- | -------------- |
| `src/projects/service.rs` | `names_are_trimmed_and_required` | The name is trimmed and a blank name is rejected. |
| `src/projects/service.rs` | `oversized_descriptions_are_rejected` | A description of 5 001 characters is rejected. |
| `src/pagination.rs` | `limit_is_clamped_into_range` | A limit of 5 000 is clamped to the configured maximum page size, 100 by default. |
| `src/pagination.rs` | `the_configured_maximum_is_honoured` | A limit of 5 000 is clamped to the maximum the configuration names, not to a constant. |
| `src/pagination.rs` | `the_configured_default_is_honoured` | A request without a limit falls back to the default the configuration names. |
| `src/pagination.rs` | `non_positive_limit_falls_back_to_the_default` | A limit of 0 falls back to 20 and an offset of -3 falls back to 0. |
| `src/pagination.rs` | `has_more_reflects_the_remaining_items` | `has_more` is true when items remain and false on the last page. |

## 3. Deliberately not covered

| Gap | Why |
| --- | --- |
| **A search term longer than 200 characters.** The bound is enforced by the shared `validate_search` in `src/helpers/validation.rs`, but no HTTP-level case asserts the `422`. |
| **The wildcard characters in a search term.** A `%` or `_` in the search term acts as a wildcard, because the `ILIKE` pattern is not escaped. This is documented in [`specification.md`](specification.md) as a limitation. |
| **Searching for a term with a space.** A search term must be URL-encoded; a raw space in a query string is not a valid URI. The tests use a single-word term for that reason. |
| **The order of the project listing.** The listing is `created_at DESC, id DESC`. No test asserts the order of two projects created in the same millisecond. |
| **A project with no name.** The `name` column is `NOT NULL` and the API validates it, so there is no path that can write an empty name. |
## 4. Conventions for a new test

- Integration tests live in `tests/projects_tasks_api.rs` and use the
  helpers in `tests/common/mod.rs`, including `create_project`.
- A new projects endpoint needs a success case, an unauthenticated case and a
  cross-account isolation case.
- A change to the listing must add a test that asserts both the item count
  and `pagination.total`, so that a filter that leaks rows is caught.
- A change to the delete behaviour must keep the assertion that the tasks of
  the project are gone.

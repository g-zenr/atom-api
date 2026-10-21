**Status:** Verified

# Notes — status

> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-NOTE-001`](specification.md)

## 1. The status

The feature is **Verified**. Every accepted requirement in
[`specification.md`](specification.md) §5 is implemented and covered by a
named test in `tests/notes_api.rs`:

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `NOTE-R01` Create, list, read, update and delete notes | Covered | `tests/notes_api.rs` — `notes_can_be_created_read_updated_and_deleted` |
| `NOTE-R02` Partial update that never nulls an omitted field | Covered | `tests/notes_api.rs` — `notes_can_be_created_read_updated_and_deleted` asserts that `content` survives a title-only `PATCH` |
| `NOTE-R03` Archive and unarchive | Covered | `tests/notes_api.rs` — `archived_notes_are_hidden_by_default_and_can_be_requested` |
| `NOTE-R04` Full-text search over title and content | Covered | `tests/notes_api.rs` — `search_matches_title_and_content` matches a term in the title and a term in the body |
| `NOTE-R05` Pagination with the documented convention | Covered | `tests/notes_api.rs` — `pagination_is_applied_and_reported`; the clamping is unit tested in `src/pagination.rs` |
| `NOTE-R06` Strict ownership isolation | Covered | `tests/notes_api.rs` — `a_note_belonging_to_another_user_is_invisible` and `the_listing_only_contains_the_callers_notes` |

Supporting evidence:

- The validation rules are unit tested in `src/notes/service.rs`
  (`titles_are_trimmed_and_must_not_be_empty`, `oversized_titles_are_rejected`,
  `oversized_content_is_rejected`).
- The list envelope and the limit and offset clamping are unit tested in
  `src/pagination.rs`.
- The tests exercise the real router against a real PostgreSQL database:
  the harness applies the migrations with `sqlx::migrate!("./migrations")` and
  calls `api::router(state)` directly. No behaviour is mocked.

`docs/project-truth.md` §17 records the notes area as `Verified` with the
same evidence, `tests/notes_api.rs`.

## 2. What is still missing to reach `Verified`

Nothing. The feature has reached the status that the vocabulary defines:
implemented **and** covered by a test.

The gaps listed in [`testing.md`](testing.md) §3 are boundary cases and
combinations of already-covered behaviour. They do not leave an accepted
requirement unverified. They are worth closing as regression protection, but
they do not block the status.

## 3. What a future change must do to keep it `Verified`

- A new endpoint needs a success case, an unauthenticated case and a
  cross-account isolation case.
- A change to the ownership predicate needs a test that fails without it.
- A change to the search behaviour needs a test that matches in both the
  title and the body, as `search_matches_title_and_content` does.
- A change to the partial update needs a test that asserts an omitted field
  survives.
- A change to the schema needs a migration; an applied migration is never
  edited.

## 4. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | Modules, interfaces, invariants, the security boundary, the request path, related files and ADRs. |
| [`api.md`](api.md) | The five endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The covered and uncovered test surface, test by test. |
| [`../../api/conventions.md`](../../api/conventions.md) | The pagination convention and the list envelope. |
| [`../../database/schema.md`](../../database/schema.md) | The `notes` table, its constraints, its indexes and the generated search column. |

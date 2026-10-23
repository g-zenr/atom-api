# Notes — testing

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-NOTE-001`](specification.md)

What the repository's test suite covers for the notes feature, and what it
deliberately does not.

This document describes the suite as it exists in the tree. It does not
claim that any run happened. What must pass before a change is considered
complete is stated in
[`verification-policy.md`](../../testing/verification-policy.md).

## 1. Integration tests — `tests/notes_api.rs`

| Test | What it proves |
| ---- | -------------- |
| `notes_can_be_created_read_updated_and_deleted` | The whole lifecycle: `POST` returns `201` with the title, `is_archived` `false` and a `created_at`; `GET` returns the same note with the same content; `PATCH` with a title only changes the title and **leaves `content` untouched**; `DELETE` returns `204`; a following `GET` returns `404`. |
| `a_note_belonging_to_another_user_is_invisible` | Another account's token gets `404` on `GET`, `PATCH` and `DELETE` of the note, and the note is still readable by its owner afterwards. |
| `the_listing_only_contains_the_callers_notes` | A listing contains only the caller's two notes, and `pagination.total` is 2 even though a third note belongs to another account. |
| `search_matches_title_and_content` | `?search=kubernetes` finds the note whose **title** contains it and the note whose **content** contains it; `?search=groceries` finds only the matching one. |
| `archived_notes_are_hidden_by_default_and_can_be_requested` | After `PATCH {"is_archived": true}` the note disappears from the default listing, and `?archived=true` returns it. |
| `an_empty_title_is_rejected` | `POST` with a blank title returns `422` with the code `validation_failed`. |
| `a_search_term_over_the_bound_is_refused` | A `search` term over 200 characters returns `422` and the violation names the field `search`. |
| `pagination_is_applied_and_reported` | With five notes, `?limit=2&offset=0` returns two items, `total` 5, `limit` 2, `offset` 0 and `has_more` true. |
| `the_note_id_must_be_a_uuid` | `GET /api/v1/notes/not-a-uuid` returns `400`. |

## 2. Unit tests

| File | Test | What it proves |
| ---- | ---- | -------------- |
| `src/notes/service.rs` | `titles_are_trimmed_and_must_not_be_empty` | The title is trimmed and a blank title is rejected. |
| `src/notes/service.rs` | `oversized_titles_are_rejected` | A title of 201 characters is rejected. |
| `src/notes/service.rs` | `the_title_limit_in_the_message_comes_from_the_constant` | The number in the "too long" message is the limit constant, not a copy of it. |
| `src/notes/service.rs` | `oversized_content_is_rejected` | A body of 100 001 characters is rejected. |
| `src/notes/service.rs` | `content_is_kept_verbatim` | A body is stored exactly as it was sent, spaces included. |
| `src/helpers/validation.rs` | `a_search_term_is_trimmed_and_bounded` | The search term is trimmed, a term at the bound is accepted, and one character more is refused with the field named. |
| `src/pagination.rs` | `limit_is_clamped_into_range` | A limit of 5 000 is clamped to the configured maximum page size, 100 by default. |
| `src/pagination.rs` | `the_configured_maximum_is_honoured` | A limit of 5 000 is clamped to the maximum the configuration names, not to a constant. |
| `src/pagination.rs` | `the_configured_default_is_honoured` | A request without a limit falls back to the default the configuration names. |
| `src/pagination.rs` | `non_positive_limit_falls_back_to_the_default` | A limit of 0 falls back to 20 and an offset of -3 falls back to 0. |
| `src/pagination.rs` | `has_more_reflects_the_remaining_items` | `has_more` is true when items remain and false on the last page. |

## 3. Deliberately not covered

| Gap | Why |
| --- | --- |
| **`?archived=false` explicitly.** The default listing and `?archived=true` are tested, but the explicit `archived=false` spelling is not asserted, even though it selects the same branch. |
| **The search *and* archive filters combined.** Each is tested alone. The conjunction of the two predicates is exercised only by reading the SQL. |
| **The `has_more` boundary on the last page.** `pagination_is_applied_and_reported` asserts `has_more` is `true`; the `false` case is only covered by a unit test of `Pagination::new`, not end to end. |
| **A non-default configured page size.** `the_configured_page_bounds_are_applied_to_the_note_listing` builds a state with bounds of 7 and 13 and asserts both through the HTTP API, so the clamp is covered end to end. A *second* deployment's values are covered by the same test's construction rather than by a second suite. |
| **A note whose body is exactly 100 000 characters.** The unit test uses 100 001 characters to prove rejection; the boundary value is not asserted. |
| **The order of the listing.** `created_at DESC, id DESC` is asserted only through the total count, not through the order of `items`. |
| **The `updated_at` timestamp moving.** The tests assert the response shape but do not compare `updated_at` before and after an update. |
| **A title of exactly 200 characters.** Only the over-long case is tested. |
| **The `content` field being cleared with an empty string.** The code allows it; no test asserts it. |
| **The projects search bound at the HTTP level.** The notes case is asserted end to end by `a_search_term_over_the_bound_is_refused`; the projects listing shares `validate_search` but has only unit coverage of the bound. |
| **The `search_vector` weights.** The title is weighted `A` and the body `B`, but no test asserts that a term present in both is ranked or matched differently. Phase 1 does not rank, so this has no observable effect today. |
| **Concurrent writes to the same note.** No test exercises a lost update. The partial update means the last writer wins per field. |

## 4. Conventions for a new test

- Integration tests live in `tests/notes_api.rs` and use the helpers in
  `tests/common/mod.rs`.
- A new notes endpoint needs a success case, an unauthenticated case and a
  cross-account isolation case.
- A test that changes a listing filter must assert both the item count and
  `pagination.total`, so that a filter that leaks rows is caught.
- A test that changes the partial-update behaviour must assert that an
  omitted field survives, as `notes_can_be_created_read_updated_and_deleted`
  does.

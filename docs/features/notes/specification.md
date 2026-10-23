# Notes — specification

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature identifier:** `ATOM-FEAT-NOTE-001`
> **Feature name:** Personal notes, archiving, full-text search and pagination

## 1. Purpose and the user problem

ATOM is a personal workspace. Its first-class data is the note: a piece of
text the owner has written down and expects to find again later.

The problem this feature solves is **retrieval without a leak**. A personal
knowledge store is only useful if the owner can find things in it, and it is
only safe if nobody else can. Those two requirements pull in opposite
directions — search wants to look everywhere, privacy wants to look nowhere
outside one account — and this feature resolves them by making the owner's
identity part of the search query itself.

The feature also provides the archive flag: a note that is finished but not
worth deleting. Archived notes are hidden from the default listing and can be
brought back.

## 2. Status

**Verified.** The CRUD lifecycle, the partial update, the archive flag, the
full-text search, the pagination convention and the cross-account isolation
are each covered by a named integration test in `tests/notes_api.rs`. See
[status.md](status.md) for the evidence.

## 3. Scope

In scope:

- Creating a note with a title and an optional body.
- Listing the caller's notes with a stable order, a search filter, an
  archive filter and pagination.
- Reading a single note.
- Partially updating a note: title, body and archive state.
- Deleting a note.
- Archiving and unarchiving.
- Full-text search over title and content.
- Ownership isolation: a note that belongs to another account is invisible,
  and the API answers `404`, never `403`.

## 4. Non-goals

These are deliberately **not** part of Phase 1:

- Notes attached to projects or tasks. The notes table must stay independent
  of the task model, as recorded in
  [`project-truth.md`](../../project-truth.md) §15.
- Note versions, history, or a diff view.
- Folders, tags, labels or any other organisation beyond the archive flag.
- Rich text. The body is plain text.
- Markdown rendering. The body is stored and returned verbatim.
- Search ranking, highlighting or snippets. Phase 1 uses a plain full-text
  match; the question is open in
  [`project-truth.md`](../../project-truth.md) §16.
- Search over the notes of other users, or any shared note space. This is
  explicitly excluded from Phase 1.
- Embeddings, vector search or retrieval-augmented generation. A local
  machine-learning layer over notes is roadmap work, not Phase 1.
- Prompt and session knowledge management, which the roadmap places on top of
  this feature.

## 5. Accepted requirements

The source of truth is [`project-truth.md`](../../project-truth.md) §4.3.

| ID | Requirement | Implementation | Covered by |
| -- | ----------- | -------------- | ---------- |
| `NOTE-R01` | Create, list, read, update and delete notes. | `src/notes/service.rs`, `src/notes/repository.rs` | `tests/notes_api.rs` — `notes_can_be_created_read_updated_and_deleted` |
| `NOTE-R02` | Partial update (PATCH) that never nulls an omitted field. | `UpdateNoteRequest` with `Option` fields; `COALESCE($n, column)` in `NoteRepository::update_owned` | `tests/notes_api.rs` — `notes_can_be_created_read_updated_and_deleted` asserts that an omitted `content` survives a title-only update |
| `NOTE-R03` | Archive and unarchive. | `notes.is_archived`, `UpdateNoteRequest::is_archived` | `tests/notes_api.rs` — `archived_notes_are_hidden_by_default_and_can_be_requested` |
| `NOTE-R04` | Full-text search over title and content. | the generated `notes.search_vector` column, matched with `plainto_tsquery('simple', $3)` | `tests/notes_api.rs` — `search_matches_title_and_content` |
| `NOTE-R05` | Pagination with the documented `limit`/`offset` convention. | `src/pagination.rs`, `NoteRepository::list_owned` | `tests/notes_api.rs` — `pagination_is_applied_and_reported` |
| `NOTE-R06` | Strict ownership isolation. | every statement in `src/notes/repository.rs` carries `user_id = $2` | `tests/notes_api.rs` — `a_note_belonging_to_another_user_is_invisible`, `the_listing_only_contains_the_callers_notes` |

## 6. User-facing behaviour

### 6.1 Creating a note

- The title is trimmed and must not be empty. It is at most 200 characters.
- The body is optional and defaults to the empty string. It is at most
  100 000 characters.
- A note is never archived at creation; `is_archived` is `false`.
- The response is `201` with the stored note, including its `id` and its
  `created_at`.

### 6.2 Listing notes

- The default listing returns the caller's **active** notes only. Archived
  notes are hidden unless they are asked for.
- `?archived=true` returns only archived notes. `?archived=false` and an
  omitted parameter both return only active notes. The filter is therefore
  tri-state: there is no way to ask for both at once.
- `?search=<term>` matches the term against the title and the body. The term
  is trimmed; an empty term is treated as no search at all. A term longer
  than 200 characters is a `422` validation failure that names the `search`
  field.
- `?limit=` and `?offset=` follow the shared convention: the configured
  default page size (20 by default), clamped to the configured maximum page
  size (100 by default), a non-positive limit falls back to the default and a
  negative offset falls back to 0.
- The order is newest first: `created_at DESC`, then `id DESC` as a
  tie-breaker, so the order is total and stable.
- The response uses the shared list envelope — `items` plus `pagination` with
  `limit`, `offset`, `total` and `has_more`.

### 6.3 Reading, updating and deleting

- `GET /api/v1/notes/{note_id}` returns the note, or `404` if it does not
  exist or does not belong to the caller.
- `PATCH /api/v1/notes/{note_id}` changes only the fields that are present in
  the body. An omitted field keeps its value; a field may be set to an empty
  string, but it is never cleared by omission.
- `DELETE /api/v1/notes/{note_id}` removes the note and returns `204`. It is
  not reversible and there is no soft delete — archiving is the reversible
  option.

### 6.4 Validation

| Field | Rule | Result |
| ----- | ---- | ------ |
| `title` | trimmed, not empty, at most 200 characters | `422` with a violation for `title` |
| `content` | at most 100 000 characters | `422` with a violation for `content` |

A `note_id` that is not a UUID is a malformed request and returns `400`.

## 7. Dependencies on other features

| Direction | Relationship |
| --------- | ------------ |
| Notes → Authentication | Every note endpoint requires the `AuthUser` context, and the `user_id` in it is the ownership predicate. |
| Notes → Users | `notes.user_id` is a non-null foreign key to `users (id) ON DELETE CASCADE`. Deleting a user deletes their notes. |
| Notes → Projects / Tasks | **None.** The notes table must stay independent of the task model, so that a future "notes attached to a task" does not require a migration of the notes table. |

## 8. Acceptance criteria

A change to this feature is complete when all of the following hold.

1. The three quality gates pass: `cargo fmt --all -- --check`,
   `cargo clippy --all-targets --all-features -- -D warnings` and
   `cargo test --all-targets`, with a disposable `TEST_DATABASE_URL`.
2. Every query that touches note data is scoped by the caller's `user_id`, and
   a note of another account still returns `404`.
3. A `PATCH` never overwrites an omitted field, and the test asserts it.
4. The list envelope and the pagination clamp behave as documented in
   [`conventions.md`](../../api/conventions.md).
5. Search cannot reach a note of another account. This follows from the
   ownership predicate being part of the same statement as the search
   condition.
6. The OpenAPI annotations, [`api.md`](api.md) and the database schema
   document are updated in the same change.
7. The status in [status.md](status.md) reflects the evidence.

## 9. Known limitations

- **Search is a plain full-text match.** `plainto_tsquery('simple', ...)`
  does not rank, does not highlight and does not stem. A multi-word term is
  matched as a conjunction of its words, so a note matching only some of them
  is not returned.
- **The `simple` text-search configuration is used deliberately.** ATOM notes
  are technical and contain identifiers and code fragments; the `simple`
  configuration keeps tokenisation predictable. See
  [`schema.md`](../../database/schema.md) §7.
- **The search term is not escaped for the user.** It is bound as a
  parameter and passed to `plainto_tsquery`, which handles it safely, but the
  term is matched as words, not as a phrase.
- **There is no bulk operation.** Archiving, moving or deleting many notes
  takes many requests.
- **Deleting a note is permanent.** The archive flag is the only reversible
  way to remove a note from the default listing.
- **The title is the only mandatory text.** A note may have an empty body,
  which is a valid state, not an error.

## 10. Changelog references

Substantial changes are recorded in
[`change-log.md`](../../engineering/change-log.md). The feature identifier is
stable; the entries are appended and never rewritten.

# Notes — architecture

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-NOTE-001`](specification.md)

How the notes feature is built. The public contract is in
[`api.md`](api.md); the agreed requirements are in
[`specification.md`](specification.md).

## 1. Responsibilities

| Module | Responsibility |
| ------ | -------------- |
| `src/notes/handlers.rs` | HTTP only: extraction, status codes, response serialization. |
| `src/notes/service.rs` | Business rules: title and content validation, the ownership-aware use cases, the mapping from a repository miss to `404`. Every client-facing string comes from the message catalogue, which the service holds. |
| `src/notes/repository.rs` | SQL: insert, find owned, list owned with filters, partial update, delete owned. |
| `src/notes/models.rs` | The `Note` persistence shape. |
| `src/notes/dto.rs` | The public contracts: `NoteResponse`, `CreateNoteRequest`, `UpdateNoteRequest`, `NoteQuery`, `NotePage`, and the length limits. |
| `src/helpers/validation.rs` | The shared text validators: `required_text` and `bounded_text`. |
| `src/pagination.rs` | The shared pagination convention: `PaginationQuery`, `Pagination`, `Page<T>`. |

The feature depends on `src/pagination.rs` for the list envelope, on the
shared text validators in `src/helpers/validation.rs` for the title and the
content, and on the `AuthUser` extractor in `src/api/middleware.rs` for the
caller's identity. It depends on nothing else in the domain: notes do not
know about projects or tasks.

## 2. Interfaces

### 2.1 `NoteService`

| Method | Contract |
| ------ | -------- |
| `create(owner, request)` | Validates the title and the content, inserts the note for `owner`, returns a `NoteResponse`. |
| `list(owner, query, pagination)` | Applies the search and archive filters, clamps the pagination, returns a `Page<NoteResponse>`. |
| `get(owner, id)` | Loads the note **owned by** `owner`; `404` otherwise. |
| `update(owner, id, request)` | Validates only the fields that are present, applies a partial update, `404` otherwise. |
| `delete(owner, id)` | Deletes the note owned by `owner`; `404` otherwise. |

The `owner` argument is the authenticated `user_id`. It is the first
parameter of every method, which makes it impossible to call a notes method
without saying whose notes are meant.

`NoteService` is built once, in `AppState::new`, and is never rebuilt per
request. It holds the `NoteRepository` and the message catalogue, so the
client-facing wording of an error is defined in `config/messages.toml` and
nowhere else.

### 2.2 `NoteRepository`

| Method | SQL |
| ------ | --- |
| `insert(id, user_id, title, content)` | `INSERT INTO notes (id, user_id, title, content) VALUES ($1, $2, $3, $4) RETURNING ...` |
| `find_owned(id, user_id)` | `SELECT ... FROM notes WHERE id = $1 AND user_id = $2` |
| `list_owned(user_id, filters, limit, offset)` | `SELECT ... FROM notes WHERE user_id = $1 AND (COALESCE($2::boolean, FALSE) = is_archived) AND ($3::text IS NULL OR search_vector @@ plainto_tsquery('simple', $3)) ORDER BY created_at DESC, id DESC LIMIT $4 OFFSET $5`, plus a matching `COUNT(*)` |
| `update_owned(id, user_id, title, content, is_archived)` | `UPDATE notes SET title = COALESCE($3, title), content = COALESCE($4, content), is_archived = COALESCE($5, is_archived), updated_at = NOW() WHERE id = $1 AND user_id = $2 RETURNING ...` |
| `delete_owned(id, user_id)` | `DELETE FROM notes WHERE id = $1 AND user_id = $2` |

Every statement is parameterised. The only dynamic fragment in the module is
the static `COALESCE($2::boolean, FALSE) = is_archived` predicate, which
contains no user input.

### 2.3 Validation

`validate_title` and `validate_content` in `src/notes/service.rs` are free
functions, so they can be unit tested and reused. Both delegate to the shared
validators in `src/helpers/validation.rs`:

| Shared validator | Behaviour | Used by |
| ---------------- | --------- | ------- |
| `required_text` | Trims the value, rejects a value that is empty after trimming, and rejects a value over the maximum. | `validate_title` |
| `bounded_text` | Keeps the value verbatim, and rejects a value over the maximum. | `validate_content` |
| `validate_search` | Trims the search term and rejects a term over 200 characters. An empty term means "no filter" rather than an error. | `NoteService::list` |

The trimming behaviour is deliberately not uniform across the API: a title, a
name and a display name are trimmed, while a body and a description are kept
verbatim, so that a body of spaces is stored as it was sent. The database
`CHECK` constraints measure the stored value, so the two must agree.

The length limits live in `src/notes/dto.rs` as `MAX_TITLE_LENGTH` (200) and
`MAX_CONTENT_LENGTH` (100 000), and the database repeats them as check
constraints.

## 3. Data ownership

| Table | Owner column | Meaning |
| ----- | ------------ | ------- |
| `notes` | `user_id` → `users (id) ON DELETE CASCADE` | A note belongs to exactly one account. Deleting the account deletes the notes. |

The feature owns exactly one table. It reads no other table, so a note can
never leak information about another user's data through a join.

The full definition is in [`schema.md`](../../database/schema.md); the table
was introduced by `migrations/0002_notes.sql`.

## 4. Invariants

| # | Invariant | Enforced by |
| - | --------- | ----------- |
| 1 | A note belongs to exactly one user. | The non-null `user_id` foreign key and `ON DELETE CASCADE`. |
| 2 | A note of another account is invisible, and the API answers `404`. | The `user_id = $2` predicate in every statement; `AppError::NotFound` in the service. |
| 3 | A listing contains only the caller's notes. | The `user_id = $1` predicate in both the item query and the `COUNT(*)` query. |
| 4 | A search cannot reach another account's notes. | The search condition and the ownership predicate are in the same statement, so the two are always applied together. |
| 5 | A title is not empty and is at most 200 characters. | `validate_title` in the service and `notes_title_length_chk` in the database. |
| 6 | A body is at most 100 000 characters. | `validate_content` and `notes_content_length_chk`. |
| 7 | An archived note is hidden from the default listing. | The tri-state predicate `COALESCE($2::boolean, FALSE) = is_archived`. |
| 8 | A partial update leaves an omitted field untouched. | `Option` fields in the request, `COALESCE($n, column)` in SQL. |
| 9 | The list order is total and stable. | `ORDER BY created_at DESC, id DESC`, with a UUID as the tie-breaker. |
| 10 | The search column always reflects the current title and body. | It is a generated stored column, recomputed by PostgreSQL on every write. |
| 11 | `updated_at` moves when a note changes. | `updated_at = NOW()` in the update statement. |

## 5. Error handling

| Situation | Error | Status |
| --------- | ----- | ------ |
| Empty or over-long title, over-long content | `validation_failed` with a violation naming the field | `422` |
| The note does not exist, or belongs to another account | `not_found` | `404` |
| The body is not valid JSON | `bad_request` | `400` |
| The `note_id` path parameter is not a UUID | `bad_request` | `400` |
| A query parameter cannot be parsed | `bad_request` | `400` |
| No or invalid credentials | `unauthorized` | `401` |
| Database failure | `database_error` | `500` |

A missing note and a note of another account produce the **same** response.
That is deliberate: a `403` would confirm that the identifier exists.

## 6. The security boundary

```text
                 ┌────────────────────────────────────────────┐
  bearer token ─►│ GET|POST /notes                           │
                 │ GET|PATCH|DELETE /notes/{note_id}          │  authenticated
                 └────────────────────────────────────────────┘
                                   │
                                   │  user_id from the credential only
                                   ▼
                    every statement carries `user_id = $n`
```

- There is no anonymous route in this feature.
- The `owner` argument of every service method comes from the `AuthUser`
  extractor and from nowhere else.
- The authorization decision is made in SQL, not in the handler, so a new
  code path in the handler cannot accidentally skip it. This is the rule
  recorded in
  [`ADR-0005-ownership-enforced-in-sql.md`](../../decisions/ADR-0005-ownership-enforced-in-sql.md).
- The search index is a column **of the note**, not a global index over all
  notes. A full-text search therefore cannot physically return another
  account's row.

## 7. The request path

### 7.1 Listing

```text
GET /api/v1/notes?search=release&archived=false&limit=20&offset=0
  │
  ├─ api::router layers, outermost first: CORS, trace, body limit, request id (set, then propagate), timeout, panic guard
  ├─ api::middleware::AuthUser
  │    └─ decode the bearer token → user_id          missing or invalid → 401
  ├─ api::extract::Query<NotesListQuery>   one parse; unparsable → 400
  ├─ into_parts() → (NoteQuery, PaginationQuery)
  ├─ notes::handlers::list_notes
  │    └─ notes::service::NoteService::list
  │         ├─ NoteFilters { search, archived }
  │         ├─ pagination.effective_limit()          the configured bounds
  │         │                                        20 by default, capped at 100
  │         ├─ pagination.effective_offset()         negative → 0
  │         └─ notes::repository::NoteRepository::list_owned
  │              ├─ SELECT ... WHERE user_id = $1
  │              │             AND (COALESCE($2::boolean, FALSE) = is_archived)
  │              │             AND ($3::text IS NULL OR search_vector @@ plainto_tsquery('simple', $3))
  │              │        ORDER BY created_at DESC, id DESC LIMIT $4 OFFSET $5
  │              └─ SELECT COUNT(*) ... the same predicate
  └─ 200 OK, NotePage { items, pagination }
```

### 7.2 Updating

```text
PATCH /api/v1/notes/{note_id}
  │
  ├─ api::middleware::AuthUser → user_id
  ├─ api::extract::Path<Uuid>                        not a UUID → 400
  ├─ api::extract::Json<UpdateNoteRequest>           not JSON → 400
  ├─ notes::service::NoteService::update
  │    ├─ title   → validate_title,    only when present
  │    ├─ content → validate_content,  only when present
  │    └─ notes::repository::NoteRepository::update_owned
  │         UPDATE notes
  │         SET title       = COALESCE($3, title),
  │             content     = COALESCE($4, content),
  │             is_archived = COALESCE($5, is_archived),
  │             updated_at  = NOW()
  │         WHERE id = $1 AND user_id = $2
  │         RETURNING ...
  │         └─ no row → 404
  └─ 200 OK, NoteResponse
```

Note the two-stage validation: the service validates only the fields that
are present, and the repository writes only those fields. A field that was
omitted is never written, so it cannot be nulled by accident.

## 8. Related files

| File | Role |
| ---- | ---- |
| `src/api/routes.rs` | Registers the five note routes and the `NoteResponse`, `NotePage`, `CreateNoteRequest` and `UpdateNoteRequest` schemas. |
| `src/api/extract.rs` | The `Path`, `Query` and `Json` extractors that keep the error envelope intact. |
| `src/api/middleware.rs` | `AuthUser`, the source of the ownership predicate. |
| `src/pagination.rs` | The shared list envelope and the limit and offset clamping. |
| `src/helpers/validation.rs` | The shared text validators used by the notes, projects and tasks services. |
| `src/notes/**` | The feature itself. |
| `migrations/0002_notes.sql` | The `notes` table, its indexes and the generated `search_vector` column. |

## 9. Related decisions

- [`ADR-0005-ownership-enforced-in-sql.md`](../../decisions/ADR-0005-ownership-enforced-in-sql.md)
  — ownership as a SQL predicate.
- [`ADR-0004-documentation-as-project-truth.md`](../../decisions/ADR-0004-documentation-as-project-truth.md)
  — the documentation tree as part of the product.
- [`ADR-0001-modular-monolith.md`](../../decisions/ADR-0001-modular-monolith.md)
  — why notes is a module and not a service of its own.

The wider context is in
[`data-architecture.md`](../../architecture/data-architecture.md) and
[`request-lifecycle.md`](../../architecture/request-lifecycle.md).

## 10. Limitations of this design

- **The search configuration is fixed at `simple`.** Changing it later means
  reindexing every note, which is a migration, not a configuration change.
- **The generated column is stored, not computed.** The column takes space
  on every row and is rewritten on every update. That is the price of the
  GIN index.
- **The count query repeats the item query.** `COUNT(*)` runs the same
  predicates over the same rows. For a very large collection this is the
  expensive part of a listing.
- **The archive filter has no "both" mode.** A client that wants active and
  archived notes must make two requests.
- **The service is built once, so the catalogue cannot change at run time.**
  `NoteService::new` is called in `AppState::new`, not in the handler, and the
  service holds the message catalogue behind an `Arc`. A change to
  `config/messages.toml` therefore needs a restart.
- **No transaction spans two statements.** The feature performs single-statement
  writes, so no transaction is needed; the moment it does, a transaction
  belongs in the repository.

# Notes — API

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-NOTE-001`](specification.md)

The HTTP contract of the notes feature. Every path is relative to the API
root. The general rules are in
[`conventions.md`](../../api/conventions.md) and
[`error-contract.md`](../../api/error-contract.md).

## 1. Endpoint summary

| Method | Path | Authentication | Success |
| ------ | ---- | -------------- | ------- |
| `GET` | `/api/v1/notes` | bearer token | `200` `NotePage` |
| `POST` | `/api/v1/notes` | bearer token | `201` `NoteResponse` |
| `GET` | `/api/v1/notes/{note_id}` | bearer token | `200` `NoteResponse` |
| `PATCH` | `/api/v1/notes/{note_id}` | bearer token | `200` `NoteResponse` |
| `DELETE` | `/api/v1/notes/{note_id}` | bearer token | `204` no body |

## 2. Shared shapes

### `NoteResponse`

```json
{
  "id": "1f9c6d2e-8f31-4a0b-9d55-2c7a1b6e4f90",
  "title": "Release checklist",
  "content": "cut the tag, run the smoke tests",
  "is_archived": false,
  "created_at": "2026-10-21T09:15:00.000000Z",
  "updated_at": "2026-10-21T09:15:00.000000Z"
}
```

### `NotePage`

The shared list envelope.

```json
{
  "items": [ { "...": "NoteResponse" } ],
  "pagination": {
    "limit": 20,
    "offset": 0,
    "total": 42,
    "has_more": true
  }
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `items` | array | The notes on this page. |
| `pagination.limit` | integer | The limit that was applied, after clamping. |
| `pagination.offset` | integer | The offset that was applied, after clamping. |
| `pagination.total` | integer | The number of notes matching the filter, ignoring pagination. |
| `pagination.has_more` | boolean | Whether more items exist after this page. |

### Query parameters — `GET /api/v1/notes`

| Parameter | Type | Default | Meaning |
| --------- | ---- | ------- | ------- |
| `search` | string | — | Free-text match over the title and the body. Trimmed; an empty value is treated as no search. |
| `archived` | boolean | `false` | `true` returns only archived notes, `false` or omitted returns only active notes. |
| `limit` | integer | `20` | Page size. Capped at 100. A value of zero or less falls back to 20. |
| `offset` | integer | `0` | Items to skip. A negative value falls back to 0. |

## 3. `GET /api/v1/notes`

Lists the caller's notes, newest first.

**Authentication:** bearer token required.

### Response — `200 OK`

```json
{
  "items": [
    {
      "id": "1f9c6d2e-8f31-4a0b-9d55-2c7a1b6e4f90",
      "title": "Release checklist",
      "content": "cut the tag, run the smoke tests",
      "is_archived": false,
      "created_at": "2026-10-21T09:15:00.000000Z",
      "updated_at": "2026-10-21T09:15:00.000000Z"
    }
  ],
  "pagination": { "limit": 20, "offset": 0, "total": 1, "has_more": false }
}
```

The order is `created_at DESC`, then `id DESC`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | A query parameter cannot be parsed, for example `limit=abc` or `archived=maybe`. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `500` | `database_error` | The database failed. |

```bash
curl -s "http://127.0.0.1:3000/api/v1/notes?search=release&limit=10&offset=0" \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 4. `POST /api/v1/notes`

Creates a note.

**Authentication:** bearer token required.

### Request

```json
{
  "title": "Release checklist",
  "content": "cut the tag, run the smoke tests"
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `title` | string | yes | Trimmed. Must not be empty. At most 200 characters. |
| `content` | string | no | Defaults to the empty string. At most 100 000 characters. |

### Response — `201 Created`

The created note, with `is_archived` `false` and both timestamps.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `422` | `validation_failed` | The title is empty or too long, or the content is too long. `details` names the field. |
| `500` | `database_error` | The database failed. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/notes \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"title":"Release checklist","content":"cut the tag, run the smoke tests"}'
```

## 5. `GET /api/v1/notes/{note_id}`

Reads one note.

**Authentication:** bearer token required.

`note_id` is a UUID. A value that is not a UUID is a malformed request.

### Response — `200 OK`

The note, as a `NoteResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `note_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The note does not exist, or it belongs to another account. The two are indistinguishable. |

```bash
curl -s http://127.0.0.1:3000/api/v1/notes/1f9c6d2e-8f31-4a0b-9d55-2c7a1b6e4f90 \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 6. `PATCH /api/v1/notes/{note_id}`

Partially updates one note.

**Authentication:** bearer token required.

### Request

Every field is optional. An omitted field is left untouched.

```json
{
  "title": "Release checklist v2"
}
```

```json
{
  "content": "cut the tag, run the smoke tests, publish the notes",
  "is_archived": true
}
```

| Field | Type | Rules when present |
| ----- | ---- | ------------------ |
| `title` | string | Trimmed. Must not be empty. At most 200 characters. |
| `content` | string | At most 100 000 characters. An empty string clears the body. |
| `is_archived` | boolean | `true` archives the note, `false` brings it back. |

### Response — `200 OK`

The updated note. `updated_at` is newer than before; `created_at` is
unchanged.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON, or `note_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The note does not exist, or it belongs to another account. |
| `422` | `validation_failed` | The title is empty or too long, or the content is too long. |

```bash
curl -i -X PATCH http://127.0.0.1:3000/api/v1/notes/1f9c6d2e-8f31-4a0b-9d55-2c7a1b6e4f90 \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"title":"Release checklist v2"}'
```

## 7. `DELETE /api/v1/notes/{note_id}`

Deletes one note permanently.

**Authentication:** bearer token required.

### Request

No body.

### Response — `204 No Content`

No body.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `note_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The note does not exist, or it belongs to another account. |

```bash
curl -i -X DELETE http://127.0.0.1:3000/api/v1/notes/1f9c6d2e-8f31-4a0b-9d55-2c7a1b6e4f90 \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 8. Notes on the OpenAPI document

The five operations are registered in `src/api/routes.rs` and appear in the
generated document at `/api/v1/openapi.json` with the `NoteResponse`,
`NotePage`, `CreateNoteRequest` and `UpdateNoteRequest` schemas. The `bearer`
security scheme is applied to all five.

# Projects — API

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-PROJ-001`](specification.md)

The HTTP contract of the projects feature. Every path is relative to the API
root. The general rules are in
[`conventions.md`](../../api/conventions.md) and
[`error-contract.md`](../../api/error-contract.md).

## 1. Endpoint summary

| Method | Path | Authentication | Success |
| ------ | ---- | -------------- | ------- |
| `GET` | `/api/v1/projects` | bearer token | `200` `ProjectPage` |
| `POST` | `/api/v1/projects` | bearer token | `201` `ProjectResponse` |
| `GET` | `/api/v1/projects/{project_id}` | bearer token | `200` `ProjectResponse` |
| `PATCH` | `/api/v1/projects/{project_id}` | bearer token | `200` `ProjectResponse` |
| `DELETE` | `/api/v1/projects/{project_id}` | bearer token | `204` no body |

The task endpoints that live under a project are documented in
[`../tasks/api.md`](../tasks/api.md).

## 2. Shared shapes

### `ProjectResponse`

```json
{
  "id": "3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31",
  "name": "ATOM Phase 1",
  "description": "Backend foundation for the personal workspace platform.",
  "created_at": "2026-10-21T09:15:00.000000Z",
  "updated_at": "2026-10-21T09:15:00.000000Z"
}
```

### `ProjectPage`

```json
{
  "items": [ { "...": "ProjectResponse" } ],
  "pagination": {
    "limit": 20,
    "offset": 0,
    "total": 3,
    "has_more": false
  }
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `items` | array | The projects on this page. |
| `pagination.limit` | integer | The limit that was applied, after clamping. |
| `pagination.offset` | integer | The offset that was applied, after clamping. |
| `pagination.total` | integer | The number of projects matching the filter, ignoring pagination. |
| `pagination.has_more` | boolean | Whether more items exist after this page. |

### Query parameters — `GET /api/v1/projects`

The endpoint accepts the two shared pagination parameters, which are declared
once for the whole API in `PaginationQuery` and described in
[`conventions.md`](../../api/conventions.md) §5, and one filter of its own:

| Parameter | Type | Default | Meaning |
| --------- | ---- | ------- | ------- |
| `search` | string | — | Case-insensitive substring match over the name **or** the description. Trimmed; an empty value is treated as no search. At most 200 characters. |

`limit` and `offset` are the shared pagination parameters; `search` is the
filter of the projects feature, and it is the only parameter this endpoint
declares of its own. A `search` term longer than 200 characters is a `422`
validation failure that names the `search` field; the bound is enforced by the
shared validator `validate_search` in `src/helpers/validation.rs`, and its
wording comes from the message catalogue in `config/messages.toml`.

## 3. `GET /api/v1/projects`

Lists the caller's projects, newest first.

**Authentication:** bearer token required.

### Response — `200 OK`

```json
{
  "items": [
    {
      "id": "3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31",
      "name": "ATOM Phase 1",
      "description": "Backend foundation for the personal workspace platform.",
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
| `400` | `bad_request` | A query parameter cannot be parsed, for example `limit=abc`. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `422` | `validation_failed` | The `search` term is longer than 200 characters. `details` names the field. |
| `500` | `database_error` | The database failed. |

```bash
curl -s "http://127.0.0.1:3000/api/v1/projects?search=phase&limit=10" \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 4. `POST /api/v1/projects`

Creates a project.

**Authentication:** bearer token required.

### Request

```json
{
  "name": "ATOM Phase 1",
  "description": "Backend foundation for the personal workspace platform."
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `name` | string | yes | Trimmed. Must not be empty. At most 120 characters. |
| `description` | string | no | Defaults to the empty string. At most 5 000 characters. |

### Response — `201 Created`

The created project.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `422` | `validation_failed` | The name is empty or too long, or the description is too long. `details` names the field. |
| `500` | `database_error` | The database failed. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/projects \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"name":"ATOM Phase 1","description":"Backend foundation for the personal workspace platform."}'
```

## 5. `GET /api/v1/projects/{project_id}`

Reads one project.

**Authentication:** bearer token required.

`project_id` is a UUID. A value that is not a UUID is a malformed request.

### Response — `200 OK`

The project, as a `ProjectResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `project_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The project does not exist, or it belongs to another account. The two are indistinguishable. |

```bash
curl -s http://127.0.0.1:3000/api/v1/projects/3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31 \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 6. `PATCH /api/v1/projects/{project_id}`

Partially updates one project.

**Authentication:** bearer token required.

### Request

Every field is optional. An omitted field is left untouched.

```json
{
  "name": "ATOM Phase 2"
}
```

| Field | Type | Rules when present |
| ----- | ---- | ------------------ |
| `name` | string | Trimmed. Must not be empty. At most 120 characters. |
| `description` | string | At most 5 000 characters. An empty string clears the description. |

### Response — `200 OK`

The updated project. `updated_at` is newer than before; `created_at` is
unchanged.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON, or `project_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The project does not exist, or it belongs to another account. |
| `422` | `validation_failed` | The name is empty or too long, or the description is too long. |

```bash
curl -i -X PATCH http://127.0.0.1:3000/api/v1/projects/3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31 \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"name":"ATOM Phase 2"}'
```

## 7. `DELETE /api/v1/projects/{project_id}`

Deletes one project **and every task inside it**.

**Authentication:** bearer token required.

### Request

No body.

### Response — `204 No Content`

No body. The response does not report how many tasks were removed.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `project_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The project does not exist, or it belongs to another account. |

```bash
curl -i -X DELETE http://127.0.0.1:3000/api/v1/projects/3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31 \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 8. Notes on the OpenAPI document

The five operations are registered in `src/api/routes.rs` and appear in the
generated document at `/api/v1/openapi.json` with the `ProjectResponse`,
`ProjectPage`, `CreateProjectRequest` and `UpdateProjectRequest` schemas.
The `bearer` security scheme is applied to all five.

The handler takes **one** `Query<ProjectsListQuery>` extractor over the whole
query string. That type splits into the projects filter (`ProjectQuery`) and
the shared `PaginationQuery`, so the string is parsed once, and the published
parameter set is `limit`, `offset` and `search`, each declared exactly once.
The convention is explained once in
[`conventions.md`](../../api/conventions.md) §5 and is not repeated here.

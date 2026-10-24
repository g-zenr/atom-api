# Tasks — API

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-TASK-001`](specification.md)

The HTTP contract of the tasks feature. Every path is relative to the API
root. The general rules are in
[`conventions.md`](../../api/conventions.md) and
[`error-contract.md`](../../api/error-contract.md).

## 1. Endpoint summary

| Method | Path | Authentication | Success |
| ------ | ---- | -------------- | ------- |
| `GET` | `/api/v1/projects/{project_id}/tasks` | bearer token | `200` `TaskPage` |
| `POST` | `/api/v1/projects/{project_id}/tasks` | bearer token | `201` `TaskResponse` |
| `GET` | `/api/v1/tasks/{task_id}` | bearer token | `200` `TaskResponse` |
| `PATCH` | `/api/v1/tasks/{task_id}` | bearer token | `200` `TaskResponse` |
| `DELETE` | `/api/v1/tasks/{task_id}` | bearer token | `204` no body |

## 2. Shared shapes

### `TaskResponse`

```json
{
  "id": "8b2d4f10-77c3-4e59-8a16-0d9e3f5c7b28",
  "project_id": "3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31",
  "title": "Draft the authentication ADR",
  "description": "Record the refresh-token rotation decision.",
  "status": "open",
  "priority": "high",
  "position": 1000.0,
  "created_at": "2026-10-21T09:20:00.000000Z",
  "updated_at": "2026-10-21T09:20:00.000000Z",
  "completed_at": null
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `id` | string (UUID) | The task identifier. |
| `project_id` | string (UUID) | The project the task belongs to. |
| `title` | string | The task title. |
| `description` | string | The task body. May be empty. |
| `status` | string | `open`, `in_progress` or `done`. |
| `priority` | string | `low`, `medium`, `high` or `urgent`. |
| `position` | number | The manual ordering key inside the project. |
| `created_at` | string | RFC 3339 UTC. |
| `updated_at` | string | RFC 3339 UTC. |
| `completed_at` | string or null | RFC 3339 UTC. Present exactly when `status` is `done`, `null` otherwise. |

### `TaskPage`

```json
{
  "items": [ { "...": "TaskResponse" } ],
  "pagination": {
    "limit": 20,
    "offset": 0,
    "total": 2,
    "has_more": false
  }
}
```

### Query parameters — `GET /api/v1/projects/{project_id}/tasks`

The endpoint accepts the two shared pagination parameters, which are declared
once for the whole API in `PaginationQuery` and described in
[`conventions.md`](../../api/conventions.md) §5, and two filters of its own:

| Parameter | Type | Default | Meaning |
| --------- | ---- | ------- | ------- |
| `status` | string | — | Filter by status: `open`, `in_progress` or `done`. |
| `priority` | string | — | Filter by priority: `low`, `medium`, `high` or `urgent`. |

`limit` and `offset` are the shared pagination parameters; `status` and
`priority` are the filters of the tasks feature, and they are the only
parameters this endpoint declares of its own. The convention is described in
[`conventions.md`](../../api/conventions.md) §5.

## 3. `GET /api/v1/projects/{project_id}/tasks`

Lists the tasks of one project.

**Authentication:** bearer token required.

### Response — `200 OK`

```json
{
  "items": [
    {
      "id": "8b2d4f10-77c3-4e59-8a16-0d9e3f5c7b28",
      "project_id": "3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31",
      "title": "Draft the authentication ADR",
      "description": "Record the refresh-token rotation decision.",
      "status": "open",
      "priority": "high",
      "position": 1000.0,
      "created_at": "2026-10-21T09:20:00.000000Z",
      "updated_at": "2026-10-21T09:20:00.000000Z",
      "completed_at": null
    }
  ],
  "pagination": { "limit": 20, "offset": 0, "total": 1, "has_more": false }
}
```

The order is `position ASC`, then `created_at ASC`, then `id ASC`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `project_id` is not a UUID, or a query parameter cannot be parsed, including an unknown `status` or `priority`. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The project does not exist, or it belongs to another account. |
| `500` | `database_error` | The database failed. |

A project that exists but has no matching task returns `200` with an empty
`items` array.

```bash
curl -s "http://127.0.0.1:3000/api/v1/projects/3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31/tasks?status=open" \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 4. `POST /api/v1/projects/{project_id}/tasks`

Creates a task inside a project.

**Authentication:** bearer token required.

### Request

```json
{
  "title": "Draft the authentication ADR",
  "description": "Record the refresh-token rotation decision.",
  "status": "open",
  "priority": "high",
  "position": 1000.0
}
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `title` | string | yes | Trimmed. Must not be empty. At most 200 characters. |
| `description` | string | no | Defaults to the empty string. At most 10 000 characters. |
| `status` | string | no | `open`, `in_progress` or `done`. Defaults to `open`. |
| `priority` | string | no | `low`, `medium`, `high` or `urgent`. Defaults to `medium`. |
| `position` | number | no | The manual ordering key. When omitted, the task is placed after the current last task of the project, at `MAX(position) + 1000`. |

### Response — `201 Created`

The created task. `completed_at` is `null` unless `status` was `done`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON, or `project_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The project does not exist, or it belongs to another account. |
| `422` | `validation_failed` | The title is empty or too long, the description is too long, or the `status` or `priority` is not a known variant. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/projects/3c5a91e7-0d64-4b28-8f10-6a2d7e4c9b31/tasks \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"title":"Draft the authentication ADR","description":"Record the refresh-token rotation decision.","priority":"high"}'
```

## 5. `GET /api/v1/tasks/{task_id}`

Reads one task.

**Authentication:** bearer token required.

`task_id` is a UUID. A value that is not a UUID is a malformed request.

### Response — `200 OK`

The task, as a `TaskResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `task_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The task does not exist, or it belongs to another account. The two are indistinguishable. |

```bash
curl -s http://127.0.0.1:3000/api/v1/tasks/8b2d4f10-77c3-4e59-8a16-0d9e3f5c7b28 \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 6. `PATCH /api/v1/tasks/{task_id}`

Partially updates one task.

**Authentication:** bearer token required.

### Request

Every field is optional. An omitted field is left untouched.

```json
{
  "status": "done"
}
```

```json
{
  "title": "Draft the ADR and circulate it",
  "priority": "urgent",
  "position": 500.0
}
```

| Field | Type | Rules when present |
| ----- | ---- | ------------------ |
| `title` | string | Trimmed. Must not be empty. At most 200 characters. |
| `description` | string | At most 10 000 characters. An empty string clears the description. |
| `status` | string | `open`, `in_progress` or `done`. Setting `done` stamps `completed_at`; any other value clears it. |
| `priority` | string | `low`, `medium`, `high` or `urgent`. |
| `position` | number | The manual ordering key. |

### Response — `200 OK`

The updated task. Setting `status` to `done` returns a `completed_at`
timestamp; setting it back to `open` returns `completed_at` as `null` again.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not valid JSON, or `task_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The task does not exist, or it belongs to another account. |
| `422` | `validation_failed` | The title is empty or too long, the description is too long, or the `status` or `priority` is not a known variant. |

```bash
curl -i -X PATCH http://127.0.0.1:3000/api/v1/tasks/8b2d4f10-77c3-4e59-8a16-0d9e3f5c7b28 \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"status":"done"}'
```

## 7. `DELETE /api/v1/tasks/{task_id}`

Deletes one task.

**Authentication:** bearer token required.

### Request

No body.

### Response — `204 No Content`

No body.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | `task_id` is not a UUID. |
| `401` | `unauthorized` | No access token, or an invalid one. |
| `404` | `not_found` | The task does not exist, or it belongs to another account. |

```bash
curl -i -X DELETE http://127.0.0.1:3000/api/v1/tasks/8b2d4f10-77c3-4e59-8a16-0d9e3f5c7b28 \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 8. Notes on the OpenAPI document

The five operations are registered in `src/api/routes.rs` and appear in the
generated document at `/api/v1/openapi.json` with the `TaskResponse`,
`TaskPage`, `CreateTaskRequest` and `UpdateTaskRequest` schemas, and with
`TaskStatus` and `TaskPriority` as named schemas. The `bearer` security
scheme is applied to all five.

The handler takes **one** `Query<TasksListQuery>` extractor over the whole
query string. That type splits into the tasks filters (`TaskQuery`) and the
shared `PaginationQuery`, so the string is parsed once, and the published
parameter set is `limit`, `offset`, `status` and `priority`, plus the
`project_id` path parameter — each declared exactly once. The convention is
explained once in [`conventions.md`](../../api/conventions.md) §5 and is not
repeated here.

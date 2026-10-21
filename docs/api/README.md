# ATOM API

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent

This directory holds the HTTP contract of the ATOM API. It describes the
contract as a whole; the per-feature endpoint references live with the
features themselves.

| Document | Contents |
| -------- | -------- |
| [`conventions.md`](conventions.md) | Versioning, resource naming, HTTP methods and status codes, the pagination convention, timestamps, identifiers, PATCH semantics, content types, the body size limit, CORS, and the health endpoints. |
| [`error-contract.md`](error-contract.md) | The single error envelope, every error code with its HTTP status, the field-level validation detail format, and the rule that internal details are never exposed. |

The per-feature endpoint references are:

| Feature | Document |
| ------- | -------- |
| Authentication | [`../features/authentication/api.md`](../features/authentication/api.md) |
| Notes | [`../features/notes/api.md`](../features/notes/api.md) |
| Projects | [`../features/projects/api.md`](../features/projects/api.md) |
| Tasks | [`../features/tasks/api.md`](../features/tasks/api.md) |

## 1. Where the OpenAPI document lives

The OpenAPI document is **generated from the handlers**, so it cannot drift
silently away from the code.

- The document is built by the `utoipa` derive on `ApiDoc` in
  `src/api/routes.rs`. Every operation, request body and response schema is
  registered there.
- It is served at **`GET /api/v1/openapi.json`**.
- The interactive explorer is served at **`GET /api/v1/docs/`** by
  `utoipa-swagger-ui`, which is vendored into the binary, so the explorer
  works without network access.

The document describes the API as it is built. Its `info.version` is the crate
version from `Cargo.toml` (`0.1.0`), and its `info.title` is the configured
`api.title` (`ATOM API` by default).

## 2. How to regenerate and serve it

There is **no separate generation step**. The document is a Rust value
compiled into the binary:

```bash
# Start the API locally.
cargo run

# Then read the document.
curl -s http://127.0.0.1:3000/api/v1/openapi.json

# Or open the explorer.
# http://127.0.0.1:3000/api/v1/docs/
```

To change the document, change the `#[utoipa::path(...)]` annotations on the
handlers in `src/auth/handlers.rs`, `src/notes/handlers.rs`,
`src/projects/handlers.rs`, `src/tasks/handlers.rs` and `src/health.rs`, and
add any new schema to the `components(schemas(...))` list in
`src/api/routes.rs`.

## 3. Endpoint inventory

Every path is relative to the API root. `Auth` states whether a bearer token
is required.

| Method | Path | Auth | Success | Feature |
| ------ | ---- | ---- | ------- | ------- |
| `GET` | `/health/live` | — | `200` | health |
| `GET` | `/health/ready` | — | `200` / `503` | health |
| `POST` | `/api/v1/auth/register` | — | `201` | authentication |
| `POST` | `/api/v1/auth/login` | — | `200` | authentication |
| `POST` | `/api/v1/auth/refresh` | — | `200` | authentication |
| `POST` | `/api/v1/auth/logout` | bearer | `204` | authentication |
| `GET` | `/api/v1/auth/me` | bearer | `200` | authentication |
| `GET` | `/api/v1/notes` | bearer | `200` | notes |
| `POST` | `/api/v1/notes` | bearer | `201` | notes |
| `GET` | `/api/v1/notes/{note_id}` | bearer | `200` | notes |
| `PATCH` | `/api/v1/notes/{note_id}` | bearer | `200` | notes |
| `DELETE` | `/api/v1/notes/{note_id}` | bearer | `204` | notes |
| `GET` | `/api/v1/projects` | bearer | `200` | projects |
| `POST` | `/api/v1/projects` | bearer | `201` | projects |
| `GET` | `/api/v1/projects/{project_id}` | bearer | `200` | projects |
| `PATCH` | `/api/v1/projects/{project_id}` | bearer | `200` | projects |
| `DELETE` | `/api/v1/projects/{project_id}` | bearer | `204` | projects |
| `GET` | `/api/v1/projects/{project_id}/tasks` | bearer | `200` | tasks |
| `POST` | `/api/v1/projects/{project_id}/tasks` | bearer | `201` | tasks |
| `GET` | `/api/v1/tasks/{task_id}` | bearer | `200` | tasks |
| `PATCH` | `/api/v1/tasks/{task_id}` | bearer | `200` | tasks |
| `DELETE` | `/api/v1/tasks/{task_id}` | bearer | `204` | tasks |
| `GET` | `/api/v1/openapi.json` | — | `200` | the document itself |
| `GET` | `/api/v1/docs/` | — | `200` | the explorer |

The health endpoints and the OpenAPI routes are the only routes that do not
require a credential. `POST /api/v1/auth/register`, `POST /api/v1/auth/login`
and `POST /api/v1/auth/refresh` are unauthenticated by design and are rate
limited by client IP.

## 4. The request pipeline

Every request passes through the same layers, in this order:

```text
request
  -> cors             an explicit origin allowlist, when one is configured
  -> trace            structured request logging
  -> body limit       a body over APP_MAX_BODY_BYTES is refused (413)
  -> request id       an x-request-id correlation id on the request and the response
  -> timeout          the server stops processing after the configured timeout (408)
  -> panic guard      a panic becomes a 500, never a dropped connection
  -> route            handler -> service -> repository -> PostgreSQL
```

An unmatched route returns `404` with the standard error envelope. The
pipeline is described in full in
[`request-lifecycle.md`](../architecture/request-lifecycle.md).

## 5. Where to look for what

| Question | Document |
| -------- | -------- |
| Which status code does this situation return? | [`error-contract.md`](error-contract.md) |
| How do I page through a list? | [`conventions.md`](conventions.md) §5 |
| What does this endpoint take and return? | The feature's `api.md` |
| Why does the API answer `404` instead of `403`? | [`error-contract.md`](error-contract.md) §6 |
| How is the schema defined? | [`../database/schema.md`](../database/schema.md) |
| What settings does the API read? | [`../../config/README.md`](../../config/README.md) |
| What are the agreed product requirements? | [`../project-truth.md`](../project-truth.md) |

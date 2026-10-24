# Local machine learning — API

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

The HTTP contract of the machine-learning feature. Every path is relative to
the API root. The general rules are in
[`conventions.md`](../../api/conventions.md), the search modes in
[`conventions.md`](../../api/conventions.md) §17, and the error codes in
[`error-contract.md`](../../api/error-contract.md).

**This contract is not implemented.** No route below answers today. It is
recorded as an accepted contract that is not built, so that the implementing
change has a contract to meet.

## 1. Endpoint summary

| Method | Path | Authentication | Success |
| ------ | ---- | -------------- | ------- |
| `POST` | `/api/v1/notes/search` | bearer token | `200` `SearchResponse` |
| `POST` | `/api/v1/job-applications/{application_id}/match` | bearer token | `200` `MatchResponse` |
| `POST` | `/api/v1/ml/reindex` | bearer token | `200` `ReindexResponse` |

All three are authenticated. There is no anonymous route in this feature, and
a request without a valid token is `401 unauthorized`.

`POST /notes/search` is owned by this module even though it lives under
`/notes`. The router has a dynamic segment `{note_id}` there, so
`GET /api/v1/notes/search` is matched by the note read and answers `400`,
because `search` is not a UUID; it is not answered by this operation. A
contract test pins that fact rather than claiming the path is unserved.

## 2. Shared shapes

### 2.1 The modes

| Value | Meaning |
| ----- | ------- |
| `lexical` | PostgreSQL full-text match. No model. |
| `semantic` | Cosine similarity against the caller's embeddings. |
| `hybrid` | Bounded lexical candidates re-ranked by cosine similarity. **The default.** |

### 2.2 The degradation fields

`SearchResponse` carries the four degradation fields:

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `requested_mode` | enum | What the client asked for. |
| `mode` | enum | What was applied. Differs from `requested_mode` only when degraded. |
| `degraded` | boolean | `true` when a scored mode was requested and lexical results were returned. |
| `degraded_reason` | enum or `null` | `ml_disabled`, `model_missing`, `inference_timeout`, or `null`. |

`MatchResponse` does **not** carry them: a match needs a model, so an
unavailable model is a `409`, not a degraded answer (`ML-R06`). The rule is in
[`conventions.md`](../../api/conventions.md) §17 and the decision in
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).

A deployment whose configuration yields the **disabled** embedder —
`ml.enabled = false`, or a model file that cannot be loaded — answers a
`semantic` or `hybrid` search with `degraded_reason` `ml_disabled` or
`model_missing`, and answers a match with the `409` above. `deterministic` is
a valid `ml.backend` value, so a test deployment can run the scored modes
without a model file; the seeder constructs the deterministic embedder
directly instead of reading the configuration. See
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §7.

### 2.3 `SearchHit`

```json
{
  "note": {
    "id": "1f9c6d2e-8f31-4a0b-9d55-2c7a1b6e4f90",
    "title": "Kubernetes rollout notes",
    "content": "blue-green with a canary",
    "is_archived": false,
    "created_at": "2026-10-21T09:15:00.000000Z",
    "updated_at": "2026-10-21T09:15:00.000000Z"
  },
  "score": 0.8123
}
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `note` | `NoteResponse` | The note, in the shape the notes feature already publishes. |
| `score` | number or `null` | The cosine similarity in `[0,1]`, rounded to four decimals. `null` in lexical mode. |

`score` is a JSON **number**, not a string: it is a similarity, not money.

### 2.4 `SearchResponse`

```json
{
  "requested_mode": "hybrid",
  "mode": "lexical",
  "degraded": true,
  "degraded_reason": "ml_disabled",
  "items": [ { "note": { "...": "NoteResponse" }, "score": null } ],
  "pagination": { "limit": 20, "offset": 0, "total": 3, "has_more": false }
}
```

`pagination` is the shared envelope. Its `total` means:

| Mode | `total` is |
| ---- | ---------- |
| `lexical` | The number of the caller's active notes that match the query. |
| `semantic` | The number of the caller's notes that have an embedding for the active model. |
| `hybrid` | The number of **scored candidates** after the bounded re-rank, which is never more than `ml.candidate_limit`. |

A client must not read the hybrid `total` as "every relevant note". It is the
size of the candidate set, and that is why the number is documented here
rather than left to inference.

### 2.5 `MatchResponse`

```json
{
  "application_id": "0f1e2d3c-4b5a-4968-8776-5a4b3c2d1e0f",
  "mode": "hybrid",
  "matches": [
    {
      "note_id": "1f9c6d2e-8f31-4a0b-9d55-2c7a1b6e4f90",
      "title": "Kubernetes rollout notes",
      "score": 0.7441
    }
  ]
}
```

`matches` is ordered by `score DESC`, then `note_id ASC`, and is bounded by the
request's `limit`. It is **not** paginated; a match is a short ranked list,
not a collection to page through.

A match is only defined when a model is available. When ML is disabled or the
model is missing, the request is a `409 conflict` rather than an empty list:
the operation could not be performed, and saying so is more useful than
returning a ranked list that ranks nothing.

### 2.6 `ReindexResponse`

```json
{
  "model_id": "deterministic-v1",
  "dimension": 384,
  "indexed": 12,
  "skipped": 188,
  "deleted": 0
}
```

| Field | Meaning |
| ----- | ------- |
| `model_id` | The embedder that produced the vectors. |
| `dimension` | The vector length it produces. |
| `indexed` | Rows written or replaced by this run. |
| `skipped` | Sources already embedded with the current model and dimension. |
| `deleted` | Orphaned rows removed. |

## 3. `POST /api/v1/notes/search`

Searches the caller's notes in one of three modes.

**Authentication:** bearer token required.

**Query parameters:** the shared `limit` and `offset` from
[`conventions.md`](../../api/conventions.md) §5, with the same defaults and
the same clamp.

### Request

```json
{ "query": "container orchestration", "mode": "hybrid" }
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `query` | string | yes | Trimmed, 1–200 characters. |
| `mode` | enum | no | `lexical`, `semantic` or `hybrid`. Defaults to `hybrid`. |

### Response — `200 OK`

A `SearchResponse`. The order is:

- `lexical`: the notes order, `created_at DESC, id DESC`, with `score` `null`;
- `semantic` and `hybrid`: `score DESC`, then `note_id ASC`.

In `hybrid`, an item always has a numeric score. In `semantic`, the same. In
`lexical`, the score is always `null`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or `limit`/`offset` cannot be parsed. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `422` | `validation_failed` | `query` is empty or over 200 characters, or `mode` is not one of the three values (an unknown enum in a **body** is `422`, per the error contract). |
| `500` | `database_error` | The database failed. |

A missing model never produces `503`, `500` or `408`. It produces a `200` with
`degraded` `true` and `mode` `lexical`, which is the whole point of
`ML-R06`.

```bash
curl -i -X POST "http://127.0.0.1:3000/api/v1/notes/search?limit=10" \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"query":"container orchestration","mode":"hybrid"}'
```

A degraded answer looks like this and is still `200`:

```json
{
  "requested_mode": "hybrid",
  "mode": "lexical",
  "degraded": true,
  "degraded_reason": "model_missing",
  "items": [ { "note": { "...": "NoteResponse" }, "score": null } ],
  "pagination": { "limit": 10, "offset": 0, "total": 2, "has_more": false }
}
```

## 4. `POST /api/v1/job-applications/{application_id}/match`

Ranks the caller's notes against one job application.

**Authentication:** bearer token required.

### Request

```json
{ "mode": "hybrid", "limit": 10 }
```

| Field | Type | Required | Rules |
| ----- | ---- | -------- | ----- |
| `mode` | enum | no | `lexical` is **not** accepted here: a match without a model has no meaning. `semantic` and `hybrid` are accepted; the default is `hybrid`. |
| `limit` | integer | no | Default 10, minimum 1, maximum 50. |

A `limit` of 0, a negative value or a value above 50 is a `422` naming
`limit`, because it is a body field and not a query parameter.

### Response — `200 OK`

A `MatchResponse`.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `400` | `bad_request` | The body is not JSON, or `application_id` is not a UUID. |
| `401` | `unauthorized` | No token, or an invalid one. |
| `404` | `not_found` | The application does not exist, or belongs to another account. |
| `409` | `conflict` | ML is disabled or the model is not loaded. A match cannot be produced without a model. |
| `422` | `validation_failed` | `mode` is `lexical` or unknown, or `limit` is out of range. |
| `500` | `database_error` | The database failed. |
| `408` | `timeout` | The inference exceeded `ml.timeout_seconds` and the request budget. Unlike search, a match has no lexical fallback, so it fails rather than degrades. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/job-applications/0f1e2d3c-4b5a-4968-8776-5a4b3c2d1e0f/match \
  -H "authorization: Bearer $ACCESS_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"mode":"semantic","limit":5}'
```

## 5. `POST /api/v1/ml/reindex`

Rebuilds the caller's embeddings. Idempotent.

**Authentication:** bearer token required.

### Request

No request body is read. A body sent by a client is ignored; the operation is
defined entirely by the caller's identity.

### Response — `200 OK`

A `ReindexResponse`.

### Behaviour

- Only the caller's notes and applications are read, embedded or written.
- A source already embedded with the current `model_id` and `dim` is skipped.
- An embedding whose source no longer exists is deleted.
- Running it twice in a row leaves the row count unchanged and reports
  `indexed` 0 on the second run.

### Errors

| Status | Code | When |
| ------ | ---- | ---- |
| `401` | `unauthorized` | No token, or an invalid one. |
| `409` | `conflict` | ML is disabled or the model is not loaded. Reindexing cannot do what it says, so it is refused rather than silently doing nothing. |
| `500` | `database_error` | The database failed. |
| `408` | `timeout` | The request exceeded the configured request timeout. A reindex of a large corpus can exceed it; the run is idempotent, so the client may start it again and the completed rows are skipped. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/ml/reindex \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 6. Notes on the OpenAPI document

When the feature is implemented, the three operations are registered in
`src/api/routes.rs` and appear in the document at `/api/v1/openapi.json` with
the `SearchRequest`, `SearchResponse`, `SearchHit`, `MatchRequest`,
`MatchResponse`, `MatchHit` and `ReindexResponse` schemas. The `bearer`
security scheme applies to all three.

`POST /notes/search` declares `limit` and `offset` **once each**, from the
shared pagination parameters, and its body carries only `query` and `mode`.
A contract test asserts the published parameter set and that no second
pagination parameter appears in the body schema.

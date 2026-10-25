# Local machine learning — API

> **Status:** Implemented
> **Last updated:** 2026-10-25
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

The HTTP contract of the machine-learning feature. Every path is relative to
the API root. The general rules are in
[`conventions.md`](../../api/conventions.md), the search modes in
[`conventions.md`](../../api/conventions.md) §17, and the error codes in
[`error-contract.md`](../../api/error-contract.md).

## 0. Implementation status

The contract below is implemented: the three operations are mounted, and
`ROUTE_TABLE` and the document agree on **51** operations. Read from the working
tree on 2026-10-24:

| Check | Result |
| ----- | ------ |
| An ML path in `ROUTE_TABLE` and in `api_routes()` | All three |
| The published surface, as pinned by `tests/openapi_contract.rs` | **51** operations |
| `src/ml/handlers.rs`, `service.rs`, `dto.rs`, `repository.rs` | Present |
| The `embeddings` table | Present (migration `0006`) |

What has happened is a run: the 40 cases in [`testing.md`](testing.md) §2 pass,
and the three operations below were observed against `atom_dev` in the manual
degradation transcript. The evidence for the status, and the one requirement
that keeps it out of `Verified`, are in [status.md](status.md) §1.3.

## 1. Endpoint summary

| Method | Path | Authentication | Success |
| ------ | ---- | -------------- | ------- |
| `POST` | `/api/v1/notes/search` | bearer token | `200` `SearchResponse` |
| `POST` | `/api/v1/job-applications/{application_id}/match` | bearer token | `200` `MatchResponse` |
| `POST` | `/api/v1/ml/reindex` | bearer token | `200` `ReindexResponse` |

All three are authenticated. There is no anonymous operation in this feature,
and a request without a valid token is `401 unauthorized`.

`POST /notes/search` is owned by this module even though it lives under
`/notes`. The router also has a dynamic segment `{note_id}` there, and axum
gives a **static** path precedence over a dynamic one, so `GET
/api/v1/notes/search` is answered `405 Method Not Allowed` by the search route
itself - not by the note read, and not `400`. A contract test pins that, because
it is the fact that proves the note read does not shadow the operation.

## 2. Shared shapes

### 2.1 The modes

| Value | Meaning |
| ----- | ------- |
| `lexical` | PostgreSQL full-text match. No model. |
| `semantic` | Cosine similarity against the caller's embeddings. |
| `hybrid` | Bounded lexical candidates re-ranked by cosine similarity. **The default.** |

### 2.2 The degradation contract

This is the part of the feature a client depends on most, so it is stated once,
in full, and without qualification. **`ml.enabled` defaults to `false`, so a
default deployment serves the first row of the table.**

| Requested | `ml.enabled = false` | The model file is missing | ML ready |
| --------- | -------------------- | ------------------------- | -------- |
| `POST /notes/search`, `mode` `lexical` | `200`, lexical results, `degraded` `false` | `200`, lexical, `degraded` `false` | `200`, lexical, `degraded` `false` |
| `POST /notes/search`, `mode` `semantic` or `hybrid` | **`200`**, lexical results, `mode` `lexical`, `degraded` `true`, `degraded_reason` `ml_disabled` | **`200`**, lexical, `degraded_reason` `model_missing` | `200`, the requested mode, `degraded` `false` |
| `POST /job-applications/{id}/match` | **`409 conflict`** | **`409 conflict`** | `200`, a ranked list |
| `POST /ml/reindex` | **`409 conflict`** | **`409 conflict`** | `200`, the counts |

Three sentences, which are the whole rule:

1. **Search degrades with `200`.** A scored mode that cannot be scored returns
   the lexical results with `mode` `lexical` and `degraded_reason`
   `ml_disabled` or `model_missing`. It never returns `500`, `503` or `408`: a
   missing optional model is a configuration state.
2. **Match refuses with `409`.** A match that ranks nothing is a fiction, so it
   is refused rather than degraded.
3. **Reindex refuses with `409`.** An operation that says "rebuild the index"
   cannot do what it says without a model.

`SearchResponse` carries the four degradation fields:

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `requested_mode` | enum | What the client asked for. |
| `mode` | enum | What was applied. Differs from `requested_mode` only when degraded. |
| `degraded` | boolean | `true` when a scored mode was requested and lexical results were returned. |
| `degraded_reason` | enum or `null` | `ml_disabled`, `model_missing`, `inference_timeout`, or `null`. |

`MatchResponse` and `ReindexResponse` do **not** carry them: those operations
fail rather than degrade (`ML-R06`). The rule is in
[`conventions.md`](../../api/conventions.md) §17 and the decision in
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §11.

`deterministic` is a valid `ml.backend` value, so a test deployment can run the
scored modes without a model file; the seeder constructs the deterministic
embedder directly instead of reading the configuration. See
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
| `semantic` | The number of the caller's notes whose published score is greater than zero. |
| `hybrid` | The number of **hits whose published score is greater than zero**, never more than `ml.candidate_limit`. |

A client must not read the hybrid `total` as "every relevant note". A hit
whose cosine is zero or below carries no similarity and is not returned, so
`total` counts only hits that scored above zero.

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
request's `limit`. It is **not** paginated; a match is a short ranked list, not
a collection to page through. Only hits with a **positive** score are returned,
so `matches` is empty with `200` when no note shares a direction with the
application.

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
[`conventions.md`](../../api/conventions.md) §5, with the same defaults and the
same clamp.

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
`degraded` `true` and `mode` `lexical` — that is `ML-R06`, and it is the first
sentence of §2.2.

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
  "degraded_reason": "ml_disabled",
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
| `409` | `conflict` | ML is disabled or the model is not loaded. A match cannot be produced without a model — the second sentence of §2.2. |
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
| `409` | `conflict` | ML is disabled or the model is not loaded. Reindexing cannot do what it says, so it is refused rather than silently doing nothing — the third sentence of §2.2. |
| `500` | `database_error` | The database failed. |
| `408` | `timeout` | The request exceeded the configured request timeout. A reindex of a large corpus can exceed it; the run is idempotent, so the client may start it again and the completed rows are skipped. |

```bash
curl -i -X POST http://127.0.0.1:3000/api/v1/ml/reindex \
  -H "authorization: Bearer $ACCESS_TOKEN"
```

## 6. Notes on the OpenAPI document

The three operations are registered in `src/api/routes.rs` and appear in the
document at `/api/v1/openapi.json` with the `SearchRequest`, `SearchResponse`,
`SearchHit`, `MatchRequest`, `MatchResponse`, `MatchHit` and `ReindexResponse`
schemas. The `bearer` security scheme applies to all three.

`POST /notes/search` declares `limit` and `offset` **once each**, from the
shared pagination parameters, and its body carries only `query` and `mode`.
A contract test asserts the published parameter set and that no second
pagination parameter appears in the body schema.

The document describes **51** operations, and the inventory in
[`../../api/README.md`](../../api/README.md) lists the three ML paths among them.

# Local machine learning — architecture

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

How the machine-learning feature is intended to be built. The public contract
is in [api.md](api.md); the agreed requirements are in
[specification.md](specification.md); the decision is
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).

**Nothing in this document exists yet.** There is no `src/ml/`, no
`embeddings` migration, no model file and no test. The design below is the
plan, recorded so that the implementing change has a shape to follow. It is
**accepted but not built**, and the text is corrected whenever the
implementation and this plan disagree.

**Correction (2026-10-24, chain `D4.M1`).** `src/ml/` now exists, but only as
the spike surface: `embedder.rs` (the trait and the error type), `disabled.rs`,
`deterministic.rs` and `burn_embedder.rs` (the `flex` experiment). There is
still **no** `service.rs`, `repository.rs`, `dto.rs`, no `embeddings`
migration, no route and no `ml.*` setting; every reference to them below is
still the plan. The whole module is gated behind the `ml` Cargo feature. The
measurements that let `M3` proceed are in
[`../../engineering/change-log.md`](../../engineering/change-log.md) under
`ATOM-CHANGE-20261024-010`.

## 1. Responsibilities

| Module | Responsibility |
| ------ | -------------- |
| `src/ml/mod.rs` | The module surface: `MlService`, `Embedder`, the error type. |
| `src/ml/embedder.rs` | The `Embedder` trait, `EmbedderStatus`, `Embedding`, and the selection at start-up. |
| `src/ml/burn_embedder.rs` | The Burn v0.22 implementation on the `flex` backend, loading SafeTensors weights. |
| `src/ml/deterministic.rs` | The hash-based embedder used by every test. |
| `src/ml/disabled.rs` | The embedder that reports unavailability and never panics. |
| `src/ml/service.rs` | The use cases: search in three modes, match, reindex, and the degradation rule. |
| `src/ml/repository.rs` | SQL for the embeddings table, every statement scoped by `user_id`. |
| `src/ml/dto.rs` | `SearchRequest`, `SearchResponse`, `SearchHit`, `MatchResponse`, `MatchHit`, `ReindexResponse`, the modes and the degradation reasons. |

The feature depends on:

| Dependency | For |
| ---------- | --- |
| `src/api/middleware.rs` (`AuthUser`) | The caller's identity. |
| `src/notes/repository.rs` | The lexical candidate query and the notes search, so the two cannot diverge. |
| `src/job_applications/repository.rs` | Loading the application that a match ranks against. |
| `src/pagination.rs` | The shared envelope and clamp for the search endpoint. |
| `src/error.rs` (`AppError`) | The one error envelope. |
| `burn` v0.22 with `flex` | The local model. |
| `src/config/` | The six `ml.*` settings. |

It does **not** depend on an HTTP client, a process spawn, or a second
runtime. Adding one is a new decision, not a feature flag.

## 2. The `Embedder` abstraction

```rust
pub trait Embedder: Send + Sync {
    fn model_id(&self) -> &str;
    fn dimension(&self) -> usize;
    fn status(&self) -> EmbedderStatus;
    fn embed(&self, text: &str) -> Result<Vec<f32>, MlError>;
}
```

| Method | Contract |
| ------ | -------- |
| `model_id` | A stable identifier stored with every row, for example `burn-minilm-v1` or `deterministic-v1`. |
| `dimension` | The length of every vector this embedder produces. Stored with the row and checked on read. |
| `status` | `Ready`, `Disabled { reason }` or `Failed { reason }`. The service uses it to decide whether to degrade. |
| `embed` | A synchronous, CPU-bound call. It returns an error, never a panic, and it never performs I/O after construction. |

| Implementation | Selected when | Used by |
| -------------- | ------------- | ------- |
| `Burn` | `ml.enabled = true` **and** the model file loads | A deployment |
| `Deterministic` | The test harness, and `ml.backend = deterministic` | Every test |
| `Disabled` | `ml.enabled = false`, or the file is missing or fails to load | The default |

Selection happens **once**, at start-up, in `AppState::new`. The service holds
an `Arc<dyn Embedder>`; a request never re-reads the file and never retries a
failed load.

When `ml.enabled = false` the configuration yields `Disabled`, whatever
`ml.backend` says. The seeder is the exception: `--with-embeddings` **constructs
the `Deterministic` embedder directly** rather than reading the configuration,
so seeding never depends on a model file or on `ml.enabled`.

### 2.1 The CPU bound

`flex` is a CPU backend. Calling `embed` directly from an async handler would
block a Tokio worker. The service therefore treats inference as blocking work:

1. `tokio::task::spawn_blocking` moves the call off the async runtime.
2. `tokio::time::timeout` bounds it with `ml.timeout_seconds`.
3. A timeout degrades a **search** to lexical results and answers `200`; a
   **match** has no lexical fallback, so it answers `408`. Neither is a `500`.

A blocking task that cannot be cancelled keeps running after the timeout has
fired. That is acceptable: the request has already returned, and the work is
bounded by one inference. It is recorded as a limitation rather than hidden.

## 3. Storage

One table, `embeddings`:

| Column | Type | Meaning |
| ------ | ---- | ------- |
| `id` | `UUID` | The primary key, generated by the application. |
| `user_id` | `UUID` | `REFERENCES users (id) ON DELETE CASCADE`. |
| `source_type` | `TEXT` | `note` or `application`, constrained by a `CHECK`. |
| `source_id` | `UUID` | The id of the note or application. **No foreign key**: the column is polymorphic. |
| `model_id` | `TEXT` | The embedder that produced the vector. |
| `dim` | `INTEGER` | The vector length, checked against the array length. |
| `vector` | `REAL[]` | The embedding. |
| `created_at`, `updated_at` | `TIMESTAMPTZ` | Standard timestamps. |

- `UNIQUE (user_id, source_type, source_id, model_id)` makes the upsert
  idempotent and makes a duplicate row impossible.
- `CHECK (dim > 0 AND dim = cardinality(vector))` keeps the declared
  dimension and the array length equal.
- The index is `(user_id, source_type, model_id)`, which is the access path of
  every query.

**The polymorphic column is the one real cost of this design.** Because
`source_id` cannot carry a foreign key, the *deleting* transaction is
responsible for removing the embeddings:

- `NotesService::delete` deletes the note's embeddings in the same
  transaction.
- The job-application delete does the same for an application.

`POST /ml/reindex` removes any orphan that an interrupted delete left behind.
A test asserts the count after each delete, so a forgetful change fails.

## 4. The search pipeline

### 4.1 Lexical

```text
POST /notes/search?limit=&offset=   {"query":"…","mode":"lexical"}
  │
  ├─ AuthUser → user_id
  ├─ validate the query (trimmed, 1–200 characters)
  ├─ NoteRepository::list_owned(user_id, search = query, archived = false, limit, offset)
  └─ 200, SearchResponse { mode: "lexical", degraded: false, score: null }
```

Lexical mode delegates to the **existing** notes query. There is no second
full-text implementation, so a change to the notes search cannot make the
search endpoint disagree with the notes listing.

### 4.2 Hybrid

```text
POST /notes/search   {"query":"…","mode":"hybrid"}
  │
  ├─ AuthUser → user_id
  ├─ candidates = NoteRepository::lexical_candidates(user_id, query, ml.candidate_limit)
  │     SELECT id, title, created_at
  │       FROM notes
  │      WHERE user_id = $1
  │        AND search_vector @@ plainto_tsquery('simple', $2)
  │      ORDER BY created_at DESC, id DESC
  │      LIMIT $3
  ├─ vectors = EmbeddingRepository::find_for_sources(user_id, 'note', candidate_ids, model_id)
  │     SELECT source_id, vector
  │       FROM embeddings
  │      WHERE user_id = $1 AND source_type = 'note'
  │        AND model_id = $4 AND source_id = ANY($3)
  ├─ q = embed(query)                       timeout → degraded
  ├─ score = cosine(q, vector)              per candidate that has a row
  ├─ sort by score DESC, note_id ASC        total order
  ├─ slice [offset, offset + limit)
  └─ 200, SearchResponse { mode, requested_mode, degraded, items }
```

Three properties follow from the pipeline and are worth stating:

1. **The re-rank is bounded.** The candidate set is at most
   `ml.candidate_limit` rows, so memory and CPU are bounded by configuration,
   not by the corpus.
2. **The score sort is total.** The tie-break is the note id, so two pages of
   the same query do not overlap or skip a row.
3. **`pagination.total` is the size of the candidate set**, not the number of
   notes that match lexically without a bound. A client must not read it as
   "all notes about this". This is the honest reporting of a re-ranker.

A candidate with no embedding row is **not returned by hybrid mode**. The
alternative — a score of zero — would bury a note that has simply never been
embedded, and a `null` score in a mode defined by its scores would make the
item shape two-shaped. The rule is therefore:

- `hybrid` returns **only** candidates that have an embedding, each with a
  score in `[0,1]`;
- a client that needs every lexical hit uses `lexical` mode;
- the response reports `total` as the number of scored candidates, so the
  number and the items agree.

This is a real behaviour a client can observe, so it is in
[api.md](api.md) and in the feature's known limitations, not only here.

### 4.3 Semantic

```text
POST /notes/search   {"query":"…","mode":"semantic"}
  │
  ├─ AuthUser → user_id
  ├─ vectors = all of the caller's note embeddings for the model
  ├─ q = embed(query)
  ├─ score, sort by score DESC, note_id ASC
  ├─ the note rows for the page are then loaded, owner-scoped
  └─ 200
```

The scan is linear in the caller's embedding count. At a personal corpus size
that is milliseconds; it is not an indexed nearest-neighbour query, and §9 of
the specification says so.

### 4.4 Matching

`POST /job-applications/{id}/match` uses the same cosine function over the
application's embedding:

1. Load the application, owner-scoped; `404` otherwise.
2. Refuse with `409` when the embedder is not ready. A match has no lexical
   fallback, so there is nothing honest to return.
3. Build the text from `role_title` and `description`.
4. Embed it (once, not per note).
5. Compare against the caller's note embeddings, sort, take the top `limit`.
6. Return the notes with their scores.

The application does not need a stored embedding for the match to work; the
text is embedded on the fly. A stored application embedding is still useful
for `source_type = application` and for a future search over applications.

## 5. Degradation

| Condition | Detected at | Result |
| --------- | ----------- | ------ |
| `ml.enabled = false` | start-up | `Disabled`; semantic and hybrid answer lexical with `degraded_reason` `ml_disabled` |
| The model file is missing or unreadable | start-up | `Disabled`; `degraded_reason` `model_missing` |
| The model fails to load | start-up | `Disabled`; `degraded_reason` `model_missing`, with the cause logged once |
| Inference exceeds `ml.timeout_seconds` | per request | a **search** degrades to lexical with `degraded_reason` `inference_timeout`; a **match** answers `408`, because it has no fallback |
| A vector's length differs from `dim` | per row | the row is skipped, and the reindex will replace it |

The response fields are:

```json
{
  "requested_mode": "hybrid",
  "mode": "lexical",
  "degraded": true,
  "degraded_reason": "ml_disabled"
}
```

`degraded_reason` is `null` when `degraded` is `false`. An unknown mode in the
body is a `422`; it is never degraded.

## 6. Reindex

```text
POST /api/v1/ml/reindex        (no body)
  │
  ├─ embedder.status() == Disabled → 409 conflict
  ├─ sources = the caller's notes and applications
  ├─ for each source:
  │    ├─ if a row exists with the current model_id and dim → skipped += 1
  │    └─ else embed, then upsert                                → indexed += 1
  ├─ delete embeddings whose source no longer exists             → deleted += n
  └─ 200, ReindexResponse { model_id, dimension, indexed, skipped, deleted }
```

- **Per-row, not one giant transaction.** A reindex of a large corpus in one
  transaction would hold locks and grow the WAL for no benefit. Each upsert is
  its own statement, and the whole run is idempotent, so an interrupted run is
  simply run again.
- **Owner-scoped by construction.** Every statement carries `user_id`.
- **The orphan sweep runs last**, so a row whose source was deleted during the
  run is removed rather than left behind.

## 7. Configuration

The six settings **do not exist in `config/` yet.** They are added by the
implementing change, in all the places `AGENTS.md` §4.11 requires: the file,
the typed structure, the loader, the validation, `.env.example`, the reference
table in
[`deployment.md`](../../architecture/deployment.md) and the tests.

| Setting | Default | Validation |
| ------- | ------- | ---------- |
| `ml.enabled` | `false` | boolean |
| `ml.backend` | `flex` | one of `flex`, `deterministic`; anything else is a start-up error |
| `ml.model_path` | unset | a path; when `enabled` is true and the file is absent, `Disabled` rather than a start-up failure |
| `ml.dimension` | `384` | `> 0`; must equal what the embedder reports |
| `ml.timeout_seconds` | `2` | `> 0` and at most the request timeout |
| `ml.candidate_limit` | `200` | `> 0` and at most the configured maximum page size × a documented factor |

The last bound deserves a note: a candidate limit below the page size would
make page two impossible. The validation refuses a limit smaller than
`api.pagination.max_limit`.

`ml.backend` accepts exactly two values:

| Value | What it selects | Why it exists |
| ----- | --------------- | ------------- |
| `flex` | The `Burn` embedder on the CPU backend | The production path. |
| `deterministic` | The hash-based embedder | A test deployment proves the scored modes without a model file or a network. The seeder constructs this embedder directly and does not read the setting. |

Any other value is a **start-up error**, not a silent fallback. This is the
amendment recorded in [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md)
§7.

`burn` and the whole `ml` module are gated behind an **`ml` Cargo feature**,
which is on by default. `cargo check --no-default-features` and
`cargo test --no-default-features` build and test the API without `burn`, so
the base build stays small and a change to the model stack cannot break it.
The reason is in the same ADR amendment.

## 8. The security boundary

```text
                 ┌──────────────────────────────────────────────┐
  bearer token ─►│ POST /notes/search                           │  authenticated
                 │ POST /job-applications/{id}/match            │
                 │ POST /ml/reindex                             │
                 └──────────────────────────────────────────────┘
                                    │
                                    │  user_id from the credential only
                                    ▼
        every statement carries `user_id = $n`
        the model is a local file; the process makes no outbound call
```

- **An embedding is derived personal data.** A vector is an approximate
  representation of the note, so it is protected like the note: owner-scoped
  on write, on read and on delete.
- **No network egress.** There is no HTTP client in `src/ml/`, no model
  download and no telemetry. That removes a third-party data-processing
  question entirely.
- **The model file is trusted operator input**, read by Burn as tensor data.
  The operator is inside the trust boundary (TB-4 in
  [`threat-model.md`](../../security/threat-model.md)); a corrupt file yields
  `Disabled`, not code execution.
- **The candidate limit is a resource bound, not a quality knob.** It caps the
  work one request may do, which is why it is validated at start-up.
- **A degraded response must not leak why a model failed.** The reason is one
  of three fixed codes; the underlying error is logged once at start-up and
  never returned.

## 9. Related files

| File | Role | Exists |
| ---- | ---- | ------ |
| `src/notes/repository.rs` | The lexical query the search modes reuse; the delete that must also remove embeddings. | Yes, to be extended. |
| `src/api/routes.rs` | Registers the three routes and the schemas. | Yes, to be extended. |
| `src/state.rs` | Holds the `Arc<dyn Embedder>`. | Yes, to be extended. |
| `src/config/` | The six `ml.*` settings. | Yes, to be extended. |
| `src/job_applications/repository.rs` | Loading the application for a match; the delete that must remove embeddings. | **No** |
| `src/ml/**` | The feature itself. | **No** |
| `migrations/00NN_embeddings.sql` | The embeddings table and its constraints. | **No** |

## 10. Related decisions

- [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) — local,
  pure-Rust inference with Burn and `flex`, and `REAL[]` instead of pgvector.
- [`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md) —
  ownership as a SQL predicate, including for derived data.
- [`ADR-0001`](../../decisions/ADR-0001-modular-monolith.md) — why inference
  is a module in the same process.
- [`project-truth.md`](../../project-truth.md) §6.1 — the amendment that
  permits local inference and keeps outbound AI excluded.

## 11. Limitations of this design

- **No index, so no recall beyond the candidate set.** The hybrid mode cannot
  find a note the lexical query missed. This is the price of avoiding the
  extension, and it is stated in the specification.
- **The deterministic embedder is a test double.** It makes the plumbing
  testable and says nothing about quality.
- **Re-embedding on write is not part of Phase 2.** An edited note keeps its
  old vector until a reindex. Hooking the notes update into the embedder is a
  later change with its own performance question.
- **The polymorphic `source_id` needs discipline.** A delete that forgets the
  embeddings leaves an orphan; the reindex sweep is the backstop.
- **Inference cannot be cancelled.** A timed-out request returns, but the
  blocking task finishes. With `ml.timeout_seconds` at 2 seconds, the wasted
  work is bounded.
- **Two models can coexist but only one is queried.** A model change is a
  reindex; the old rows are deleted by the orphan sweep only after the new
  `model_id` is refused by the `find` queries, which is a nuance the
  implementing change must handle explicitly.
- **A degraded response is hard to notice.** `degraded: true` is the only
  signal, and a client that ignores it shows weaker results without saying so.
  A future readiness check could expose the embedder status.

# Local machine learning — architecture

> **Status:** Implemented
> **Last updated:** 2026-10-25
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

How the machine-learning feature is intended to be built. The public contract
is in [api.md](api.md); the agreed requirements are in
[specification.md](specification.md); the decision is
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).

This document is the design the feature was built against. It is written in the
present tense of the plan, and the corrections below record what existed at each
chain; read them together with [status.md](status.md) §1.1, §1.4 and §5. The
`embeddings` migration, the module, the three routes and the seven `ml.*`
settings exist. The Burn encoder and the WordPiece tokenizer exist as of chain
`D4.M6` and are measured by `src/bin/atom-ml-bench.rs`, so `ML-R02` is **met**;
`ML-R09`'s manual quality evaluation is still outstanding (status.md §4).

**Correction (2026-10-24, chain `D4.M1`).** `src/ml/` was created as the spike
surface: `embedder.rs` (the trait and the error type), `disabled.rs`,
`deterministic.rs` and `burn_embedder.rs` (the `flex` experiment). At that commit
there was still **no** `service.rs`, `repository.rs`, `dto.rs`, no `embeddings`
migration, no route and no `ml.*` setting. `D4.M3`-`M6` added them; the current
state is the table in §1. The measurements that let `M3` proceed are in
[`../../engineering/change-log.md`](../../engineering/change-log.md) under
`ATOM-CHANGE-20261024-010`.

**Correction (2026-10-24, chain `D4.M2`).** The spike passed and
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §8 records the
result, including what it does **not** measure: the timing is a lower bound
because no attention, feed-forward block or LayerNorm ran, and the tokenizer is
a stand-in. Two decisions were taken on top of the measurement and are binding
on `M3`:

- **§2 gains one more invariant: `embed` returns a unit vector.** The stored
  representation is normalised at write time
  ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §9), so cosine
  similarity is exactly the dot product and a row's meaning never depends on
  which code path wrote it.
- **§7 gains a seventh setting, `ml.tokenizer_path`**, because WordPiece needs
  a vocabulary that the SafeTensors file does not contain
  ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §10).

**Correction (2026-10-25, chain `D4.M7`).** The encoder and tokenizer landed and
`atom-ml-bench` measured the real forward pass: **88 ms** for one text,
**112.6 ms** per text sustained, **804 ms** worst case at 256 tokens, 183.8 MiB
RSS. §7 was re-derived from that measurement, and two things changed with it:

- **`ml.timeout_seconds` bounds one inference, not one re-rank.** A hybrid
  search embeds the query and then scores **stored** vectors; `MlService::hybrid`
  contains no inference call. The earlier text that described the timeout as
  bounding the re-rank (§4.2, §11) was wrong, and §7 now states which setting
  bounds which. The defaults did not change: **2 s**, **200**.
- **`ml.candidate_limit` gained a ceiling and lost its floor.** The rule
  "`candidate_limit >= api.pagination.max_limit`" is withdrawn: its stated
  reason — that a page two would be impossible — was false, because a ranked set
  shorter than the page size yields a well-formed short page. It is replaced by
  `MAX_ML_CANDIDATE_LIMIT = 1_000`. See
  [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §12.

§12 is the **build contract for chains `M3`–`M6`**: the exact trait, the three
implementations, the table, the endpoints with their status codes, the
degradation rule, the test names and the files each agent owned. Everything in it
is built.

## 1. Responsibilities

| Module | Responsibility | Exists |
| ------ | -------------- | ------ |
| `src/ml/mod.rs` | The module surface: `MlService`, `Embedder`, the error type. | Exists. |
| `src/ml/embedder.rs` | The `Embedder` trait, `EmbedderStatus`, `MlError`, `l2_norm`, and the selection at start-up. | Exists. |
| `src/ml/burn_embedder.rs` | The Burn v0.22 encoder on the `flex` backend: the six-layer BERT module, masked mean pooling, L2 normalisation, and SafeTensors weights loaded with `burn_store`. | Exists (§12.3). |
| `src/ml/tokenizer.rs` | The WordPiece tokenizer: vocabulary load, greedy longest-match, `[CLS]`/`[SEP]`, `[UNK]`, truncation. | Exists. |
| `src/ml/deterministic.rs` | The hash-based embedder used by every test and by the seeder. | Exists. |
| `src/ml/disabled.rs` | The embedder that reports unavailability and never panics. | Exists. |
| `src/ml/service.rs` | The use cases: search in three modes, match, reindex, and the degradation rule. | Exists. |
| `src/ml/repository.rs` | SQL for the embeddings table, every statement scoped by `user_id`. | Exists. |
| `src/ml/dto.rs` | `SearchRequest`, `SearchResponse`, `SearchHit`, `MatchResponse`, `MatchHit`, `ReindexResponse`, the modes and the degradation reasons. | Exists. |
| `src/ml/handlers.rs` | The three HTTP handlers: extraction, status codes, serialization. No business rule. | Exists. |

The feature depends on:

| Dependency | For |
| ---------- | --- |
| `src/api/middleware.rs` (`AuthUser`) | The caller's identity. |
| `src/notes/repository.rs` | The lexical candidate query and the notes search, so the two cannot diverge. |
| `src/job_applications/repository.rs` | Loading the application that a match ranks against. |
| `src/pagination.rs` | The shared envelope and clamp for the search endpoint. |
| `src/error.rs` (`AppError`) | The one error envelope. |
| `burn` v0.22 with `flex` and `burn_store` | The local model and its SafeTensors weights. |
| `src/config/` | The seven `ml.*` settings. |

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
| `model_id` | A stable identifier stored with every row, for example `burn-all-minilm-l6-v2-v2` or `deterministic-sha256-v1-384`. Changing what a vector *means* requires a new `model_id`, not a rewrite of the old rows. |
| `dimension` | The length of every vector this embedder produces. Stored with the row and checked on read. |
| `status` | `Ready` or `Disabled { reason }`. There is no third variant: a load failure **is** the disabled state ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §8.3), so the service decides whether to degrade from one place. |
| `embed` | A synchronous, CPU-bound call. It returns an error, never a panic, and it never performs I/O after construction. The returned vector has `dimension()` elements and is **unit length**. |

`EmbedderStatus` and `MlError` are mapped onto the client-visible
`degraded_reason`; they are not themselves part of the published contract, but
adding a variant to either is a contract change all the same.

**The invariants every implementation must satisfy.** They are the reason the
trait exists, and a new implementation that breaks one is a defect rather than a
variation:

1. `embed` returns exactly `dimension()` values.
2. **The vector is unit length**: `(1.0 - l2_norm(&v)).abs() <= 1e-5` for every
   input that produces a vector at all. This is
   [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §9, and it is
   what makes a stored row comparable with a query without knowing which writer
   produced it.
3. A zero or non-finite norm has no unit vector, so `embed` returns
   `MlError::Model` instead. No row is written for a failed embed.
4. `status()` is decided at construction and never changes while the process
   runs. A request never triggers a retry of a failed load.
5. `embed(&self, …)` is callable from several threads at once. No implementation
   uses interior mutability; everything mutable is set before the value is
   shared.

| Implementation | Selected when | Used by |
| -------------- | ------------- | ------- |
| `Burn` | `ml.enabled = true` **and** the weights and the vocabulary load | A deployment |
| `Deterministic` | The test harness, and `ml.backend = deterministic` | Every test, and the seeder |
| `Disabled` | `ml.enabled = false`, or either model artifact is missing or fails to load | The default |

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
| `vector` | `REAL[]` | The embedding, **unit length by construction** ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §9). |
| `created_at`, `updated_at` | `TIMESTAMPTZ` | Standard timestamps. |

- `UNIQUE (user_id, source_type, source_id, model_id)` makes the upsert
  idempotent and makes a duplicate row impossible.
- `CHECK (dim > 0 AND dim = cardinality(vector))` keeps the declared
  dimension and the array length equal.
- The index is `(user_id, source_type, model_id)`, which is the access path of
  every query.

**The stored vector is unit length, and that is decided at write time.** Every
`Embedder` implementation normalises its output, so the row's value and the
query's value are both unit vectors and cosine similarity is exactly the dot
product ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §9). The
spike measured an L2 norm of `0.059342` because it did not normalise; a table
that started unnormalised and gained normalisation later would change what every
existing row meant.

- **What it costs:** one pass over 384 `f32` values per embed, after the forward
  pass. It is re-measured with everything else in `M3`; it is not a reason to
  change the design.
- **What it prevents:** a table mixing two conventions, where a row's meaning
  depends on which code path wrote it. `model_id` records the model, not the
  convention.
- **Why the ranking is unaffected:** cosine is scale-invariant, so the pass
  changes no ordering for a correct cosine. It changes what is stored, and it
  makes the `[0,1]` score range, after clamping, a property of the data.
- **What the reader must not do:** re-normalise a row. It validates the
  dimension and clamps the score; repairing a broken writer on read would hide
  the defect the invariant exists to expose.

**The polymorphic column is the one real cost of this design.** Because
`source_id` cannot carry a foreign key, the *deleting* statement is responsible
for removing the embeddings. The mechanism is fixed here, because three
plausible implementations differ in whether the guarantee is real:

**One statement, therefore one transaction.** Each source delete becomes a
data-modifying CTE that removes the source and its derived rows together. This
is the only shape that keeps the guarantee with the code that exists: the
repositories own their transactions (`self.pool.begin()`), and two repository
calls would be two transactions.

```sql
-- notes/repository.rs: DELETE_NOTE_OWNED_WITH_EMBEDDINGS
WITH removed AS (
    DELETE FROM notes
     WHERE id = $1 AND user_id = $2
     RETURNING id
), purged AS (
    DELETE FROM embeddings
     WHERE user_id = $2
       AND source_type = 'note'
       AND source_id IN (SELECT id FROM removed)
)
SELECT EXISTS (SELECT 1 FROM removed) AS removed;
```

- The same shape replaces `DELETE_APPLICATION_OWNED` in
  `src/job_applications/repository.rs`, with `source_type = 'application'`.
- `SELECT EXISTS (…)` returns `bool`, so both methods keep their current return
  type and their `404`-vs-deleted distinction. `rows_affected()` does not work
  on a trailing `SELECT`; the caller reads the scalar.
- **A trigger is rejected** for the reason the rest of the schema gives: no
  rule of this database lives in a trigger, and a hidden delete is harder to
  audit than a statement.
- **A second repository call is rejected** because it cannot share the
  transaction, and an orphan between two commits is exactly what the ADR says
  cannot happen.
- The `embeddings` table is owned by this module, but the *source* delete is
  allowed to purge its own derived rows: it is the source's invariant, and the
  column set is pinned by
  [`schema.md`](../../database/schema.md) §11.6.
- The write is the mirror image: an embedding is inserted in the same statement
  that proves the source belongs to the caller, and the upsert target is the
  unique index of §11.6:

  ```sql
  INSERT INTO embeddings (id, user_id, source_type, source_id, model_id, dim, vector)
  VALUES ($1, $2, $3, $4, $5, $6, $7)
  ON CONFLICT (user_id, source_type, source_id, model_id)
  DO UPDATE SET vector = EXCLUDED.vector, dim = EXCLUDED.dim, updated_at = NOW()
  ```

`POST /ml/reindex` removes any orphan that a delete outside the service left
behind. A test asserts the count after each delete, so a forgetful change fails.


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
   `ml.candidate_limit` rows, and `ml.candidate_limit` is at most
   `MAX_ML_CANDIDATE_LIMIT` (1,000), so memory and CPU are bounded by
   configuration, not by the corpus. This is the only bound the re-rank has:
   `ml.timeout_seconds` bounds the query's inference, not the scoring loop (§7).
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
| A model artifact is missing or unreadable: the weights file, or the `vocab.txt` that `ml.tokenizer_path` names | start-up | `Disabled`; `degraded_reason` `model_missing`. **Start-up still succeeds** |
| The weights or the vocabulary fails to load | start-up | `Disabled`; `degraded_reason` `model_missing`, with the cause logged once |
| Inference exceeds `ml.timeout_seconds` | per request | a **search** degrades to lexical with `degraded_reason` `inference_timeout`; a **match** answers `408`, because it has no fallback |
| A vector's length differs from `dim` | per row | the row is skipped, and the reindex will replace it |
| `embed` yields a zero or non-finite norm | per call | `MlError::Model`; no row is written, a search degrades with `model_missing`, a match answers `409` |

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

The seven settings exist in `config/ml.toml`, in `MlSettings`, in the loader, in
`validate_ml`, in `.env.example`, in the reference table in
[`deployment.md`](../../architecture/deployment.md) and in the tests. Below is
what each means and what the validation enforces.

| Setting | Default | Validation |
| ------- | ------- | ---------- |
| `ml.enabled` | `false` | boolean |
| `ml.backend` | `flex` | one of `flex`, `deterministic`; anything else is a start-up error |
| `ml.model_path` | unset | a path; when `enabled` is true and the file is absent, `Disabled` rather than a start-up failure |
| `ml.tokenizer_path` | unset — the `vocab.txt` beside `ml.model_path` | a path; a missing vocabulary is `Disabled`, not a start-up failure ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §10) |
| `ml.dimension` | `384` | `> 0`; must equal what the embedder reports |
| `ml.timeout_seconds` | `2` | `> 0` and at most the request timeout |
| `ml.candidate_limit` | `200` | `> 0` and at most `MAX_ML_CANDIDATE_LIMIT` (1,000) |

**The two budget settings are not a pair, and the validation no longer pretends
they are.** They bound different things:

| Setting | What it bounds | Where it is applied |
| ------- | -------------- | ------------------- |
| `ml.timeout_seconds` | **One inference.** The query's, for a search; the application's, for a match; each source's, for a reindex. | `tokio::time::timeout` in `MlService::embed` (`src/ml/service.rs`) |
| `ml.candidate_limit` | **One re-rank.** The number of stored vectors a hybrid search scores. | The `LIMIT` of `NoteRepository::lexical_candidates` |

A search performs **one** inference and then scores stored vectors: `hybrid`
never embeds a candidate ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md)
§12.2). So the two settings are independent, and neither is derived from the
other or from `api.pagination.max_limit`.

**The candidate limit is a work bound, so it has a ceiling.** At the ceiling the
re-rank is ≈1.2 million `f64` operations and 1.5 MB of held vectors — on the
order of a millisecond, against an 88 ms inference. Negligible, but finite, and
until `ADR-0008` §12.5 nothing enforced it: the validation bounded
`candidate_limit` from below and left it unbounded above. The ceiling is
`MAX_ML_CANDIDATE_LIMIT = 1_000`, a constant in `src/config/validation.rs`
beside `MAX_PAGINATION_LIMIT`, not a setting, because a deployment has no reason
to raise it and `AGENTS.md` §4.11 reserves a setting for a value a deployment
might need to change. It equals `MAX_PAGINATION_LIMIT`, so a legal page and a
legal candidate set cannot outgrow each other.

**A page is a separate bound, and a short page is not an error.** `limit` is
clamped by the shared envelope. When the ranked set is smaller than the page
size — a candidate set smaller than `limit`, or fewer scored candidates than
asked for — the response is a well-formed short page: the items that exist, the
true `total`, and `has_more: false`
([`pagination.rs`](../../../src/pagination.rs)). The validation therefore
imposes no relation between `ml.candidate_limit` and `api.pagination.max_limit`.
It did, until
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §12.5 withdrew the
rule on the grounds that its justification was false and that it left the work
bound unbounded above.

**`semantic` mode is the exception, and it is bounded by the corpus, not by
these settings.** It scores every embedding the caller has
([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §4), so
`candidate_limit` does not apply to it and `ml.timeout_seconds` bounds only its
query embed. That is the accepted cost of having no vector index, and it is a
limitation of §11, not a gap in the validation.

**The reindex is bounded by the request timeout, not by `ml.timeout_seconds`.**
It embeds one text per source that is not already current, so its wall clock is
`sources × per-inference` and the binding limit is `app.limits.request_timeout`
(30 s by default) enforced by `timeout_middleware`. At the measured cost that is
**≈266 short sources, or ≈37 at 256 tokens**, per request; past it the answer is
`408`, the rows already written stay, and the next run skips them. This is a
recorded limit ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md)
§12.6), not a setting.

The section lives in `config/ml.toml`. Two things are needed for it to exist at
all, and both are done:

- `src/config/loader.rs` lists `"ml.toml"` in `BASE_FILES` **and** has an arm in
  `fn embedded(name: &str)`. A file that is only in the directory is absent from
  an embedded build, and a file that is only in `embedded` is absent from a
  directory build. Both, or neither works.
- Each setting has its `APP_ML_*` override in the `apply_*` function that reads
  the environment, matching `APP_AUTH_*` and `APP_API_*`. `ml.model_path` and
  `ml.tokenizer_path` are **not** secrets, so a literal default in a committed
  file is allowed; a machine-specific path belongs in the environment and not in
  `config/ml.toml`.

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
  work one request may do, which is why it is validated at start-up — in both
  directions: it must be positive, and it must not exceed
  `MAX_ML_CANDIDATE_LIMIT` (§7). The ceiling is what makes the sentence true; a
  floor alone bounds recall, not work.
- **A degraded response must not leak why a model failed.** The reason is one
  of three fixed codes; the underlying error is logged once at start-up and
  never returned.

## 9. Related files

| File | Role | Exists |
| ---- | ---- | ------ |
| `src/notes/repository.rs` | The lexical query the search modes reuse; the delete statement that also purges the note's embeddings. | Yes. |
| `src/api/routes.rs` | Registers the three routes and the schemas, in `api_routes()` and in `ROUTE_TABLE`. | Yes. |
| `src/state.rs` | Holds the `MlService`, which holds the `Arc<dyn Embedder>`. | Yes. |
| `src/config/` | The seven `ml.*` settings, including `BASE_FILES` and `embedded()`. | Yes. |
| `src/job_applications/repository.rs` | Finds the application a match ranks against; the delete statement that also purges the application's embeddings. | Yes. |
| `src/ml/**` | The feature itself. | Yes: `embedder.rs`, `disabled.rs`, `deterministic.rs`, `burn_embedder.rs`, `tokenizer.rs`, `service.rs`, `repository.rs`, `dto.rs`, `handlers.rs`. |
| `src/bin/atom-ml-spike.rs` | The `D4.M1` measurement. **Frozen**: it is the record behind `ADR-0008` §8 and is not edited to measure a later network. | Yes. |
| `src/bin/atom-ml-bench.rs` | The `M3` re-measurement of the real encoder and tokenizer; the source of the numbers in `ADR-0008` §12.1. | Yes. |
| `migrations/0006_embeddings.sql` | The `embeddings` table, its constraints and its two indexes, exactly as [`schema.md`](../../database/schema.md) §11.6 specifies. | Yes. |
| `config/ml.toml` | The seven settings. | Yes. |
| `tests/ml_api.rs` | The integration suite of §12.7. | Yes. |

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
- **The candidate set is the *newest* lexical matches, not the best ones.** The
  candidate query reuses the notes order, `created_at DESC, id DESC`, because
  that is the order `lexical` mode publishes and a second ordering would make
  the two disagree. When a query has more than `ml.candidate_limit` matches, an
  old but highly relevant note can be cut before the re-rank ever sees it. The
  alternative — ordering candidates by `ts_rank` — is a deliberate future
  change: it would need its own justification and a documented difference
  between the two modes, not a quiet `ORDER BY`.
- **The deterministic embedder is a test double.** It makes the plumbing
  testable and says nothing about quality.
- **Re-embedding on write is not part of Phase 2.** An edited note keeps its
  old vector until a reindex. Hooking the notes update into the embedder is a
  later change with its own performance question.
- **The polymorphic `source_id` needs discipline.** A delete that forgets the
  embeddings leaves an orphan; the reindex sweep is the backstop.
- **Inference cannot be cancelled.** A timed-out request returns, but the
  blocking task finishes. With `ml.timeout_seconds` at 2 seconds, the wasted
  work is bounded by one inference.
- **The reindex is bounded by one request.** It embeds one text per source that
  is not already current, so 30 s of request budget is ≈266 short sources or
  ≈37 at 256 tokens; past that the answer is `408` and the next run resumes.
  A corpus larger than that needs a resumable job, which is a future decision.
- **Two models can coexist but only one is queried.** A model change is a
  reindex; the old rows are deleted by the orphan sweep only after the new
  `model_id` is refused by the `find` queries, which is a nuance the
  implementing change must handle explicitly.
- **A degraded response is hard to notice.** `degraded: true` is the only
  signal, and a client that ignores it shows weaker results without saying so.
  A future readiness check could expose the embedder status.
- **The measured cost is one machine's.** `ADR-0008` §12.1 records 88 ms per
  text and 804 ms at 256 tokens on the reference development machine, and the
  `2 s` budget is 2.4× the worst case there. A slower host raises
  `APP_ML_TIMEOUT_SECONDS`; the default is not re-derived from a guess.
- **Concurrent requests each hold their own activations.** The embedder is
  shared and immutable, so two requests may embed at once and each allocates for
  its own sequence. The time a request spends **in inference** is bounded by
  `ml.timeout_seconds`; the re-rank is bounded separately by
  `ml.candidate_limit`; memory is bounded by the API's ordinary request
  concurrency, not by a semaphore this design adds. At single-owner traffic this
  is not a bound worth engineering.

## 12. The build contract for chains `M3`–`M6`

**Status: built.** This section is the
contract `M3`, `M4`, `M5` and `M6` were held to. The requirements are
[`specification.md`](specification.md) §5, the public shapes are
[`api.md`](api.md), the test list is [`testing.md`](testing.md), the table is
[`schema.md`](../../database/schema.md) §11.6. Where this section and an
implementing change disagree, the change stops and asks: a deviation is a
contract change, not a local decision.

### 12.1 The `Embedder` trait, exactly

`src/ml/embedder.rs` already contains this, and `M3` keeps the signatures:

```rust
pub const DEFAULT_DIMENSION: usize = 384;

pub trait Embedder: Send + Sync {
    fn model_id(&self) -> &str;
    fn dimension(&self) -> usize;
    fn status(&self) -> EmbedderStatus;
    fn embed(&self, text: &str) -> Result<Vec<f32>, MlError>;
}

pub enum EmbedderStatus {
    Ready,
    Disabled { reason: String },
}

pub enum MlError {
    Unavailable { reason: String }, // code(): "ml_disabled"
    Model { message: String },      // code(): "model_missing"
}

// Free function already in the module: the shared norm, as f64 so a long
// vector does not lose precision while summing.
pub fn l2_norm(values: &[f32]) -> f64;
```

| Item | Binding rule |
| ---- | ------------ |
| `model_id` | Stored in every row. `"burn-all-minilm-l6-v2-v2"` for the encoder, `"deterministic-sha256-v1-{dim}"` for the test double, `"disabled"` for the disabled one. Changing what a vector means is a new id, never a silent rewrite. |
| `dimension` | Must equal `ml.dimension` for `Burn` and `Deterministic`; `Disabled` reports the configured dimension so the response shape does not change. |
| `status` | Two variants only. Every load failure is `Disabled { reason }`. |
| `embed` | `Ok(v)` has exactly `dimension()` elements and `(1.0 - l2_norm(&v)).abs() <= 1e-5`. A zero or non-finite norm is `Err(MlError::Model)`. It never panics and does no I/O after construction. |
| Threading | `&self` from several threads at once; no interior mutability. |
| Growth | A new method, a new variant or a fourth implementation changes what the service can map, so it returns to the architect first. `ML-R03` fixes the count at three. |

### 12.2 The three implementations

| Implementation | `model_id` | `status()` | What `M3` must do |
| -------------- | ---------- | ---------- | ----------------- |
| `BurnEmbedder` (`src/ml/burn_embedder.rs`) | `burn-all-minilm-l6-v2-v2` | `Ready` | Replace the spike network with the real encoder and tokenizer (§12.3), normalise the output, and load weights with `burn_store`. |
| `Deterministic` (`src/ml/deterministic.rs`) | `deterministic-sha256-v1-{dim}` | `Ready` | Keep as it is: SHA-256 in counter mode, no file, no network, already unit length. It is what every test and the seeder use. |
| `Disabled` (`src/ml/disabled.rs`) | `disabled` | `Disabled { reason }` | Keep as it is: `embed` returns `MlError::Unavailable`, never panics. |

**Selection, once, in `AppState::new`** — never per request:

```text
ml.enabled = false                        -> Disabled("ml_disabled")
ml.enabled = true, backend = deterministic -> Deterministic(dim)
ml.enabled = true, backend = flex,
   weights or vocabulary unreadable        -> Disabled("model_missing"), cause logged once,
                                              start-up SUCCEEDS
ml.enabled = true, backend = flex,
   weights and vocabulary load             -> Burn
```

`ml.backend` outside `{flex, deterministic}` is a `ConfigError` at start-up.
Nothing in this table may abort the process, and nothing may download a file.

### 12.3 What `M3` must build that the spike did not

The spike measured a lookup, a mean-pool and a projection. Four pieces are
missing, and the measured latency in `ADR-0008` §8 does **not** cover them.

1. **A real BERT encoder.** A Burn `Module` mirroring
   `sentence-transformers/all-MiniLM-L6-v2`: token, position and token-type
   embeddings; six layers, each with 12-head self-attention (head size 64), a
   384→1536→384 feed-forward pair, GELU and two LayerNorms; and the final
   LayerNorm. Load the weights with **`burn_store`**'s SafeTensors recorder and
   an explicit key map from the Hugging Face names
   (`bert.encoder.layer.{n}.attention.self.query.weight`, …). The spike's
   hand-written `read_header`/`read_f32` header parser is **removed**: it was
   adequate for three tensors and is not adequate for a hundred-odd.
2. **A WordPiece tokenizer.** Lowercase, accent strip, split on whitespace and
   punctuation, greedy longest-match against `vocab.txt`, `##` continuations,
   `[UNK]`, and `[CLS]`/`[SEP]` framing with an attention mask and a 256-token
   truncation bound. Hand-written, roughly 150 lines, tested against a small
   fixture of known tokenisations; **no new dependency**, because a tokenizer
   crate for one fixed vocabulary is a supply-chain and build-time cost this
   decision does not accept. If that judgement is wrong, adding one is a
   dependency decision recorded in the change log, not a silent `Cargo.toml`
   edit.
3. **The model's own pooling, not the pooler.** `all-MiniLM-L6-v2` as shipped by
   sentence-transformers is **mean pooling over the last hidden state, masked by
   attention, then L2 normalisation**. The spike used the Hugging Face
   `pooler.dense` projection, which that model does not use for sentence
   embeddings. `M3` uses mean pooling, ignores `pooler.dense`, and normalises
   (ADR §9). This is a change of vector meaning as well as of geometry, which is
   why the `model_id` changes and why the spike's vectors were never storable.
4. **A re-measurement.** A new binary `src/bin/atom-ml-bench.rs`
   (`required-features = ["ml"]`) that prints, for the real encoder and
   tokenizer: construction time, first inference, steady state at 1, 10 and 100
   texts, peak RSS, the output L2 norm, and bit-exact determinism. The
   `atom-ml-spike` binary is **not edited**; it is the record behind `ADR-0008`
   §8. The bench's numbers go into the change log, and they are what justifies
   `ml.timeout_seconds` and `ml.candidate_limit`. The 100-text row measures
   **embedding 100 texts**, which is a **reindex** of 100 sources: a search
   embeds one query. If a 100-source reindex no longer fits the request budget,
   the implementing change **reports the number and asks**; it does not quietly
   change a published default.

A note on the fixture text: the bench uses note-shaped sentences, as the spike
did, so the two tables are comparable.

### 12.4 The table

Migration `0006_embeddings.sql`, exactly
[`schema.md`](../../database/schema.md) §11.6. Nothing here is a new decision;
it is repeated so that the implementing chains do not have to reconcile two
documents mid-flight.

| Column | Type | Rule |
| ------ | ---- | ---- |
| `id` | `UUID` | Primary key, generated by the application. |
| `user_id` | `UUID` | `REFERENCES users (id) ON DELETE CASCADE`. |
| `source_type` | `TEXT` | `CHECK (source_type IN ('note', 'application'))`. |
| `source_id` | `UUID` | The note or application id. **No foreign key** — polymorphic. |
| `model_id` | `TEXT` | 1–200 characters, `CHECK`. |
| `dim` | `INTEGER` | `CHECK (dim > 0 AND dim = cardinality(vector))`. |
| `vector` | `REAL[]` | Unit length by construction (`ADR-0008` §9). |
| `created_at`, `updated_at` | `TIMESTAMPTZ` | `NOW()`; `updated_at` on the upsert's `DO UPDATE`. |

| Object | Definition |
| ------ | ---------- |
| `embeddings_source_type_chk` | `CHECK (source_type IN ('note', 'application'))` |
| `embeddings_model_id_length_chk` | `CHECK (CHAR_LENGTH(model_id) BETWEEN 1 AND 200)` |
| `embeddings_dimension_chk` | `CHECK (dim > 0 AND dim = cardinality(vector))` |
| `embeddings_user_id_source_model_unique_idx` | `UNIQUE (user_id, source_type, source_id, model_id)` — the upsert target |
| `embeddings_user_id_source_type_model_id_idx` | `(user_id, source_type, model_id)` — the search scan |

**Cleanup is the one thing the database cannot do for this table.** §3 fixes the
mechanism: the source delete and the embedding purge are one statement (one
data-modifying CTE), so the guarantee is a transaction without a transaction
handle. The reindex orphan sweep is the backstop. The polymorphic `source_id`
must be checked against the caller's `user_id` on write, because no foreign key
does it: `WHERE id = $1 AND user_id = $2` in the statement that inserts.

### 12.5 The three endpoints

| Method | Path | Success | Errors |
| ------ | ---- | ------- | ------ |
| `POST` | `/api/v1/notes/search?limit=&offset=` | `200` `SearchResponse` | `400` body not JSON or a pagination parameter unparsable; `401`; `422` `query` empty/over 200, or an unknown `mode`; `500` `database_error` |
| `POST` | `/api/v1/job-applications/{application_id}/match` | `200` `MatchResponse` | `400` not JSON or `application_id` not a UUID; `401`; `404` absent or foreign; `409` ML unavailable; `422` `mode` `lexical`/unknown or `limit` out of `1..=50`; `408` inference over budget; `500` |
| `POST` | `/api/v1/ml/reindex` | `200` `ReindexResponse` | `401`; `409` ML unavailable; `408` request budget exceeded; `500` |

- Search has **no** `408` and no `409`: it degrades instead.
- The body shapes, the four degradation fields, the ordering rules and the
  `pagination.total` meaning per mode are
  [`api.md`](api.md) §2–§3 and are not restated here.
- `mode` defaults to `hybrid`; `limit` defaults to 10 and is bounded by the
  shared envelope for search and by `1..=50` for match.
- All three are registered in `api_routes()` **and** in `ROUTE_TABLE`, in the
  same change, or `tests/openapi_contract.rs` fails.

### 12.6 The degradation rule, as one sentence and five branches

**A model that is absent, disabled or slow must never turn a working endpoint
into a server error: a search answers `200` with lexical results, a match
answers `409`, and a reindex answers `409`.**

| Branch | Search | Match | Reindex |
| ------ | ------ | ----- | ------- |
| `ml.enabled = false` | `200` lexical, `degraded_reason: ml_disabled` | `409` | `409` |
| Model or vocabulary missing / unloadable | `200` lexical, `model_missing` | `409` | `409` |
| Embed error at request time | `200` lexical, `model_missing` | `409` | `409`; the run stops and writes nothing further |
| One inference over `ml.timeout_seconds`, or the request budget exhausted | `200` lexical, `inference_timeout` | `408` | `408`; the rows already written stay, and the next run skips them |
| Database failure | `500` | `500` | `500` |

`lexical` mode never degrades and never reads the embedder. An unknown `mode` is
`422`, never a silent degradation. `degraded_reason` is `null` exactly when
`degraded` is `false`, and the four fields are asserted together in every
degradation test, because any one alone can pass by accident.

### 12.7 The tests, by file

**`tests/ml_api.rs`** — the integration suite, real router, real database, one
user per test, embeddings planted through a helper that writes unit vectors.

Search modes: `lexical_mode_matches_the_notes_search`,
`lexical_mode_returns_a_null_score`, `hybrid_returns_a_score_for_every_hit`,
`hybrid_orders_by_score_then_note_id`,
`hybrid_never_returns_more_than_the_candidate_limit`,
`a_note_without_an_embedding_is_not_returned_by_hybrid`,
`semantic_search_returns_the_callers_notes`,
`the_search_pagination_bounds_are_applied`, `an_empty_query_is_refused`,
`a_query_over_the_bound_is_refused`, `an_unknown_search_mode_is_refused`.

Degradation: `search_degrades_to_lexical_when_ml_is_disabled`,
`search_degrades_to_lexical_when_the_model_is_missing`,
`a_degraded_response_is_a_complete_response`,
`a_degraded_request_never_returns_500_or_503`, `lexical_mode_is_not_degraded`.

Ownership: `search_never_returns_another_users_note`,
`reindex_is_owner_scoped`, `matching_a_foreign_application_is_not_found`,
`the_ml_routes_require_a_credential`.

Match: `a_match_ranks_the_callers_notes`,
`a_match_returns_an_empty_list_when_nothing_is_embedded`,
`matching_returns_conflict_when_ml_is_disabled`,
`a_match_limit_out_of_range_is_refused`, `lexical_mode_is_refused_by_match`,
`a_match_request_with_no_body_is_refused`.

Reindex: `reindexing_twice_leaves_the_same_rows`,
`reindex_is_refused_when_ml_is_disabled`, `reindex_writes_one_row_per_source`,
`reindex_deletes_an_orphan`, `reindex_reports_its_counts`,
`deleting_a_note_removes_its_embeddings`,
`deleting_an_application_removes_its_embeddings`.

The contract: `the_ml_routes_are_in_the_openapi_document`,
`post_notes_search_is_not_shadowed_by_the_note_read`,
`the_search_endpoint_publishes_pagination_once`, `the_score_is_a_number`.

The two integration tests this contract adds: `reindex_writes_unit_vectors`
(every row's `l2_norm` is 1 within `1e-5`, read back through SQL and checked in
Rust) and `a_hybrid_score_is_the_cosine_of_the_planted_vectors` (two planted
unit vectors with a known dot product re-appear with that score, rounded to four
decimals).

**`src/ml/*.rs`, unit tests** (owned by `rust-backend`; they live in `src/`, so
`test-engineer` cannot own the file, but they are written from this contract and
must be shown to fail first):

| File | Test |
| ---- | ---- |
| `embedder.rs` | `a_missing_model_selects_the_disabled_embedder`, `a_missing_tokenizer_selects_the_disabled_embedder`, `ml_disabled_selects_the_disabled_embedder`, `the_disabled_embedder_reports_its_reason`, `an_unknown_backend_is_refused_at_start_up`, `normalising_a_zero_vector_is_an_error` |
| `deterministic.rs` | `the_deterministic_embedder_is_stable`, `the_deterministic_embedder_has_the_configured_dimension`, `the_deterministic_embedder_returns_a_unit_vector`, `the_deterministic_embedder_performs_no_io` |
| `tokenizer.rs` (new) | `the_tokenizer_splits_a_word_into_wordpieces`, `the_tokenizer_marks_an_unknown_word_as_unk`, `the_tokenizer_frames_the_sequence_with_cls_and_sep`, `the_tokenizer_truncates_to_the_bound` |
| `service.rs` | `cosine_similarity_is_symmetric_and_bounded`, `a_vector_of_the_wrong_dimension_is_skipped`, `a_timeout_maps_to_the_inference_timeout_degradation` |
| `src/config/tests.rs` | `the_ml_settings_have_their_documented_defaults`, `the_candidate_limit_is_bounded_above`, `the_ml_backend_is_one_of_the_supported_values`, `the_tokenizer_path_defaults_to_the_models_directory` |

**`tests/seeder.rs`** — `the_seeder_writes_embeddings_only_with_the_flag`
(without `--with-embeddings` the `embeddings` count is 0; with it, one row per
note and per application, every vector unit length, `model_id` the
deterministic one).

**`tests/openapi_contract.rs`** — no new test; the existing table-versus-document
assertions cover the new operations once `ROUTE_TABLE` has them.

**`the_fixture_queries_find_the_expected_notes`** — `ML-R09`'s fifteen queries,
in `tests/ml_api.rs`, under the deterministic embedder. It proves the plumbing
and nothing about quality.

### 12.8 The files each agent owns

| Agent | Files | Chains |
| ----- | ----- | ------ |
| `database` | `migrations/0006_embeddings.sql` — the table, three `CHECK`s, two indexes. | `M3` |
| `rust-backend` | `src/ml/embedder.rs`, `disabled.rs`, `deterministic.rs`, `burn_embedder.rs`, `tokenizer.rs` (new), `service.rs` (new), `repository.rs` (new), `dto.rs` (new), `handlers.rs` (new), `mod.rs`; `src/state.rs`; `src/config/{settings,loader,validation,tests}.rs`; `config/ml.toml` (new); `.env.example`; `src/notes/repository.rs` (the lexical candidate query and the delete statement) and `src/job_applications/repository.rs` (the application lookup for a match and the delete statement); `src/api/routes.rs`; `src/bin/atom-ml-bench.rs` (new); `src/bin/atom-seed.rs` (the `--with-embeddings` flag and the `embeddings` delete statement already listed in `seeding.md`); `Cargo.toml` only if a dependency is justified and recorded. | `M3`–`M6` |
| `test-engineer` | `tests/ml_api.rs` (new), `tests/seeder.rs` (extend), `tests/openapi_contract.rs` (only if the shared assertions need the new routes named). Unit tests inside `src/` are written by `rust-backend` from this contract, because the file's owner is the module. | `M4`–`M6` |
| `docs` | `docs/features/ml/{specification,architecture,api,testing,status}.md`, `docs/database/{schema,seeding}.md`, `docs/engineering/{change-log,implementation-status,known-issues}.md`, `docs/api/README.md`, `docs/architecture/deployment.md` (the settings table), `docs/testing/strategy.md`. | every chain's close |
| `architect` | `docs/decisions/ADR-0008-local-pure-rust-ml.md`, `docs/features/ml/architecture.md`, `docs/features/ml/status.md`. | `M2`, and any contract change |
| `code-reviewer`, `quality-gate` | Read-only. They inspect, they do not edit. | `M6` |

### 12.9 What these chains must not do

- **No fourth `Embedder` implementation**, including a "slow" one to force a
  timeout: `ML-R03` fixes three, and the timeout mapping is unit-tested instead.
- **No new `degraded_reason` value.** It is a client-visible enum; a fourth
  value is a contract change in [`api.md`](api.md),
  [`../../api/conventions.md`](../../api/conventions.md) §17 and the OpenAPI
  schema, together.
- **No model download, at build time or at run time**, and no HTTP client in
  `src/ml/`.
- **No trigger** for the embedding cleanup, and no second transaction that
  pretends to be one.
- **No `.unwrap()`/`.expect()` in a request path**, no SQL built by
  concatenation, and no query over `embeddings` without `user_id`.
- **No deleting the degradation tests** because "the model is not there in CI".
  They are the only proof that the feature is safe without one.

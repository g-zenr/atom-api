# Local machine learning — specification

> **Status:** Implemented
> **Last updated:** 2026-10-26
> **Owner:** maintained by the `docs` agent
> **Feature identifier:** `ATOM-FEAT-ML-001`
> **Feature name:** Local embeddings, hybrid note search and application matching

## 1. Purpose and the user problem

Full-text search finds the words the owner typed. It does not find the note
that says the same thing in different words: a search for "container
orchestration" misses a note about "Kubernetes rollout", and a search for
"salary negotiation" misses "asked for 75k".

This feature adds **meaning-based retrieval over the owner's own notes**,
computed locally. It also ranks the owner's notes against one job application,
so the notes that are relevant to an interview are the ones that surface.

The constraint is as important as the capability: **the owner's data does not
leave the machine.** There is no hosted embedding API, no model server and no
second language runtime. The model is a local file read by the Rust process.
That is what makes it acceptable to embed a private note at all.

The feature is deliberately a **re-ranker**, not a search engine. It narrows
with the lexical index that Phase 1 already has, then orders the candidates by
cosine similarity. The design is honest about that: a note that shares no
lexical term with the query is not found by hybrid search, and that limitation
is stated in §9 rather than hidden.

## 2. Status

**Implemented, not `Verified`.** The requirements are recorded in
[`project-truth.md`](../../project-truth.md) §18.3 as `ML-R01`…`R09` and agreed
on 2026-10-24. Migration `0006_embeddings.sql`, the module `src/ml/`, the three
published operations and the suite exist, and the suite is measured green at
`4df383e` (511 passed, 0 failed). **`ML-R02` is met**: `embedder::select` returns
a `BurnEmbedder` when both the SafeTensors file and the vocabulary load. The
feature is not `Verified` because no independent reviewer has read the encoder
span and no re-gate of `ATOM-REVIEW-20261024-011` is recorded; see
[status.md](status.md) §1.3.

The model rests on assumption **A4** in
[`../../engineering/runs/archive/phase-2/plan.md`](../../engineering/runs/archive/phase-2/plan.md) §3,
which the owner did not explicitly confirm (owner decision O1). **The stack is
not provisional:** O6 fixes Rust-only, Burn v0.22 and the `flex` backend, and
`AGENTS.md` §4.12 repeats it. The decision is
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).

## 3. Scope

In scope:

- An `Embedder` abstraction with three implementations: a Burn model, a
  deterministic test embedder and a disabled one.
- An embedding index over the owner's notes and job applications, stored in
  PostgreSQL as `REAL[]`.
- `POST /api/v1/notes/search` with the modes `lexical`, `semantic` and
  `hybrid`.
- `POST /api/v1/job-applications/{application_id}/match`, which ranks the
  owner's notes against one application.
- `POST /api/v1/ml/reindex`, idempotent and owner-scoped.
- A documented degradation to lexical results with `200` when ML is off or
  the model is missing.
- Owner-scoped reads and writes for every embedding and every search.
- A fixed evaluation set that proves the plumbing, and a recorded manual run
  that reports the real model's quality.

## 4. Non-goals

These are deliberately **not** part of Phase 2:

- **Text generation and retrieval-augmented generation.** No summary, no
  answer, no cover letter. RAG needs generation, which
  [`project-truth.md`](../../project-truth.md) §6.1 excludes.
- **A vector-database extension or service.** No pgvector, no Qdrant, no
  external index. The reason is in
  [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).
- **Any outbound call.** No hosted embedding API, no model download at build
  or run time, no telemetry.
- **A second language runtime.** No Python, Node, Go or model server.
- **Cross-account search.** An embedding belongs to one account and is never
  compared with another account's vector.
- **Clustering, topic modelling, summarisation, classification or
  recommendations.** Only retrieval and ranking.
- **An approximate-nearest-neighbour index.** The re-rank is over a bounded
  candidate set.
- **Fine-tuning or training.** The model is used as shipped.
- **Embedding claims, tasks or projects.** The `source_type` values are
  `note` and `application` only.
- **A model-selection API.** The model is a deployment setting.

## 5. Accepted requirements

The source of truth is [`project-truth.md`](../../project-truth.md) §18.3.

| ID | Requirement | Planned implementation | Planned coverage |
| -- | ----------- | ---------------------- | ---------------- |
| `ML-R01` | All inference runs locally in the process; no outbound call; no second runtime. | `src/ml/` with no HTTP client ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md)) | `the_deterministic_embedder_performs_no_io`, and an inspection recorded in [status.md](status.md) |
| `ML-R02` | Burn v0.22, `flex` backend, SafeTensors weights, no extension. | `src/ml/burn_embedder.rs` | `a_missing_model_selects_the_disabled_embedder`, `an_unknown_backend_is_refused_at_start_up` |
| `ML-R03` | The `Embedder` trait has exactly `Burn`, `Deterministic` and `Disabled`. | `src/ml/embedder.rs` | `the_deterministic_embedder_is_stable`, `the_deterministic_embedder_has_the_configured_dimension` |
| `ML-R04` | `REAL[]` storage with `model_id` and `dim`, unique per source. | `embeddings` table | `reindex_writes_one_row_per_source`, `a_vector_of_the_wrong_dimension_is_skipped` |
| `ML-R05` | Three search modes; hybrid is a bounded lexical candidate set re-ranked by cosine. | `src/ml/service.rs` | `lexical_mode_matches_the_notes_search`, `hybrid_returns_a_score_for_every_hit`, `hybrid_never_returns_more_than_the_candidate_limit` |
| `ML-R06` | Degradation to lexical with `200` when ML is off or the model is missing. | `EmbedderStatus` and the search path | `search_degrades_to_lexical_when_ml_is_disabled`, `search_degrades_to_lexical_when_the_model_is_missing` |
| `ML-R07` | Reindex is idempotent, owner-scoped, and `409` when ML is disabled. | `MlService::reindex` | `reindexing_twice_leaves_the_same_rows`, `reindex_is_owner_scoped`, `reindex_is_refused_when_ml_is_disabled` |
| `ML-R08` | Every read and write is owner-scoped; a foreign source is `404`. | `user_id = $n` in every statement | `search_never_returns_another_users_note`, `matching_a_foreign_application_is_not_found` |
| `ML-R09` | 15 fixture queries with expected notes; the real model's quality is a recorded manual run. | the fixture set and [status.md](status.md) | `the_fixture_queries_find_the_expected_notes` |

## 6. User-facing behaviour

### 6.1 The three modes

| Mode | What it returns | Needs a model |
| ---- | --------------- | ------------- |
| `lexical` | The notes that the Phase 1 full-text search returns, in the notes order, with no score. | No |
| `semantic` | The caller's notes ordered by cosine similarity between the query embedding and the note embedding. | Yes |
| `hybrid` | The top lexical candidates, re-ranked by cosine similarity. | Yes |

- `mode` defaults to `hybrid`.
- The response reports `requested_mode` (what the client asked for) and `mode`
  (what was applied), plus `degraded` and `degraded_reason`.
- `lexical` mode on an ML-enabled deployment is **still lexical**: asking for
  it never loads the model.
- A `score` is a JSON number in `[0,1]`, rounded to four decimals, and is
  `null` in lexical mode. A score is not money and is not a string.
- Ties are broken by `note_id`, so the order is total and pagination is
  stable.

### 6.2 Degradation

```text
requested semantic or hybrid
        │
        ├─ ml.enabled = false            → lexical results, 200, degraded_reason "ml_disabled"
        ├─ the model file is missing     → lexical results, 200, degraded_reason "model_missing"
        ├─ inference exceeds the budget  → lexical results, 200, degraded_reason "inference_timeout"
        └─ otherwise                     → the requested mode, 200, degraded = false
```

The timeout branch applies to **search**. A match has no lexical fallback, so
a match that exceeds the budget is a `408`, not a degraded `200`.

- **The status is `200` in every degraded case.** A missing optional model is
  a configuration state, not a server error.
- `lexical` requests are never degraded; they report `degraded` `false`.
- A degraded response is otherwise a complete response: the items, the
  pagination and the envelope are the same.
- The two mandatory triggers are "ML off" and "model missing" (`ML-R06`). The
  inference timeout is a defensive third trigger, so that a slow model cannot
  turn a working search into a `408` or a `500`.

### 6.3 Matching

`POST /job-applications/{application_id}/match` embeds the application's
`role_title` and `description`, compares it with the caller's note
embeddings, and returns the top matches with their scores.

- The application must be the caller's; otherwise `404`.
- **A match needs a model.** When ML is disabled or the model is missing, the
  request is a `409 conflict`, not a degraded empty list: a ranking with no
  model would be a fiction.
- The result is bounded by `limit` (default 10, maximum 50) and is not
  paginated.

### 6.4 Reindexing

- Reindex is **idempotent**: a note already embedded with the current
  `model_id` and `dim` is skipped.
- It only touches the caller's rows.
- It deletes orphaned embeddings, whose source no longer exists.
- It returns counts of what it wrote, skipped and deleted.
- It refuses with `409 conflict` when ML is disabled, because the operation
  cannot do what it says.

### 6.5 Ownership

- A search returns only the caller's notes, in every mode, even if an
  embedding row for another account exists.
- A match on another account's application is `404`.
- A reindex never reads or writes another account's rows.
- An embedding is derived personal data and is treated with the same
  sensitivity as the note it came from.

## 7. Dependencies on other features

| Direction | Relationship |
| --------- | ------------ |
| ML → Authentication | Every endpoint requires `AuthUser`; its `user_id` is the ownership predicate. |
| ML → Notes | **Required.** Notes are one `source_type`, and lexical mode reuses the notes full-text query. Deleting a note must delete its embeddings. |
| ML → Job applications | **Required for matching.** Applications are the second `source_type`. Deleting an application must delete its embeddings. |
| ML → Projects / Tasks | None. |
| ML → Database | `REAL[]` and a unique index; **no extension** ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md)). |

## 8. Acceptance criteria

A change to this feature is complete when all of the following hold.

- [ ] The three quality gates pass with a disposable `TEST_DATABASE_URL`, and
      `atom-docs` reports no error.
- [ ] `cargo check --no-default-features` passes: ML is not a mandatory
      feature for the base build.
- [ ] Every requirement in §5 has a named test in [testing.md](testing.md),
      and the test fails if the rule is removed.
- [ ] Both mandatory degradation cases return `200` with `mode` `lexical` and
      the correct `degraded_reason`.
- [ ] `hybrid` never re-ranks more than `ml.candidate_limit` rows, asserted
      with a corpus larger than the bound.
- [ ] A semantic search cannot return another account's note, asserted with a
      row planted directly in `embeddings`.
- [ ] A reindex run twice leaves the row count unchanged.
- [ ] Deleting a note and an application deletes their embeddings in the same
      transaction, asserted by a count after the delete.
- [ ] Every path and method is in `/api/v1/openapi.json`, asserted by the
      contract test.
- [ ] The migration applies from an empty database, and
      [`schema.md`](../../database/schema.md) describes it.
- [ ] The seeder can write embeddings with its `--with-embeddings` flag, using
      the deterministic embedder.
- [ ] The evaluation set of 15 queries passes under the deterministic
      embedder, and the real model's run is recorded.
- [ ] [status.md](status.md) reflects the evidence, and nothing is marked
      `Verified` without a recorded passing run.

## 9. Known limitations

- **Hybrid recall is bounded by the lexical candidate set.** A note that
  shares no term with the query is invisible to hybrid search, however similar
  it is in meaning. This is the direct consequence of not having a vector
  index, and it is the most important limitation of the feature.
- **A note that has never been embedded is not returned by hybrid or semantic
  mode.** It is still returned by `lexical` mode. Until a reindex runs, a
  newly created note is invisible to the scored modes.
- **Semantic mode is a linear scan of the caller's embeddings.** It is a
  sequential cosine over the owner's notes, not an indexed nearest-neighbour
  query. It is acceptable at a personal corpus size and it does not scale to
  millions of rows.
- **The deterministic embedder is not semantic.** It proves the plumbing —
  storage, the upsert, the re-rank, the degradation — and says nothing about
  retrieval quality. Only the recorded manual run does.
- **One model at a time.** Embeddings carry a `model_id`, so two models can
  coexist in the table, but the service queries one. A model change is a
  reindex, and the old rows are orphaned until the next reindex deletes them.
- **The model file is an operator artifact.** It is not in the repository and
  not downloaded by the build. A missing file degrades to lexical search,
  which is safe and easy to miss.
- **A dimension change is a full reindex.** A vector of the old dimension
  cannot be compared with one of the new dimension, so a mismatch is skipped.
- **The index can be stale.** A note edited without its embedding being
  refreshed keeps its old vector until a reindex. Phase 2 does not re-embed on
  every write; the reindex endpoint is the mechanism.
- **No per-note feedback.** There is no relevance judgement and no learning
  from clicks.

## 10. Changelog references

Substantial changes are recorded in
[`change-log.md`](../../engineering/change-log.md). The feature identifier is
stable; the entries are appended and never rewritten.

- The Phase 2 product intake that accepted
  `ML-R01`…`R09` and
  [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md). The change-log
  identifier is assigned when the intake is committed; no identifier is
  claimed here, because the log is the authority for it.

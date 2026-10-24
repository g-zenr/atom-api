**Status:** Accepted

# Local machine learning — status

> **Last updated:** 2026-10-24 (chain `D4.M2`)
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

## 1. The status

The feature is **Accepted**. The requirements `ML-R01`…`R09` are agreed in
[`project-truth.md`](../../project-truth.md) §18.3, with acceptance criteria.
The design decision is
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md), finalised with the
spike's measured result in its §8.

The feature is **not** `Implemented` and **not** `Verified`:

- `src/ml/` exists only as the **spike surface**: `embedder.rs` (the trait and
  the error type), `disabled.rs`, `deterministic.rs` and a `burn_embedder.rs`
  that is a lookup, a mean-pool and a projection — **not** a BERT encoder, and
  not the pooling this model uses for sentence embeddings;
- there is no `service.rs`, `repository.rs`, `dto.rs`, `handlers.rs` or
  `tokenizer.rs`, so nothing in the module is reachable from the HTTP API;
- there is no `embeddings` migration: `migrations/` ends at `0005`;
- there is no `tests/ml_api.rs` and no ML route in the router;
- no ML test has been written, and therefore none has run or passed;
- the seven `ml.*` settings do not exist in `config/` or `.env.example`;
- there is no model file in the repository, and there never will be.

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `ML-R01` Local inference only, no second runtime | Decided, partly evidenced | [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §1; `src/bin/atom-ml-spike.rs` ran in-process with no HTTP client. No endpoint uses it yet. |
| `ML-R02` Burn v0.22, `flex`, SafeTensors, no extension | Decided, partly evidenced | `burn` 0.22 is an optional dependency behind the `ml` feature; the spike loaded a real 86.66 MB SafeTensors file on `flex` ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §8). The encoder is not built. |
| `ML-R03` `Burn`, `Deterministic`, `Disabled` | Spike surface only | The trait and three implementations exist; `Burn` is a stand-in, and the count is fixed at three by §12.1 of the architecture. |
| `ML-R04` `REAL[]` storage | Not built | The table is specified in [`../../database/schema.md`](../../database/schema.md) §11.6; migration `0006` does not exist. |
| `ML-R05` Three modes with a bounded re-rank | Not built | [`architecture.md`](architecture.md) §4 and §12.5. |
| `ML-R06` Degradation to lexical with `200`, and `409` on a match when ML is unavailable | Not built | The rule is in [`api.md`](api.md) §3 and §12.6 of the architecture; no route exists to answer. |
| `ML-R07` Idempotent, owner-scoped reindex | Not built | — |
| `ML-R08` Owner-scoped reads and writes | Not built | — |
| `ML-R09` 15 fixture queries and a manual run | Not built | — |

`project-truth.md` §17 records the whole area as **Not started**.

### 1.1 What `D4.M1` and `D4.M2` established

- **The delivery gate of `ADR-0008` §6 passed** for the question it asked: Burn
  0.22 with the `flex` backend loads a real SafeTensors file and runs a forward
  pass in-process, at 0.017–0.020 ms per text and 93.5 MiB peak RSS. The numbers
  are in `ATOM-CHANGE-20261024-010` and are summarised in
  [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §8.
- **The gate did not answer the question `M3` depends on.** No attention block,
  no feed-forward block and no LayerNorm ran, and the tokenizer is a stand-in,
  so the latency is a **lower bound** and no retrieval quality was measured.
  `ML-R05` and `ML-R09` are untouched by the spike.
- **Two decisions were taken and are now binding**:
  1. **The stored vector is normalised at write time**
     ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §9). The
     spike's output L2 norm was `0.059342`; the invariant is now
     `(1.0 - l2_norm(&v)).abs() <= 1e-5`, so cosine is exactly the dot product
     and a row's meaning never depends on which code path wrote it.
  2. **The tokenizer is a second model artifact** with its own setting,
     `ml.tokenizer_path` ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md)
     §10), because a SafeTensors file does not contain the WordPiece vocabulary.
- **The contract for `M3`–`M6` is written** in
  [`architecture.md`](architecture.md) §12: the trait, the three
  implementations, the `embeddings` table and its cleanup rule, the three
  endpoints with their status codes, the degradation rule, the test names and
  the files each agent owns.

## 2. What is still missing to reach `Verified`

1. ~~**The spike.**~~ Done, and recorded: `D4.M1`, the numbers in
   [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §8, the caveats
   intact. It is **not** a measurement of a MiniLM embedding.
2. ~~**The dependency.**~~ Done: `burn` 0.22 behind the default `ml` feature,
   with the reason recorded and the cost as `KI-022`.
3. **The encoder and the tokenizer.** A real six-layer BERT encoder loaded with
   `burn_store`, mean pooling with the attention mask, L2 normalisation, and a
   WordPiece tokenizer (`architecture.md` §12.3). **This is the largest
   remaining piece and the fallback if it does not fit; see §3.1.**
4. **The re-measurement.** `src/bin/atom-ml-bench.rs` over the real encoder;
   the numbers decide whether the `ml.timeout_seconds` default survives, and
   they are reported, not silently absorbed.
5. **The migration.** `0006_embeddings.sql`: the table, three `CHECK`s and two
   indexes of [`../../database/schema.md`](../../database/schema.md) §11.6.
6. **The configuration.** The seven `ml.*` settings in `config/ml.toml`, the
   typed structure, the loader (`BASE_FILES` **and** `embedded()`), the
   validation, `.env.example`, the reference table and the tests — in one
   change (`AGENTS.md` §4.11).
7. **The module.** `service.rs`, `repository.rs`, `dto.rs` and `handlers.rs`,
   the start-up selection of the embedder, and the `MlService` in `AppState`.
8. **The routes.** Registered in `api_routes()` and in `ROUTE_TABLE`, in the
   same change.
9. **The delete statements.** `notes` and `job_applications` purge their own
   embeddings in the same statement that deletes the source
   (`architecture.md` §3).
10. **The tests.** Every test named in [`testing.md`](testing.md) and in
    `architecture.md` §12.7, written first, shown to fail, then green.
11. **The evaluation.** Fifteen fixture queries under the deterministic
    embedder, and one recorded manual run with the real model.
12. **The seeder.** `--with-embeddings` writes unit vectors with the
    deterministic embedder, and the `embeddings` row of its delete order starts
    executing.
13. **The documents.** [`../../database/schema.md`](../../database/schema.md) and
    [`../../database/seeding.md`](../../database/seeding.md) describing what lands;
    [`api.md`](api.md) and [`../../api/conventions.md`](../../api/conventions.md) §17
    unchanged or amended deliberately.
14. **A recorded run.** The gates and the suite, with the result written down.

Only after 3–14 is the feature `Implemented`; it becomes `Verified` when the
tests pass in a recorded run.

## 3. What a future change must do to keep it `Accepted` and then `Verified`

- **Do not change an accepted requirement silently.** A revision goes through
  [`project-truth.md`](../../project-truth.md) §18.4.
- **A4 is a provisional default.** The stack is not: O6 and `AGENTS.md` §4.12
  fix Rust-only, Burn and `flex`.
- **A second runtime or an outbound call is a new decision**, not an
  implementation detail. It would need an amendment to
  [`project-truth.md`](../../project-truth.md) §6.1 and a superseding ADR.
- **A change to `REAL[]`, to the hybrid design, to the normalisation invariant
  or to the store's `model_id` convention** needs a new ADR that supersedes
  [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).
- **A new degradation trigger** is a contract change: it adds a value to
  `degraded_reason` that a client switches on.
- **A fourth `Embedder` implementation** is refused by `ML-R03` and by
  `architecture.md` §12.9. A test that needs a slow or broken embedder is
  rewritten as a unit test of the mapping, not as a new implementation.

### 3.1 The pre-agreed fallback

If the encoder and tokenizer do not fit this phase, **ML ships as `Disabled`,
lexical search is the behaviour, `ml.enabled` keeps its default `false`, and no
endpoint depends on a model file.** The three routes, the table, the
degradation rule, the ownership predicates, the reindex and the suite still
ship, and `semantic` and `hybrid` still answer `200` with lexical results.

That outcome is **legitimate and pre-agreed**
([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §11). It is not a
success either: `ML-R05`'s scored modes and `ML-R09`'s manual run are **not
met**, this file must say so, the reason is filed in
[`../../engineering/known-issues.md`](../../engineering/known-issues.md), and
the feature stays `Accepted` rather than being relabelled. What it must not
become is a fake encoder, a downloaded model, a second runtime, or a deleted
degradation test.

## 4. The manual evaluation record

> **Not run.** There is no encoder, no endpoint and no model file in the
> repository, so there is nothing to evaluate. This section is where the result
> goes, with the model, the dimension, the latency, the memory and the fifteen
> queries' outcome. The spike's numbers do **not** belong here: they measured a
> lookup and a projection, not retrieval.

| Field | Value |
| ----- | ----- |
| Model | `all-MiniLM-L6-v2`, SafeTensors, dimension 384 (operator-supplied, not in the repository) |
| Tokenizer | `vocab.txt` beside the weights |
| Latency | Not measured for the encoder. The spike's 0.017–0.020 ms per text is a lower bound on a different network. |
| Memory | Not measured for the encoder. The spike's 93.5 MiB peak RSS is the lookup-only network. |
| Fifteen queries | Not run. |

## 5. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | The modules, the `Embedder` trait and its invariants, storage and cleanup, the search pipeline, degradation, configuration, the security boundary, and §12 — the build contract for `M3`–`M6`. |
| [`api.md`](api.md) | The three endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The planned test suite, the evaluation split, and what is deliberately not covered. |
| [`../../api/conventions.md`](../../api/conventions.md) | The search modes and the degradation rule, §17. |
| [`../../database/schema.md`](../../database/schema.md) | The `embeddings` table, §11.6. |
| [`../../database/seeding.md`](../../database/seeding.md) | The seeder's `--with-embeddings` contract, `S4`. |
| [`../../decisions/ADR-0008-local-pure-rust-ml.md`](../../decisions/ADR-0008-local-pure-rust-ml.md) | The local, pure-Rust decision; §8 the measured result, §9 normalisation, §10 the tokenizer, §11 the fallback. |
| [`../../engineering/change-log.md`](../../engineering/change-log.md) | `ATOM-CHANGE-20261024-010`, including its Correction block: the numbers and their three caveats. |

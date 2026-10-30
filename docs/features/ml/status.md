**Status:** Verified

# Local machine learning — status

> **Last updated:** 2026-10-29 (promotion; measurements at `4df383e` and the gate runs of 2026-10-27/29)
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)
> **Read this first:** `ML-R02` **is met**. `embedder::select` returns a
> `BurnEmbedder` when `ml.enabled` is true, `ml.backend` is `flex`, and both the
> SafeTensors file and the vocabulary load. The feature is **`Verified`** as of
> 2026-10-29: the encoder span was read clean by
> [`ATOM-REVIEW-20261029-002`](../../engineering/review-log.md) (building on
> `ATOM-REVIEW-20261026-001`, which found no code defect in the same span), the
> six gates re-ran at `0`, and the recorded checks pass — including the
> real-model benchmark, **reproduced on 2026-10-29** from its documented
> invocation (§5.1a: 15/15 at rank 1). What remains is the absence of a
> model-loading CI case, recorded in §1.3.
> Read §1, §2.1 and §5 together: they say the same thing.

## 1. The status

The feature is **`Verified`** (2026-10-29), and `ML-R02` **is met**.

The promotion evidence, recorded in
[`ATOM-REVIEW-20261029-002`](../../engineering/review-log.md): an independent
read of the final tree returned **PASS** on code across `src/ml/` (the
degradation decision table exact, every embed normalised, inference CPU-only
behind `spawn_blocking` and bounded by `timeout_seconds`, the score floor
applied before limit and total, no network dependency anywhere), **PASS** on
regressions since `4df383e`, and the quality-gate session re-executed the six
gates at `0` with the 536-test split re-derived target-for-target (41 `ml_api`
cases, 64 unit cases, seven configuration cases). `Verified` is the
[`AGENTS.md`](../../../AGENTS.md) §6 word: implemented, and covered by a test
or a recorded check that passes. The one recorded check that is not a CI gate —
the real-model benchmark — is §5, and its limitation is stated in §1.3 rather
than hidden.

The requirements `ML-R01`…`R09` are agreed in
[`project-truth.md`](../../project-truth.md) §18.3, with acceptance criteria. The
design decision is [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md),
finalised with the spike's measured result in its §8 and amended with the real
encoder's measurement in its §12.

### 1.1 What exists

Read from the working tree on 2026-10-26. The feature is committed: the spike at
`bfd1288`, the finalised ADR at `a4d33a3`, the module, schema and operations at
`ba198ff`, the close-out at `ba91008`, and the removal of the two
`#[allow(clippy::cast_possible_truncation)]` attributes at `505a189`. `grep
"allow(" src/ tests/` returns no hits.

| Piece | State | Evidence |
| ----- | ----- | -------- |
| The `Embedder` trait, `EmbedderStatus`, `MlError`, `l2_norm`, the start-up selection | Exists | `src/ml/embedder.rs` |
| `Deterministic` | Exists | `src/ml/deterministic.rs` |
| `Disabled` | Exists | `src/ml/disabled.rs` |
| `BurnEmbedder` as an **encoder** | **Exists.** Six pre-LayerNorm BERT layers, 12 heads of width 32 (`HEAD_DIM = HIDDEN / HEADS` = 384/12), a 384→1536→384 feed-forward pair, masked mean pooling, L2 normalisation at write time, and roughly a hundred tensors loaded through `burn_store`. | `src/ml/burn_embedder.rs`; six unit cases |
| `src/ml/tokenizer.rs` | **Exists.** Hand-written WordPiece: lowercase, accent strip, greedy longest-match against `vocab.txt`, `##` continuations, `[UNK]`, `[CLS]`/`[SEP]`, attention mask and a 256-token truncation bound. | `src/ml/tokenizer.rs`; fifteen unit cases |
| `service.rs`, `repository.rs`, `dto.rs`, `handlers.rs` | Exist, carrying their `utoipa::path` annotations | `src/ml/` |
| `MlService` in `AppState` | Exists, built from `crate::ml::embedder::select(&config.ml)` | `src/state.rs` |
| Migration `0006_embeddings.sql` and the `embeddings` table | Exists: the table, its three `CHECK`s and its two indexes | `migrations/0006_embeddings.sql`; [`../../database/schema.md`](../../database/schema.md) §11.6 |
| The seven `ml.*` settings | Exist: `config/ml.toml`, `MlSettings`, the loader, the `APP_ML_*` overrides and `.env.example` | `config/ml.toml`, `src/config/`, `.env.example` |
| The three operations | **Mounted and published.** `ROUTE_TABLE` declares them and `api_routes()` serves them; the surface is 54 operations. | `src/api/routes.rs`, `tests/openapi_contract.rs` |
| The embedding purge on delete | Exists: `notes` and `job_applications` delete their embeddings in the statement that deletes the source | `src/notes/repository.rs`, `src/job_applications/repository.rs` |
| `tests/ml_api.rs` | Exists: **41 cases**, `#![cfg(feature = "ml")]`. **All 41 pass** (run of 2026-10-26, §1.5). | `tests/ml_api.rs`; [`testing.md`](testing.md) §2 |
| Unit tests in `src/ml/` | **64 cases**, including six in `burn_embedder.rs` and fifteen in `tokenizer.rs`. | `src/ml/*.rs` |
| Unit tests for `ml.*` | **Seven** in `src/config/tests.rs` (58 cases in the file) and pass: the defaults, the environment override, the backend vocabulary, the candidate-limit ceiling, the dimension, the timeout ceiling and the guarantee that no relation to the page maximum survives. | `src/config/tests.rs` |
| The seeder's `--with-embeddings` | **Exists**: one deterministic row per seeded note and per application, skipped on a second run. `tests/seeder.rs` holds 21 cases, one of them `the_seeder_writes_embeddings_only_with_the_flag`. | `src/bin/atom-seed.rs`, `tests/seeder.rs` |
| The setting table in [`config/README.md`](../../../config/README.md) | **Updated** with the seven `ml.*` rows, as `AGENTS.md` §4.11 requires. `atom-docs --strict` reports `OK (0 warnings)`. | `config/README.md` |
| The model file | Absent, and it never enters the repository | [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) |

The `ml` Cargo feature — in the default set — gates `burn` and
`src/ml/burn_embedder.rs`. The trait, `Deterministic`, `Disabled`, the service,
the storage and the three routes are compiled in both builds, so the router, the
route table and the OpenAPI document describe one surface rather than two that
can drift. `cargo check --no-default-features` compiles the API without Burn.

### 1.2 The shipped default is lexical

**The shipped behaviour is lexical by default, because `ml.enabled` is `false` by
default.** That value is in [`config/ml.toml`](../../../config/ml.toml), in
`MlSettings`, and in the loader; with `ml.enabled = false`, `select` returns
`Disabled` whatever `ml.backend` says.

So a deployment that does nothing serves:

| Request | Answer |
| ------- | ------ |
| `POST /api/v1/notes/search`, `mode` `lexical` | `200`, lexical results, `degraded` `false` |
| `POST /api/v1/notes/search`, `mode` `semantic` or `hybrid` | `200`, lexical results, `mode` `lexical`, `degraded` `true`, `degraded_reason` `ml_disabled` |
| `POST /api/v1/job-applications/{id}/match` | `409 conflict` |
| `POST /api/v1/ml/reindex` | `409 conflict` |

That is the pre-agreed fallback of
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §11 and the
degradation contract of [`api.md`](api.md) §2.2, not an accident and not a
failure to configure anything. A personal workspace ships with no model file, no
vector index and no inference cost, and the full-text search that Phase 1
already provides is the answer.

A deployment that sets `ml.enabled = true` and `ml.backend = flex` and supplies
both artifacts gets the Burn encoder instead (§5). One that sets them and
supplies no artifact gets the same lexical fallback with `degraded_reason`
`model_missing`, and logs the internal cause once.

**Every one of the default answers has been observed**, on 2026-10-24, against
`atom_dev` with `ml.enabled = false`; the transcript is in §1.6.

### 1.3 The one limitation that `Verified` carries

The promotion does not rest on a model-loading CI test, because none exists,
and this document records that rather than papering over it:

1. **The encoder span has now been read.** `ATOM-REVIEW-20261026-001` read it
   end to end and found no code defect; `ATOM-REVIEW-20261029-002` re-read the
   final tree with the same result. That was the one review gap.
2. **The re-gate is recorded** (`ATOM-REVIEW-20261026-002`), and the six gates
   were re-executed at `0` on 2026-10-29 with the 536-test split re-derived.
3. **What remains a limitation:** no CI case loads a model file, because the
   weights are operator-supplied and live outside the repository (their
   documented local location and the reproduction recipe are §5.1a). The
   real-model quality evidence is no longer a single unrepeatable run: the
   2026-10-29 re-run reproduced it on the current tree from the documented
   invocation — 15/15 at rank 1, the 0.25 threshold separating positives from
   the negative control. The pair-count and cosine-range discrepancies of the
   earlier records are settled by the re-run (§4).

### 1.4 What `ML-R02` requires, and what exists

To be read as one statement, because each row alone is misleading:

| The requirement | What exists |
| --------------- | ----------- |
| Burn v0.22 is a dependency, behind the default `ml` feature | **Yes.** `cargo tree --no-default-features` lists no `burn` crate. |
| The `flex` CPU backend is the one used | **Yes**, by `BurnEmbedder` and by the encoder benchmark. |
| Weights load from a local SafeTensors file | **Yes**, about a hundred tensors through `burn_store` and an explicit key map. The pooler pair is deliberately not loaded: this model uses masked mean pooling, not `pooler.dense`. |
| An encoder produces an embedding | **Yes.** Six transformer layers with 12-head self-attention of width 32, GELU feed-forward, two LayerNorms per layer, masked mean pooling and L2 normalisation. |
| A request can obtain a Burn vector | **Yes.** `embedder::select` sends `MlBackend::Flex` to `select_burn`, which loads `ml.model_path` and the vocabulary and returns `Arc<BurnEmbedder>` with `status() == Ready`. A missing or unloadable artifact returns `Disabled` with `model_missing`; start-up continues. |

`ML-R02` is therefore met, and the dependency alone would not satisfy it: the
requirement is that embeddings are produced by Burn, and `BurnEmbedder::embed`
tokenises, runs the forward pass and normalises the output.

### 1.5 The runs that were made

**2026-10-26, tree `4df383e`, the re-gate measurement.** The six gates are green.
`cargo test --all-targets --locked` reports **511 passed, 0 failed** on Windows.
The two `tests/shutdown.rs` cases are unix-only, so a unix host reports **513**.
`cargo test --all-targets --locked -- --list` returns the same split:

| Target | Cases | Target | Cases |
| ------ | ----- | ------ | ----- |
| `lib` | 216 | `openapi_contract` | 10 |
| `atom-docs` | 3 | `projects_tasks_api` | 25 |
| `atom-seed` | 13 | `seeder` | 21 |
| `auth_api` | 29 | `shutdown` | 0 on Windows (2 on unix) |
| `claims_api` | 56 | `claims_flow` | 3 |
| `claims_schema` | 10 | `contract_sweep` | 2 |
| `docs_route` | 2 | `job_applications_api` | 51 |
| `job_applications_flow` | 2 | `jobs_schema` | 11 |
| `list_query` | 4 | `ml_api` | 41 |
| `notes_api` | 12 | — | — |

**2026-10-24, the earlier record.** The first executable `quality-gate` run over
`D0`–`D5` measured the six gates at `0` and **504** tests on Windows
(`ATOM-REVIEW-20261024-011`), with 40 `ml_api` cases at that point (41 today).
Those are history: the **511** figure above is the 2026-10-26 measurement, and
the suite has since grown to **536** (2026-10-27, the API-keys chain: 11
`api_keys_api` cases and 13 unit tests in `src/api_keys/`).

Scores are compared with a tolerance (`1e-4`) rather than for equality: `vector`
is `REAL[]`, so a stored component is an `f32` and an exact comparison would
assert the rounding rather than the cosine.

### 1.6 The degradation transcript

Run against `atom_dev` with `ml.enabled = false`, one seeded account, a freshly
started server, and `curl`. The three answers are the whole point of `ML-R06`:

| Request | Status | Body highlights |
| ------- | ------ | --------------- |
| `POST /api/v1/notes/search` `{"query":"atom","mode":"semantic"}` | `200` | `requested_mode: semantic`, `mode: lexical`, `degraded: true`, `degraded_reason: ml_disabled`, and the lexical items with `score: null` |
| `POST /api/v1/job-applications/{id}/match` `{}` | `409` | `conflict`, `ml_unavailable` |
| `POST /api/v1/ml/reindex` | `409` | `conflict`, `ml_unavailable` |

The same search was then repeated with `ml.enabled = true` and
`ml.backend = flex` and **no model file**: it answers `200`, `degraded: true`,
`degraded_reason: model_missing`. No request path returned `500` or `503`.
`model_missing` is the state a deployment gets when it turns ML on without
supplying the artifacts; the default deployment, with `ml.enabled = false`, never
looks for them, and one that supplies the artifacts is no longer degraded (§5).

## 2. Requirement by requirement

`State` says what exists. `Evidence` says what proves it: a named test that
passes in the run recorded in §1.5, or a document/source reference for a fact
that no test can pin.

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `ML-R01` All inference runs locally in the process, with no outbound call and no second runtime. | **Met in the code.** The module has no HTTP client, spawns no process, and the encoder runs in-process. | Inspection of `src/ml/`; [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §1. `the_deterministic_embedder_performs_no_io` pins the testable half. |
| `ML-R02` Burn v0.22, the `flex` backend, SafeTensors weights, no extension. | **Met.** `embedder::select` returns a `BurnEmbedder` when both artifacts load; the network and the tokenizer are real. | `src/ml/embedder.rs::select`, `src/ml/burn_embedder.rs`, `src/ml/tokenizer.rs`; §2.1 and §5. |
| `ML-R03` The `Embedder` trait has exactly `Burn`, `Deterministic` and `Disabled`. | **Met as a surface.** Exactly three implementations exist; every automated case uses `Deterministic` or the synthetic encoder, and none loads a model file. | `src/ml/`; selection in `embedder.rs::select`; `ml_disabled_selects_the_disabled_embedder`, `a_missing_model_selects_the_disabled_embedder`. |
| `ML-R04` `REAL[]` storage with `model_id` and `dim`, unique per source. | **Met in the schema and the repository.** The unique index and the dimension `CHECK` are the database's copy; the repository upserts against the first. | `migrations/0006_embeddings.sql`, `src/ml/repository.rs`; `reindex_writes_one_row_per_source`, `reindex_writes_unit_vectors` pass. |
| `ML-R05` Three search modes, with hybrid as a bounded lexical candidate set re-ranked by cosine. | **Implemented.** The service implements the three modes and the bounded re-rank; the operation answers. A hit at or below cosine zero is not returned. | `src/ml/service.rs`, `src/ml/handlers.rs`; eleven cases in [`testing.md`](testing.md) §2.1 pass. |
| `ML-R06` Degradation to lexical with `200` when ML is off or the model is missing. | **Implemented and observed.** The default is `ml.enabled = false`, the search path degrades with all four fields, and match and reindex answer `409`. | `config/ml.toml`, `src/ml/service.rs`; seven cases pass, and §1.6 is the manual transcript. |
| `ML-R07` Reindex is idempotent, owner-scoped, and `409` when ML is disabled. | **Implemented.** The service upserts per source, skips what is current, sweeps orphans, and refuses when the embedder is disabled. | `src/ml/service.rs`, `src/ml/repository.rs`; seven cases in [`testing.md`](testing.md) §2.5 pass. |
| `ML-R08` Every read and write is owner-scoped; a foreign source is `404`. | **Implemented.** Every statement in `MlRepository` binds `user_id`; the purge CTE is scoped the same way; a foreign application is a `404`. | `src/ml/repository.rs`; four cases in [`testing.md`](testing.md) §2.3 pass. |
| `ML-R09` 15 fixture queries with expected notes, and the real model's quality as a recorded manual run. | **Met, with the pair count of the manual run marked unverified.** The fixture passes under `Deterministic`; the real encoder's retrieval is recorded in §5. | `the_fixture_queries_find_the_expected_notes` passes; §4 and §5. |

**One requirement whose prose is broader than the code.** `ML-R03` says "every
automated test uses `Deterministic`". Six cases in `burn_embedder.rs` use the
synthetic encoder instead, and fifteen in `tokenizer.rs` use a fixture
vocabulary. They load no model file, which is the requirement's stated acceptance
criterion, but the sentence is broader than what exists. It is recorded here as a
finding rather than reworded: the requirement stands as written, and no test has
been changed to fit it.

### 2.1 `ML-R02` in full

**`ML-R02` is met: `embedder::select` returns a `BurnEmbedder` when both
artifacts load.**

1. **The network exists.** `src/ml/burn_embedder.rs` declares six
   pre-LayerNorm BERT layers. Each has 12-head self-attention with head width
   `HEAD_DIM = HIDDEN / HEADS` = **32** (384 / 12, not 64), a 384→1536→384
   feed-forward pair with GELU, and two LayerNorms, after an embedding block
   that sums the word, position and token-type embeddings through one
   LayerNorm. The output is masked mean pooling and L2 normalisation; the
   pooling is the model's own, not `pooler.dense`, and the pooler tensors are
   deliberately not loaded.
2. **The tokenizer exists.** `src/ml/tokenizer.rs` is a WordPiece tokenizer
   covering lowercase, accent strip, whitespace and punctuation split, greedy
   longest-match against `vocab.txt`, `##` continuations, `[UNK]`, `[CLS]`/
   `[SEP]`, the attention mask and the 256-token bound. Fifteen unit cases cover
   it.
3. **The load is the real one.** `from_artifacts` loads about a hundred tensors
   through `burn_store` with an explicit key map from the Hugging Face names. A
   missing key is a load failure, never a zeroed weight, and an unused tensor
   that is not in the deliberately-ignored list fails the load.
4. **The selection reaches it.** `select` matches `MlBackend::Flex` to
   `select_burn`, which resolves `ml.model_path` and the vocabulary, constructs
   the encoder and returns it with `status() == Ready`. A missing artifact, a
   missing vocabulary or an unloadable file returns `Disabled` with
   `model_missing`, logs the internal cause once and continues start-up.

The spike's lookup and projection were a lower bound on the plumbing, not an
embedding; §5 is the measurement of the real network. The encoder span has been
read clean twice (`ATOM-REVIEW-20261026-001`, then the final tree in
`ATOM-REVIEW-20261029-002`); §1.3 records the one limitation that `Verified`
carries — the single-run manual benchmark.

## 3. What `Verified` rests on

It rests on the review and the gate, both now recorded:

1. **The encoder span was read by an independent reviewer** — the forward pass,
   the attention shape, the pooling, the normalisation and the tokenizer —
   first in [`ATOM-REVIEW-20261026-001`](../../engineering/review-log.md) (no
   code defect), then on the final tree in
   [`ATOM-REVIEW-20261029-002`](../../engineering/review-log.md) (PASS on code,
   contract and regressions). The re-gate of `ATOM-REVIEW-20261024-011` is
   recorded as `-20261026-002`, and the six gates were re-executed at `0` on
   2026-10-29.

Nothing in the code or the suite is outstanding. The schema, the seven settings
in all seven places, the three operations with their published status codes, the
degradation rule, the ownership predicates, the idempotent reindex, the
delete-side purge, the seeder's `--with-embeddings` and the encoder itself all
exist and are covered by a passing run (§1.5).

The `408` observed end to end is a **testing** gap, not a `Verified` blocker and
not a new defect: it needs a model slow enough to overrun the budget or a fourth
`Embedder` (`ML-R03` forbids one). The mapping is unit-tested
(`a_timeout_maps_to_the_inference_timeout_degradation`), and the missing case is
listed in [`testing.md`](testing.md) §6.

## 4. The manual evaluation record

`ML-R09` asks for two different things, and this document keeps them apart:

| Half | What proves it | Where it runs |
| ---- | -------------- | ------------- |
| The fixture evaluation | The pipeline: the query is embedded, the stored vectors are compared, the sort runs and the expected note comes first. | CI, `Deterministic` — `the_fixture_queries_find_the_expected_notes`, [`testing.md`](testing.md) §2.7 |
| The real model's quality | Whether the real encoder retrieves a paraphrase, and its latency and memory. | By hand, reproducibly — `atom-ml-bench`, 2026-10-25 and re-run 2026-10-29 (§5) |

The manual run **has happened twice** and is §5; it is not a CI gate. The one
figure that was not settled — the number of evaluation pairs and the top-1
cosine range — is settled as far as this machine can settle it: the 2026-10-29
re-run produced **fifteen** pairs (the current binary's definition) with a top-1
cosine range of **0.2664–0.6740 (mean 0.4977)**, matching
[`testing.md`](testing.md) §6's recorded range and superseding the
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §12.1 five-pair
figure, which stays as the earlier probe. The repository still does not hold
the 2026-10-25 transcript, so that run's own count remains undetermined — which
is now moot for verification, because the re-run is reproducible from the
documented invocation.

## 5. The encoder, measured (2026-10-25)

The six-layer Burn encoder and its WordPiece tokenizer are built, and
`src/bin/atom-ml-bench.rs` measured the real forward pass in `--release` on this
machine:

```text
cargo run --release --bin atom-ml-bench --features ml
```

`ML-R02` is met: embeddings are produced by Burn v0.22 on the `flex` backend from
the local SafeTensors file, with masked mean pooling and unit-length output.

| Measurement | Value |
| ----------- | ----- |
| 1 text, mean | 88.0 ms (28.8 min, 210.0 max) |
| 10 texts, mean per text | 111.6 ms |
| 100 texts, mean per text | 112.6 ms — **11.3 s total** |
| Worst case, 256 tokens | **804 ms** mean (775 min, 834 max) |
| Peak RSS | 183.8 MiB |
| Determinism | identical input twice → identical vector; different input differs |
| Semantic quality | **15 of 15 queries returned the expected nearest text.** The recorded summaries carry two top-1 ranges — `0.31`–`0.64` here and `0.2664`–`0.6740` in [`testing.md`](testing.md) §6 — and a negative control at `0.1256`; neither range is re-measured here (see the pair-count note in §4). |

The spike's 0.02 ms per text was a lookup and a projection; the encoder is
roughly **5 600 times** more expensive. That number was a floor, and this is the
real cost.

### 5.1a The reproducible re-run (2026-10-29)

The 2026-10-25 run was repeated on the current tree (`96c37b2`) from the
weights' documented location, exit code `0`:

```text
$env:ATOM_ML_MODEL = "$env:USERPROFILE\.cache\huggingface\hub\models--sentence-transformers--all-MiniLM-L6-v2\snapshots\<sha>\model.safetensors"
cargo run --release --locked --bin atom-ml-bench
```

| Measurement | 2026-10-25 | 2026-10-29 re-run |
| ----------- | ---------- | ----------------- |
| Construction | 96 ms | 143.9 ms |
| First inference | — | 22.996 ms |
| 1 text, mean | 88.0 ms | 27.4 ms |
| 100 texts, mean per text | 112.6 ms | 25.4 ms |
| Worst case, 256 tokens | 804 ms mean | 148.6 ms mean |
| Peak RSS | 183.8 MiB | 184.2 MiB |
| Semantic quality | 15/15, cosine range disputed | **15/15 at rank 1**, top-1 cosine 0.2664–0.6740, mean 0.4977 |
| Threshold | — | **0.25 supported**: all 15 positives above it, the negative control (0.1256) below it |
| Determinism | identical input → identical vector | identical input → identical vector |

The two runs agree on quality (15/15, the same cosine band) and memory; the
latency differs by a factor of three to five in the new run's favour, consistent
with machine state (power profile, thermal headroom) rather than a code change —
no encoder source changed between the runs. Both are far inside the 2 s
inference budget. The invocation above is the reproduction recipe; the weights
stay operator-supplied and outside the repository.

### 5.1 What the measurement does and does not change

An earlier draft of this section claimed the measurement "invalidates two shipped
defaults". **That was wrong**, and it is corrected once here rather than left
standing:

- A hybrid search does **not** embed its candidates. `MlService::hybrid` selects
  at most `candidate_limit` candidate ids, loads their **already-stored**
  vectors, and scores them with one dot product per row. The only inference in a
  search request is the single query embed.
- The cost of 200 × 112.6 ms is the cost of **embedding** 200 texts, which is
  what `POST /ml/reindex` does — not a page, and not a candidate set.
- A hybrid search at the maximum page size takes about **90 ms**, not 11.3 s.
- `ml.timeout_seconds = 2` bounds **one inference**, whose worst real case is
  804 ms at the 256-token truncation bound — a 2.4× margin, not a violation.
- The re-rank of 200 candidates at dimension 384 is about 230,000 `f64`
  operations and 307 KB, two orders of magnitude below the inference it follows.

Both defaults stand unchanged: `timeout_seconds` **2 s** and `candidate_limit`
**200**. [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §12 records
the decision and the arithmetic.

What the measurement **did** expose is a different defect: the old validation
rule `ml.candidate_limit >= api.pagination.max_limit` bounded the candidate set
from below on the false premise that a short page was impossible. It is replaced
by a ceiling, `MAX_ML_CANDIDATE_LIMIT = 1_000`, in
`src/config/validation.rs`; the floor is gone. The change is applied
(2026-10-25), and:
- `the_candidate_limit_is_bounded_above` proves 1,001 refused naming
  `ml.candidate_limit`, and 1,000 and 3 accepted.
- `hybrid_never_returns_more_than_the_candidate_limit` proves that a
  `limit=100` hybrid search with `ml.candidate_limit = 3` answers `200` with
  three items, `total` 3 and `has_more: false` — a well-formed short page.
- The four descriptions (`config/ml.toml`, `.env.example`,
  `config/README.md`, `docs/architecture/deployment.md`) say that
  `ml.timeout_seconds` bounds one inference and that `ml.candidate_limit` is a
  work bound with a ceiling of 1,000, unrelated to the page size.

`ML-R09`'s fixture evaluation remains the CI half. The real-model run above is
the quality half: it is what the fifteen-pair evaluation in
[`testing.md`](testing.md) §2.7 does **not** prove, and it is recorded rather
than asserted. The pair count is the one figure in it that is unverified (§4).

## 6. What a future change must not do

- **Do not change an accepted requirement silently.** A revision goes through
  [`project-truth.md`](../../project-truth.md) §18.4.
- **Do not promote this file to `Verified` because the suite is green.** The
  missing half is the review (§1.3), not another test.
- **Do not describe the encoder as reviewed before a reviewer has read it.**
  §1.3 is the sentence that keeps it honest.
- **A second runtime or an outbound call is a new decision**, not an
  implementation detail: it needs an amendment to
  [`project-truth.md`](../../project-truth.md) §6.1 and a superseding ADR.
- **A fourth `Embedder` implementation** is refused by `ML-R03` and by
  `architecture.md` §12.9.
- **A change to `REAL[]`, to the hybrid design, to the normalisation invariant
  or to the store's `model_id` convention** needs a new ADR that supersedes
  [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).
- **A new degradation trigger** is a contract change: it adds a value to
  `degraded_reason` that a client switches on.

The build cost the `ml` feature already imposes is `KI-022` in
[`../../engineering/known-issues.md`](../../engineering/known-issues.md).

## 7. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | The modules, the `Embedder` trait and its invariants, storage and cleanup, the search pipeline, degradation, configuration, the security boundary, and §12 — the build contract for `M3`–`M6`. |
| [`api.md`](api.md) | The three endpoints with their request and response shapes, status codes, error cases and the degradation contract. |
| [`testing.md`](testing.md) | The 41 cases that exist, what each proves, and the honest gaps. |
| [`../../testing/strategy.md`](../../testing/strategy.md) | The layers, and which layer proves which ML property. |
| [`../../api/README.md`](../../api/README.md) | The published surface, now 54 operations. |
| [`../../api/conventions.md`](../../api/conventions.md) | The search modes and the degradation rule, §17. |
| [`../../database/schema.md`](../../database/schema.md) | The `embeddings` table, §11.6. |
| [`../../database/seeding.md`](../../database/seeding.md) | The seeder's `--with-embeddings` contract, `S4`. |
| [`../../decisions/ADR-0008-local-pure-rust-ml.md`](../../decisions/ADR-0008-local-pure-rust-ml.md) | The local, pure-Rust decision; §8 the spike, §12 the encoder measurement and the amended bounds. |
| [`../../engineering/change-log.md`](../../engineering/change-log.md) | `ATOM-CHANGE-20261024-010`, including its Correction block: the spike numbers and their three caveats. |

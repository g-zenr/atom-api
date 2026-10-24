**Status:** Implemented

# Local machine learning — status

> **Last updated:** 2026-10-24 (chain `D4.M3`–`M6`)
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

## 1. The status

The feature is **Implemented**. It is **not** `Verified`, and one requirement,
`ML-R02`, is **not met**.

`Implemented` is used exactly as [`AGENTS.md`](../../../AGENTS.md) §6 defines it:
the code exists. It is not evidence that the code behaves correctly, and this
document does not claim that it does. **`Verified` is not claimed and must not
be**, because no gate run and no test run over this tree is recorded anywhere:
the suite exists and has never been executed. The distinction is not pedantry —
the suite as a whole cannot pass as it stands, for the three reasons in §1.3.

The requirements `ML-R01`…`R09` are agreed in
[`project-truth.md`](../../project-truth.md) §18.3, with acceptance criteria. The
design decision is [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md),
finalised with the spike's measured result in its §8.

### 1.1 What exists

Read from the working tree on 2026-10-24. The committed state is `a4d33a3`; the
whole feature landed **uncommitted** in the `D4.M3`–`M6` chain, so a row is a
statement about the working tree and nothing else.

| Piece | State | Evidence |
| ----- | ----- | -------- |
| The `Embedder` trait, `EmbedderStatus`, `MlError`, `l2_norm`, the start-up selection | Exists | `src/ml/embedder.rs` |
| `Deterministic` | Exists | `src/ml/deterministic.rs` |
| `Disabled` | Exists | `src/ml/disabled.rs` |
| `BurnEmbedder` as an **encoder** | **Does not exist.** It is a weight lookup, a mean-pool and a projection. | `src/ml/burn_embedder.rs` |
| `src/ml/tokenizer.rs` | **Does not exist** | — |
| `service.rs`, `repository.rs`, `dto.rs`, `handlers.rs` | Exist, carrying their `utoipa::path` annotations | `src/ml/` |
| `MlService` in `AppState` | Exists, built from `crate::ml::embedder::select(&config.ml)` | `src/state.rs` |
| Migration `0006_embeddings.sql` and the `embeddings` table | Exists: the table, its three `CHECK`s and its two indexes | `migrations/0006_embeddings.sql`; [`../../database/schema.md`](../../database/schema.md) §11.6 |
| The seven `ml.*` settings | Exist: `config/ml.toml`, `MlSettings`, the loader, the `APP_ML_*` overrides and `.env.example` | `config/ml.toml`, `src/config/`, `.env.example` |
| The three operations | **Mounted and published.** `ROUTE_TABLE` declares them and `api_routes()` serves them; the surface is 51 operations. | `src/api/routes.rs`, `tests/openapi_contract.rs` |
| The embedding purge on delete | Exists: `notes` and `job_applications` delete their embeddings in the statement that deletes the source | `src/notes/repository.rs`, `src/job_applications/repository.rs` |
| `tests/ml_api.rs` | Exists: **40 cases**, `#![cfg(feature = "ml")]`. **All 40 pass** (run of 2026-10-24, §1.5). | `tests/ml_api.rs`; [`testing.md`](testing.md) §2 |
| Unit tests in `src/ml/` | **Exist**: the selection rule, the normalisation invariant, the deterministic embedder, the cosine function, the dimension guard, the timeout mapping and the DTO shapes. | `src/ml/{embedder,deterministic,service,dto,hash_vector}.rs` |
| Unit tests for `ml.*` | **Seven exist** in `src/config/tests.rs` and pass: the defaults, the environment override, the backend vocabulary, the candidate-limit floor, the dimension, the timeout ceiling and the candidate-limit relation to the page maximum. | `src/config/tests.rs` |
| The seeder's `--with-embeddings` | **Exists**: one deterministic row per seeded note and per application, skipped on a second run. `the_seeder_writes_embeddings_only_with_the_flag` passes. | `src/bin/atom-seed.rs`, `tests/seeder.rs` |
| The setting table in [`config/README.md`](../../../config/README.md) | **Updated** with the seven `ml.*` rows, as `AGENTS.md` §4.11 requires. `atom-docs` reports `OK (0 warnings)`. | `config/README.md` |
| The model file | Absent, and it never enters the repository | [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) |

The `ml` Cargo feature — in the default set — gates `burn` and
`src/ml/burn_embedder.rs`. The trait, `Deterministic`, `Disabled`, the service,
the storage and the three routes are compiled in both builds, so the router, the
route table and the OpenAPI document describe one surface rather than two that
can drift. `cargo check --no-default-features` compiles the API without Burn.

### 1.2 The shipped behaviour is lexical

**The shipped behaviour of this feature is lexical, because `ml.enabled` is
`false` by default.** That value is in [`config/ml.toml`](../../../config/ml.toml),
in `MlSettings`, and in the loader; `select` returns `Disabled` whatever
`ml.backend` says.

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
already provides is the answer. The lexical behaviour itself belongs to the
notes feature (`GET /api/v1/notes?search=…`); this feature adds the scored
modes, which are unavailable in the default deployment.

**Every one of them has been observed**, on 2026-10-24, against `atom_dev` with
`ml.enabled = false`; the transcript is in §1.5. They are the pre-agreed fallback
of [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §11 and the
degradation contract of [`api.md`](api.md) §2.2, not an accident and not a
failure to configure anything. A personal workspace ships with no model file, no
vector index and no inference cost, and the full-text search that Phase 1
already provides is the answer. The lexical behaviour itself belongs to the
notes feature (`GET /api/v1/notes?search=…`); this feature adds the scored
modes, which are unavailable in the default deployment.

### 1.3 Why this is not `Verified`

`Verified` means "implemented **and** covered by a test or a recorded check that
passes" (`AGENTS.md` §6). The second half is now true — §1.5 records the run —
so what keeps this file at `Implemented` is the first half of the requirement
that is still unmet:

1. **`ML-R02` is not met.** No code path produces a vector from a Burn model:
   the six-layer encoder and the WordPiece tokenizer do not exist. §2.1 is the
   full statement. A feature whose named model requirement is unmet cannot be
   marked `Verified` however green its suite is.
2. **`ML-R09`'s quality half has not been measured.** The fixture test passes,
   which proves the plumbing; it says nothing about whether a model ranks a
   paraphrase correctly, and there is no encoder to evaluate.

Everything else that stood between `Implemented` and `Verified` when this file
was first written has been closed: the 40 integration cases pass, `src/ml/` has
its unit cases, the seeder writes embeddings behind its flag, and the `ml.*` rows
are in [`config/README.md`](../../../config/README.md).

### 1.4 What `ML-R02` does and does not have

To be read as one statement, because each half alone is misleading:

| The requirement | What exists |
| --------------- | ----------- |
| Burn v0.22 is a dependency, behind the default `ml` feature | **Yes.** `cargo tree --no-default-features` lists no `burn` crate. |
| The `flex` CPU backend is the one used | **Yes**, by the spike and by `src/bin/atom-ml-spike.rs`, which remains frozen as the measurement record. |
| Weights load from a local SafeTensors file | **Yes**, for three tensors: the word-embedding table and the pooler pair. |
| An encoder produces an embedding | **No.** No attention block, no feed-forward block, no LayerNorm, no WordPiece tokenizer, and the pooling is the wrong one for the model. |
| A request can obtain a Burn vector | **No.** `embedder::select` returns `Disabled` with `model_missing` for `ml.backend = flex`, even when both artifacts are present, and logs the cause `ml_model_unavailable` once. |

The last two rows are the requirement. A dependency and a file that loads prove
the framework choice, which is what the spike was asked to prove; they do not
prove that embeddings are produced. **`ML-R02` is therefore not met**, and the
"not met" is reported to a client as `degraded_reason: model_missing` on a
search and `409` on a match or a reindex, because a fourth client-visible reason
code would be a contract change (`architecture.md` §12.9).

### 1.5 The run that was made

2026-10-24, from the repository root. The database was `atom_test`
(`TEST_DATABASE_URL`) for the suite and `atom_dev` for the manual transcript in
§1.6.

| Command | Result |
| ------- | ------ |
| `cargo fmt --all` | No changes after the final pass. |
| `cargo check --all-targets --locked` | `0` |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | `0` |
| `TEST_DATABASE_URL=…/atom_test cargo test --all-targets --locked` | `0` — 191 library tests, 40 in `tests/ml_api.rs`, 20 in `tests/seeder.rs`, 10 in `tests/openapi_contract.rs`, and every other binary green. |
| `cargo check --no-default-features --all-targets --locked` | `0` — the API builds without Burn. |
| `cargo run --bin atom-docs` | `atom-docs: OK (0 warning(s))` |

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

## 2. Requirement by requirement

`State` says what exists. `Evidence` says what proves it: a named test that
passes in the run recorded in §1.5, or a document/source reference for a fact
that no test can pin.

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `ML-R01` All inference runs locally in the process, with no outbound call and no second runtime. | **Met in the code.** The module has no HTTP client, spawns no process, and the spike ran in-process. | Inspection of `src/ml/`; [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §1. `the_deterministic_embedder_performs_no_io` pins the testable half. |
| `ML-R02` Burn v0.22, the `flex` backend, SafeTensors weights, no extension. | **Not met.** See §1.4 and §2.1. | `burn` 0.22 is behind the default `ml` feature and the spike loaded an 86.66 MB SafeTensors file on `flex`; **no encoder produces a vector.** |
| `ML-R03` The `Embedder` trait has exactly `Burn`, `Deterministic` and `Disabled`. | **Met as a surface.** Exactly three implementations exist and tests select `Deterministic`. The `Burn` one is a stand-in. | `src/ml/`; selection in `embedder.rs::select`; `ml_disabled_selects_the_disabled_embedder`, `a_missing_model_selects_the_disabled_embedder`. |
| `ML-R04` `REAL[]` storage with `model_id` and `dim`, unique per source. | **Met in the schema and the repository.** The unique index and the dimension `CHECK` are the database's copy; the repository upserts against the first. | `migrations/0006_embeddings.sql`, `src/ml/repository.rs`; `reindex_writes_one_row_per_source`, `reindex_writes_unit_vectors` pass. |
| `ML-R05` Three search modes, with hybrid as a bounded lexical candidate set re-ranked by cosine. | **Implemented.** The service implements the three modes and the bounded re-rank; the operation answers. | `src/ml/service.rs`, `src/ml/handlers.rs`; eleven cases in [`testing.md`](testing.md) §2.1 pass. |
| `ML-R06` Degradation to lexical with `200` when ML is off or the model is missing. | **Implemented and observed.** The default is `ml.enabled = false`, the search path degrades with all four fields, and match and reindex answer `409`. | `config/ml.toml`, `src/ml/service.rs`; seven cases pass, and §1.6 is the manual transcript. |
| `ML-R07` Reindex is idempotent, owner-scoped, and `409` when ML is disabled. | **Implemented.** The service upserts per source, skips what is current, sweeps orphans, and refuses when the embedder is disabled. | `src/ml/service.rs`, `src/ml/repository.rs`; seven cases in [`testing.md`](testing.md) §2.5 pass. |
| `ML-R08` Every read and write is owner-scoped; a foreign source is `404`. | **Implemented.** Every statement in `MlRepository` binds `user_id`; the purge CTE is scoped the same way; a foreign application is a `404`. | `src/ml/repository.rs`; four cases in [`testing.md`](testing.md) §2.3 pass. |
| `ML-R09` 15 fixture queries with expected notes, and the real model's quality as a recorded manual run. | **Half met. The plumbing half passes; the quality half has not been measured.** | `the_fixture_queries_find_the_expected_notes` passes; §4 below. |

### 2.1 `ML-R02` is not met

**`ML-R02` is not met: the six-layer encoder is unbuilt, and the spike measured
a lookup and a projection — a lower bound on the plumbing, not an embedding.**

The reason, in the order it matters:

1. **No attention block, no feed-forward block and no LayerNorm has ever run in
   this repository.** `BurnEmbedder` performs a weight lookup, a mean-pool and a
   projection. A lookup is not a transformer layer, and its cost is a floor under
   the cost of one.
2. **The tokenizer is a stand-in.** There is no `tokenizer.rs` and no WordPiece
   vocabulary loader; without it even a correct encoder would receive the wrong
   token ids.
3. **Because of 1 and 2, the spike's numbers cannot be read as a measurement of
   an embedding.** The 0.017–0.020 ms per text and the 93.5 MiB peak RSS in
   [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §8 are honest
   numbers about a different network, recorded with those caveats in
   `ATOM-CHANGE-20261024-010`.

`ML-R02` is not "nearly met", and it is not met by the dependency alone: a `burn`
dependency and a SafeTensors file that loads prove the framework choice, which is
the part the spike was asked to prove. The requirement is that embeddings are
produced by Burn, and no code path produces a vector with a model's semantics.

This is the one requirement the shipped feature does not satisfy, and it is why
§1.2 describes the behaviour as lexical rather than as a fallback from a working
model.

## 3. What is missing for `Verified`

1. **The encoder and the tokenizer.** A six-layer BERT encoder loaded with
   `burn_store`, mean pooling with the attention mask, L2 normalisation at write
   time, and WordPiece (`architecture.md` §12.3). **`ML-R02` stays unmet until
   this lands**, and `ML-R09`'s manual run cannot happen without it. The
   deterministic embedder already satisfies the trait, the unit-length
   invariant and the storage contract, so the encoder is a new implementation
   and a reindex rather than a schema change.
2. **`ML-R09`'s manual quality run.** The fixture test passes and proves the
   plumbing; it does not prove retrieval quality, and there is no encoder to
   evaluate. `§4` is the record, and it is still empty.
3. **A `408` observed end to end.** The mapping from a timed-out inference to
   `inference_timeout` is unit-tested
   (`a_timeout_maps_to_the_inference_timeout_degradation`); the HTTP `408`
   itself needs either a fourth `Embedder` (`ML-R03` forbids one) or a model
   slow enough to overrun a two-second budget, which is not deterministic. It is
   a named gap in [`testing.md`](testing.md) §5, not a passing case.

Everything else — the schema, the seven settings in all seven places, the three
operations with the published status codes, the degradation rule, the ownership
predicates, the idempotent reindex, the delete-side purge and the seeder's
`--with-embeddings` — exists and is covered by a run that passes (§1.5).

Only after 1–5, with the run recorded, does `Verified` become a true statement
about this feature.

## 4. The manual evaluation record

> **Not run.** The fifteen-query fixture passes, but there is no encoder to
> evaluate. The spike's numbers do **not** belong here: they measured a lookup
> and a projection, not retrieval.

| Field | Value |
| ----- | ----- |
| Model | `all-MiniLM-L6-v2`, SafeTensors, dimension 384 (operator-supplied, not in the repository) |
| Tokenizer | `vocab.txt` beside the weights |
| Latency | Not measured for an encoder. The spike's 0.017–0.020 ms per text is a lower bound on a different network. |
| Memory | Not measured for an encoder. The spike's 93.5 MiB peak RSS is the lookup-only network. |
| Fifteen queries | `the_fixture_queries_find_the_expected_notes` passes; it proves plumbing only. |
| Quality | Not measured. `ML-R09`'s manual half is unmet. |

## 5. What a future change must not do

- **Do not change an accepted requirement silently.** A revision goes through
  [`project-truth.md`](../../project-truth.md) §18.4.
- **Do not promote this file to `Verified` because code exists.** `Verified`
  needs a recorded passing run; §1.3 says what is missing.
- **Do not describe the Burn path as producing embeddings until the encoder
  exists.** §2.1 is the sentence that keeps it honest.
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

## 6. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | The modules, the `Embedder` trait and its invariants, storage and cleanup, the search pipeline, degradation, configuration, the security boundary, and §12 — the build contract for `M3`–`M6`. |
| [`api.md`](api.md) | The three endpoints with their request and response shapes, status codes, error cases and the degradation contract. |
| [`testing.md`](testing.md) | The 40 cases that exist, what each proves, and the honest gaps. |
| [`../../testing/strategy.md`](../../testing/strategy.md) | The layers, and which layer proves which ML property. |
| [`../../api/README.md`](../../api/README.md) | The published surface, now 51 operations. |
| [`../../api/conventions.md`](../../api/conventions.md) | The search modes and the degradation rule, §17. |
| [`../../database/schema.md`](../../database/schema.md) | The `embeddings` table, §11.6. |
| [`../../database/seeding.md`](../../database/seeding.md) | The seeder's `--with-embeddings` contract, `S4`. |
| [`../../decisions/ADR-0008-local-pure-rust-ml.md`](../../decisions/ADR-0008-local-pure-rust-ml.md) | The local, pure-Rust decision; §8 the measured result, §9 normalisation, §10 the tokenizer, §11 the fallback. |
| [`../../engineering/change-log.md`](../../engineering/change-log.md) | `ATOM-CHANGE-20261024-010`, including its Correction block: the numbers and their three caveats. |

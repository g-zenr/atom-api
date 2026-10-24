# Local machine learning — testing

> **Status:** Implemented
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

What the suite contains, what each case proves, and what is **not** covered. The
rule for what must pass before a change is complete is in
[`verification-policy.md`](../../testing/verification-policy.md); the layer map
is in [`../../testing/strategy.md`](../../testing/strategy.md) §7.

## 0. What is on disk today

Read from the working tree on 2026-10-24. The committed state is `a4d33a3`.

| Layer | File | On disk | ML cases |
| ----- | ---- | ------- | -------- |
| Integration | `tests/ml_api.rs` | **Yes** — 40 cases, `#![cfg(feature = "ml")]` | 40 |
| Test harness | `tests/common/mod.rs` | **Yes** — `test_state_with_deterministic_ml`, `test_state_with_ml_disabled`, `test_state_with_a_missing_model`, `test_state_with_candidate_limit` | support only |
| Unit, the module | `src/ml/*.rs` | The files exist; **no test does** — no `#[test]`, no `#[cfg(test)]` | 0 |
| Unit, the configuration | `src/config/tests.rs` | **Seven cases** exist: the defaults, the environment override, the backend vocabulary, the candidate-limit floor, the dimension, the timeout ceiling and the candidate-limit relation to the page maximum. **None has run.** | 7 |
| Seeder | `tests/seeder.rs` | **One case**: `the_seeder_writes_embeddings_only_with_the_flag`. The seeder binary has **no `--with-embeddings` flag**, so the case cannot pass. | 1 |
| Contract | `tests/openapi_contract.rs` | Pins the surface at **51**; the route table and the document now describe the same 51. | 0 ML cases |
| Manual | this document, §5 | **Not run** | — |

**No case has been run, and none has passed.** The suite is written; the run is
missing. `tests/ml_api.rs` was written before the handlers, which is the rule
([`../../testing/strategy.md`](../../testing/strategy.md) §7.4); the three
operations are now mounted, so the endpoint cases can execute — they simply have
not. The seeder case cannot pass until the flag exists, and the module's unit
cases (§3.2) have not been written. Nothing in this feature is `Verified`.

## 1. The rule that makes a test evidence

A test that cannot fail for the reason it names is not evidence. In this feature
the risk is unusually high, because a case run against the deterministic
embedder passes whether or not an embedding is meaningful. Each case below must
be shown to fail when its rule is removed — the degradation branch deleted, the
`user_id` predicate dropped, the candidate `LIMIT` raised — and pass again when
restored.

**No test loads a model file and none needs the network.** Every scored case runs
under `ml.backend = "deterministic"`, the implementation `ADR-0008` §7.1 reserves
for the suite. The two degradation triggers are produced by the two
configuration states that need no artifact at all: `ml.enabled = false`, and
`ml.enabled = true` with `flex` and no weights. A test that needs the real model
belongs to the recorded manual evaluation in §5, not to CI.

## 2. `tests/ml_api.rs` — the 40 cases that exist

Embedding rows are planted through `plant_vector`, a fixture that writes a unit
vector straight into `embeddings`, so a case can state the score it expects
without depending on the Burn path. Every planted vector is asserted unit length
first, because `ADR-0008` §9 forbids the product from storing anything else.

### 2.1 Search modes — `ML-R05`

| Case | Rule it protects |
| ---- | ---------------- |
| `lexical_mode_matches_the_notes_search` | `POST /notes/search` with `lexical` returns the same ids, in the same order, and the same pagination as `GET /notes?search=`. A second text search would let the two disagree. |
| `lexical_mode_returns_a_null_score` | Every lexical hit has `score` `null`, and the answer is not degraded. |
| `hybrid_returns_a_score_for_every_hit` | Two embedded candidates produce two hits, each with a numeric score in `[0,1]`. |
| `hybrid_orders_by_score_then_note_id` | One note outranks two notes planted with the same vector; the tie is ordered by `note_id` ascending, so the order is total. |
| `hybrid_never_returns_more_than_the_candidate_limit` | Six matching notes with `ml.candidate_limit` at 3 yield at most three items and a `total` of exactly 3. A three-note corpus could not prove the bound. |
| `a_note_without_an_embedding_is_not_returned_by_hybrid` | A lexical hit with no vector is absent from hybrid and its absence is not a zero score; the `total` counts the scored candidates. |
| `semantic_search_returns_the_callers_notes` | A query matching neither note lexically still returns both embedded notes and no unembedded one. |
| `the_search_pagination_bounds_are_applied` | The configured default, an in-range limit, a clamp above the maximum, a non-positive limit falling back, and an offset past the end returning an empty page. |
| `an_empty_query_is_refused` | `""` and a whitespace-only query are `422`, and the detail names `query`. |
| `a_query_over_the_bound_is_refused` | 200 characters are accepted, 201 are `422`; the bound counts characters, not bytes, so 200 two-byte characters pass. |
| `an_unknown_search_mode_is_refused` | `fuzzy`, `SEMANTIC` and `lexical-ish` are `422` error envelopes, never a silent fallback to lexical. |

### 2.2 Degradation — `ML-R06`

| Case | Rule it protects |
| ---- | ---------------- |
| `search_degrades_to_lexical_when_ml_is_disabled` | With `ml.enabled = false`, `hybrid` and `semantic` each answer `200` with `requested_mode` preserved, `mode` `lexical`, `degraded` `true` and `degraded_reason` `ml_disabled`, carrying the lexical hits and their total. |
| `search_degrades_to_lexical_when_the_model_is_missing` | With ML enabled, `flex` and no weights, the same `200` with `degraded_reason` `model_missing`; the lexical hit survives. |
| `a_degraded_response_is_a_complete_response` | The degraded page equals the first page of the lexical answer: same pagination, same order, full `NoteResponse` per hit and a `null` score. |
| `a_degraded_request_never_returns_500_or_503` | Both triggers, three bodies including an omitted `mode`, are asserted not to be `500`/`503` and to be `200`. |
| `lexical_mode_is_not_degraded` | `lexical` asked for and applied reports `degraded` `false`, even when the model is missing. |
| `matching_returns_conflict_when_ml_is_disabled` | A match is `409 conflict`, and the same application answers `200` once a model is selected, so the `409` is the model's absence and not the request. |
| `reindex_is_refused_when_ml_is_disabled` | A reindex is `409 conflict`, writes no row, and answers `200` for an account whose model is selected. |

The third trigger, `inference_timeout`, is **not** forced end to end. It would
need a fourth `Embedder` — which `ML-R03` forbids — or a real model slow enough
to exceed the budget, which is not deterministic. The mapping is unit-tested
instead (§3), and the absence of an end-to-end case is in §6.

### 2.3 Ownership — `ML-R08`

| Case | Rule it protects |
| ---- | ---------------- |
| `search_never_returns_another_users_note` | A note and its vector belong to one account; the owner sees it and a second account gets an empty page with `total` 0 in all three modes — the row exists, and the query must still not see it. The owner assertion keeps the case from passing vacuously. |
| `reindex_is_owner_scoped` | A second account's reindex writes, skips and deletes nothing, and does not touch the owner's rows. |
| `matching_a_foreign_application_is_not_found` | `404` on another account's application and on a well-formed unknown id; the owner's own application answers `200`, so the `404`s are the ownership rule. |
| `the_ml_routes_require_a_credential` | All three operations answer `401` without a token and with an unusable one. |

### 2.4 Match — `ML-R05`, `ML-R06`

| Case | Rule it protects |
| ---- | ---------------- |
| `a_match_ranks_the_callers_notes` | The response names the application and the mode, ranks exactly the three embedded notes by descending score in `[0,1]`, and omits the unembedded one. |
| `a_match_returns_an_empty_list_when_nothing_is_embedded` | ML ready with no vectors: `200` and `matches` `[]`, not `404`. |
| `a_match_limit_out_of_range_is_refused` | `0`, `-1` and `51` are `422` naming `limit`; `1` and `50` are accepted. |
| `lexical_mode_is_refused_by_match` | `lexical` and `fuzzy` are `422`: a match has no lexical form. |
| `a_match_request_with_no_body_is_refused` | An absent body is `400 bad_request` — the extractor's mapping, not a `422` and not a default body. |

### 2.5 Reindex — `ML-R07`, with the storage contract of `ML-R04`

| Case | Rule it protects |
| ---- | ---------------- |
| `reindexing_twice_leaves_the_same_rows` | The first run writes three rows; the second reports `indexed` 0, `skipped` 3, `deleted` 0, and the rows are byte-identical. |
| `reindex_writes_one_row_per_source` | Two notes and one application produce exactly three rows, one per source, each recording `model_id` and `dim`. |
| `reindex_deletes_an_orphan` | A planted row whose source does not exist is deleted (`deleted` 1) while the real source is skipped. |
| `reindex_reports_its_counts` | `indexed`, `skipped` and `deleted` are JSON numbers and describe the run; `model_id` and `dimension` are reported. |
| `reindex_writes_unit_vectors` | Every stored vector has norm `1.0 ± 1e-5` and its declared `dim`. The tolerance is necessary (`REAL[]` rounds each component) and is the one the contract states; a looser one would stop detecting an unnormalised writer. |
| `deleting_a_note_removes_its_embeddings` | After `DELETE /notes/{id}`, the row is gone and a following reindex has no orphan to sweep — the delete is in the source's own statement, not a later sweep. |
| `deleting_an_application_removes_its_embeddings` | The same for an application. |

### 2.6 The published contract

| Case | Rule it protects |
| ---- | ---------------- |
| `the_ml_routes_are_in_the_openapi_document` | The three `(method, path)` pairs are operations with a `200`, the seven schemas are published, and only `POST` is declared on the search path. |
| `post_notes_search_is_not_shadowed_by_the_note_read` | `GET /notes/search` is the note read and answers `400` because `search` is not a UUID; `POST /notes/search` without a credential answers `401`, which proves the operation is mounted rather than a `404` fallback. |
| `the_search_endpoint_publishes_pagination_once` | The search publishes exactly `limit` and `offset` on the query string, once each, and its body schema holds only `query` and `mode`. |
| `the_score_is_a_number` | A score is a JSON number and never a string; lexical mode publishes `null`; `SearchHit.score` is a number in the document. |
| `a_hybrid_score_is_the_cosine_of_the_planted_vectors` | Two vectors planted at known cosines (`0.8`, `0.6`) are published as `0.8` and `0.6` to four decimals. A re-rank using anything but the dot product fails. |

### 2.7 The evaluation fixture — `ML-R09`, plumbing half

| Case | Rule it protects |
| ---- | ---------------- |
| `the_fixture_queries_find_the_expected_notes` | Fifteen query/title pairs, each note planted with its own query vector; every query returns its note in the top five with a score above `0.999`. |

**This case does not prove retrieval quality and must not be described as if it
did.** The deterministic embedder hashes its input, so a note is "about" a query
only because the fixture planted the query's own vector. What the fifteen cases
establish is the pipeline: the query is embedded, the stored vectors are
compared, the sort runs, the expected note is on the page. The quality question
is §5, and it is unmeasured.

## 3. Unit tests

**Seven configuration cases are on disk and none has run.** `src/ml/` has **no
`#[test]` at all**, so every case in the second table is still to be written —
including the one for the start-up selection rule, which is the only place the
`Disabled`/`Deterministic`/`flex` decision is made.

### 3.1 On disk, not run — `src/config/tests.rs`

| Case | Rule it protects |
| ---- | ---------------- |
| `the_ml_settings_have_their_documented_defaults` | The seven defaults match the reference table; `ml.enabled` is `false` and the compiled `Default` agrees with the file. |
| `the_ml_settings_can_be_set_from_the_environment` | Each `APP_ML_*` variable overrides its file value. |
| `the_ml_backend_is_one_of_the_supported_values` | `flex` and `deterministic` are accepted; anything else is refused. |
| `a_non_positive_candidate_limit_is_refused` | A candidate limit of zero or less is refused. |
| `a_zero_ml_dimension_is_refused` | A dimension of zero is refused. |
| `an_ml_timeout_above_the_request_timeout_is_refused` | The inference budget may not exceed `app.limits.request_timeout_seconds`. |
| `the_candidate_limit_must_be_at_least_the_max_page_size` | A candidate limit below `api.pagination.max_limit` is refused, because a candidate set smaller than one page makes page two impossible. |

### 3.2 Still to be written — `src/ml/`

| File | Case | Rule it protects |
| ---- | ---- | ---------------- |
| `src/ml/deterministic.rs` | `the_deterministic_embedder_is_stable` | The same input produces the same vector across calls and instances. |
| `src/ml/deterministic.rs` | `the_deterministic_embedder_has_the_configured_dimension` | The returned length equals `dimension()`, and equals `ml.dimension`. |
| `src/ml/deterministic.rs` | `the_deterministic_embedder_returns_a_unit_vector` | `(1.0 - l2_norm(&v)).abs() <= 1e-5`, the invariant of [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §9. |
| `src/ml/embedder.rs` | `a_missing_model_selects_the_disabled_embedder` | The selection function returns `Disabled` with `model_missing`, not a panic and not a start-up failure. |
| `src/ml/embedder.rs` | `ml_disabled_selects_the_disabled_embedder` | `ml.enabled = false` yields `Disabled` whatever `ml.backend` says. The seeder does not go through this path; it constructs `Deterministic` directly. |
| `src/ml/embedder.rs` | `the_disabled_embedder_reports_its_reason` | `status()` carries the reason and `embed()` returns an error. |
| `src/ml/embedder.rs` | `an_unknown_backend_is_refused_at_start_up` | `ml.backend = "cuda"` is a `ConfigError`, not a silent fallback. |
| `src/ml/service.rs` | `cosine_similarity_is_symmetric_and_bounded` | Identical vectors score 1, orthogonal 0, opposite −1, and the published value is clamped to `[0,1]`. |
| `src/ml/service.rs` | `a_vector_of_the_wrong_dimension_is_skipped` | A row whose length differs from `dim` does not panic and does not contribute a score. |
| `src/ml/service.rs` | `a_timeout_maps_to_the_inference_timeout_degradation` | The mapping from a timed-out inference to `degraded_reason` `inference_timeout` is pure: the error variant in, the reason out, no clock. |

## 4. The seeder

| Case | State | Rule it protects |
| ---- | ----- | ---------------- |
| `the_seeder_writes_embeddings_only_with_the_flag` | **On disk in `tests/seeder.rs`. Cannot pass:** `src/bin/atom-seed.rs` has no `--with-embeddings` flag. | Without the flag no `embeddings` row is written; with it every seeded note and application gets one unit vector from `Deterministic`, never a network call and never the Burn path. |
| `a_reset_run_deletes_the_embeddings_too` | **Not on disk.** | The `embeddings` row of the seeder's delete order starts executing, so `--reset` leaves no orphan. |

The seeder constructs `Deterministic` directly rather than reading
`ml.backend`, which is what lets a seed run need no model file. The contract is
`S4` in [`../../database/seeding.md`](../../database/seeding.md).

## 5. The manual evaluation — a recorded run, not a CI gate

`ML-R09` asks for two different things, and conflating them would be dishonest.

| Layer | What it proves | Where it runs |
| ----- | -------------- | ------------- |
| `the_fixture_queries_find_the_expected_notes` | The plumbing, as §2.7 describes. | CI, deterministic embedder |
| The manual quality run | Whether a real model retrieves paraphrases, and its latency and memory on the reference machine. | Once, by the owner or the implementing agent, recorded in [status.md](status.md) |

**The manual run has not happened.** There is no encoder, so there is nothing to
evaluate. [status.md](status.md) §4 is where the result goes, and the spike's
numbers do not belong there: they measured a lookup and a projection.

## 6. Deliberately not covered, and the honest gaps

| Gap | Why |
| --- | --- |
| **Any passing run.** | No case in this feature has been executed. The operations are mounted, so the endpoint cases can run; they have not. |
| **The Burn model's output, in CI.** | The weights are not in the repository, and downloading them in CI would break the no-outbound rule. `Deterministic` covers the plumbing. **No run of the Burn path has ever produced a vector.** |
| **Retrieval quality of a real model.** | It is the manual run in §5, recorded rather than asserted. A quality threshold would be a product decision nobody has made. **It has not been measured.** |
| **An inference timeout through the API.** | It needs a fourth `Embedder` (forbidden by `ML-R03`) or a real model slow enough to exceed the budget. The mapping is unit-tested; the end-to-end `408` on a match stays a gap. |
| **Concurrency.** | Nothing exercises two searches, two reindexes, or a reindex concurrent with a note delete for the same account. For ML this matters more than elsewhere: reindex writes while search reads. |
| **A live `database_error`.** | Fault injection; the mapping is unit-tested in `src/`. |
| **A cancellation of a blocking inference.** | Tokio cannot cancel `spawn_blocking`; a case would assert a property of the runtime rather than of ATOM. |
| **Two models coexisting across a reindex.** | Specified in [`architecture.md`](architecture.md) §11; a case needs a second deterministic model id. |
| **The seeder's reset against embeddings.** | The flag does not exist, so the delete order cannot be exercised. |

The first four rows are the ones a reader should weigh before trusting this
feature.

## 7. The error codes

| Code | Status | Case on disk |
| ---- | ------ | ------------ |
| `bad_request` | `400` | `a_match_request_with_no_body_is_refused` |
| `unauthorized` | `401` | `the_ml_routes_require_a_credential` |
| `not_found` | `404` | `matching_a_foreign_application_is_not_found` |
| `conflict` | `409` | `matching_returns_conflict_when_ml_is_disabled`, `reindex_is_refused_when_ml_is_disabled` |
| `validation_failed` | `422` | `an_unknown_search_mode_is_refused`, `a_match_limit_out_of_range_is_refused`, `an_empty_query_is_refused` |
| `timeout` | `408` | none — see §6 |
| `database_error` | `500` | not provable through the API without fault injection |

## 8. Conventions for a new test

- A test never loads a model file and never needs the network.
- A test that writes an embedding uses `plant_vector`, which sets `user_id`
  explicitly, so a missing predicate is visible.
- A degradation test asserts all four fields — `requested_mode`, `mode`,
  `degraded`, `degraded_reason` — because any one of them alone can pass by
  accident.
- A score assertion uses a tolerance, and an order assertion uses the
  tie-break, not the tolerance.
- A reindex test asserts the row count both before and after.
- A new route is added to `ROUTE_TABLE` and to `CONTRACT_OPERATIONS` in the same
  change.
- A case that cannot run yet is **not** committed `#[ignore]`d: an ignored test
  reads like evidence and is not.

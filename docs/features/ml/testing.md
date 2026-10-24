# Local machine learning — testing

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

What the test suite must contain for this feature. **None of these tests
exists yet**: the feature has no code, so every name below is a test to be
written with the implementation chain.

This document describes the planned suite. It does not claim that any run
happened; the rule for what must pass is in
[`verification-policy.md`](../../testing/verification-policy.md), and the
result of a run belongs in the verification record and in [status.md](status.md).

**The rule that makes this list evidence:** a test that cannot fail for the
reason it names is not evidence. In this feature the risk is unusually high,
because a test with the deterministic embedder passes whether or not the
embedding is meaningful. Each test must be shown to fail when the rule is
removed — the degradation branch deleted, the `user_id` predicate dropped, the
candidate `LIMIT` raised — and pass again when restored.

**No test loads a model file.** Every test selects the `Deterministic`
embedder. A test that needs the real model belongs to the recorded manual
evaluation in §4, not to CI.

## 1. Integration tests — `tests/ml_api.rs`

The tests use the shared harness: the real router, a real PostgreSQL database,
a unique user per test. Embedding rows are inserted through a small helper
that writes them directly, so a test can control the vectors without depending
on the Burn model.

### 1.1 Search modes

| Test | What it proves |
| ---- | -------------- |
| `lexical_mode_matches_the_notes_search` | `POST /notes/search {"mode":"lexical"}` returns the same note ids, in the same order, as `GET /api/v1/notes?search=…`. |
| `lexical_mode_returns_a_null_score` | Every hit has `score` `null` and `degraded` `false`. |
| `hybrid_returns_a_score_for_every_hit` | With embeddings written for three notes, every hit has a numeric `score` in `[0,1]`. |
| `hybrid_orders_by_score_then_note_id` | Two notes with the same vector come back ordered by `note_id` ascending, so the order is total. |
| `hybrid_never_returns_more_than_the_candidate_limit` | A corpus larger than `ml.candidate_limit` yields at most that many items, and `pagination.total` equals the item count when the limit is not reached. |
| `a_note_without_an_embedding_is_not_returned_by_hybrid` | The documented limitation: a lexical hit with no embedding does not appear in hybrid mode, and does appear in lexical mode. |
| `semantic_search_returns_the_callers_notes` | A query ranked against planted vectors returns them in cosine order. |
| `the_search_pagination_bounds_are_applied` | `?limit=1&offset=1` returns the second scored hit; a limit above the configured maximum is clamped; an offset past the end is an empty page, not `404`. |
| `an_empty_query_is_refused` | `""` and a whitespace-only query are `422` naming `query`. |
| `a_query_over_the_bound_is_refused` | 201 characters is `422`. |
| `an_unknown_search_mode_is_refused` | `{"mode":"fuzzy"}` is `422`, not a silent fallback to lexical. |

### 1.2 Degradation

| Test | What it proves |
| ---- | -------------- |
| `search_degrades_to_lexical_when_ml_is_disabled` | With `ml.enabled = false`, a `semantic` request returns `200`, `mode` `lexical`, `degraded` `true`, `degraded_reason` `ml_disabled`, and the lexical notes. |
| `search_degrades_to_lexical_when_the_model_is_missing` | With ML enabled and a path that does not exist, the same `200`, with `degraded_reason` `model_missing`. |
| `a_degraded_response_is_a_complete_response` | The degraded answer carries the same `items` shape, a non-null `pagination` and the standard envelope — not an error object. |
| `a_degraded_request_never_returns_500_or_503` | Asserted explicitly for both triggers: a missing optional model is a configuration state. |
| `lexical_mode_is_not_degraded` | `mode` and `requested_mode` are both `lexical` and `degraded` is `false`, even when the model is missing. |

The third degradation trigger, `inference_timeout`, is **not** forced end to
end. Doing so would need either a fourth `Embedder` implementation — which
`ML-R03` forbids — or a real model slow enough to exceed the budget, which is
not deterministic. It is covered at the unit level instead
(`a_timeout_maps_to_the_inference_timeout_degradation`), and the absence of an
end-to-end case is recorded in §5 rather than hidden.

### 1.3 Ownership

| Test | What it proves |
| ---- | -------------- |
| `search_never_returns_another_users_note` | A note and an embedding row are planted for another account; a semantic and a hybrid search by the caller return neither. This is the security case: the row exists, and the query must still not see it. |
| `reindex_is_owner_scoped` | A reindex by the caller does not add, change or delete a row owned by another account. |
| `matching_a_foreign_application_is_not_found` | `404` on another account's application. |
| `the_ml_routes_require_a_credential` | All three operations, without a token and with an invalid one, return `401`. |

### 1.4 Match

| Test | What it proves |
| ---- | -------------- |
| `a_match_ranks_the_callers_notes` | With three planted note vectors and one application, the response lists them by score and respects `limit`. |
| `a_match_returns_an_empty_list_when_nothing_is_embedded` | ML ready, no embeddings: `200` with `matches` `[]`, not `404`. |
| `matching_returns_conflict_when_ml_is_disabled` | `409 conflict` (`ML-R06`), and no ranking is fabricated. |
| `a_match_limit_out_of_range_is_refused` | `0`, `-1` and `51` are `422` naming `limit`. |
| `lexical_mode_is_refused_by_match` | `{"mode":"lexical"}` is `422`: a match has no lexical form. |
| `a_match_request_with_no_body_is_refused` | An empty body is `400`; `{}` is accepted and applies the defaults. |

### 1.5 Reindex

| Test | What it proves |
| ---- | -------------- |
| `reindexing_twice_leaves_the_same_rows` | The first run reports `indexed > 0`; the second reports `indexed` 0 and the same `skipped`; the row count is unchanged. |
| `reindex_is_refused_when_ml_is_disabled` | `409 conflict`. |
| `reindex_writes_one_row_per_source` | Two notes and one application produce three rows, and a second run does not produce six. |
| `reindex_deletes_an_orphan` | A planted row whose `source_id` does not exist is deleted, and `deleted` is 1. |
| `reindex_reports_its_counts` | `indexed`, `skipped` and `deleted` sum to the work the run did, and the response reports the `model_id` and `dimension`. |
| `deleting_a_note_removes_its_embeddings` | After `DELETE /api/v1/notes/{id}`, a direct count of rows for that source is zero. |
| `deleting_an_application_removes_its_embeddings` | The same for an application. |

### 1.6 The contract

| Test | What it proves |
| ---- | -------------- |
| `the_ml_routes_are_in_the_openapi_document` | All three `(method, path)` pairs are operations in `/api/v1/openapi.json`, and no ML operation exists that the router does not serve. |
| `post_notes_search_is_not_shadowed_by_the_note_read` | `POST /api/v1/notes/search` reaches the search operation; `GET /api/v1/notes/search` is matched by `GET /notes/{note_id}` and answers `400` because `search` is not a UUID. The test pins the routing as it is, not as one might wish it were. |
| `the_search_endpoint_publishes_pagination_once` | `POST /notes/search` publishes `limit` and `offset` exactly once each, and its body schema contains neither. |
| `the_score_is_a_number` | The `score` field is `type: number` in the document, while every money field remains `type: string`. |

## 2. Unit tests — in the module and the configuration

| File | Test | What it proves |
| ---- | ---- | -------------- |
| `src/ml/deterministic.rs` | `the_deterministic_embedder_is_stable` | The same input produces the same vector across calls and instances. |
| `src/ml/deterministic.rs` | `the_deterministic_embedder_has_the_configured_dimension` | The returned length equals `dimension()`, and equals `ml.dimension`. |
| `src/ml/deterministic.rs` | `the_deterministic_embedder_performs_no_io` | It is constructed with no path and no network client; the test documents `ML-R01`'s testable half. |
| `src/ml/embedder.rs` | `a_missing_model_selects_the_disabled_embedder` | The selection function returns `Disabled` with `model_missing`, not a panic and not a start-up failure. |
| `src/ml/embedder.rs` | `ml_disabled_selects_the_disabled_embedder` | `ml.enabled = false` yields `Disabled` whatever `ml.backend` says; the seeder does not go through this path, because it constructs `Deterministic` directly. |
| `src/ml/embedder.rs` | `the_disabled_embedder_reports_its_reason` | `status()` carries the reason and `embed()` returns an error. |
| `src/ml/embedder.rs` | `an_unknown_backend_is_refused_at_start_up` | `ml.backend = "cuda"` is a `ConfigError`, not a silent fallback. |
| `src/ml/service.rs` | `cosine_similarity_is_symmetric_and_bounded` | Identical vectors score 1, orthogonal 0, opposite −1, and the result is clamped to `[0,1]` for the response. |
| `src/ml/service.rs` | `a_vector_of_the_wrong_dimension_is_skipped` | A row whose length differs from `dim` does not panic and does not contribute a score. |
| `src/ml/service.rs` | `a_timeout_maps_to_the_inference_timeout_degradation` | The mapping from a timed-out inference to `degraded_reason` `inference_timeout` is a pure function, and it is asserted without a clock: the error variant in, the reason out. |
| `src/config/tests.rs` | `the_ml_settings_have_their_documented_defaults` | The six defaults match the reference table. |
| `src/config/tests.rs` | `the_candidate_limit_must_be_at_least_the_max_page_size` | A candidate limit below `api.pagination.max_limit` is refused. |
| `src/config/tests.rs` | `the_ml_backend_is_one_of_the_supported_values` | `flex` and `deterministic` are accepted and anything else is refused. `deterministic` exists so a test deployment needs no model file and no network; the seeder constructs it directly rather than through this setting. |

## 3. The error codes

| Code | Status | Proving test |
| ---- | ------ | ------------ |
| `bad_request` | `400` | `a_match_request_with_no_body_is_refused`, and a malformed search body |
| `unauthorized` | `401` | `the_ml_routes_require_a_credential` |
| `not_found` | `404` | `matching_a_foreign_application_is_not_found` |
| `conflict` | `409` | `reindex_is_refused_when_ml_is_disabled`, `matching_returns_conflict_when_ml_is_disabled` |
| `validation_failed` | `422` | `an_unknown_search_mode_is_refused`, `a_match_limit_out_of_range_is_refused` |
| `timeout` | `408` | none — see §5 |
| `database_error` | `500` | not provable through the API without fault injection; the mapping is unit-tested in `src/error.rs` |
| `payload_too_large` | `413` | covered globally by `a_body_over_the_limit_is_refused` in `tests/auth_api.rs` |

## 4. The evaluation — a recorded run, not a CI gate

`ML-R09` asks for two different things, and conflating them would be dishonest.

| Layer | What it proves | Where it runs |
| ----- | -------------- | ------------- |
| `the_fixture_queries_find_the_expected_notes` | The plumbing: the fixture query is embedded, the stored vectors are compared, the sort works, the expected note is in the top results. Fifteen queries, each with an expected note. | CI, deterministic embedder |
| The manual quality run | Whether the chosen model actually retrieves paraphrases. Latency and memory on the reference machine. | Once, by the owner or the implementing agent, recorded in [status.md](status.md) |

The fixture test does **not** prove retrieval quality, and it must not be
described as if it did. The manual run is a note in [status.md](status.md), not
a green check in CI, because it depends on a model file that is not in the
repository.

## 5. Deliberately not covered

| Gap | Why |
| --- | --- |
| **The Burn model's output, in CI.** | The weights are not in the repository and downloading them in CI would break the no-outbound rule. `Deterministic` covers the plumbing. |
| **Retrieval quality of the real model.** | It is the manual run in §4, recorded rather than asserted. A quality threshold would be a product decision nobody has made. |
| **A live `database_error`.** | Fault injection; the mapping is already unit-tested. |
| **A cancellation of a blocking inference.** | Tokio cannot cancel `spawn_blocking`; the design accounts for it and a test would assert a timing property of the runtime rather than of ATOM. |
| **A live inference timeout through the API.** | It would need a fourth `Embedder` (forbidden by `ML-R03`) or a real model slow enough to exceed the budget, which is not deterministic. The mapping is unit-tested; the end-to-end `408` on a match is a known gap, named in §3. |
| **An exact float comparison of a score.** | Scores are compared with a tolerance, and the response's rounding to four decimals is asserted separately. |
| **Two models coexisting across a reindex.** | The behaviour is specified in [architecture.md](architecture.md) §11; a test needs a second deterministic model id, which the implementing change may add if it is cheap. |

## 6. Conventions for a new test

- A test never loads a model file and never needs the network.
- A test that writes an embedding directly uses the helper, and the helper
  always sets `user_id` explicitly, so a missing predicate is visible.
- A degradation test asserts all four fields — `requested_mode`, `mode`,
  `degraded`, `degraded_reason` — because any one of them alone can pass by
  accident.
- A score assertion uses a tolerance, and an order assertion uses the
  tie-break, not the tolerance.
- A reindex test asserts the row count both before and after.
- A new route is added to `ROUTE_TABLE` in the same change.

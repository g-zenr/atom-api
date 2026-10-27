# Local machine learning — testing

> **Status:** Implemented
> **Last updated:** 2026-10-26
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

What the suite contains, what each case proves, and what is **not** covered. The
rule for what must pass before a change is complete is in
[`verification-policy.md`](../../testing/verification-policy.md); the layer map
is in [`../../testing/strategy.md`](../../testing/strategy.md) §7.

## 0. What is on disk today

Read from the working tree on 2026-10-26. The feature is committed: the spike at
`bfd1288`, the finalised ADR at `a4d33a3`, the module, schema, three operations
and seeder flag at `ba198ff`, the close-out at `ba91008`, and the removal of the
two `#[allow(clippy::cast_possible_truncation)]` attributes in
`src/ml/burn_embedder.rs` at `505a189`. `grep "allow(" src/ tests/` returns no
hits.

| Layer | File | On disk | ML cases |
| ----- | ---- | ------- | -------- |
| Integration | `tests/ml_api.rs` | **Yes** — 41 cases, `#![cfg(feature = "ml")]` | 41 |
| Test harness | `tests/common/mod.rs` | **Yes** — `test_state_with_deterministic_ml`, `test_state_with_ml_disabled`, `test_state_with_a_missing_model`, `test_state_with_candidate_limit` | support only |
| Unit, the module | `src/ml/*.rs` | **Yes** — 64 cases: `burn_embedder.rs` 6, `deterministic.rs` 5, `dto.rs` 7, `embedder.rs` 11, `hash_vector.rs` 6, `service.rs` 14, `tokenizer.rs` 15 | 64 |
| Unit, the configuration | `src/config/tests.rs` | **Seven ML cases** in a file of 58: the defaults, the environment override, the backend vocabulary, the candidate-limit ceiling, the dimension, the timeout ceiling and the guarantee that the old page-maximum relation is gone. **All seven pass.** | 7 |
| Seeder | `tests/seeder.rs` | **21 cases**, including `the_seeder_writes_embeddings_only_with_the_flag`. `src/bin/atom-seed.rs` implements `--with-embeddings`. | 21, one ML-specific |
| Contract | `tests/openapi_contract.rs` | Pins the surface at **51**; the route table and the document now describe the same 51. | 0 ML cases |
| Manual | this document, §5 | **Recorded:** `atom-ml-bench`, 2026-10-25 (status.md §5); not a CI gate. | — |

**Every automated case passes.** The 41 `ml_api` cases, the 64 module cases, the
seven configuration cases and the seeder cases passed in the run of
`cargo test --all-targets --locked` on 2026-10-26 at `4df383e`: **511 passed, 0
failed** on Windows (513 on a unix host, where the two `shutdown` cases run).
What no automated case proves is a real model *in CI*: every executed case uses
`Deterministic` or the synthetic encoder and none loads a model file. The real
encoder is measured by the manual `atom-ml-bench` run recorded in §5, which is
not a CI gate. Nothing in this feature is `Verified`, because no reviewer has
read the encoder span and the re-gate of `ATOM-REVIEW-20261024-011` is recorded
separately in the engineering record.

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

## 2. `tests/ml_api.rs` — the 41 cases that exist

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
| `hybrid_never_returns_more_than_the_candidate_limit` | Six matching notes with `ml.candidate_limit` at 3 yield at most three items and a `total` of exactly 3, while the request asks for the configured maximum page of 100. The answer is a **short page**: three items, true `total`, `has_more: false`. A three-note corpus could not prove the bound, and a page bound of 3 could not prove the short page. |
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
| `post_notes_search_is_not_shadowed_by_the_note_read` | `GET /notes/search` is the note read and answers `405` because `search` is not a UUID; `POST /notes/search` without a credential answers `401`, which proves the operation is mounted rather than a `404` fallback. |
| `the_search_endpoint_publishes_pagination_once` | The search publishes exactly `limit` and `offset` on the query string, once each, and its body schema holds only `query` and `mode`. |
| `the_score_is_a_number` | A score is a JSON number and never a string; lexical mode publishes `null`; `SearchHit.score` is a number in the document. |
| `a_hybrid_score_is_the_cosine_of_the_planted_vectors` | Two vectors planted at known cosines (`0.8`, `0.6`) are published as `0.8` and `0.6` to four decimals. A re-rank using anything but the dot product fails. |

### 2.7 The evaluation fixture — `ML-R09`, plumbing half

`the_fixture_queries_find_the_expected_notes` **is** the fifteen-query fixture
evaluation `ML-R09` asks for; there is no second file and no second set. Each of
the fifteen notes is created through the published API and planted with the
deterministic vector of *its own* query, and each query is then sent to
`POST /notes/search` with `mode: "semantic"` and `limit=5`, against the same
account.

For every pair the case asserts six things, and all six pass:

| # | Assertion |
| - | --------- |
| 1 | The set holds **fifteen** pairs. A shorter table would quietly weaken the requirement. |
| 2 | `pagination.total` equals the number of planted notes that this query scores above zero. The case derives that number from its own planted vectors (`tests/ml_api.rs`); it does **not** assert `15`, because the score floor drops every row at or below zero. |
| 3 | The hit at **rank 1** is the note the fixture created for that query, compared by **id** — the identity the API returned, not a title that happens to look right. |
| 4 | That note's title is the expected one, which only makes a failure readable. |
| 5 | Its published score is above `0.999` (the contract's stated floor) **and** `1.0 ± 1e-4`: the planted vector *is* the query's own, so a pipeline that compared a different pair of vectors would fail even if it landed on the right note. |
| 6 | It **outranks every other hit on the page**, so rank 1 comes from the score and not from the `note_id` tie-break. |

The fifteen pairs, and the result of the recorded run:

| # | Query | Expected note | Observed | Published score | Result |
| - | ----- | ------------- | -------- | --------------- | ------ |
| 1 | `kubernetes rollout` | `Kubernetes rollout` | rank 1 | `1.0` | **pass** |
| 2 | `postgres index tuning` | `PostgreSQL index tuning` | rank 1 | `1.0` | **pass** |
| 3 | `rust ownership rules` | `Rust ownership rules` | rank 1 | `1.0` | **pass** |
| 4 | `interview preparation` | `Interview preparation` | rank 1 | `1.0` | **pass** |
| 5 | `expense report deadline` | `Expense report deadline` | rank 1 | `1.0` | **pass** |
| 6 | `canary release checklist` | `Canary release checklist` | rank 1 | `1.0` | **pass** |
| 7 | `on-call rotation` | `On-call rotation` | rank 1 | `1.0` | **pass** |
| 8 | `incident postmortem` | `Incident postmortem` | rank 1 | `1.0` | **pass** |
| 9 | `salary negotiation` | `Salary negotiation` | rank 1 | `1.0` | **pass** |
| 10 | `remote work setup` | `Remote work setup` | rank 1 | `1.0` | **pass** |
| 11 | `database migration plan` | `Database migration plan` | rank 1 | `1.0` | **pass** |
| 12 | `api rate limiting` | `API rate limiting` | rank 1 | `1.0` | **pass** |
| 13 | `observability dashboard` | `Observability dashboard` | rank 1 | `1.0` | **pass** |
| 14 | `technical debt backlog` | `Technical debt backlog` | rank 1 | `1.0` | **pass** |
| 15 | `promotion case` | `Promotion case` | rank 1 | `1.0` | **pass** |

**15 of 15 pass.** The runner-up scores in the recorded run were far below the
expected note — `0.0539` and `0.0233` behind a top score of `1.0` on the first
query — so assertion 6 is not measuring noise.

The **control**: the case ends with `"zeppelin airship"`, a query no note was
planted for and that no fixture note contains, and asserts that it returns **no
hits** with `total` 0 rather than a confident wrong match. The control runs in
`hybrid` mode because `semantic` has no lexical candidate set: it scores every
stored vector and returns every positive score, so it cannot express "the query
matched nothing" the way an empty lexical candidate set can. The account is not
empty at that point — the fifteen queries above scored perfectly against it — so
the empty answer is the pipeline declining to name a best match, not an empty
corpus.

One behaviour the control measurement exposed, and that a reader should know:
**hybrid applies a score floor.** `src/ml/service.rs` retains only hits with a
score above zero (`scored.retain(|hit| is_match(hit.score))`, where `is_match`
is `value > 0.0`), so a lexical candidate whose cosine is `0.0` or negative is
**not** returned. That is why the control needs a query with no lexical candidate
at all, and it is the honest limit of what this control proves: it shows that an
unmatched query yields nothing, not that every weak candidate is filtered.

**This case does not prove retrieval quality and must not be described as if it
did.** The deterministic embedder hashes its input, so a note is "about" a query
only because the fixture planted the query's own vector: pair 1 would score
exactly as well if the note were titled "Groceries". What the fifteen pairs
establish is the **pipeline** — the query is embedded, the stored vectors are
compared by cosine, the sort runs, the expected note comes first, the score is
the one the arithmetic implies, and an unmatched query yields nothing. They say
nothing about whether any model understands English, and the `1.0` in the table
above is a property of the fixture, not a quality signal.

The only quality evidence that exists is the encoder benchmark recorded in
[status.md](status.md) §5, *The encoder, measured*: **15 of 15 queries returned
the expected nearest text** through the real Burn encoder
(`cargo run --release --bin atom-ml-bench --features ml`, 2026-10-25), with the
top-1 cosine range and the negative control recorded in §6.
That is a recorded manual run, not a CI gate. Its pair count is the one figure
the record does not settle: the record also carries an earlier five-query probe,
and [status.md](status.md) §4 marks that count **unverified**. What `ML-R09`'s
quality half stands on is the recorded run, not an assertion.

The case was shown to fail for the reason it names: pointing pair 1's expected id
at another note fails at that pair with both ids and the whole returned page in
the message (exit `101`), and restoring the id passes. Pointing the control at
`"kubernetes"` — a query that *does* lexically match — fails the empty-page
assertion the same way.

## 3. Unit tests

**64 cases in `src/ml/`, seven for the configuration, are on disk and pass.**
The start-up selection rule — the only place the
`Disabled`/`Deterministic`/`flex` decision is made — is covered in
`embedder.rs`.

### 3.1 On disk and passing — `src/config/tests.rs`

| Case | Rule it protects |
| ---- | ---------------- |
| `the_ml_settings_have_their_documented_defaults` | The seven defaults match the reference table; `ml.enabled` is `false` and the compiled `Default` agrees with the file. |
| `the_ml_settings_can_be_set_from_the_environment` | Each `APP_ML_*` variable overrides its file value. |
| `the_ml_backend_is_one_of_the_supported_values` | `flex` and `deterministic` are accepted; anything else is refused. |
| `a_non_positive_candidate_limit_is_refused` | A candidate limit of zero or less is refused. |
| `a_zero_ml_dimension_is_refused` | A dimension of zero is refused. |
| `an_ml_timeout_above_the_request_timeout_is_refused` | The inference budget may not exceed `app.limits.request_timeout_seconds`. |
| `the_candidate_limit_is_bounded_above` | A candidate limit of 1,001 is refused naming `ml.candidate_limit`; 1,000 is accepted; **3 is accepted too**, because a candidate set below the page size is a work bound, not an error ([`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §12.5). |

### 3.2 On disk and passing — `src/ml/`

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

The encoder and the tokenizer contribute 21 of the 64: six in
`src/ml/burn_embedder.rs` (the identifier, the status and dimension, a unit
vector, determinism and input dependence, and two load refusals) and fifteen in
`src/ml/tokenizer.rs` (the vocabulary, the special tokens, casing and accents,
`##` continuations, unknown words, the 256-token truncation and the attention
mask). They run against the synthetic encoder and a fixture vocabulary, so no
case loads a model file.

## 4. The seeder

| Case | State | Rule it protects |
| ---- | ----- | ---------------- |
| `the_seeder_writes_embeddings_only_with_the_flag` | **On disk in `tests/seeder.rs` and passing.** | Without the flag no `embeddings` row is written; with it every seeded note and application gets one unit vector from `Deterministic`, never a network call and never the Burn path. |
| `a_reset_run_deletes_the_embeddings_too` | **Not on disk.** The `DELETE FROM embeddings` statement exists in the seeder's reset order, but no case asserts that purge specifically. | The `embeddings` row of the seeder's delete order starts executing, so `--reset` leaves no orphan. |

The seeder constructs `Deterministic` directly rather than reading
`ml.backend`, which is what lets a seed run need no model file. The contract is
`S4` in [`../../database/seeding.md`](../../database/seeding.md).

## 5. The manual evaluation — a recorded run, not a CI gate

`ML-R09` asks for two different things, and conflating them would be dishonest.

| Layer | What it proves | Where it runs |
| ----- | -------------- | ------------- |
| `the_fixture_queries_find_the_expected_notes` | The plumbing, as §2.7 describes. | CI, deterministic embedder |
| The manual quality run | Whether a real model retrieves paraphrases, and its latency and memory on the reference machine. | Once, by the owner or the implementing agent, recorded in [status.md](status.md) |

**The full manual run has happened; its pair count is unverified.** The encoder
and its WordPiece tokenizer exist (committed `97d7b4b`, measured in `a3e8c46`),
and [status.md](status.md) §5 records what `src/bin/atom-ml-bench.rs` measured —
latency, peak RSS, determinism and a retrieval evaluation. The current binary
defines fifteen pairs, and the record says fifteen in one place and five in
another; [status.md](status.md) §4 explains why the repository cannot settle
which count the transcript shows and marks the five-pair figure **unverified**.
Either way it is a recorded run, not a CI gate, and it asserts no threshold.
[status.md](status.md) §4 is the record; the spike's numbers do not belong there:
they measured a lookup and a projection.

## 6. Deliberately not covered, and the honest gaps

| Gap | Why |
| --- | --- |
| **A real-model run in the suite.** | No automated case loads a model: the weights are not in the repository and every executed case uses `Deterministic`. The encoder has been run once, by hand, in `src/bin/atom-ml-bench.rs`, and that run is recorded in [status.md](status.md) §5 — it is a measurement, not an assertion. |
| **The Burn model's output, in CI.** | The weights are not in the repository, and downloading them in CI would break the no-outbound rule. `Deterministic` covers the plumbing. **No automated case has ever embedded through the Burn path.** |
| **Retrieval quality of a real model.** | Recorded, not asserted: fifteen paraphrase queries against fifteen planted texts in `atom-ml-bench`, **15 of 15 at rank 1**, top-1 cosine `0.2664`-`0.6740`, with a negative control at `0.1256`. The data supports a threshold in `(0.1256, 0.2664)`; none was set in code, because that is a product decision. The `0.0` floor removes orthogonal vectors, not merely unrelated ones. The record also carries a five-query probe; its count is **unverified** ([status.md](status.md) §4). |
| **An inference timeout through the API.** | It needs a fourth `Embedder` (forbidden by `ML-R03`) or a real model slow enough to exceed the budget. The mapping is unit-tested; the end-to-end `408` on a match stays a gap. |
| **Concurrency.** | Nothing exercises two searches, two reindexes, or a reindex concurrent with a note delete for the same account. For ML this matters more than elsewhere: reindex writes while search reads. |
| **A live `database_error`.** | Fault injection; the mapping is unit-tested in `src/`. |
| **A cancellation of a blocking inference.** | Tokio cannot cancel `spawn_blocking`; a case would assert a property of the runtime rather than of ATOM. |
| **Two models coexisting across a reindex.** | Specified in [`architecture.md`](architecture.md) §11; a case needs a second deterministic model id. |
| **The seeder's reset against embeddings.** | `--reset` purges `embeddings` (`DELETE FROM embeddings WHERE user_id = ANY($1)`), but no case asserts that purge specifically; `a_reset_deletes_rows_owned_by_the_seeded_accounts` covers the owned rows generically. |

The first three rows and concurrency are the ones a reader should weigh before
trusting this feature.

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

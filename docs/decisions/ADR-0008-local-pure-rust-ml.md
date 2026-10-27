# ADR-0008 — Machine learning is local and pure Rust

> **Status:** Accepted
> **Date:** 2026-10-24
> **Amended:** 2026-10-26 (§11 correction, §12.1 pair-count note)
> **Owner:** architect

## Context

Phase 2 adds local machine learning over the owner's own data: note search by
meaning, and a ranking of the owner's notes against one job application. The
requirements are `ML-R01`…`ML-R09` in
[`../project-truth.md`](../project-truth.md) §18.3.

Four constraints decide the shape, and three of them are not negotiable.

1. **The technology stack is Rust only** (owner decision **O6** in
   [`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md)).
   There is no Python, Node or Go runtime in the API, and no external model
   server such as Ollama. `AGENTS.md` §4.12 additionally fixes the framework:
   **Burn v0.22**, the **`flex`** backend, weights stored with
   `store`/SafeTensors.
2. **No outbound call** to a hosted or paid service, at build time or at run
   time ([`../project-truth.md`](../project-truth.md) §6 and §8).
3. **The app is a single-owner personal workspace.** The corpus is one person's
   notes: hundreds to a few thousand rows, not millions.
4. **The deployment must be the same on the development machine and in CI.**
   This is the constraint that removed the obvious design.

**The pgvector finding.** The PostgreSQL server on the development machine has
the `vector` extension available (0.8.1), but the `atom` role is **not** a
superuser, and `CREATE EXTENSION vector` is denied locally
([`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md),
baseline). The plan records that CI's `atom` role *is* a superuser
([`../engineering/runs/chain/plan.md`](../engineering/runs/chain/plan.md) §6).
A migration containing `CREATE EXTENSION vector` would therefore **pass CI and
fail on the development machine** — the worst possible divergence, because the
failure would appear only where nobody expects it. No requirement needs a
vector index badly enough to accept that.

The requirement is also modest. `ML-R05` asks for three search modes and a
hybrid that re-ranks a bounded candidate set in Rust; it does not ask for an
approximate-nearest-neighbour index over millions of vectors.

## Decision

**ATOM computes embeddings locally, inside the API process, in pure Rust with
Burn on the `flex` CPU backend; stores each vector as a PostgreSQL `REAL[]`;
and implements hybrid search as a bounded lexical candidate set re-ranked by
cosine similarity in Rust. No PostgreSQL extension, no model server, no second
runtime and no network call are involved.**

### 1. Where inference runs

- Inside the `atom-api` process. There is no sidecar, no daemon and no
  subprocess.
- The model weights are a **local file** in SafeTensors format, read from the
  path in `ml.model_path`. They are not downloaded at build time or at run
  time. The operator supplies the file.
- The framework is **Burn v0.22** with the **`flex`** backend: pure Rust, CPU.
  `wgpu`, `cuda` and `tch` are not added. A future backend is a new decision
  with a measurement, not a feature flag.

### 2. The embedder interface

An `Embedder` trait, with exactly three implementations:

| Implementation | Used by | Behaviour |
| -------------- | ------- | --------- |
| `Burn` | A deployment that has a model file and enables ML | Loads the SafeTensors weights and computes a vector for a text. |
| `Deterministic` | **Every automated test** | A hash-based vector of the configured dimension. Stable for the same input, no file, no network, not semantically meaningful. |
| `Disabled` | The default, and any deployment without a model | Reports that it is unavailable. It never panics and never blocks a request. |

The trait is what makes the feature testable without a model file and
deployable without one. The selection is made once at start-up from
`ml.enabled` and from whether the file can be read; it is not re-evaluated per
request.

### 3. Storage

- One table, `embeddings`, with `user_id`, `source_type` (`note` or
  `application`), `source_id`, `model_id`, `dim`, and `vector REAL[]`.
- A unique constraint on `(user_id, source_type, source_id, model_id)`, so a
  re-embed is an upsert and a reindex is idempotent.
- `source_id` is polymorphic, so it carries **no foreign key**. Deleting a note
  or an application deletes its embeddings **in the same transaction** as the
  delete, and a reindex removes any orphan. That obligation is the cost of the
  polymorphic column, and it is recorded under *Negative* below.
- `dim` is stored per row, so a change of model is a new `model_id` with new
  rows rather than a rewrite of the old ones.

### 4. Search

Three modes on `POST /api/v1/notes/search`:

| Mode | What it does | Needs ML |
| ---- | ------------ | -------- |
| `lexical` | The existing PostgreSQL full-text match (`plainto_tsquery('simple', …)`) over the caller's notes. | No |
| `semantic` | Embeds the query, then computes cosine similarity against the caller's note embeddings. | Yes |
| `hybrid` | Runs the lexical match to produce a **bounded candidate set** (`ml.candidate_limit`), then re-ranks those candidates by cosine similarity in Rust. | Yes |

- The re-rank is in Rust, over the candidate rows only. Memory and time are
  bounded by `ml.candidate_limit`, not by the size of the corpus.
- `ml.timeout_seconds` bounds **one inference** — the query's, for a search.
  The re-rank performs none, and is bounded by `ml.candidate_limit` instead
  (§12.2 and §12.5).
- **Degradation is explicit.** When ML is disabled or the model file is
  missing, `semantic` and `hybrid` return **lexical results with `200`**, and
  the response names the requested mode, the applied mode and a degradation
  reason. A missing model is a configuration state, not a server error.

### 5. Configuration

The feature adds the settings below, which **do not exist in `config/` yet** and
must be added in one change with the typed structure, the loader, the
validation, `.env.example`, the reference table and the tests, as
`AGENTS.md` §4.11 requires:

| Setting | Default | Meaning |
| ------- | ------- | ------- |
| `ml.enabled` | `false` | Whether the `Burn` embedder is attempted at all. |
| `ml.backend` | `flex` | One of `flex` or `deterministic`; anything else is refused at start-up (see §7). |
| `ml.model_path` | unset | The local SafeTensors file. |
| `ml.dimension` | `384` | The vector length the embedder must produce. |
| `ml.timeout_seconds` | `2` | The budget for **one inference** (§12.3). |
| `ml.candidate_limit` | `200` | The largest lexical candidate set a hybrid search will re-rank. At most `MAX_ML_CANDIDATE_LIMIT`, 1,000 (§12.5). |

**Amended 2026-10-24:** this table has a **seventh** row, `ml.tokenizer_path`,
added by §10. Read the two together; the value is not optional in the sense that
matters — a tokenizer is required for `ml.backend = flex`, and its absence is
the disabled state, not a start-up failure.

### 6. The delivery gate

The ML chain opens with a **spike**, before any endpoint depends on it: load a
small sentence-embedding model on this machine, measure latency and memory,
and record the numbers. If the spike fails, the feature ships with the
`Disabled` embedder and lexical search is the behaviour. It is not replaced
with a service, and it is not replaced with another language. This is the
"honest failure" path, and it is part of the decision rather than an
afterthought.

### 7. Amendment (2026-10-24) — the backend values, the disabled mode, and packaging

**This is an amendment, and it is marked as one.** It corrects §5 and makes
three operational points explicit. Nothing else in the decision changes.

1. **`ml.backend` accepts two values: `flex` and `deterministic`.** Any other
   value is a start-up error, never a silent fallback. `deterministic` is not a
   Burn backend: it selects the non-Burn embedder. It exists because the
   automated suite must be able to exercise the scored modes **without a model
   file and without a network**; the seeder uses the same implementation
   directly (point 2). A test that loaded real weights would make CI depend on
   an artifact that is not in the repository, which §1 forbids.
2. **`ml.enabled = false` yields `Disabled`, whatever `ml.backend` says.** The
   seeder therefore does **not** read the configuration: it constructs the
   `Deterministic` embedder directly, so seeding is independent of the
   deployment's ML settings and of whether a model file exists.
3. **An `ml` Cargo feature gates `burn` and the module.** The feature is on by
   default, and `cargo check --no-default-features` and `cargo test
   --no-default-features` build and test the API without it. The reason is that
   a model-stack change must not break, slow or bloat the base build, and the
   delivery gate that compiles the tree with no default features has to keep
   holding.

### 8. The measured result of the delivery gate (2026-10-24)

**This section records a measurement; it does not change the decision.** §6 made
the spike the gate in front of the endpoint work. The gate has been run, and
this is what it established, recorded here so that no later document has to
reconstruct it from a change log.

**Subject.** `src/bin/atom-ml-spike.rs`, a binary with
`required-features = ["ml"]`, over the real
`all-MiniLM-L6-v2` SafeTensors file (86.66 MB, vocabulary 30522,
dimension 384) on the development machine: Windows, `x86_64-pc-windows-msvc`,
`--release`.

**Source of the numbers.** The **idle release run** in
[`../engineering/change-log.md`](../engineering/change-log.md)
`ATOM-CHANGE-20261024-010`, whose **Correction block is authoritative** over the
original table. An earlier run of the same code shared the CPU with a release
build and is not quoted here.

| Measurement | Idle release run |
| ----------- | ---------------- |
| Model construction | **96 ms** |
| First inference | **0.1933 ms** |
| Steady state, 1 text | **0.0170 ms** mean (min 0.0140, max 0.2262) |
| Steady state, 10 texts | **0.0178 ms** mean (min 0.0117) |
| Steady state, 100 texts | **0.0198 ms** mean (min 0.0123) |
| Peak RSS | **93.5 MiB**, unchanged between construction and the end of the run |
| Output L2 norm | **0.059342** — see §9 |
| Determinism | bit-identical for the same input; different for a different input |
| `Disabled` | returns error code `ml_disabled`, no panic, no file read |
| `Deterministic` | constructs in ~0 ms, dim 384, unit norm, stable, no file |

Run-to-run variance on an idle machine is roughly a factor of two: an earlier
idle build of the same source measured 0.0365 / 0.0476 / 0.0414 ms for the three
sizes. The table quotes the final build.

#### 8.1 What those numbers measure, and what they do not

**The timing is a lower bound on a real encoder, not a MiniLM measurement.** The
measured network is:

1. a lookup in the real `embeddings.word_embeddings.weight` table,
2. a mean-pool over the sequence,
3. a 384→384 projection using the real `pooler.dense` tensors.

**No attention block, no feed-forward block and no LayerNorm ran**, and the
tokenizer is a stable stand-in (whitespace plus an FNV-1a hash into the
vocabulary), not WordPiece. The 0.017–0.020 ms per-text figure therefore says
that **Burn 0.22, the `flex` backend and a real SafeTensors file work
in-process on this machine, at a cost far below the request budget** — it does
**not** say what a six-layer BERT encoder costs, and it must never be quoted as
the cost of a MiniLM embedding. `M3` re-measures with the encoder and the real
tokenizer before any timeout or candidate bound is planned around this figure;
the obligation is written into the build contract in
[`../features/ml/architecture.md`](../features/ml/architecture.md) §12.3.

The spike established nothing about **retrieval quality** either. The vectors it
produced are not meaningful for search: the projection is the wrong pooling for
this model (see [`../features/ml/architecture.md`](../features/ml/architecture.md)
§12.3) and no transformer layer ran. That question belongs to the `M6` fixture
evaluation and the recorded manual run.

#### 8.2 The cost of the dependency, and its operational consequence

| Cost | Value |
| ---- | ----- |
| MSRV declared by `burn` 0.22 | **rustc 1.95** |
| Clean `cargo build --release` of the Burn graph | **21 m 31 s** |
| `target/` after that build | **12.9 GB** |
| Release spike binary | **2.21 MiB** |

The operational consequence is recorded as **`KI-022`** in
[`../engineering/known-issues.md`](../engineering/known-issues.md): every
default build of this crate compiles Burn, a contributor on an older toolchain
gets an MSRV error rather than a working build, and CI cache sizing has to be
set deliberately. That is the accepted price of §7.3 — `ml` stays **on by
default**, so a plain `cargo build` and `cargo test` exercise the same path a
deployment does.

**The escape hatch is verified, not assumed.**
`cargo check --no-default-features --all-targets --locked` exits 0 with Burn
absent from the dependency graph, and `cargo tree --no-default-features` lists
no `burn*` crate. A build that does not want the model stack keeps working; it
pays nothing for this decision.

#### 8.3 A missing model file is a state, never a start-up failure

This is the property that makes the whole design deployable, so it is recorded
as a result rather than as an intention:

- **A missing, unreadable or unloadable model artifact selects `Disabled`. It
  never fails start-up, never aborts a request and never panics.** The reason is
  logged once, at start-up, and the process serves every route.
- **CI has no weights and must never download one.** That is why every test
  selects `Deterministic`, why the seeder constructs it directly, and why
  `semantic` and `hybrid` must still answer `200` with lexical results: the
  suite has to prove the degradation path on a machine that has no model at
  all. A test that needed a real file would make CI depend on an artifact that
  §1 forbids it to fetch.

### 9. Amendment (2026-10-24) — the stored vector is normalised at write time

**This is an amendment, and it is marked as one.** It fixes a property of the
data that §2–§4 left implicit, and it is decided *before* the first row is
written, because after that it is a migration instead of a decision.

**Decision: `Embedder::embed` returns a unit-length vector.** Every
implementation normalises its output before returning it, so the value stored in
`embeddings.vector` is unit length, the query vector is unit length, and
**cosine similarity is exactly the dot product** for this table.

Why normalising at write time, rather than at read time or not at all:

| Placement | Consequence |
| --------- | ----------- |
| **Write time — chosen** | The representation is fixed once, by the only code that can produce a vector. Every reader may rely on it. It is impossible to store a vector from one code path in one convention and a vector from another in the other, because there is only one way out of `embed`. |
| Read time — rejected | It hides a writer that violated the invariant, it costs a pass over 384 floats per row per query (200 candidates per hybrid request), and it leaves the stored meaning ambiguous: a row's value would not be a vector, but an input to a conversion. |
| Not at all — rejected | It is what the spike did, and it is why the measured norm was **0.059342** rather than ~1. A table that starts unnormalised and gains normalisation later changes what every existing row means, silently, at a commit that looks like a tiny cleanup. That is exactly the class of change this section exists to prevent. |

**What it costs.** One pass over 384 `f32` values per embed: 384 multiplications,
384 additions and 384 scalar multiplications, after the forward pass. It is
smaller than the forward pass it follows by a wide margin — the spike's whole
embedding path measured 0.017–0.020 ms per text, so the pass does not need its
own measurement to be justified, and it is re-measured with everything else in
`M3`.

**What it prevents.** A `embeddings` table holding a mixture of normalised and
unnormalised vectors, which is a table whose rows cannot be compared with each
other or with a query without knowing which writer produced them. The
`model_id` column records *which model* produced a vector; it does not record
*which convention*, and adding a second convention column would be a worse
answer than making the convention unconditional.

**Cosine ranking is scale-invariant, which is why the spike's timings stand.**
The spike measured the cost of producing a vector, not the cost of ranking one,
and `cos(a·x, b·y) = cos(x, y)` for positive scalars. Adding the normalisation
pass therefore changes no ordering for a correct cosine implementation; it
changes what is stored and what a reader may assume. It also makes the
published score range (`[0, 1]` after clamping) a property of the data rather
than of the model's output scale.

**What the reader may and may not assume.**

- It **may** compute the score as a dot product: both operands are unit vectors,
  so this is cosine, and the result is clamped to `[0, 1]` for the response.
- It **must still validate** the row: a row whose `cardinality(vector)` is not
  `dim` is skipped rather than scored (the schema's `CHECK` and the reader's
  check are the same rule at two layers).
- It **must not re-normalise** a row. Re-normalising on read would silently
  repair a broken writer, which is precisely what a write-time invariant is for.
- The test helper in `tests/ml_api.rs` that plants vectors must plant unit
  vectors, so a test never exercises a shape the product cannot produce.

**Degenerate output.** A zero or non-finite norm has no unit vector, so `embed`
returns `MlError::Model` instead of a vector. No row is written, a search
degrades exactly as it does for any model fault, and a match answers `409`. The
fixed reason codes in §5 do not grow: "the model produced something unusable" is
`model_missing` to a client, and the cause is logged once.

**The spike's model identifier is retired, not reused.**
`burn-minilm-l6-v2-spike-v1` named an unnormalised, semantically meaningless
vector. The real encoder is a new `model_id` (§12 of the feature architecture),
which is a reindex rather than a rewrite. No migration or backfill exists,
because the spike never wrote a row: there is no table and no endpoint in that
change.

### 10. Amendment (2026-10-24) — the tokenizer is a second model artifact

**This is an amendment, and it is marked as one.** It adds one setting to the
table in §5.

A WordPiece tokenizer needs its vocabulary, and the vocabulary is not inside the
SafeTensors file. It ships beside the weights as `vocab.txt`. The path is
therefore configuration, not a constant:

| Setting | Default | Meaning |
| ------- | ------- | ------- |
| `ml.tokenizer_path` | unset — the `vocab.txt` beside `ml.model_path` | The WordPiece vocabulary for `ml.backend = flex`. |

- Unset means "the sibling of the weights". That makes the common case — an
  extracted `sentence-transformers/all-MiniLM-L6-v2` directory — work without a
  second line of configuration.
- A missing or unreadable vocabulary yields `Disabled` with reason
  `model_missing`, exactly like a missing weights file, and start-up still
  succeeds (§8.3). A client does not need to know *which* artifact is missing;
  the cause is logged once.
- `ml.backend = deterministic` ignores the setting entirely, and so does the
  seeder.

§5's table is read as having seven rows. The value is added in the same change
in all the places `AGENTS.md` §4.11 lists, and `config/ml.toml` is added to
`BASE_FILES` and to the `embedded()` match in `src/config/loader.rs` — a
configuration file that is not in both is silently absent from an embedded
build.

### 11. The pre-agreed fallback if the encoder does not fit this phase

§6 fixed the first half of the honest-failure path: if the spike fails, the
feature ships with `Disabled` and lexical search. §8 records that the spike
passed, but it also records what the spike did **not** measure — the six
transformer layers and the tokenizer. A real encoder written in Burn, with a
hand-written WordPiece tokenizer and a key-mapped SafeTensors load, is a large
piece of work. If it proves too large for this phase, the following is the
**agreed outcome**, and it is a legitimate one:

**Correction (2026-10-26).** The condition did **not** happen: the encoder and
the tokenizer were built in `D4.M5`/`D4.M6`, `ML-R02` is met, and a deployment
that supplies both artifacts gets a Burn embedding rather than `Disabled`. What
follows is the fallback that *would* have applied. It is retained because the
decision and its reasoning are part of the record; read the "what would have
been not met" bullet below as a statement about that fallback, not about the
feature as built.

- **ML ships as `Disabled` in every deployment.** `semantic` and `hybrid` answer
  with lexical results and `200`, `degraded: true`,
  `degraded_reason: "ml_disabled"`. A match answers `409`. A reindex answers
  `409`.
- **Lexical search is the behaviour.** It is the Phase 1 full-text search, which
  already exists and is covered by the Phase 1 suite; nothing about the working
  product gets worse.
- **`ml.enabled` keeps its documented default `false`**, so a default
  deployment never looks for a model file, and **no endpoint depends on a model
  file to answer**. The three routes exist, are documented, are owner-scoped and
  are tested.
- **What ships and is real:** the `embeddings` table, the `Embedder` boundary,
  `Deterministic` and `Disabled`, the three endpoints, the degradation rule, the
  ownership predicates, the idempotent reindex, the seeder's `--with-embeddings`,
  and the whole test suite.
- **What would have been not met in that fallback:** `ML-R05`'s scored modes and
  `ML-R09`'s manual quality run. The fallback was not taken: the encoder landed,
  `ML-R02` is met, `ML-R05` is implemented, and `ML-R09`'s quality half is the
  recorded `atom-ml-bench` run. No requirement was redefined; the fallback stays
  in this section because it is the gate §6 built, and this correction records
  that the gate was not needed.
- **What must not happen instead:** a fake encoder that returns plausible
  numbers, a downloaded model, a second language runtime, a sentence-embedding
  service, or the deletion of the degradation tests because "there is no model
  anyway". Those are the ways this decision gets undone by accident.

This is not a failure of the chain. It is the gate §6 built, working as
designed: the product is correct and smaller, the limitation is visible in the
API response and is available to a readiness check, and a later chain adds the
encoder **without a schema change**, because `model_id` is a column and the
degraded path is the same code path a working model uses.

### 12. Amendment (2026-10-25) — the measured encoder cost, and which bound applies

**This is an amendment, and it is marked as one.** It records the second run of
§6's gate — the real six-layer encoder and WordPiece tokenizer, not the spike's
lookup — and settles the two numbers that run was said to invalidate. §4's
sentence about what `ml.timeout_seconds` bounds is corrected: it bounds **one
inference**, not one re-rank. No other part of the decision changes.

#### 12.1 The measurement

`src/bin/atom-ml-bench.rs`, `--release`, the real
`sentence-transformers/all-MiniLM-L6-v2` SafeTensors file and its `vocab.txt`,
on the development machine. Recorded in
[`../features/ml/status.md`](../features/ml/status.md) §5.

| Measurement | Value |
| ----------- | ----- |
| One text, steady state | **88.0 ms** mean (min 28.8, max 210.0) |
| Ten texts | **111.6 ms** per text |
| One hundred texts | **112.6 ms** per text — **11.3 s** total |
| Worst case, 256 tokens (the truncation bound) | **804 ms** mean (min 775, max 834) |
| Peak RSS | **183.8 MiB** |
| Determinism | bit-identical for the same input; different for a different input |
| Semantic quality | **15 of 15** evaluation queries returned the expected nearest text (see the pair-count note below) |

Roughly **5,600 times** the spike's 0.017–0.020 ms per text, which measured a
lookup, a mean-pool and a projection (§8.1). This is the real cost of an
embedding, and §8's numbers are superseded for every purpose except the record
of what the spike proved.

**Pair-count note (2026-10-26).** Two counts are attached to the same benchmark
in the record: **fifteen**, which the current `src/bin/atom-ml-bench.rs` defines
and which [`../features/ml/status.md`](../features/ml/status.md) §5 records, and
**five**, which this table carried when it was first written from an earlier
version of the probe. The repository does not hold the transcript, so which count
the recorded run produced cannot be determined here. The five-query figure is
marked **unverified** in status.md §4; the fifteen-pair figure is the one the
current binary produces. No number was invented to reconcile them.

#### 12.2 What it invalidates, and what it does not

The chain that prompted this amendment was opened with the arithmetic
`200 candidates × 112.6 ms = 22 s`, and proposed therefore
`ml.candidate_limit ≈ 100` with `ml.timeout_seconds ≈ 12`.

**That is not the arithmetic of a search.** A hybrid or semantic request embeds
exactly **one** text — the query — and compares it against vectors that are
already stored:

- `MlService::search` (`src/ml/service.rs`, `fn search`) calls `embed_query` once and
  then hands the vector to `hybrid`.
- `MlService::hybrid` (`src/ml/service.rs`, `fn hybrid`) contains **no inference call at
  all**. It selects at most `ml.candidate_limit` candidate *identifiers*
  (`NoteRepository::lexical_candidates`), loads their **stored** vectors
  (`MlRepository::vectors_for_sources`), and ranks them with `rank` — one dot
  product per row, over 384 `f32` values.
- `ml.timeout_seconds` is applied inside `MlService::embed`
  (`src/ml/service.rs`, `fn embed`), which wraps **one** `Embedder::embed` call.

The number of inferences per request is therefore:

| Request | Inferences |
| ------- | ---------- |
| `POST /notes/search`, `semantic` or `hybrid` | **1** (the query) |
| `POST /job-applications/{id}/match` | **1** (the application) |
| `POST /ml/reindex` | **one per source that is not already current** |

The 11.3 s and the 22 s are the cost of *embedding* 100 and 200 texts. That is
the cost of `POST /ml/reindex` over 100 and 200 notes. It is not the cost of a
page, and it is not the cost of a candidate set.

So the measurement does not force a change to the search budget. It forces two
things this ADR and the feature architecture had got wrong: the reindex's
ceiling, which nobody had computed, and the claim that
`ml.timeout_seconds` bounds the re-rank, which it does not.

#### 12.3 `ml.timeout_seconds` stays `2`

The budget is **per inference**, and no inference this feature can be asked to
perform comes near it:

- The worst single inference that exists is a 256-token sequence at **804 ms**
  mean, **834 ms** maximum. The tokenizer truncates at 256 tokens
  ([`../features/ml/architecture.md`](../features/ml/architecture.md) §12.3), so
  nothing can be longer, and 834 ms is a hard ceiling on the reference machine.
  `2 s` is **2.4×** it.
- A search embeds a query capped at **200 characters**
  (`MAX_SEARCH_LENGTH`, `src/helpers/validation.rs:101`), which is a short
  sequence: **88.0 ms** mean, **210.0 ms** observed maximum. `2 s` is **9.5×**
  the observed maximum and **23×** the mean.
- A match embeds `role_title` and `description`, truncated at the same 256
  tokens: bounded by the same 834 ms.

A value of `12` would be **dead configuration**. Nothing this encoder can be
asked to do takes two seconds, let alone twelve, so the only observable effect
of raising the budget is that a genuinely wedged call holds a `spawn_blocking`
thread for six times as long before the honest `inference_timeout` is reported.
Timing out later is not a performance improvement; it is a slower failure.

Slower hardware is what `APP_ML_TIMEOUT_SECONDS` exists for. `2 s` absorbs a
host **2.4× slower** than the reference machine at the absolute worst case, and
**9.5× slower** on the path a user actually waits for. A deployment that
measures otherwise raises the value; the default describes the measurement, not
a guessed fleet.

#### 12.4 `ml.candidate_limit` stays `200`

The re-rank's entire cost is `candidate_limit × dim` multiply-adds and
`candidate_limit × dim × 4` bytes. `score()` computes a dot product and two
norms per row, so count roughly three passes over 384 values per candidate:

| At 200 candidates, dimension 384 | Value | Against the 88 ms inference |
| -------------------------------- | ----- | --------------------------- |
| Floating-point operations | ≈230,000 (`f64`) | sub-millisecond — two orders of magnitude below |
| Vectors held in memory | **307 KB** | negligible |

The candidate bound was never the source of the 22 s: it costs well under a
millisecond, against an inference of tens to hundreds of milliseconds. It is
negligible at 200, and 200 buys recall that a bound of 15 or 100 does not. It is
not changed.

#### 12.5 The `candidate_limit >= api.pagination.max_limit` rule is withdrawn

`validate_ml` (`src/config/validation.rs:502`) refuses a candidate limit below
`api.pagination.max_limit`. Its stated reason is:

> A candidate set smaller than one page would make page two impossible: the
> re-rank would have nothing left to slice.

**That is false.** `Pagination::new(limit, offset, total)`
(`src/pagination.rs:64`) computes `has_more: offset + limit < total`, so a
ranked list shorter than the page size yields a well-formed **short page**: the
items that exist, the true `total`, and `has_more: false`. Nothing is
impossible, and the feature's own public contract already publishes exactly this
behaviour — [`../features/ml/api.md`](../features/ml/api.md) §2.3 says the
`hybrid` `pagination.total` "is never more than `ml.candidate_limit`".

Two further facts make the floor actively harmful rather than merely wrong:

1. **It is satisfied in the suite only by moving the other setting.** The test
   that proves the bound, `hybrid_never_returns_more_than_the_candidate_limit`,
   needs a candidate bound of 3, so `test_state_with_candidate_limit`
   (`tests/common/mod.rs:237`) lowers `api.pagination.max_limit` to 3 as well.
   A rule that every test can only obey by changing a second value is not
   testing a product constraint; it is testing a workaround.
2. **It leaves `candidate_limit` with no upper bound at all.** The floor's own
   maximum is 1,000 only by accident, inherited from `MAX_PAGINATION_LIMIT` on
   `max_limit`; the setting itself is accepted at `10_000_000`. §8 says the
   value "is a resource bound, not a quality knob … which is why it is validated
   at start-up". As implemented, validation bounded it from **below** and not
   from above — and §12.2 shows that the re-rank is the one part of a search
   that `ml.timeout_seconds` does **not** bound, so `candidate_limit` is the
   re-rank's **only** bound. The bound was missing where it was needed and
   present where it was not.

**Decision: the floor is replaced by a ceiling.** `ml.candidate_limit` must be
positive and **at most `MAX_ML_CANDIDATE_LIMIT = 1_000`**, a constant declared in
`src/config/validation.rs` beside `MAX_PAGINATION_LIMIT` — the same character of
value: a hard ceiling on a configurable quantity, not a deployment policy, and
therefore not a setting (`AGENTS.md` §4.11).

The value is derived, not chosen for roundness. At 1,000 the re-rank is ≈1.2
million `f64` operations — on the order of a millisecond, two orders of
magnitude below the 88 ms inference that precedes it — and holds **1.5 MB** of
vectors. It also equals `MAX_PAGINATION_LIMIT`, so a legal page and a legal
candidate set can never outgrow the other and **no relation between the two
settings is needed at all**.

The direction of the old rule was the error. A page is a **presentation** bound;
a candidate set is a **work** bound. Requiring the work bound to be at least the
presentation bound expressed neither, and left the work bound unbounded.

#### 12.6 The reindex has a ceiling, and it is the request timeout

With `ml.timeout_seconds` at 2, the per-inference budget is never the binding
limit for a reindex: `POST /ml/reindex` embeds one text per source that is not
already current, so its wall clock is `sources × per-inference`, and the limit
that binds is `app.limits.request_timeout` — **30 s** by default — enforced
globally by `timeout_middleware` (`src/api/mod.rs:145`, installed at `:94`).

| Source shape | Per source | Sources inside one 30 s request |
| ------------ | ---------- | ------------------------------- |
| Short note (the bench's text shape) | 112.6 ms | **≈266**, before the per-row SQL |
| Worst case, 256 tokens | 804 ms | **≈37** |

Past that the request answers `408`, the rows already written stay, and the next
run skips them, because the reindex is idempotent (§3). That is the behaviour
[`../features/ml/architecture.md`](../features/ml/architecture.md) §12.6 already
defines. What had never been written down is the **number**, and it now is.

This is a recorded limit for Phase 1's personal corpus, not a defect this
amendment fixes. A reindex that must exceed one request needs its own decision —
a resumable job with a cursor, or a request-scoped budget — and it is named as
such here rather than bought by enlarging a timeout that no inference needs.

#### 12.7 What this amendment corrects in the other documents

| Claim | Where it was | What it becomes |
| ----- | ------------ | --------------- |
| "`ml.timeout_seconds` bounds the inference and the re-rank" | §4 of this ADR; §4.2 and §11 of the feature architecture | It bounds **one inference**. The re-rank is bounded by `ml.candidate_limit`, and now by a ceiling on it. |
| "a candidate set smaller than one page would make page two impossible" | `validate_ml`; `config/ml.toml`; `.env.example`; `config/README.md`; [`../architecture/deployment.md`](../architecture/deployment.md) §1.3 and §1.5 | Withdrawn. A short page is well formed and already published. |
| "If a 100-text **re-rank** no longer fits the default budget …" | [`../features/ml/architecture.md`](../features/ml/architecture.md) §12.3 | "… a 100-source **reindex** …". The mislabelling is what made `200 × 112.6 ms` look like a page. |

#### 12.8 Rejected alternatives

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **`candidate_limit = 100`, `timeout_seconds = 12`** — the arithmetic this chain opened with | Refuted by §12.2: it prices a reindex as if it were a search. It would halve hybrid recall, and make a wedged inference take twelve seconds to report a `408`, for a workload no measurement produces. |
| **`candidate_limit = 15`** — `2 s ÷ 112.6 ms` | The same error with a smaller number. It would cap the default 10-row page at 15 candidates and destroy hybrid recall to buy headroom against a cost that is not on the request path. |
| **Keep the floor and add the ceiling** | The floor's justification is false (§12.5), and floor-plus-ceiling would keep the accidental coupling — two independent bounds expressed as one relation, with the direction of the surviving rule still wrong. |
| **A new `ml.reindex_budget` setting** | A new setting, in `AGENTS.md` §4.11's seven places, for a value no deployment has yet needed to change, in a chain whose question was what two existing numbers should be. `app.limits.request_timeout` already bounds it; §12.6 records the number so an operator can size a corpus. |
| **Raise `timeout_seconds` to cover a slower host** | A default describes the measured reference machine. A slower host sets `APP_ML_TIMEOUT_SECONDS`; inventing a number for unknown hardware is the same class of mistake as the 12 s. |

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **pgvector with `CREATE EXTENSION vector`** | The extension is the standard answer, and it would give an indexed nearest-neighbour query. It was not chosen because the `atom` role is not a superuser on the development machine, so the migration is denied locally while it would pass in CI, where the role is a superuser. Making `CREATE EXTENSION` a deployment prerequisite is a real option — the plan says so — but it would make every developer and every deployment depend on an extension and a privilege the current environment does not grant. The divergence is worse than the missing index. |
| **An external model server (Ollama, TEI, a hosted embedding API)** | The best model quality per unit of work, and no model code in the process. It was not chosen because it violates O6 and the no-outbound-call constraint, and because it adds a service to operate, a network path inside the request, and a new failure mode. |
| **A Python or Node sidecar** | The richest ecosystem, and the format every embedding model ships in. It was not chosen because it adds a second language runtime and violates O6 outright. |
| **pgvector without the extension: brute force over all rows in SQL** | It avoids the extension and keeps the vector in the database. It was not chosen because a full scan over every embedding of the owner, per query, has no bound: cost grows linearly with the corpus, and the memory for the result set is unbounded. The bounded lexical candidate set gives a defensible bound instead. |
| **An in-process approximate index (HNSW) over all embeddings** | It would remove the linear scan and keep recall. It was not chosen because it adds an index to build, persist, invalidate and rebuild, for a corpus that a bounded re-rank already handles. It is the natural next step if the corpus outgrows the candidate bound — as a new ADR, not a hidden change. |
| **Lexical search only, no embeddings** | It is the cheapest and the most predictable. It was not chosen because `ML-R05` requires semantic and hybrid modes; the requirement would not be met. `Disabled` remains the *degraded* behaviour, not the accepted feature. |
| **Storing the vector in a file rather than in PostgreSQL** | It would keep the database small. It was not chosen because it introduces a second store with its own consistency, backup and deletion problems, and because the owner-scoping of a file store is a new access-control surface. |
| **`vector(n)` from pgvector, if the extension were available** | It would type-check the dimension in the database. `REAL[]` plus a `dim` column and a length check gives the same guarantee without the extension privilege. |

## Consequences and trade-offs

### Positive

- **The schema is identical on every machine.** No extension, so no privilege
  difference between the development machine, CI and production.
- **No new trust boundary.** The model is a local file read by the process;
  no request leaves the machine and no credential for a third party exists.
- **The growth is bounded.** Time per search is one inference, bounded by
  `ml.timeout_seconds`; the re-rank's time and memory are bounded by
  `ml.candidate_limit`, which is bounded above at start-up. A large corpus
  degrades the *quality* of the candidate set rather than the stability of the
  API.
- **The feature is testable without a model.** The deterministic embedder
  makes the plumbing — storage, upsert, re-rank, degradation — fully covered
  by the suite.
- **Degradation is a first-class state.** A deployment without a model is a
  supported configuration with `200` answers, not a broken one.

### Negative

- **No approximate-nearest-neighbour index.** Semantic recall is limited to
  what the lexical candidate set contains in hybrid mode, and a semantic query
  is a scan of the caller's embeddings. A note that shares no lexical term
  with the query cannot be found by hybrid search at all. This is the real
  cost of the decision, and it is recorded in the feature's
  [`../features/ml/specification.md`](../features/ml/specification.md) as a
  known limitation.
- **The polymorphic `source_id` has no foreign key.** Deleting a note or an
  application must delete its embeddings in the same transaction, and a
  reindex must clean up what an interrupted delete left behind. A forgotten
  delete leaves an orphan row; it is unreachable through the owner-scoped
  queries, but it is still a leak of storage.
- **The model file is an operational artifact.** It is not in the repository,
  it is not downloaded by the build, and it has to be placed on the host. A
  missing or renamed file silently degrades to lexical search, which is safe
  but easy to miss; the readiness output and the response's degradation reason
  are the signals.
- **The deterministic embedder proves plumbing, not quality.** A test with it
  says nothing about whether the real model ranks a paraphrase correctly.
  That is why `ML-R09` requires a separate recorded manual run.
- **A Burn upgrade is a code change, not a version bump.** The tensor and
  store APIs are the ones `AGENTS.md` §4.12 pins; moving to another Burn
  release needs the same spike the first integration needed.
- **The reindex is a per-row write.** There is no bulk vector load; a full
  reindex of a large corpus costs one round trip per row. It is bounded by the
  corpus size, which is personal-scale, and it is not on the request path
  except when the client asks for it.

## Security and operational implications

- **Embeddings are derived personal data.** A vector is a compressed,
  approximate representation of a note; it must be as private as the note. Every
  embedding query carries the caller's `user_id`, and a search never reads
  another account's vectors — even to compute a score.
- **No outbound traffic.** This is the strongest security property of the
  decision: there is no third-party endpoint to be compromised, no API key to
  leak, and no data-processing agreement to sign. `src/ml/` contains no HTTP
  client.
- **The model file is trusted operator input.** It is deserialised by Burn as
  tensor data, not executed. A malicious file is a corrupt model, not code
  execution; the operator is already inside the trust boundary (TB-4 in
  [`../security/threat-model.md`](../security/threat-model.md)), so this is
  consistent with how the rest of the process treats its host.
- **Resource use is bounded by configuration, and the configuration is
  bounded.** `ml.timeout_seconds` bounds one inference; `ml.candidate_limit`
  bounds one re-rank and is itself refused above `MAX_ML_CANDIDATE_LIMIT`
  (§12.5), so neither an operator nor a client can make one request's CPU or
  memory grow without a start-up error. `semantic` mode's corpus-sized scan is
  the one unbounded path, and it is the accepted cost of having no index (§12.2,
  and *Negative* below).
- **The degradation path must not be a silent quality drop.** The response
  reports the applied mode, so a client can tell the owner that results are
  lexical. A `500` would be worse: it would turn a missing optional file into
  an outage of a working endpoint.
- **No new secret.** There is no model API key, because there is no model
  service. The existing secret rules in `AGENTS.md` §8 are unchanged.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §18.3 `ML-R01`…`ML-R09` | The requirements this decision serves. |
| [`../project-truth.md`](../project-truth.md) §6.1 | The amendment that permits local inference and keeps outbound AI excluded. |
| [`../engineering/runs/chain/plan.md`](../engineering/runs/chain/plan.md) §6 | The pgvector finding and the design this ADR formalises. |
| [`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md) | Owner decisions O1, O6 and O7. |
| [`../features/ml/architecture.md`](../features/ml/architecture.md) | How the decision is built. |
| [`../features/ml/api.md`](../features/ml/api.md) | The search, match and reindex contracts. |
| [`../api/conventions.md`](../api/conventions.md) §17 | The published search modes and the degradation rule. |
| [`../database/schema.md`](../database/schema.md) | The `embeddings` table. |
| [`ADR-0002-rust-axum-postgresql.md`](ADR-0002-rust-axum-postgresql.md) | The stack this decision extends. |
| [`../engineering/change-log.md`](../engineering/change-log.md) | The change that implements it, and `ATOM-CHANGE-20261024-010` with its Correction block: the measured result recorded in §8. |
| [`../engineering/known-issues.md`](../engineering/known-issues.md) `KI-022` | The build cost and MSRV consequence of §7.3 and §8.2. |
| [`../engineering/runs/chain/plan.md`](../engineering/runs/chain/plan.md) §7, Phase 4 | Chains `M1`–`M6`; `M3`–`M6` build against [`../features/ml/architecture.md`](../features/ml/architecture.md) §12. |

## Supersedes / Superseded by

Supersedes: none. No earlier decision covered machine learning.

Superseded by: none. A review of this decision is due when the corpus outgrows
the bounded re-rank, when the Phase 4 spike fails and ML ships disabled, when
the encoder is deferred under §11, when a reindex must exceed one request
(§12.6), or when the owner changes O6.

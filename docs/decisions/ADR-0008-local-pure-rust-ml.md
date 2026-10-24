# ADR-0008 — Machine learning is local and pure Rust

> **Status:** Accepted
> **Date:** 2026-10-24
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
- `ml.timeout_seconds` bounds the inference and the re-rank, so a large model
  cannot hold a request beyond the configured budget.
- **Degradation is explicit.** When ML is disabled or the model file is
  missing, `semantic` and `hybrid` return **lexical results with `200`**, and
  the response names the requested mode, the applied mode and a degradation
  reason. A missing model is a configuration state, not a server error.

### 5. Configuration

The feature adds six settings, which **do not exist in `config/` yet** and
must be added in one change with the typed structure, the loader, the
validation, `.env.example`, the reference table and the tests, as
`AGENTS.md` §4.11 requires:

| Setting | Default | Meaning |
| ------- | ------- | ------- |
| `ml.enabled` | `false` | Whether the `Burn` embedder is attempted at all. |
| `ml.backend` | `flex` | One of `flex` or `deterministic`; anything else is refused at start-up (see §7). |
| `ml.model_path` | unset | The local SafeTensors file. |
| `ml.dimension` | `384` | The vector length the embedder must produce. |
| `ml.timeout_seconds` | `2` | The budget for one embed or re-rank. |
| `ml.candidate_limit` | `200` | The largest lexical candidate set a hybrid search will re-rank. |

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
- **The growth is bounded.** Time and memory per search are bounded by
  `ml.candidate_limit` and `ml.timeout_seconds`, so a large corpus degrades
  the *quality* of the candidate set rather than the stability of the API.
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
- **Resource use is bounded by configuration.** `ml.timeout_seconds` and
  `ml.candidate_limit` bound one request. A large model still costs memory at
  load time, once, which is why the Phase 4 spike measures it before the
  endpoint depends on it.
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
| [`../engineering/change-log.md`](../engineering/change-log.md) | The change that implements it. |

## Supersedes / Superseded by

Supersedes: none. No earlier decision covered machine learning.

Superseded by: none. A review of this decision is due when the corpus outgrows
the bounded re-rank, when the Phase 4 spike fails and ML ships disabled, or
when the owner changes O6.

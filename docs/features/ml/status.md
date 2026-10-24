**Status:** Accepted

# Local machine learning — status

> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-ML-001`](specification.md)

## 1. The status

The feature is **Accepted**. The requirements `ML-R01`…`R09` are agreed in
[`project-truth.md`](../../project-truth.md) §18.3, with acceptance criteria.
The design decision is
[`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).

The feature is **not** `Implemented` and **not** `Verified`:

- there is no `src/ml/` module;
- there is no `embeddings` migration;
- there is no model file, and `burn` is not in `Cargo.toml`;
- there is no `tests/ml_api.rs` and no ML route in the router;
- no test has been written, and therefore none has run or passed;
- the six `ml.*` settings do not exist in `config/` or `.env.example`.

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `ML-R01` Local inference only, no second runtime | Decided | [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md); no code |
| `ML-R02` Burn v0.22, `flex`, SafeTensors | Decided | [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md); no dependency yet |
| `ML-R03` `Burn`, `Deterministic`, `Disabled` | Not built | — |
| `ML-R04` `REAL[]` storage | Not built | — |
| `ML-R05` Three modes with a bounded re-rank | Not built | — |
| `ML-R06` Degradation to lexical with `200`, and `409` on a match when ML is unavailable | Not built | — |
| `ML-R07` Idempotent, owner-scoped reindex | Not built | — |
| `ML-R08` Owner-scoped reads and writes | Not built | — |
| `ML-R09` 15 fixture queries and a manual run | Not built | — |

`project-truth.md` §17 records the whole area as **Not started**.

## 2. What is still missing to reach `Verified`

1. **The spike.** Load a small sentence-embedding model with Burn on `flex`,
   measure latency and memory on the reference machine, and record the
   numbers. `ADR-0008` §6 makes this the gate: if it fails, the feature ships
   with `Disabled` and lexical search, and this file says so.
2. **The dependency.** `burn` v0.22 with the `flex` feature, behind the `ml`
   Cargo feature, with the reason recorded in the change log (`AGENTS.md` §4.8
   and §4.12, [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md) §7).
3. **The configuration.** The six `ml.*` settings in the file, the typed
   structure, the loader, the validation, `.env.example`, the reference table
   and the tests — in one change (`AGENTS.md` §4.11).
4. **The migration.** The `embeddings` table, the unique constraint, the
   dimension check and the index.
5. **The module.** `Embedder` and its three implementations, the service, the
   repository and the DTOs.
6. **The routes.** Registered in `src/api/routes.rs` and in `ROUTE_TABLE`.
7. **The delete hooks.** `NotesService::delete` and the job-application delete
   remove the embeddings in the same transaction.
8. **The tests.** Every test in [testing.md](testing.md), run and green.
9. **The evaluation.** Fifteen fixture queries under the deterministic
   embedder, and one recorded manual run with the real model.
10. **The seeder.** `--with-embeddings` writes embeddings with the
    deterministic embedder.
11. **The schema document.** [`schema.md`](../../database/schema.md) describing
    the table once it exists.
12. **A recorded run.** The gates and the suite, with the result written down.

Only after 1–12 is the feature `Implemented`; it becomes `Verified` when the
tests pass in a recorded run.

## 3. What a future change must do to keep it `Accepted` and then `Verified`

- **Do not change an accepted requirement silently.** A revision goes through
  [`project-truth.md`](../../project-truth.md) §18.4.
- **A4 is a provisional default.** The stack is not: O6 and `AGENTS.md` §4.12
  fix Rust-only, Burn and `flex`.
- **A second runtime or an outbound call is a new decision**, not an
  implementation detail. It would need an amendment to
  [`project-truth.md`](../../project-truth.md) §6.1 and a superseding ADR.
- **A change to `REAL[]` or to the hybrid design** needs a new ADR that
  supersedes [`ADR-0008`](../../decisions/ADR-0008-local-pure-rust-ml.md).
- **A new degradation trigger** is a contract change: it adds a value to
  `degraded_reason` that a client switches on.

## 4. The manual evaluation record

> **Not run.** There is no model file, no `burn` dependency and no endpoint to
> evaluate. This section is where the result goes, with the model, the
> dimension, the latency, the memory and the fifteen queries' outcome.

## 5. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | The planned modules, the `Embedder` trait, storage, the search pipeline, degradation, configuration and the security boundary. |
| [`api.md`](api.md) | The three endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The planned test suite, the evaluation split, and what is deliberately not covered. |
| [`../../api/conventions.md`](../../api/conventions.md) | The search modes and the degradation rule, §17. |
| [`../../database/schema.md`](../../database/schema.md) | The `embeddings` table, once the migration lands. |
| [`../../decisions/ADR-0008-local-pure-rust-ml.md`](../../decisions/ADR-0008-local-pure-rust-ml.md) | The local, pure-Rust decision. |

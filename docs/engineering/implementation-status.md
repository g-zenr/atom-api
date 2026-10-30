# ATOM implementation status

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-30
> **Owner:** maintained by the `docs` agent

The current verified state of every part of ATOM: what exists, what status it
carries, and the evidence behind that status. The requirements live
in [`../project-truth.md`](../project-truth.md); this document compares them
with the repository.

The per-feature detail is in the `status.md` of each feature directory under
[`../features/`](../features/). This document is the single overview and must
agree with them.

---

## 1. The status vocabulary

These are the only words a status may use. The definitions are the same in
[`../../AGENTS.md`](../../AGENTS.md) §6 and in [`../README.md`](../README.md) §2.

| Status | Meaning |
| ------ | ------- |
| `Proposed` | Suggested, not agreed. Must not be implemented as agreed scope. |
| `Accepted` | An agreed product requirement. Not built yet. |
| `In progress` | Being built right now. |
| `Implemented` | The code exists and compiles. **Not** evidence that it behaves correctly. |
| `Verified` | Implemented **and** covered by a test or a recorded check that passes. |
| `Deprecated` | Accepted earlier and deliberately dropped. The ADR or the note says why. |
| `Blocked` | Cannot proceed. The reason is recorded in [`known-issues.md`](known-issues.md). |

A feature is never `Verified` because the code exists or compiles.

## 2. The overview

| Area | Status | Evidence | Last verified |
| ---- | ------ | -------- | ------------- |
| Rust/Axum modular monolith | `Implemented` | [`../../src/`](../../src/), [`../../Cargo.toml`](../../Cargo.toml) | 2026-10-22 |
| Typed configuration with start-up validation | `Verified` | [`../../src/config/`](../../src/config/) — three layers (the committed files, the environment overlay, the environment variables), resolved once and validated before the socket is bound. The loader is exercised by the integration harness, which starts from `Config::defaults()`. The validation rules and the precedence between the layers are covered by the unit tests in [`../../src/config/tests.rs`](../../src/config/tests.rs) — 58 cases; see [`../testing/strategy.md`](../testing/strategy.md) §6. | 2026-10-22 |
| Client-facing message catalogue | `Implemented` | [`../../config/messages.toml`](../../config/messages.toml), [`../../src/config/messages.rs`](../../src/config/messages.rs) — every string that reaches a client through the error envelope. The stable codes live in the `code` module in [`../../src/error.rs`](../../src/error.rs); the messages for a database failure and for an unexpected failure stay in the error type. | 2026-10-22 |
| Shared text validators | `Verified` | [`../../src/helpers/validation.rs`](../../src/helpers/validation.rs) — 7 unit tests. `required_text` trims and rejects an empty value; `bounded_text` keeps the value verbatim. | 2026-10-22 |
| Central error contract with a stable error code | `Verified` | [`../../src/error.rs`](../../src/error.rs) — 5 unit tests | 2026-10-22 |
| SQLx migrations, append-only | `Implemented` | [`../../migrations/`](../../migrations/) - seven migrations (`0001`-`0007`), applied by the harness and at start-up | 2026-10-27 |
| Structured logging with request correlation | `Implemented` | [`../../src/telemetry.rs`](../../src/telemetry.rs), [`../../src/api/mod.rs`](../../src/api/mod.rs) | 2026-10-22 |
| Liveness and readiness endpoints | `Verified` | [`../../src/health.rs`](../../src/health.rs), exercised by an operator run | 2026-10-21 |
| Versioned REST API under `/api/v1` | `Verified` | [`../../src/api/routes.rs`](../../src/api/routes.rs), exercised by the integration suite | 2026-10-22 |
| OpenAPI document and a local API explorer | `Verified` | [`../../src/api/routes.rs`](../../src/api/routes.rs), served at `/api/v1/openapi.json` and `/api/v1/docs/`; the content is asserted by [`../../tests/openapi_contract.rs`](../../tests/openapi_contract.rs) (operation set, methods, version prefix, list parameter sets) and the serving by [`../../tests/docs_route.rs`](../../tests/docs_route.rs). Response bodies and schemas are not snapshotted. | 2026-10-24 |
| Documentation tree | `Verified` | [`../`](../) — `cargo run --bin atom-docs` reports no error and no warning | 2026-10-22 |
| Registration with a unique, normalized email | `Verified` | [`../../src/users/`](../../src/users/), [`../../migrations/0001_users_and_sessions.sql`](../../migrations/0001_users_and_sessions.sql) — the cases in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 2026-10-22 |
| Argon2id hashing and the password policy | `Verified` | [`../../src/auth/password.rs`](../../src/auth/password.rs) — 6 unit tests; [`../../src/users/service.rs`](../../src/users/service.rs) — 9 unit tests; the weak-password case in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 2026-10-22 |
| Access tokens with issuer and audience validation | `Verified` | [`../../src/auth/token.rs`](../../src/auth/token.rs) — 7 unit tests; the authentication cases in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 2026-10-22 |
| Refresh rotation, reuse detection and revocation | `Verified` | [`../../src/auth/service.rs`](../../src/auth/service.rs), [`../../src/auth/repository.rs`](../../src/auth/repository.rs) — the rotation claims the old session atomically; the rotation, reuse and logout cases in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 2026-10-22 |
| Expired refresh-session pruning | `Implemented` | [`../../src/main.rs`](../../src/main.rs) — the background task calls `delete_expired` every hour with a one-day grace. **No test covers it.** | 2026-10-22 |
| Generic authentication failure | `Verified` | [`../../src/auth/service.rs`](../../src/auth/service.rs); two cases in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 2026-10-22 |
| Ownership isolation | `Verified` | [`../../src/notes/repository.rs`](../../src/notes/repository.rs), [`../../src/projects/repository.rs`](../../src/projects/repository.rs), [`../../src/tasks/repository.rs`](../../src/tasks/repository.rs); the isolation cases in [`../../tests/notes_api.rs`](../../tests/notes_api.rs) and [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) | 2026-10-22 |
| Rate limiting on the authentication endpoints | `Verified` | [`../../src/ratelimit.rs`](../../src/ratelimit.rs) - 6 unit tests for the algorithm; `the_authentication_endpoints_are_rate_limited` in [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | 2026-10-22 |
| CORS allowlist, body limit, request timeout, panic containment | `Verified` | [`../../src/api/mod.rs`](../../src/api/mod.rs), [`../../src/api/extract.rs`](../../src/api/extract.rs); `an_oversized_body_is_refused` and `an_unknown_route_returns_the_error_envelope` in [`../../tests/auth_api.rs`](../../tests/auth_api.rs), `a_slow_request_is_abandoned_with_the_error_envelope` in [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) | 2026-10-22 |
| Notes CRUD, archive, search, pagination | `Verified` | [`../../src/notes/`](../../src/notes/) — 5 unit tests; 12 cases in [`../../tests/notes_api.rs`](../../tests/notes_api.rs) | 2026-10-22 |
| Projects CRUD, search, pagination | `Verified` | [`../../src/projects/`](../../src/projects/) — 5 unit tests; the project cases in [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) | 2026-10-22 |
| Tasks CRUD, status, priority, filtering, ordering, completion timestamp | `Verified` | [`../../src/tasks/`](../../src/tasks/) — 7 unit tests; the task cases in [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) | 2026-10-22 |
| Pagination convention | `Verified` | [`../../src/pagination.rs`](../../src/pagination.rs) — 5 unit tests; the pagination case in [`../../tests/notes_api.rs`](../../tests/notes_api.rs) | 2026-10-22 |
| Contract tests against the OpenAPI document | `Verified` | [`../../tests/openapi_contract.rs`](../../tests/openapi_contract.rs) - `the_route_table_and_the_document_describe_the_same_operations` (the tables and the document agree in both directions), `every_route_in_the_table_is_served_by_the_router`, `the_version_prefix_is_declared_once`, `each_list_endpoint_publishes_its_filter_and_pagination_parameters_once` and `each_list_handler_extracts_the_query_string_once`. What is asserted is the operation set, the methods, the version prefix and the list parameter sets; response bodies and schemas are not snapshotted. | 2026-10-24 |
| The disposable test database guard | `Verified` | [`../../tests/common/mod.rs`](../../tests/common/mod.rs) refuses to start unless the database name contains `test` | 2026-10-22 |
| Development database seeder (`atom-seed`) | `Verified` | [`../../src/bin/atom-seed.rs`](../../src/bin/atom-seed.rs) and [`../../tests/seeder.rs`](../../tests/seeder.rs) — 21 integration tests and 13 tests in the binary, passing in the **536-test** run of 2026-10-27. Development tooling, not product behaviour: a whole-span review on 2026-10-26 read the seeder, but the per-chain rows `D2.S1` 10–11 remain `Fail`. | 2026-10-26 |
| Reimbursements: claims, items and the lifecycle | `Verified` | [`../features/reimbursements/status.md`](../features/reimbursements/status.md) is the single source: migration `0004`, twelve published operations, 56 endpoint + 3 flow + 10 schema cases and twelve unit tests (committed), the final-tree review **PASS** (`ATOM-REVIEW-20261029-002`), six gates at `0`. The suite was re-executed on 2026-10-30 (`ATOM-CHANGE-20261030-001`): 541 tests on the working tree, 540 passed, the one failure the uncommitted-WIP seeding race; the committed tree alone is 536/536 on Windows. Of the recorded gaps, the "any status" delete half is asserted by the committed endpoint case `a_claim_can_be_deleted_in_any_status`, four more have uncommitted working-tree cases (executed 2026-10-30, passing), and the concurrency case remains open. | 2026-10-29 |
| Job applications: companies, applications, stages and events | `Verified` | [`../features/job-applications/status.md`](../features/job-applications/status.md) is the single source: migration `0005`, fourteen published operations, 51 endpoint + 11 schema + 2 flow cases and eighteen unit tests, the review history closed by the final-tree read **PASS** (`ATOM-REVIEW-20261029-002`), six gates at `0`. The uncovered halves remain recorded. | 2026-10-29 |
| Local ML (`D4.M1`-`D4.M4`): settings, storage, three operations, degradation | `Verified` | [`../features/ml/status.md`](../features/ml/status.md) is the single source: migration `0006`, three published operations, 41 `ml_api` + 64 unit + 7 configuration cases, `ML-R02` met, the encoder span read clean (`ATOM-REVIEW-20261026-001`, `ATOM-REVIEW-20261029-002`), six gates at `0`. The real-model benchmark was **reproduced on 2026-10-29** from its documented invocation (§5.1a); the recorded limitation is the absence of a model-loading CI case. | 2026-10-29 |
| Local ML spike (`D4.M1`): Burn 0.22 on `flex` | `Verified` | [`../../src/bin/atom-ml-spike.rs`](../../src/bin/atom-ml-spike.rs) and [`../../src/ml/`](../../src/ml/) behind the `ml` Cargo feature — a recorded `--release` run over a real `all-MiniLM-L6-v2` SafeTensors file (vocab 30522, dim 384): construction 96 ms, first inference 0.1933 ms, steady state 0.0198 ms per text at 100 texts, peak RSS 93.5 MiB, bit-identical output for identical input, `Disabled` returns `ml_disabled` without a file, `Deterministic` needs no file. The numbers and both runs are in [`change-log.md`](change-log.md) `ATOM-CHANGE-20261024-010`. This measures the **plumbing**, not retrieval quality: the encoder that supersedes it is `ATOM-CHANGE-20261025-003`. | 2026-10-24 |
| Scoped API keys for machine clients (`K1`) | `Verified` | [`../features/api-keys/status.md`](../features/api-keys/status.md) is the single source for this feature: migration `0007`, three published operations, the surface pinned at 54, 11 integration and 13 unit cases, the quality-gate verdict **PASS** (`ATOM-REVIEW-20261027-001`) and the final independent read of the repaired tree **PASS** (`ATOM-REVIEW-20261029-001`); the six gates were re-run at `0` on 2026-10-29. A key has no scope and the full authority of its owner (`KI-024`). | 2026-10-29 |
| CI pipeline | `Implemented` | [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) - formatting, linting, the test suite, a release build, the documentation check and the OpenCode configuration check. | 2026-10-22 |
| Container image | `Implemented` | [`../../Dockerfile`](../../Dockerfile) builds a two-stage image: pinned base images, `--locked`, no toolchain in the runtime stage, unprivileged UID 10001, no secret inside. [`../../.github/workflows/container.yml`](../../.github/workflows/container.yml) builds and verifies it on every relevant change. **Verified** in run [`40011694239`](https://github.com/g-zenr/atom-api/actions/runs/40011694239): `Config.User=10001:10001`, `uid=10001(atom)`, no `cargo` or `rustc` in the runtime stage, no missing shared library, and the binary exits 1 with `required environment variable DATABASE_URL is missing or empty`, which is the evidence that nothing was baked in. | 2026-10-24 |
| Compile-time SQLx verification | `Proposed` | Queries are checked at run time. The workflow is documented, not enabled. | — |
| Edge rate limiting | `Proposed` | The requirement in [`../project-truth.md`](../project-truth.md) §9.10 applies to a production deployment. | — |
| The rest of the roadmap | `Proposed` | By design, apart from the seeder and the reimbursements schema above. See [`../roadmap.md`](../roadmap.md). | — |
The last full run of the test suite on a Windows host was on 2026-10-29, on the
Phase 2 close-out tree, through `scripts/gate.ps1`: **536 tests passed, 0
failed** - 229 unit, 3 `atom-docs`, 13 `atom-seed`, 11 `api_keys_api`, 29
authentication, 56 claims, 3 claims flow, 10 `claims_schema`, 2
`contract_sweep`, 2 explorer, 51 `job_applications_api`, 2 job application flow,
11 `jobs_schema`, 4 `list_query`, 41 `ml_api`, 12 notes, 11 `openapi_contract`,
25 projects and tasks, 21 `seeder`, 0 shutdown. The split is the one
`cargo test --all-targets --locked` prints. The `quality-gate` session
re-derived the 536-test split independently on 2026-10-29
(`ATOM-REVIEW-20261029-002`), closing the earlier "not re-derived by a second
process" note.
On CI's `ubuntu` runner the same tree is **538**, because the two unix-only
shutdown tests run there. The run covers the current tree, including the claim
CRUD under `ATOM-CHANGE-20261024-006`, items and the lifecycle under `-007`, the
flow and sweep under the `D2.R5` commit, the job-application schema, endpoints
and flow under `D3.J1`-`J5`, the machine-learning settings, schema, three
operations, seeder flag and encoder under `D4.M1`-`M6` and
`ATOM-CHANGE-20261025-003`, and the whole-surface contract and ownership sweeps
under `D5.H1`/`H2`. The configuration module
contributes 58 unit tests in
[`../../src/config/tests.rs`](../../src/config/tests.rs), the shared validators
seven in [`../../src/helpers/validation.rs`](../../src/helpers/validation.rs),
the pagination convention five in
[`../../src/pagination.rs`](../../src/pagination.rs), and the seeder binary
thirteen (seven of its own and the six from `src/ml/hash_vector.rs`, which it
includes by path) in [`../../src/bin/atom-seed.rs`](../../src/bin/atom-seed.rs).
That is why the configuration row above is `Verified` and not `Implemented`.

The checks that were run for the current tree, and their real output, are recorded
in [`change-log.md`](change-log.md) under `ATOM-CHANGE-20261024-011`. The earlier
failure of the formatting and lint gates at the end of 2026-10-21 — on a file
that another agent was editing concurrently — is recorded in
[`change-log.md`](change-log.md) under `ATOM-CHANGE-20261021-001` (status
note) and `ATOM-CHANGE-20261021-002` (checks and results).

## 3. What is not implemented yet

Each item names the requirement it serves, so that the gap is visible against
the accepted scope rather than as an anonymous backlog.

| # | Missing | Requirement it serves | Why it is not done yet | Recorded as |
| - | ------- | ---------------------- | ---------------------- | ----------- |
| 1 | A CI pipeline that runs the three gates on every change | [`../project-truth.md`](../project-truth.md) §13.6, and the open-source contribution goal in §3 | **Verified.** [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) runs the gates on every push and pull request. It has been observed passing on GitHub-hosted runners, on runs `39761383251` and `39765107169`, and it found two real defects in its own first executions. | `KI-005` in [`known-issues.md`](known-issues.md), Closed |
| 2 | ~~A container image for the API~~ | The deployment story in [`../architecture/deployment.md`](../architecture/deployment.md) | **Done.** `Dockerfile` and `.github/workflows/container.yml`; see the feature table above and `KI-006`. | `KI-006` in [`known-issues.md`](known-issues.md), Closed |
| 3 | Compile-time SQLx verification (`cargo sqlx prepare` plus a CI check) | [`../project-truth.md`](../project-truth.md) §12 and the reproducibility requirement | It would make the build depend on a live database, which conflicts with the constraint that the build must not require a hosted or paid service. | `KI-004` in [`known-issues.md`](known-issues.md) |
| 4 | Edge rate limiting in front of the API | [`../project-truth.md`](../project-truth.md) §9.10 | The in-process limiter is a defence-in-depth control. Edge limiting is a deployment concern. | `KI-001` in [`known-issues.md`](known-issues.md) |
| 5 | A proxy-aware client address for the rate-limit key | [`../project-truth.md`](../project-truth.md) §9.11 | The server installs the connection information, so the limiter is keyed by the peer address. There is still no way to declare a trusted proxy, so behind a proxy every caller shares the proxy's address. | `KI-003` in [`known-issues.md`](known-issues.md) |
| 6 | An account-lockout and breach-detection mechanism | Not an accepted requirement; a gap against [`../project-truth.md`](../project-truth.md) §9.5 | It needs its own product rules so that it does not become a denial-of-service vector against a legitimate owner. | `KI-012` in [`known-issues.md`](known-issues.md), accepted risk `AR-6` in [`../security/threat-model.md`](../security/threat-model.md) |
| 7 | A password-change and session-management surface | Not an accepted requirement | The owner can revoke a session, but cannot change a password or list the active sessions. | `Q-08` in [`open-questions.md`](open-questions.md) |
| 8 | An independent review of the Phase 1 change set | [`../testing/verification-policy.md`](../testing/verification-policy.md) §2 | **Done.** `ATOM-REVIEW-20261021-001` in [`review-log.md`](review-log.md) records the review, its eleven findings, their resolutions and the re-review evidence. | [`review-log.md`](review-log.md) |
| 9 | Everything in the roadmap | — | By design. A roadmap item must be accepted before it is built. | [`../roadmap.md`](../roadmap.md) |

## 4. How this document is maintained

- It is updated in the same change as the code it describes, never later.
- A status changes only when the evidence changes. `Implemented` becomes
  `Verified` when a passing check is recorded, not when the code is written.
- It must agree with the `status.md` of every feature. When they disagree, the
  discrepancy is recorded in [`known-issues.md`](known-issues.md) and resolved,
  not averaged.
- **Single sourcing.** A feature's status and evidence are recorded **once**,
  in its feature card. A row here carries the status word, the pointer and a
  date — not a restatement of the card's evidence. The index documents
  ([`../README.md`](../README.md), [`../features/README.md`](../features/README.md),
  [`../api/README.md`](../api/README.md)) link the card likewise. When a status
  changes, the card changes and the rows that point at it change their status
  word and date; no evidence paragraph is copied anywhere. This rule exists
  because the same evidence, restated in nine places, went stale three times
  during the `K1` review.
- The evidence column names a file or a recorded check. "It works" is not
  evidence.

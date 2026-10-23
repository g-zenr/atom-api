# Phase 2 run — ledger

> **Status:** In progress (run state)
> **Last updated:** 2026-10-24 03:15 (UTC+8)
> **Owner:** maintained by the orchestrator

This file is the only source of truth for resuming the run. It is updated after
every chain, in the same commit as the chain. A chat transcript is not a
resume point; this file is.

---

## Owner decisions (2026-10-24, before the first run)

Recorded so the run never has to ask again. These override the defaults in the
plan and the prompt where they conflict; they do not change `AGENTS.md`.

| # | Decision | Consequence |
| - | -------- | ----------- |
| O1 | Assumptions A1–A6 in [plan.md](plan.md) §3 were **not explicitly confirmed**; the owner said to proceed and see what can be finished. | They are recorded as **provisional defaults** and marked as such in every document Day 1 writes. Day 1 must not present them as owner-approved. If the owner revises one, the affected spec is amended in a new change, not silently. |
| O2 | Git: **push directly to `develop`** for now, no pull requests. | Each day works on a day branch for rollback points, then merges into `develop` and pushes at the checkpoint. Never force-push. CI runs on the push to `develop`. The deviation from `docs/engineering/git-workflow.md` is temporary and recorded in the change log. |
| O3 | Range: **as far as it gets**, through Day 5. | Chains that do not fit are parked with evidence. Quality gates are not traded for distance. |
| O4 | Disk cleanup authorised. | Build artifacts and unused Docker images may be deleted. Current free space after cleanup: see Evidence. |
| O5 | The concurrent session was **not** identified by the owner. | The runner never kills it. It waits for ten quiet minutes, and aborts if the folder is still busy after sixty. |
| O6 | **Tech stack: Rust only.** The owner does not want another language runtime. | ML inference must be pure Rust. **Burn is the chosen framework** (owner, 2026-10-24). No Ollama, no Python sidecar, no external model server. Model weights are local files, not a service. PowerShell, `gh` and Docker remain developer tooling. |
| O7 | **n8n is a later integration**, not Phase 2. | A separate Node service that calls the API; the API never depends on it and it never sits inside the data path of a private record. It stays `Proposed` in `docs/roadmap.md`. It does not violate O6 because it is not part of the API runtime. |
| O8 | **Full authority granted** (2026-10-24, before sleep): proceed without approval, API only, no frontend, attempt every day. | The runner passes `--auto`. The agent must not pause for confirmation, and must not ask for permission for a tool it already has. Full authority is not permission to fake a result: the anti-cheat rules and the stop conditions still hold. A day that cannot be finished is parked with evidence; the runner then stops rather than build the next day on a broken base. |

---

## Baseline

Recorded on 2026-10-24 at 03:15, before Day 1. Re-verify before trusting it.

**Day 1 preflight, 2026-10-24 (this session).** The previous session ran the
Day 0 preflight, locked contract `C-D0` and stopped before its fan-out; its
work was uncommitted. This session re-ran every check rather than trusting it.

| Fact | Value |
| ---- | ----- |
| HEAD | `eb33f3e`, branch `develop`, CI and Container green on that push (runs `40021296191`, `40021296057`) |
| Writers | no `cargo`, `rustc`, `python` or `atom-api` process running |
| Tree | `M config/README.md` (CRLF dirt, no content change), `M docs/engineering/known-issues.md`, `M docs/engineering/runs/phase-2/ledger.md`, `?? docs/audit/handoff.md` |
| Database | `atom_test` answers `select 1`; `TEST_DATABASE_URL` set for the run |
| Disk | 23.54 GB free |
| `gh` | authenticated as `g-zenr`, `repo` and `workflow` scopes |
| Docker | local daemon still unusable; CI's Container job is the only image evidence |
| **Day 0** | **NOT closed.** The `C-D0` code is absent: no `API_PREFIX`, no `ROUTE_TABLE`, no `NotesListQuery`, no `tests/openapi_contract.rs`, no `tests/list_query.rs`, no value-comparison tests, and `config/README.md` still says `100`. |

Baseline gates, measured this session on the inherited tree:

| Gate | Result |
| ---- | ------ |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | exit 0 |
| `cargo test --all-targets --locked` | exit 0 — **179 tests** (111 lib, 29 auth, 2 docs_route, 12 notes, 25 projects/tasks; `shutdown` is unix-only and 0 on Windows) |
| `cargo check --no-default-features --all-targets --locked` | exit 0 |
| `atom-docs` / `--strict` | exit 0 / exit 0, 0 warnings |

Disposition of the inherited uncommitted work:

| Item | What it is | Disposition |
| ---- | ---------- | ----------- |
| `config/README.md` | CRLF dirt only; its `100` is the drift `C-D0` must fix | Keep; `rust-backend` makes the one content edit, without rewriting the whole file's line endings |
| `docs/engineering/known-issues.md` | Closes `KI-006` with the Container run `40019454167` as evidence; verified true against `gh run list` | Keep and commit with the Day 0 chain |
| `docs/engineering/runs/phase-2/ledger.md` | `C-D0` plus the previous session's preflight record | The contract is kept (verified against the code); the preflight record is replaced by this one |
| `docs/audit/handoff.md` | Untracked, stale (claims nothing is merged while #1–#5 are), mis-encoded | Deleted, as `C-D0` assigns; the ledger and the change log supersede it |

**§0.3 ruling.** `DAY=1` requires Day 0 to be closed and green. It is not, so
this session runs the `D0` chain first, from `C-D0`, and only then the Day 1
product intake.

| Fact | Value |
| ---- | ----- |
| HEAD | `6e8a0c4` — `origin/develop` was the same commit at the time |
| Branch | `chore/post-remediation-followups` |
| Phase 1 backlog | 83 of 83 findings accounted for: 78 `verified`, 5 `deferred` (`F-23`, `F-44`, `F-45`, `F-46`, `F-49`), 11 retained |
| Last change-log entry | `ATOM-CHANGE-20261023-001` |
| Last full test run | 2026-10-22: 177 tests (111 unit, 29 auth, 12 notes, 25 projects/tasks) |
| PostgreSQL | 18, accepting connections on 127.0.0.1:5432; `atom_test` exists |
| `gh` | authenticated as `g-zenr`, scopes include `repo` and `workflow` |
| Docker | 29.2.1 available |
| Rust | 1.92.0 |
| Disk | 5.7 GB free on `C:` at the time — keep above 3 GB before a release build |
| pgvector | extension `vector` 0.8.1 is available, but the `atom` role is **not** a superuser and `CREATE EXTENSION vector` is denied locally |

Gates at the time of the snapshot, on a tree another session was editing:

| Gate | Result |
| ---- | ------ |
| `cargo check --no-default-features --all-targets --locked` | exit 0 |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 1 — only `tests/shutdown.rs`, unformatted |
| `cargo test --all-targets --locked` | not run: the tree was in flux |
| `atom-docs` / `--strict` | not run at the snapshot |

---

## Queue

Scope addition, 2026-10-24: a complete database seeder (`atom-seed`) is part of
Phase 2. It is specified in [plan.md](plan.md) §8 and tracked by the S-rows
below. Every chain that adds a table extends it in the same chain.

| Item | Wave | State | Commit | Notes |
| ---- | ---- | ----- | ------ | ----- |
| D0 | Close Phase 1 | **in progress** | `71600cb` | PR #5 merged the `docs` feature, the container image, the shutdown test, `020` and `027` names-only. Remaining in this run: `F-23` (021), `F-49` (023), the value comparison the plan's item 4 asks for, the `config/README.md` drift and dirt, and the fate of `docs/audit/handoff.md`. Contract: `C-D0`. |
| D1 | Product intake | pending | — | Documents only. Blocks D2. Specifies the seeder in `docs/database/seeding.md`. |
| D2.S1 | Seeder foundation | pending | — | `atom-seed` for users, notes, projects and tasks. Guards, determinism, `--reset`, tests, docs. No new dependency. |
| D2.R1 | Reimbursements — schema and types | pending | — | |
| D2.R2 | Claim CRUD | pending | — | |
| D2.R3 | Items and the total invariant | pending | — | |
| D2.R4 | Lifecycle actions | pending | — | |
| D2.R5 | Flow and contract tests | pending | — | Uses the second seeded account for the ownership sweep. |
| D2.R6 | Close: review, gate, commit | pending | — | Also D2.S2: extend the seeder with claims. |
| D3 | Job applications | pending | — | Blocked by D2. |
| D3.S3 | Seeder: companies and applications | pending | — | Lands with the Day 3 close. |
| D4 | Local ML | pending | — | Blocked by D3; needs the embedding provider decision from Day 1. |
| D4.S4 | Seeder: embeddings | pending | — | `--with-embeddings` uses the deterministic embedder, never a network call. |
| D5 | Hardening and handover | pending | — | Blocked by D4. |
| D5.S5 | Seeder: CI smoke and final proof | pending | — | Runs the seeder against the CI database and asserts the counts. |

A day is one paste of [prompt.md](prompt.md). A chain is one row above.

---

## Contracts

Locked contracts are appended here, newest last, and pasted verbatim into every
agent brief for that chain. Empty until D2.R1.

### C-D0 — Close Phase 1 (locked 2026-10-24, before the D0 fan-out)

Scope: `ATOM-TASK-20261022-021` (`F-23`), `ATOM-TASK-20261022-023` (`F-49`),
[plan.md](plan.md) §7 item 4 (compare setting **values**, not only names) and
item 1 (reconcile the tree). No product behaviour changes.

**ENDPOINTS**

- No endpoint is added, removed, renamed, re-statused or re-badied. The 22
  operations currently in `/api/v1/openapi.json` must all still be there, with
  the same paths, methods, status codes, request bodies and response schemas.
- Query parameter sets, asserted **exactly** (no duplicate, no extra):
  - `GET /api/v1/notes` → `limit`, `offset`, `search`, `archived`
  - `GET /api/v1/projects` → `limit`, `offset`, `search`
  - `GET /api/v1/projects/{project_id}/tasks` → `limit`, `offset`, `status`,
    `priority` (plus the `project_id` **path** parameter)
- `GET /health/live` and `GET /health/ready` are outside the version prefix and
  remain part of the document.

**TYPES**

1. `atom_api::api::routes::API_PREFIX: &str = "/api/v1"` — the single
   declaration of the version prefix.
2. `atom_api::api::routes::ROUTE_TABLE: &[(&str, &str)]` — `(METHOD in
   uppercase, path relative to API_PREFIX in axum syntax)`, ordered as the
   router declares them, e.g. `("GET", "/notes/{note_id}")`.
3. `atom_api::api::routes::UNVERSIONED_ROUTE_TABLE: &[(&str, &str)]` —
   `[("GET", "/health/live"), ("GET", "/health/ready")]`.
4. One query type per list endpoint that deserializes the whole query string
   **once** and exposes the existing pair: `NotesListQuery` in `notes::dto`
   with `limit: Option<i64>`, `offset: Option<i64>`, `search: Option<String>`,
   `archived: Option<bool>`, plus accessors returning `NoteQuery` and
   `PaginationQuery`. The same for `ProjectsListQuery` and `TasksListQuery`.
   `#[serde(flatten)]` must **not** be used: axum's `Query` deserializes with
   `serde_urlencoded`, whose map access yields strings, so a flattened
   `Option<i64>` fails at run time. The explicit fields are the price of one
   deserialization and the module doc says so.
5. No new dependency. No new schema in the OpenAPI components. No new setting.

**SQL**

None. No migration, no statement, no index. The seeder does not exist yet.

**INVARIANTS**

- I1 The route table and the document describe the same operations, in both
  directions.
- I2 `/api/v1` is declared once, in `API_PREFIX`. The document's paths are
  built from it; the prefix is not typed again anywhere in the agreement.
- I3 Each list handler extracts the query string exactly once (one
  `Query<...>` extractor per handler).
- I4 PATCH semantics, status codes, error envelopes, ownership (`404` for a
  foreign resource), pagination behaviour and clamping are unchanged.
- I5 The settings reference check compares **integer and boolean** defaults
  between each configuration file and the reference table, and reports a
  mismatch. Sizes, arrays and prose are not compared; the reason is in the
  code comment.
- I6 Nothing is weakened: no test deleted, renamed or ignored, no `allow`, no
  `unwrap`/`expect` in a request path, no relaxed constraint.

**TESTS** (names fixed by this contract)

`tests/openapi_contract.rs` (new):

- `the_route_table_and_the_document_describe_the_same_operations`
- `every_route_in_the_table_is_served_by_the_router`
- `the_version_prefix_is_declared_once`
- `each_list_endpoint_publishes_its_filter_and_pagination_parameters_once`

`tests/list_query.rs` (new):

- `a_note_list_parses_filters_and_pagination_from_one_query_string`
- `a_project_list_parses_filters_and_pagination_from_one_query_string`
- `a_task_list_parses_filters_and_pagination_from_one_query_string`
- `a_malformed_list_parameter_is_a_bad_request`

`src/bin/atom-docs.rs` unit tests:

- `a_numeric_default_that_differs_from_the_file_is_reported`
- `a_numeric_default_that_matches_is_not_reported`
- `a_boolean_default_that_differs_is_reported`

**FILES** (exclusive)

| Owner | Files |
| ----- | ----- |
| `rust-backend` | `src/api/routes.rs`, `src/notes/handlers.rs`, `src/notes/dto.rs`, `src/projects/handlers.rs`, `src/projects/dto.rs`, `src/tasks/handlers.rs`, `src/tasks/dto.rs`, `src/bin/atom-docs.rs`, `config/README.md` |
| `test-engineer` | `tests/openapi_contract.rs`, `tests/list_query.rs`, `tests/common/mod.rs` (only if a helper is genuinely missing) |
| `docs` | `docs/api/README.md`, `docs/api/conventions.md`, `docs/project-truth.md` |
| `devops` | `Dockerfile`, `scripts/run-phase-2-overnight.ps1`, `.github/**` |
| orchestrator | `docs/engineering/**`, `docs/README.md`, `docs/audit/**` (including the deletion of `docs/audit/handoff.md`) |

**SEEDER**

Not applicable: no table is added or changed, and the seeder is a Day 2 chain.

---

## Evidence

Only commands actually run, with their real result.

| Date | Command | Result |
| ---- | ------- | ------ |
| 2026-10-24 03:03 | `cargo check --no-default-features --all-targets --locked` | exit 0 |
| 2026-10-24 03:03 | `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| 2026-10-24 03:03 | `cargo fmt --all -- --check` | exit 1, `tests/shutdown.rs` |
| 2026-10-24 03:09 | `psql -c "create extension vector"` as `atom` | `permission denied` — must be superuser |
| 2026-10-24 03:09 | `pg_available_extensions` | `vector 0.8.1`, `pg_trgm 1.6`, `pgcrypto 1.4`, `unaccent 1.1` |
| 2026-10-24 03:11 | `docker --version`, `gh auth status`, `pg_isready` | 29.2.1; logged in as `g-zenr`; accepting connections |

| 2026-10-24 03:58 | `git log --oneline` | PR #5 merged as `71600cb`: the `docs` feature, the container image, the shutdown test, `020`, `027`. `develop` was two run-folder commits ahead of origin. |
| 2026-10-24 03:53 | `cargo run --bin atom-docs` | exit 0, 0 warnings; `--strict` exit 0, 0 warnings |
| 2026-10-24 03:52 | `python` UTF-8 check of the run folder | valid, 0 replacement characters |
| 2026-10-24 03:52 | disk cleanup | removed incremental, PDB and release artifacts: 1.3 GB free to 9.27 GB |

Day 0 preflight, this run (2026-10-24, after 04:38):

| Command | Result |
| ------- | ------ |
| `Get-Process cargo,rustc,python,atom-api` | no output — no writer active |
| `git status --short` | `M Dockerfile`, `M config/README.md`, `M scripts/run-phase-2-overnight.ps1`, `?? docs/audit/handoff.md` |
| `git log --oneline -5`, `git branch --show-current` | HEAD `a0e7144`, branch `develop` |
| `psql -h 127.0.0.1 -U atom -d atom_test -c "select 1"` | `1` — exit 0 |
| `Get-PSDrive C` | 23.8 GB free |
| `gh auth status` | logged in as `g-zenr`, scopes include `repo` and `workflow` |
| `gh run list --limit 6` | CI and Container green on the last three `develop` pushes |
| `docker info` | `500 Internal Server Error` on `/info` — no usable daemon locally |

**Day 2 preflight, 2026-10-24 (this session, DAY=2).** Re-ran every check; did
not trust the two records above.

| Fact | Value |
| ---- | ----- |
| HEAD | `eb33f3e`, branch `develop`; CI `40021296191` and Container `40021296057` green on it |
| Writers | `Get-Process cargo,rustc,python,atom-api` → no output. No writer active. |
| Tree | `M config/README.md` (CRLF dirt, empty content diff), `M docs/engineering/known-issues.md`, `M docs/engineering/runs/phase-2/ledger.md` |
| Database | `psql -h 127.0.0.1 -U atom -d atom_test -c "select 1"` → `1`, exit 0; `TEST_DATABASE_URL` set for the run |
| Disk | 20.9 GB free — above the 3 GB floor |
| `gh` | authenticated as `g-zenr`, `repo` and `workflow` scopes |
| Docker | local daemon still unusable; CI's Container job remains the only image evidence |

**§0.3 ruling for DAY=2.** This check **fails**. Day 0 is still not closed and
Day 1 is still `pending`; the two records above say so, and the code confirms
it. Verified against the tree rather than the ledger:

| `C-D0` artefact | Present? | Evidence |
| --------------- | -------- | -------- |
| `API_PREFIX`, `ROUTE_TABLE`, `UNVERSIONED_ROUTE_TABLE` | **no** | `Select-String src\api\routes.rs` → no match; routes are bare literals and `/api/v1` is typed three times in `swagger_ui` |
| `NotesListQuery` / `ProjectsListQuery` / `TasksListQuery` | **no** | each list handler still takes two `Query<...>` extractors (`notes/handlers.rs:38-39`, `projects:37-38`, `tasks:36-37`) |
| value comparison in `atom-docs` | **no** | `check_settings_reference` compares key **sets** only |
| `tests/openapi_contract.rs`, `tests/list_query.rs` | **no** | `tests/` holds `auth_api`, `docs_route`, `notes_api`, `projects_tasks_api`, `shutdown` only |
| `config/README.md` drift | **still there** | README `100`, `config/app.toml` `10` |
| `docs/database/seeding.md` (Day 1) | **no** | `docs/database/` holds `migration-policy.md`, `README.md`, `schema.md` |
| `migrations/0004*` | **no** | only `0001`-`0003` exist |

The prompt forbids building Phase 2 on an unverified base. The remedy is the
one the plan already provides: the queue lists `D0` and `D1` as the pending
chains ahead of `D2.S1`. This session therefore runs the queue **in order** —
`D0` from the locked `C-D0`, then `D1`, then the Day 2 queue (`S1`, `R1`…) —
and records the deviation here instead of silently skipping the base. `D0`
and `D1` are not new scope; they are the preconditions the brief assumes.

**Disposition of the inherited uncommitted work.** Read, verified against the
tree and `gh run list`, then committed deliberately rather than discarded:

| Item | Verified? | Disposition |
| ---- | --------- | ----------- |
| `docs/engineering/known-issues.md` | Yes — `40019454167` (Container) and `40021296057` are green in `gh run list`, so the five probes did execute | Committed; the `KI-006` closure is true |
| `docs/engineering/runs/phase-2/ledger.md` | Its `C-D0` block was re-checked against the code and stands; its preflight record is correct for its own session | Kept and superseded by this record |
| `config/README.md` | Empty content diff — CRLF only | Left uncommitted until `rust-backend` makes the one-line `100` → `10` edit, so the diff stays one line |

---

## Failures

One line per repair, so the next chain does not repeat it.

```
FAILURE: <command> -> <error> | ROOT CAUSE: <one line> | FIX: <one line> | TEST: <that now passes>
```

Empty. Nothing has been repaired by this run yet.

---

## Checkpoints

One scorecard table per chain, per the prompt's §7. Empty.

---

## Parked

| Item | Blocker | Next step |
| ---- | ------- | --------- |
| pgvector | The `atom` role is not a superuser locally; CI's `atom` is. A migration with `CREATE EXTENSION` would pass CI and fail on this machine. | Default design avoids the extension (see [plan.md](plan.md) §6). If pgvector is wanted, run `CREATE EXTENSION vector` as `postgres`, document it as a deployment prerequisite, and add a preflight check. |
| Assumptions A1–A6 | They are product decisions, not engineering ones. | Confirm or correct them before Day 1 writes acceptance criteria. Unanswered assumptions become open questions and their behaviour is excluded from the specs. |
| `docs/audit/handoff.md` | Untracked, and stale: it says no pull request has been merged, while #1–#4 are. | Day 0 either updates and commits it or deletes it. |

---

## Resume

An overnight run was launched on 2026-10-24 to execute Days 0-5, stopping at the
first day that cannot be finished honestly. Its log is under `target/overnight/`
and the STOP sentinel is `STOP-RUN` in the repository root.

If the run stopped early, read `## Queue` and `## Parked` first, then paste
[prompt.md](prompt.md) with the first day whose state is not `done`. If it
reached Day 1, review the specifications before Day 2: A1-A6 are provisional
(owner decision O1) and the specs must say so. Day 2 starts with **S1**, the
seeder foundation, then **R1** from the locked contract in [plan.md](plan.md) §5.

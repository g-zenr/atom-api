# The chain prompt

> **Status:** Accepted (run procedure)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the orchestrator

Paste the block below into a fresh session started in the repository root.
Replace `PHASE=<n>` with the phase you are running. One paste is one phase: the run
stops at the phase's exit criteria or at a stop condition, never by guessing.

| Parameter | Value |
| --------- | ----- |
| `PHASE` | the phase number from [plan.md](plan.md) §7 |
| Plan | `docs/engineering/runs/chain/plan.md` |
| Ledger | `docs/engineering/runs/chain/ledger.md` |
| Queue | section 3 of this file — the complete queue for every phase |

If a run stopped early, do not re-paste blindly. Read the ledger's `Resume`
section first, then continue from the first queue item that is not `done`.

````markdown
# ATOM PHASE 2 — SELF-HEALING AUTONOMOUS RUN (RUN PHASE=<n>)

You are the orchestrator of the ATOM engineering team. This run executes one phase
of the Phase 2 plan. Every chain is built, tested, documented, verified and
checkpointed before the next one starts. You have 8 hours.

Repository: C:\Users\Renz\Documents\automation\atom\api
Plan:       docs/engineering/runs/chain/plan.md
Ledger:     docs/engineering/runs/chain/ledger.md

---

## 0. Preflight (30 minutes, blocking)

1. Read: AGENTS.md, docs/README.md, docs/project-truth.md, the plan, the ledger,
   the newest change-log entry, known-issues, open-questions, git-workflow.
   Read the reference standards in AGENTS.md §4.12 before writing Rust or ML
   code. Load the atom-delivery and atom-docs skills.
2. Prove no other writer is active:
   Get-Process cargo,rustc,python,atom-api -ErrorAction SilentlyContinue
   git status --short
   If a build or an agent process is running: STOP, never race another session.
   The runner already waited for the tree to be quiet before this phase started.
   Uncommitted changes are the previous session's work: read them, verify them,
   then commit them or revert them deliberately. Never discard another author's
   work.
3. If PHASE is greater than 0, confirm Phase 0 is closed and green. PHASE=0 is the
   close-out itself. Never build Phase 2 on an unverified base.
4. Git policy: read the `## Owner decisions` block in the ledger and follow it.
   Default policy is a phase branch and a draft pull request. The override
   recorded for 2026-10-24 is: a phase branch for rollback points, merged into
   `develop` and pushed at each phase checkpoint, with no pull request and no
   force push.
5. Database:
   $env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
   psql -h 127.0.0.1 -U atom -d atom_test -c "select 1"   (PGPASSWORD=atom)
6. Disk above 3 GB, else remove target/**/*.pdb. Re-check before every release
   build and before spawning a parallel wave.
7. Make CI run. Under the direct-to-develop override the push to `develop`
   triggers the workflow; confirm with `gh run list --limit 3`. Under the
   default policy, open a draft pull request to `develop` so CI runs on every
   push. The Linux runner is the only place the shutdown test can execute.
8. Write the preflight result into the ledger. If a check fails, stop and report.

## 1. Non-negotiable rules

Follow AGENTS.md. The rules this run will be tempted to break:

- Ownership is in SQL: every table carries `user_id`, every statement says
  `WHERE user_id = $1`, an unowned resource is `404` and never `403`.
- Money is `NUMERIC(19,4)` in the database and a JSON string. Never a float.
- Migrations are append-only. One new migration per change.
- Never `.unwrap()`/`.expect()` in a request path; never concatenate SQL; never
  log a secret; never suppress a lint.
- No new dependency without a recorded, approved reason. A new setting means
  seven places in one change (file, typed field, loader, validation,
  .env.example, deployment reference, tests).
- A public contract change means OpenAPI, docs/api/ and the feature documents.
- ML is local only and **Rust only**. No outbound call to a hosted or paid
  service, and no second language runtime: no Python, no Node, no Go, no
  external model server such as Ollama. **Burn is the chosen framework.**
  Model weights are local files. n8n is a later external integration, never
  part of the API runtime.
- The seeder is destructive. It refuses production, refuses a database whose
  name lacks dev/local/test, refuses a database that holds foreign rows, and
  never prints the connection string or a hash.
- Never mark anything Verified without a recorded passing check, and never
  report a result you did not observe.

### Anti-cheat (forbidden "repairs")

You may never make a failure go away by:

- weakening, deleting, renaming or `#[ignore]`-ing a test;
- adding `#[allow(...)]`, `#![allow(...)]`, `-A warnings`, or dropping
  `-D warnings`;
- adding `unwrap`/`expect`/`unsafe` to a request path;
- relaxing a validation rule, a CHECK constraint or the seeder's guard;
- changing a published response shape, status code or path to satisfy a test;
- committing with `--no-verify`, or editing CI to skip a step;
- marking a gate "passed" when it did not run.

A failure is fixed in the code, or it is parked with evidence. There is no
third option.

## 2. The fan-out: contract lock, four agents, then integration

### Step 1 — Contract lock (serial, 20–30 minutes, architect + you)

Before any code, produce one contract block and paste it verbatim into every
agent brief:

  ENDPOINTS   method, path, auth, request DTO, response DTO, status codes, errors
  TYPES       Rust structs and enums, with serde behaviour
  SQL         tables, columns, types, constraints, indexes, ownership predicate
  INVARIANTS  the rules that must hold across endpoints
  TESTS       the acceptance test names, one per rule
  FILES       the exclusive file list of each agent
  SEEDER      the rows this chain adds to the seeder

Write it into the ledger. If the contract is wrong, stop the fan-out, fix the
contract, restart the affected agents. An agent that contradicts the contract is
wrong even when it compiles.

### Step 2 — Four agents in parallel

Launch them in ONE message with background=true. Each brief states: objective,
the contract block, exclusive files, acceptance criteria, its own verification
command, edit permission, and the prohibition on touching another agent's files.

  | Agent         | Owns exclusively | Must not touch |
  | database      | migrations/**, docs/database/** | src/, tests/, docs/features/ |
  | rust-backend  | src/**, config/**, .env.example, config/README.md, Cargo.toml | tests/, migrations/, docs/ |
  | test-engineer | tests/** | src/, migrations/, docs/ |
  | docs          | docs/features/**, docs/api/** | src/, tests/, migrations/, docs/database/ |
  | auth-security | one named tests/<feature>_security.rs | everything else (read-only elsewhere) |
  | devops        | .github/**, Dockerfile, .dockerignore | everything else |
  | orchestrator  | docs/engineering/**, docs/README.md | src/, tests/, migrations/ |

Rules of the fan-out:

- Two agents never share a file. If two briefs would, merge them into one agent.
- Only rust-backend and test-engineer run cargo. The build lock is shared, so
  test-engineer starts its first check after rust-backend's first green compile.
  Lock waits are expected, not an error.
- database and docs run no cargo, so they never contend for the lock.
- If an agent returns without doing the work, read its diff, narrow the brief,
  re-delegate. Never re-run it blindly.
- If a specialist is blocked on something the contract already answers, the
  contract wins. If the contract is silent, you decide and record it.

### Step 3 — Integration (serial, yours)

Read every diff yourself. You own the merge, the gates and the commit. No
agent's "done" is evidence.

## 3. The queue

This section is the complete queue. Run only the block for `PHASE=<n>`. A phase is
complete only when every chain in its block has a checkpoint and a commit. The
detailed contracts, data model and acceptance criteria are in `plan.md`; this is
the executable order.

Two labels, deliberately kept apart: these are **run phases**, not product
phases. The product's Phase 1 is the backend foundation closed by `PHASE=0`, and
the product's Phase 2 is the workspace accepted by `PHASE=1`. The ledger and the
harness label their queue rows `D0`-`D5`; `D<N>` is the same run phase as
`PHASE=<N>`.

### PHASE=0 — close Phase 1

1. **Reconcile the tree.** `cargo fmt --all`. Fix the `config/README.md`
   `shutdown_grace_seconds` drift (`100` -> `10`). Decide `docs/audit/handoff.md`
   (update or delete). Reconcile the OpenAPI explorer status between
   `project-truth.md` and `implementation-status.md`.
2. **F-44 / ATOM-TASK-022.** Prove the `docs` feature: `cargo tree
   --no-default-features` has no `utoipa-swagger-ui`; measure the release binary
   both ways; record the sizes; document the build flag; update the backlog.
3. **Shutdown test.** `tests/shutdown.rs` must fail against the old defect and
   pass after it; get the Linux CI run that executes it.
4. **ATOM-TASK-027.** Compare setting values, not only names; prove it catches
   the `100`/`10` drift; no false failures.
5. **ATOM-TASK-021.** Make the OpenAPI document and the route table agree by
   construction, or add a contract test that fails when they differ.
6. **ATOM-TASK-023.** Parse the list query string once without changing the
   OpenAPI parameters.
7. **ATOM-TASK-020.** Serve the profile from the row the auth extractor already
   loaded; keep the per-request account-status check.
8. **KI-006.** The container image, built and probed in CI.
9. **Close.** Six gates green; commit; push; tag `phase-1-complete`.

### PHASE=1 — product intake (documents only)

1. `docs/database/seeding.md`: the seeder specification (profiles, guards,
   determinism, `--reset`, invariants, tests).
2. `project-truth.md`: accept `REIMB-R01..`, `JOB-R01..`, `ML-R01..` with
   acceptance criteria; amend section 6 so local inference is permitted and
   outbound AI calls stay excluded.
3. `roadmap.md`: define Phase 2 (answers Q-11).
4. Three feature specifications under `docs/features/reimbursements/`,
   `job-applications/` and `ml/` - all five documents each.
5. ADR-0007 money (`NUMERIC(19,4)`, JSON string); ADR-0008 local pure-Rust ML
   with Burn and the `flex` backend; ADR-0009 receipt references, not uploads.
6. `docs/database/schema.md`: the proposed tables and invariants.
7. `docs/api/conventions.md`: money, filtering, search modes.
8. `docs/testing/strategy.md`: the Phase 2 matrix and the >=150 test target.
9. Record A1-A6 as provisional (owner decision O1) in every document they touch.
   Exit: `atom-docs --strict` green, architect review, **zero** `src/`,
   `migrations/` or `tests/` changes.

### PHASE=2 — seeder foundation, then reimbursements

1. **S1 Seeder foundation.** `atom-seed` for users, notes, projects and tasks:
   guards (production, database name, foreign rows), deterministic ids and
   timestamps, `--reset`, post-insert verification, `tests/seeder.rs`,
   `docs/database/seeding.md`. No new dependency.
2. **R1 Schema.** Migration `0004`: `expense_claims`, `expense_items`, enums,
   CHECKs, indexes, ownership columns.
3. **R2 Claim CRUD.** List/create/get/update/delete, ownership, pagination,
   filters by status and date.
4. **R3 Items and the total.** One transaction; recompute the total; reject a
   currency mismatch; exact decimals.
5. **R4 Lifecycle.** submit / approve / reject / mark-paid; guard every
   transition; `decided_at` and `paid_at`.
6. **R5 Flow and contract.** End-to-end journey test; OpenAPI path and method
   assertions; ownership sweep with the second seeded account.
7. **R6 Close.** Extend the seeder with claims (S2); finalise the docs; review;
   gate; commit.

### PHASE=3 — job applications

1. **J1 Schema.** Migration `0005`: `companies`, `job_applications`,
   `application_events`, enums, constraints, indexes.
2. **J2 Companies CRUD**, owner-scoped, unique name per owner.
3. **J3 Applications CRUD** and the stage machine (`wishlist`, `applied`,
   `screening`, `interview`, `offer`, `rejected`, `withdrawn`; `offer` terminal).
4. **J4 Events CRUD**, ordered by `occurred_at`, cascading with the application.
5. **J5 Flow and contract.** Journey test, OpenAPI assertions, ownership sweep.
6. **J6 Close.** Extend the seeder with companies and applications (S3); docs;
   review; gate; commit.

### PHASE=4 — local machine learning

1. **M1 Spike (timeboxed).** Burn v0.22 with the `flex` backend; load a small
   sentence-embedding model and a tokenizer; measure latency and memory; record
   the numbers before any endpoint depends on it.
2. **M2 ADR-0008 finalised** with the measured result. If the spike fails, ML
   ships as `Disabled` and lexical search is the behaviour.
3. **M3 Schema and module.** Migration `0006`: `embeddings` (`REAL[]`, unique
   key, ownership index). `src/ml/`: `Embedder` trait, Burn and deterministic
   implementations, hybrid rank.
4. **M4 Endpoints.** `POST /notes/search`, `POST /job-applications/{id}/match`,
   `POST /ml/reindex`; all owner-scoped.
5. **M5 Degradation.** ML off or the model missing returns lexical results with
   `200`, never `500`; test it. Tests use the deterministic embedder; CI never
   downloads a model.
6. **M6 Evaluation and close.** 15 fixture queries with expected notes; record
   the pass/fail; extend the seeder with embeddings (S4); docs; review; gate.

### PHASE=5 — hardening and handover

1. **H1 Contract tests** over every path and method, Phase 1 included.
2. **H2 Ownership sweep** for every new endpoint with a second account.
3. **H3 Performance.** 20,000 rows, `ANALYZE`, generic plan; record before and
   after for the new list and search queries.
4. **H4 Frontend handover.** `docs/api/frontend-handover.md`: auth flow, error
   contract, pagination, money format, ML endpoints and degradation, versioning.
5. **H5 Final review and quality gate** over the whole Phase 2 diff.
6. **H6 Close.** Tag `phase-2-complete`; update roadmap, implementation status,
   known issues, open questions, change log; seeder CI smoke (S5).

Each numbered entry is one chain. Do not start the next chain until the
checkpoint says ADVANCE.

**Seeder rule, every chain:** a chain that adds or changes a table extends the
seeder in the same chain, and a chain that adds an entity adds its rows to the
`dev` profile. A feature without seed data is not complete.

## 4. The chain loop

For every queue item:

1. LEDGER: item to `in progress`, with the contract reference.
2. CONTRACT: §2 step 1. If a contract already exists, verify it against the code
   and amend only with a recorded reason.
3. TESTS FIRST: test-engineer writes the failing tests from the contract, before
   or while rust-backend implements. A chain with no failing test at the start
   has no proof that it does anything.
4. BUILD: fan out per §2. Wait for all four, collect claimed results and the
   files each agent actually touched.
5. INTEGRATE: read the diffs, resolve conflicts yourself, make the tree compile.
6. VERIFY: run the gates (§5), the chain's tests and the feature's flow tests.
7. REPAIR: if anything is red, run the ladder (§6) and return to 6.
8. CHECKPOINT: scorecard (§7). Only ADVANCE authorises the next chain.
9. COMMIT: one Conventional Commit per chain, body naming the requirement ids
   and the tests that prove them. Update the ledger in the same commit.

If a chain exceeds its budget by 50 percent: PARK it (§7), write the blocker
with the exact error, and move on. One chain never consumes the phase.

**Roll-forward.** When the phase's queue is complete, every checkpoint says
ADVANCE, the tree is committed, and at least 90 minutes of the eight-hour budget
remain: continue straight into the next phase's queue in this same session.
Record `phase N complete, rolling into phase N+1` in the ledger, do not repeat the
preflight, and do repeat the gates. A phase with a parked or failing chain never
rolls forward. This is the only way a night gets past one phase; use it.

## 5. Gates (run them yourself, capture exit codes)

$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
cargo check --no-default-features --all-targets --locked
cargo run --quiet --bin atom-docs
cargo run --quiet --bin atom-docs -- --strict

Capture explicitly: `$out = <command> 2>&1; $code = $LASTEXITCODE`. PowerShell
turns cargo's stderr progress into error records and can report a false failure.
Trust `$LASTEXITCODE`.

Fast loop while a chain is red, never the full suite:

  cargo test --lib
  cargo test --test <feature>
  cargo test --test <feature> <name>

The full `cargo test --all-targets` runs at the checkpoint and at phase end.

## 6. The self-healing ladder

Classify the failure, then apply exactly one rung.

  | Class      | Signal                         | Rung 1                   | Rung 2                          | Rung 3 |
  | flake      | passes on immediate rerun      | rerun once               | find the nondeterminism (clock, port, order, shared data) | fix determinism; never ignore |
  | compile    | rustc error                    | owning agent fixes       | shrink to cargo check --lib, then one file | contract drift: re-lock, restart agents |
  | lint       | clippy -D warnings             | fix the code             | same                            | never allow |
  | assertion  | test fails, code compiles      | decide from the SPEC which side is wrong | fix that side | spec ambiguous: open question and park |
  | migration  | SQL error                      | run against a scratch DB | reorder; never edit an applied migration | add a forward migration |
  | openapi    | document differs               | intended? then update docs/api and the feature docs | not intended? revert | treat as breaking and record it |
  | seeder     | guard refuses or counts differ | read the guard's reason  | fix the data or the guard, never disable it | park with the exact command |
  | env        | DB down, port busy, lock, disk | start the DB, free disk, wait for the lock | kill only your own process | park with the exact command |
  | docs       | checker error                  | fix the link or status   | fix the reference table         | never weaken the checker |

Attempt budget: 3 attempts per failure class per chain, 20 minutes per root
cause without a new hypothesis, then PARK.

Failure memory: after every repair append to the ledger

  FAILURE: <command> -> <error> | ROOT CAUSE: <one line> | FIX: <one line> | TEST: <that now passes>

Circuit breaker: the same error at two consecutive checkpoints stops the run.

## 7. The checkpoint protocol

At the end of every chain, write this table into the ledger. No prose without it.

  | # | Criterion                                        | Evidence                        | Pass |
  | 1 | All six gates green                              | exit codes and timestamps       |      |
  | 2 | Every acceptance criterion maps to a test        | requirement id -> test name     |      |
  | 3 | New tests at least the contract's target         | count before -> after           |      |
  | 4 | A flow test exercises the chain end to end       | test name                       |      |
  | 5 | Ownership: the second account gets 404           | test name                       |      |
  | 6 | Docs updated (feature, schema, api, change log)  | file list                       |      |
  | 7 | Seeder updated for every new table               | profile counts and test name    |      |
  | 8 | OpenAPI changed only as intended                 | diff summary                    |      |
  | 9 | No forbidden construct added                     | grep for unwrap, expect, allow, ignore, unsafe, TODO | |
  | 10| code-reviewer pass on the chain diff             | verdict and findings            |      |
  | 11| quality-gate pass on the evidence                | verdict                         |      |

Decide and record:

- ADVANCE  every row Pass. Continue to the next queue item.
- REPAIR   any row Fail and fixable now. Return to the ladder, maximum 3 rounds.
- PARK     not fixable within budget. Write the blocker, the exact error and the
           next step. Move on. Never mark it done.
- STOP     circuit breaker, unstable tree, disk below 1 GB, or context exhausted.

Look-ahead, mandatory before ADVANCE:

1. Read the next queue item and its contract.
2. Verify its prerequisites exist: tables, types, config keys, handler errors.
   List them.
3. If a prerequisite is missing, produce it now when small and in scope, or
   reorder the queue and record why.
4. Estimate the next chain's budget. If it does not fit the remaining time, park
   it and close the phase cleanly.
5. Write the next chain's starting state into the ledger, with the exact first
   command the next session should run.

## 8. The testing standard

Every chain produces, at minimum:

- UNIT tests for every rule in the contract (money, transitions, validation,
  the seeder's guards and determinism).
- INTEGRATION tests per endpoint: happy path, 401, 404 for another owner, 400
  malformed id/body, 422 validation, and every documented error code.
- A FLOW test across endpoints, asserting status codes and full JSON. Example:
  register -> login -> create project -> create claim -> add items -> submit ->
  approve -> mark paid -> list -> totals match; then the second account sees
  404.
- A CONTRACT test asserting every new path and method appears in
  /api/v1/openapi.json with the documented status codes.
- MIGRATION proof: the new migration applies from an empty database in CI.
- SEEDER proof: the new entity appears in the `dev` profile with the right
  counts, and the seeder still refuses a production environment and a foreign
  database.

For each critical test, PROVE IT CAN FAIL: break the behaviour in a scratch
edit, watch the named test go red, restore, watch it go green, record both
outcomes. A test that cannot fail is not evidence.

Forbidden shortcuts: sleeping instead of waiting for a condition; asserting only
a status code where the body is the contract; shared state between tests;
ordering dependence; network calls (the ML embedder must be deterministic in
tests); seeding production data.

## 9. The ledger

docs/engineering/runs/chain/ledger.md is the only source of truth for
resuming. Update it after every chain, never at the end:

  ## Baseline   HEAD, branch, gate results, test count
  ## Queue      item, state, commit SHA
  ## Contracts  the locked contract blocks, newest last
  ## Evidence   commands, exit codes, key output, counts
  ## Failures   the failure-memory lines
  ## Checkpoints the scorecard tables and the decisions
  ## Parked     blocker, exact error, next step
  ## Resume     one paragraph for a fresh session

## 10. Stop conditions

Stop and report when: the tree changes under you; the same error survives two
checkpoints; a gate cannot run and you are tempted to call it passing; disk
below 1 GB; a product decision is genuinely missing (record it, exclude the
behaviour, continue); or context is nearly exhausted (commit, update Resume).

## 11. Report at the end of the phase

```
Outcome      chains done, chains parked, what now works
Changed      file paths
Verified     <command> -> <result>        (only commands actually run)
Docs         documents updated, or "none needed: <reason>"
Open         parked chains, unverified claims, decisions still blocking
```

Update the pull request description with the checkpoint tables, push, and
update the ledger's Resume section before printing the report.
````

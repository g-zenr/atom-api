# ATOM change log

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent

The chronological record of substantial changes to the ATOM backend, newest
first. It explains **how** the system reached its current state; it does not
define the target state. The requirements live in
[`../project-truth.md`](../project-truth.md), the current state in
[`implementation-status.md`](implementation-status.md), and the reviews in
[`review-log.md`](review-log.md).

---

## How to read this log

- **Newest first.** The most recent entry is at the top.
- **Append-only.** An entry is never rewritten. An inaccurate statement is
  corrected by adding a clearly marked **Correction** to the same entry, with
  its own date and a reason.
- **One entry per substantial change.** A change is substantial when it alters
  behaviour, a contract, the schema, a security control or the architecture.
  A typo fix does not need an entry.
- **The checks are recorded with their real results.** A check that did not run
  is recorded as *not run*, with the command needed to run it. A result is
  never claimed that was not observed. See
  [`../testing/verification-policy.md`](../testing/verification-policy.md) §4.

## How to add an entry

1. Copy [`../templates/change-record.md`](../templates/change-record.md).
2. Give it the next identifier: `ATOM-CHANGE-YYYYMMDD-NNN`, where `NNN` is a
   three-digit sequence that has not been used before. Identifiers are never
   reused, and the date is the UTC date of the change.
3. Fill in every section. A section that does not apply says
   `Not applicable: <reason>`; it is never deleted, because a deleted section
   hides the question.
4. Insert the entry directly under the `## Entries` heading, above the previous
   entry.
5. Run the three quality gates and record their real output.

## Entries

### ATOM-CHANGE-20261024-007 - Item endpoints and the claim lifecycle

**Date:** 2026-10-24 (UTC)
**Type:** Feature (endpoints)
**Author:** The ATOM engineering team - the orchestrator with the `rust-backend`, `test-engineer` and `docs` subagents.
**Status:** Complete - **chains `D2.R3`/`D2.R4`, review rows outstanding**

#### Summary

Three item routes and four lifecycle actions take the published surface from
**27 to 34 operations**. All twelve reimbursement operations now answer, so the
feature moves to `Implemented`; it is not `Verified`.

#### What changed

| Area | Change |
| ---- | ------ |
| Endpoints | `POST /claims/{id}/items`, `PATCH`/`DELETE /claims/{id}/items/{item_id}`, `POST /claims/{id}/{submit,approve,reject,pay}` |
| Money | `helpers::validation::validate_money`, `^[0-9]{1,15}\.[0-9]{4}$`, refuses rather than rounds |
| Transaction | Item write and total recompute in one transaction |
| Lifecycle | Five edges, 15 refusals, one guarded `UPDATE` per action |
| Tests | 21 in `tests/claims_api.rs`, one contract test, eight unit tests; **266 -> 296** |

#### Three decisions that are not obvious

1. **The item write and the total recompute are one transaction.** Two
   statements in sequence let the item and `total_amount` diverge if the second
   fails, and `REIMB-R03` says the total *always* equals the sum.
2. **A lifecycle action is one guarded `UPDATE ... WHERE status = $expected`.**
   A read-then-write lets two concurrent `approve` requests both pass the check.
   Zero rows affected is `409` - or `404` when the claim is not the caller's. The
   transition table is consulted on the *error* path, to choose `409` from the
   submit `422`.
3. **Lifecycle stamps use `clock_timestamp()`, not `NOW()`.** `NOW()` is the
   transaction start, which after waiting on the row lock can predate the
   `submitted_at` it is compared against, and
   `expense_claims_timestamp_order_chk` would then reject a *correct* approval
   with `23514`. That would have been an intermittent `500` on the approve path.

#### The reopen edge, which is the one that could have failed

`rejected -> draft` must clear `submitted_at` **and** `decided_at`, because the
`draft` arm of the lifecycle `CHECK` requires all three stamps null; clearing
only `decided_at` - which the earlier specification said - is a `23514` **at
commit**, i.e. a `500` on the only reopen edge. The hand run confirmed the fix:
`POST /claims/B/submit` on a rejected claim returned `200 draft` with
`submitted_at`, `decided_at` and `paid_at` all null, then accepted a new item and
resubmitted.

#### Checks and results

Six gates: `fmt` 0, `clippy -D warnings` 0, `test --all-targets` 0 (**296**),
`check --no-default-features` 0, `atom-docs` 0, `atom-docs --strict` 0.

Hand run on `atom_dev`: two items summing to `149.8000`; a patched amount moved
the total to `153.9000`; a delete moved it to `129.9000`; `submit`, `approve` and
`pay` each `200` with exactly their own stamp; a second `pay` and a `reject` from
`approved` both `409`; an item write on a submitted claim `409`; a claim with no
items `422` naming `items`; `"12.5"` `422` naming `amount`; a `javascript:`
receipt `422`; an item under the wrong claim `404`.

#### What is not verified

No reviewer has read this chain. The `408`/`500` paths on these routes are
unexercised, the flow test lands in `R5`, and the seeded dataset has not been
read through the API.

### ATOM-CHANGE-20261024-006 - Claim CRUD for reimbursements

**Date:** 2026-10-24 (UTC)
**Type:** Feature (endpoints)
**Author:** The ATOM engineering team - the orchestrator with the `rust-backend`, `test-engineer`, `docs` and `database` subagents.
**Status:** Complete - **chain `D2.R2`, review rows outstanding**

#### Summary

Five claim endpoints answer: list, create, read, partial update and delete. The
published surface moves from 22 operations to **27**, and
`tests/openapi_contract.rs` pins the new number. The feature is still
`Accepted`: items, the lifecycle, and the total invariant are `R3`/`R4`.

#### What changed

| Area | Change |
| ---- | ------ |
| Endpoints | `GET`/`POST /api/v1/claims`, `GET`/`PATCH`/`DELETE /api/v1/claims/{claim_id}`; `409` is unreachable in this chain |
| Module | `src/reimbursements/{dto,service,repository,handlers}.rs`; nine static statements; `ClaimService` on `AppState` |
| Contract | 22 -> 27 operations, seven components, a `claims` tag, and the claims row in the parameter-set test |
| Tests | `tests/claims_api.rs` (34), two new contract tests, five unit tests; **225 -> 266** |
| Docs | reimbursements `status`/`architecture`/`testing`/`api`, `docs/api/README.md`, `docs/testing/strategy.md`, `schema.md`, `docs/README.md` |

#### The three decisions that were not obvious

1. **`project_id` is a tri-state.** `Option<Option<Uuid>>` with plain serde
   collapses "absent" and `"null"` into `None`, because `serde_json` answers an
   explicit `null` with `visit_none` on the *outer* `Option`. Without
   `#[serde(default)]` plus a local `double_option` deserializer, "clear the
   link" is inexpressible and `PATCH {"project_id": null}` - a published
   operation - silently does nothing. The unit test deserializes the three
   bodies rather than constructing them, because construction would not have
   caught it.
2. **`currency`, `status` and `total_amount` are accepted and ignored on
   `PATCH`.** The published `api.md` says so; `schema.md` and `status.md` said
   "a service rule with a `422`". Rejecting would require *adding* the fields to
   `UpdateClaimRequest` in order to refuse them, which widens the published
   request schema and contradicts the same paragraph. Immutability is stronger
   this way: none of the three appears in any `SET` clause, so what was dropped
   is the error, not the rule. The three documents now agree on the published
   behaviour.
3. **A foreign `project_id` is resolved inside the write statement**, with
   `WHERE EXISTS (SELECT 1 FROM projects WHERE id = $n AND user_id = $m)`, so it
   answers `404` without a second round trip. The composite key cannot express
   it: `ON DELETE SET NULL (col)` needs PostgreSQL 15 and the documented floor
   is 14.

#### Checks and results

Six gates on the integrated tree: `fmt` 0, `clippy -D warnings` 0,
`test --all-targets` 0 (**266 tests**, 225 -> 266), `check --no-default-features`
0, `atom-docs` 0, `atom-docs --strict` 0.

A hand run against a real server on `atom_dev` observed: create `201` with
`"total_amount":"0.0000"`; read, list and patch `200`; `project_id` set and
cleared; delete `204`; read after delete `404`. Negatives observed: a foreign
project `404` on create and patch; another account's claim `404` on every verb;
`PATCH {"currency":"USD","status":"paid","total_amount":"999.0000"}` `200` with
all three unchanged; `PATCH {}` `200` with only `updated_at` moved; `422` for a
blank title, a bad currency and a reversed date range; `400` for a non-JSON
body, a non-UUID path and a bad list parameter; `401` without a token.

#### What is not verified

`code-reviewer` and `quality-gate` have not read this chain. Items, the
lifecycle actions and the total-equals-sum invariant are not built, so the
feature is not `Implemented` and no requirement is `Verified` beyond the claim
operations the tests exercise. Which listing statement ran is not observable
from a response, so "the status-filtered list uses the status index" is an index
decision, not a test result.


### ATOM-CHANGE-20261024-005 - The reimbursements schema and its row types

**Date:** 2026-10-24 (UTC)
**Type:** Feature (schema)
**Author:** The ATOM engineering team - the orchestrator with the `database`, `rust-backend`, `test-engineer` and `docs` subagents.
**Status:** Complete - **chain `D2.R1`, review rows outstanding**

#### Summary

`migrations/0004_expense_claims.sql` creates `expense_claim_status`,
`expense_claims` and `expense_items`, and `src/reimbursements/` holds the row
types. **No endpoint exists**: no handler, service, repository, DTO or route, and
`/api/v1/openapi.json` is unchanged.

#### What changed

| Area | Change |
| ---- | ------ |
| Schema | `0004_expense_claims.sql`: the five-state enum, two tables, every named CHECK, the composite foreign key, and five indexes |
| Types | `src/reimbursements/{mod,models}.rs`: `ClaimStatus`, `ExpenseClaim`, `ExpenseItem`; money is a `String` read from `::text` |
| Seeder | Claims 1/25/50 and items 0/48/98 over the profiles, `V1` total-equals-SUM, `V2` extended to claim and item ownership, `V6` covers both new tables |
| Tests | `tests/claims_schema.rs` (10) and two new `tests/seeder.rs` cases; 213 -> 225 |
| Docs | `schema.md` §11, `seeding.md`, the reimbursements `status`/`architecture`/`testing`, `testing/strategy.md`, `project-truth.md` §17, `architecture/*` |

#### The two constraints that do the real work

`expense_items_claim_fk` references `expense_claims (id, user_id)`, so an item
whose owner disagrees with its claim's owner **cannot be inserted by any code
path**, not even one that forgets to check. `expense_claims_lifecycle_timestamps_chk`
makes "a submitted claim without a submission date" and "a paid claim without a
decision" unrepresentable. `tests/claims_schema.rs` asserts the SQLSTATE and the
constraint name for both, not merely that an insert failed.

#### Checks and results

Six gates: `fmt` 0, `clippy -D warnings` 0, `test --all-targets` 0 (**225**),
`check --no-default-features` 0, `atom-docs` 0, `atom-docs --strict` 0. A `dev`
seed run reports `claims 25, items 48, verification passed (6 checks)`.

#### Two things measured, not assumed

- `CHAR(3)` rejects a four-character currency with SQLSTATE `22001` **before**
  the `CHECK` is evaluated, so the contract named the wrong mechanism while the
  value is still unrepresentable.
- PostgreSQL **rounds** a fifth decimal into `NUMERIC(19,4)` (`149.876549` ->
  `149.8765`) rather than refusing it; the four-decimal wire form follows from
  the column type, not from validation.

#### What is not verified

No endpoint exists, so no endpoint test exists and the feature is `Accepted`,
not `Implemented` or `Verified`. `code-reviewer` and `quality-gate` did not run
on this chain; that is recorded as rows 10-11 of its checkpoint.
### ATOM-CHANGE-20261024-004 - The atom-seed development seeder

**Date:** 2026-10-24 (UTC)
**Type:** Feature (development tooling)
**Author:** The ATOM engineering team - the orchestrator with the `rust-backend`, `test-engineer`, `database`, `auth-security` and `devops` subagents.
**Status:** Complete - **chain D2.S1, review rows outstanding**

#### Summary

`cargo run --bin atom-seed -- --profile dev --reset` now gives a complete,
deterministic dataset for the Phase 1 entities. It is a development tool: it
writes known credentials into a database it refuses to touch unless the name
says development and `APP_ENV` says development.

No endpoint, path, method, status code, request body or response schema
changed. No migration was added.

#### What changed

| Area | Change |
| ---- | ------ |
| Binary | `src/bin/atom-seed.rs` - argument parser, guards, deterministic ids and timestamps, three profiles, `--reset`, inserts, five post-insert checks, redacted error classification |
| Contract | `C-S1` plus amendments `A-C-S1-1`...`A-C-S1-10`; `A-C-S1-5`...`10` come from the security review and supersede the original guard text |
| Tests | `tests/seeder.rs` - 16 tests, including the ten locked by `C-S1` and the two the security review required |
| Specification | `docs/database/seeding.md` - profiles, guards, determinism, the reset blast radius, the invariants and the test list |
| Documentation | `schema.md` (the `PATCH` on events is a correction in place), the planned Phase 2 sections in `docs/architecture/`, `README`/`CONTRIBUTING`/`Makefile`/`.env.example`/CI, `KI-020` and `KI-021`, `SD-5` |

#### The guards, and why they are shaped that way

`G1` is an allow-list, not a substring test: the original rule refused
"production" but admitted `APP_ENV=prod`, which `Environment::parse` maps to
production. `G2` reads the database name through the driver's own URL parser
and accepts whole names (`atom_dev`, `atom_test`, `atom_local`,
`atom_demo`, or a `_dev`/`_test`/`_local` suffix), because the substring
rule admitted `latest` and `production_test` while refusing the tool's own
`atom_demo` profile. `G3` runs before the migration step as well as inside
the transaction, so a refused run really does write nothing. `--reset` deletes
every row owned by the two seeded accounts and says so before it runs.

#### Checks and results

Six gates on the integrated tree: `fmt` 0, `clippy -D warnings` 0,
`test --all-targets` 0 (**213 tests**, up from 191), `check
--no-default-features` 0, `atom-docs` 0, `atom-docs --strict` 0.

A real run in the documented setup (`createdb atom_dev`, then the command
above): 2 users, 200 notes, 8 projects, 60 tasks; `verification passed (5
checks); 270 rows inserted`. Two `--reset` runs produced an identical
`md5` over `(id, created_at, updated_at)`. `APP_ENV=prod` and
`DATABASE_URL=.../atom` are both refused, with no part of the URL in the
output.

#### What is not verified

No independent review has read the seeder diff: code-reviewer and
quality-gate did not run on this chain. The container image was not started,
because this machine has no usable daemon (KI-006); docker compose config
validates the file and KI-021 records the limitation.

#### Correction (2026-10-24) — the author line was corrupted

The `**Author:**` line was written with escape sequences that were interpreted
rather than kept literal: `\r`, `\t` and `\a` became control characters, so the
line read "…with the" followed by a carriage return and `ust-backend,`, a tab
and `est-engineer, database,`, a bell character and `uth-security…` — parts of
three agent names were missing. The line was repaired in place to name the
agents properly: `rust-backend`, `test-engineer`, `database`, `auth-security`
and `devops`. A corrupted attribution cannot be read at all, which is why this
one line was rewritten; nothing else in the entry was.

#### Correction (2026-10-24) — the seeder test count and the check count

This entry records `tests/seeder.rs` as **16** tests and **five** post-insert
checks. Those were the counts at this entry's own commit, `5688de7`; they are
not the counts of the tree. Chain `D2.R1` (`ATOM-CHANGE-20261024-005`) added the
two claim cases, so the file holds **18** tests, and `verify()` in
`src/bin/atom-seed.rs` runs **six** checks, printing `verification passed (6
checks)`. The original sentences stay as written, because this log is
append-only; a reader must take the figures in this correction as current and
the ones in the entry as the state at `5688de7`.

### ATOM-CHANGE-20261024-003 - The Phase 2 product intake

**Date:** 2026-10-24 (UTC)
**Type:** Documentation (product intake)
**Author:** The ATOM engineering team — the `architect`, `database` and `docs` agents with the orchestrator.
**Status:** Complete — **documents only**

#### Summary

Phase 2 now has accepted requirements, three feature specifications, three
architecture decisions, a proposed schema and a seeding specification. No code,
no migration and no test was written for any of the three features: they are
`Accepted`, not `Implemented`, and nothing in this change is `Verified`.

Per owner decision `O1`, the six working assumptions `A1`–`A6` are recorded as
**provisional defaults**, each document that depends on one says so, and
[`open-questions.md`](open-questions.md) `Q-13` asks the owner to confirm or
revise them. They are not presented as approved.

#### What changed

| Area | Change |
| ---- | ------ |
| Requirements | [`project-truth.md`](../project-truth.md) §18: 18.1 reimbursements (`REIMB-R01`…`R10`), 18.2 job applications (`JOBS-R01`…), 18.3 local machine learning (`ML-R01`…), 18.4 the provisional-status note. §17 gains the Phase 2 rows as `Not started`. |
| Features | Three new directories with the full five documents each: [`features/reimbursements/`](../features/reimbursements/), [`features/job-applications/`](../features/job-applications/), [`features/ml/`](../features/ml/). All three are `Accepted`. |
| Decisions | [`ADR-0007`](../decisions/ADR-0007-money-representation.md) money as `NUMERIC(19,4)` and a JSON string; [`ADR-0008`](../decisions/ADR-0008-local-pure-rust-ml.md) local, pure-Rust ML with `REAL[]` rather than pgvector; [`ADR-0009`](../decisions/ADR-0009-receipt-references.md) a receipt is a reference, not an upload. |
| Schema | [`database/schema.md`](../database/schema.md) records the proposed `expense_claims`, `expense_items`, `companies`, `job_applications`, `application_events` and `embeddings` tables as proposed, with no migration written. |
| Seeder | [`database/seeding.md`](../database/seeding.md) — the profiles, the guards, determinism, `--reset`, the post-insert verification and the credentials. |
| Conventions | [`api/conventions.md`](../api/conventions.md): money is a JSON string, four decimal places, never a float. |
| Test matrix | [`testing/strategy.md`](../testing/strategy.md): the acceptance tests each requirement will need, one per rule. |
| Questions | `Q-11` answered — Phase 2 is defined in [`roadmap.md`](../roadmap.md) §2. `Q-13` opened for the provisional assumptions. |

#### Why `REAL[]` and not pgvector

`vector` 0.8.1 is available on the development server, but the `atom` role is not
a superuser there while CI's is, so a migration containing `CREATE EXTENSION`
would pass CI and fail on the developer's machine — the worst possible
divergence. Phase 2 therefore does not depend on the extension: embeddings are
`REAL[]`, and search is hybrid — the existing lexical search narrows the owner's
rows to a bounded candidate set, and cosine similarity re-ranks them in Rust. That
is recorded in `ADR-0008`, with the trade-off that `source_id` is polymorphic and
carries no foreign key.

#### What this change does not do

It adds no endpoint, no handler, no service, no repository, no migration and no
test. `/api/v1/openapi.json` is unchanged. The three features cannot be
`Implemented` until their migrations and modules exist, and cannot be `Verified`
until their tests have run and passed.

#### Checks and results

`cargo run --bin atom-docs` and `--strict` both exit 0 with 0 warnings. The
checker's `FEATURES` list was extended from four to seven so that the three new
directories are validated too — the only code change in this intake. The three
new `status.md` files carried their status line inside the header blockquote,
where the checker cannot see it; they now open with the bare `**Status:** Accepted`
line the other feature documents use.

The full six gates were run on the integrated tree: `fmt` 0, `clippy -D warnings`
0, `test --all-targets` 0 (**191 tests**), `check --no-default-features` 0,
`atom-docs` 0, `atom-docs --strict` 0.

#### Compatibility impact

None. No behaviour, contract or schema changed.

---

### ATOM-CHANGE-20261024-002 - The route table, the document and the setting values are asserted, not assumed

**Date:** 2026-10-24 (UTC)
**Type:** Fix, test and documentation
**Author:** The ATOM engineering team - the orchestrator with the `rust-backend`, `test-engineer`, `code-reviewer` and `quality-gate` subagents.
**Status:** Complete

#### Summary

Two deferred findings were closed by making three things that had only been
believed into things that are asserted: that the route table and the OpenAPI
document describe the same operations, that each list endpoint parses its query
string once, and that the operator reference tables agree with the configuration
files about the **values** of their integer and boolean defaults, not only about
their names.

The third check found the drift it was written for. `config/README.md`
documented `app.limits.shutdown_grace_seconds` as `100`; `config/app.toml` says
`10`, and the code has always used ten seconds. The reference table was wrong by
an order of magnitude, and nothing had reported it.

No endpoint, path, method, status code, request body or response schema changed.
No migration was added or edited. No dependency was added.

#### What changed

| Area | Change |
| ---- | ------ |
| `F-23` | `API_PREFIX` is declared once in `src/api/routes.rs`; `src/api/mod.rs` mounts it; the explorer paths are built from it. `ROUTE_TABLE` and `UNVERSIONED_ROUTE_TABLE` publish the 22 operations, and `tests/openapi_contract.rs` compares them with the document in both directions. |
| `F-49` | Each list endpoint has one query type - `NotesListQuery`, `ProjectsListQuery`, `TasksListQuery` - and one `Query<…>` extractor, split into the existing filter and pagination halves by `into_parts()`. |
| Checker | `check_settings_reference` now compares integer and boolean defaults against **both** operator reference tables, in addition to the existing name comparison. |
| `config/README.md` | `app.limits.shutdown_grace_seconds` default `100` → `10`. |
| Run folder | `docs/engineering/runs/phase-2/` → `docs/engineering/runs/chain/`, and the two scripts that pointed at the old path were corrected. |

#### Why the query types do not change the published contract

The obvious way to combine the two halves is `#[serde(flatten)]`. It fails here:
axum's `Query` deserializes with `serde_urlencoded`, whose map access yields
strings, so a flattened `Option<i64>` would be asked to deserialize itself from
a string and would fail at run time, on a real request, rather than at compile
time. The fields are therefore written out explicitly in each query type, and the
module documentation says so.

The second risk was the document. Deriving `IntoParams` on the new types would
either publish a new schema or drop the two existing parameter types, so they
derive `Deserialize` only, and the handlers' `#[utoipa::path(params(…))]`
attributes still name the published types. The published parameter sets are
asserted exactly - no duplicate, no missing parameter, no extra - by
`each_list_endpoint_publishes_its_filter_and_pagination_parameters_once`.

#### What is asserted, and what is not

The operation set, the methods, the version prefix and the three list parameter
sets are asserted. A snapshot of every response body and schema is **not**, and
the status rows say so. `every_route_in_the_table_is_served_by_the_router`
distinguishes mounted from unmounted by status, so a wrong verb answers `405` and
passes it; method agreement is held by the table-versus-document test instead.

The one-parse rule is structural, not behavioural: a handler with two extractors
over the same URI produces an identical response, so the behaviour tests cannot
see it. It is held by `each_list_handler_extracts_the_query_string_once`, which
asserts one `Query<` per list handler module and names the expected type.

#### The checker's new check, and why it is narrow

Only a plain TOML integer or boolean is compared. A reference cell that holds
prose (`1 MiB`, `20 (default)`, `At least 12 characters.`) is skipped rather than
reported, and a key that appears on more than one row of the same document is not
compared for value there, because the repeated rows describe the overlay, floor
and environment layers with a different layer's value. `deployment.md` documents
43 setting rows for 28 distinct keys; `config/README.md` documents 28 for 28. A
false failure in a gate is worse than a check narrower than it could be, and the
drift this check exists for is a plain integer.

The check was proved to bite: with the cell set back to `100`, `atom-docs` exits
1 with `ERROR config/README.md documents the setting app.limits.shutdown_grace_seconds as 100, but the configuration file value is 10`; with `10`, it exits 0.

#### Compatibility impact

None for a client. The document describes the same 22 operations with the same
paths, methods, status codes, request bodies, response schemas and query
parameters. The behaviour of every endpoint is unchanged; the three new query
types are an internal seam.

#### Checks and results

Run from the repository root, on the integrated tree:

| Command | Exit | Result |
| ------- | ---- | ------ |
| `cargo fmt --all -- --check` | 0 | no diff |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | 0 | clean |
| `cargo test --all-targets --locked` | 0 | **190 passed, 0 failed** in 9 targets (111 lib, 3 `atom-docs`, 29 `auth_api`, 2 `docs_route`, 12 `notes_api`, 4 `list_query`, 5 `openapi_contract`, 25 `projects_tasks_api`, 0 `shutdown` — unix-only) |
| `cargo check --no-default-features --all-targets --locked` | 0 | clean |
| `cargo run --quiet --bin atom-docs` | 0 | `OK (0 warning(s))` |
| `cargo run --quiet --bin atom-docs -- --strict` | 0 | `OK (0 warning(s))` |

The suite grew from 179 to 190 tests: 11 new, matching the 11 the contract named.
The shutdown tests were not executed: they are unix-only and this host is Windows.

#### Review

`code-reviewer` returned **PASS WITH FINDINGS** with no blocking finding; its
nine non-blocking and four documentation findings were fixed in this change or
recorded. `quality-gate` returned **FAIL** on the first pass with four blocking
and two required items - a false "no test asserts its content" claim, two stale
request-path diagrams, a missing review record, an untested invariant and a stale
ledger row - all of which were resolved before the change was committed. Entries:
[`review-log.md`](review-log.md) `ATOM-REVIEW-20261024-001` and `-002`.

#### Requirements and decisions referenced

| Reference | What it is |
| --------- | ---------- |
| `ATOM-TASK-20261022-021` (`F-23`) | The prefix, the route tables and the agreement test. |
| `ATOM-TASK-20261022-023` (`F-49`) | The one-deserialisation query types. |
| [`../audit/remediation-backlog.md`](../audit/remediation-backlog.md) | Both rows moved from `deferred` to `verified`, each with a Correction. |
| [`project-truth.md`](../project-truth.md) | The OpenAPI row moved to `Verified`, with its scope stated. |
| [`runs/chain/ledger.md`](runs/chain/ledger.md) | `C-D0` and amendments `A-C-D0-1`…`A-C-D0-10`, the evidence and the scorecard. |

#### Correction, 2026-10-24 — the test total was miscounted

This entry, its commit message and four other documents said the suite grew to
**190** tests. The recorded per-target counts sum to **191**: 111 lib, 3
`atom-docs`, 29 `auth_api`, 2 `docs_route`, 12 `notes_api`, 4 `list_query`,
5 `openapi_contract`, 25 `projects_tasks_api`, 0 `shutdown`. The growth is
**179 → 191** — twelve new tests, not eleven: the contract named eleven, and the
quality gate required a twelfth, `each_list_handler_extracts_the_query_string_once`,
to hold `I3` rather than leave it verified by inspection only.

No gate result changes. `cargo test --all-targets --locked` exited 0 and every
target passed; the error was arithmetic in prose, not in the run. It is corrected
here rather than in the original text, per the append-only rule of this log.

#### Correction (2026-10-24) — the commit also carried the intake

Commit `906d6dc`, which this entry describes, also carried the `D1` intake
documents (`project-truth.md` §18, `api/conventions.md` §15–§17, the run folder
rename). That was a consequence of two orchestrator sessions running the same
queue on one working tree; it is recorded in
[`runs/chain/ledger.md`](runs/chain/ledger.md) `## Parked`. The statements in
this entry remain true — no endpoint, path, method, status code, request body or
response schema changed — but a reader bisecting the log should expect
`906d6dc` to be larger than this entry alone, and should read
[`ATOM-CHANGE-20261024-003`](#atom-change-20261024-003---the-phase-2-product-intake)
for the intake half.

---

### ATOM-CHANGE-20261024-001 - The documentation feature, the container image, and the shutdown test

**Date:** 2026-10-24 (UTC)
**Type:** Fix, performance and process
**Author:** The ATOM engineering team - the orchestrator with the `rust-backend`,
`auth-security`, `devops` and `test-engineer` subagents.
**Status:** Complete

#### Summary

Four deferred items were taken up. One became a measured 12 MiB saving, one
became a reproducible deployment artifact, one became a test (which has not been
executed on the platform it targets), and one was **reviewed and deliberately not
changed** with the measurement that justified it. The documentation checker also
gained a check it was missing.

No API contract changed. No migration was added or edited.

#### What changed

| Area | Change |
| ---- | ------ |
| `F-44` | `utoipa-swagger-ui` is now optional behind a default-on `docs` Cargo feature. A production build drops 12.03 MiB of Swagger assets it never serves. |
| `KI-006` | `Dockerfile` and `.github/workflows/container.yml` add a reproducible container image and a job that verifies it rather than trusting the build. |
| Shutdown | `tests/shutdown.rs` adds two unix-only tests that deliver a real `SIGTERM`. |
| `F-45`, `F-46` | Reviewed with SQLx's query logger; both confirmed; neither changed, for the reason below. |
| Checker | `check_settings_reference` now checks the **second** operator table, in `config/README.md`, in both directions. |

#### `F-44` — measured, not estimated

Two real release builds of the same tree, `strip = "symbols"`, thin LTO,
`codegen-units = 1`:

| Build | Bytes | MiB |
| ----- | ----- | --- |
| default features (`docs` on) | 20,370,432 | 19.43 |
| `--no-default-features` | 7,760,896 | 7.40 |
| **removed** | **12,609,536** | **12.03 (62%)** |

`SwaggerUIBundle` is present in the first binary at offset 7,626,840 and absent
from the second, confirmed by searching the bytes rather than inferring it from
the size. Both binaries were then **run**: with the feature on, `/api/v1/docs`
redirects to `/api/v1/docs/` and serves, `swagger-ui.css` serves, and
`/api/v1/openapi.json` serves; without it, those paths return `404` with the
standard envelope while `/health/live` still answers.

A build without the feature that is told `api.docs_enabled = true` does not fail
and does not 404 silently: it logs one warning naming the feature, the two paths
that will 404, and how to build with it.

**A note on a figure in an earlier record.** The remediation handoff quoted a
19.35 MB release binary, and the artifact left on disk was 8.68 MiB. Both were
stale: the 8.68 MiB file had been built during the transient experiment that
removed `vendored`, so it contained no Swagger assets at all. The numbers above
replace both.

#### `KI-006` — the container image

Two stages. `rust:1.99.1-slim-bookworm` builds with `--locked` and
`--no-default-features`; `debian:bookworm-slim` carries the binary and a CA
bundle. Both base images are pinned by tag **and** digest. Nothing runs as root
(UID 10001). Neither `config/` nor `migrations/` is copied in, because both are
embedded in the binary, and a second copy would be a second source of truth. No
secret is baked in — a build argument would not help, since an argument is
visible in the image history.

The workflow **verifies** the image rather than trusting it: the declared user is
not root, `id` reports `uid=10001`, `cargo` and `rustc` are absent, `ldd` reports
no missing library, and starting the binary with no environment exits non-zero,
which is the evidence that no configuration was compiled in. It never logs in to
a registry and never pushes.

#### The shutdown tests, and what they do not prove

Two tests in `tests/shutdown.rs` spawn the real binary, wait for
`/health/ready`, and send `SIGTERM`:

- with an 8 s grace and nothing in flight, the process must exit within 4 s. The
  original implementation slept the grace *inside* the signal future, so it would
  take ~8 s and fail;
- with a 2 s grace and a request that never completes, the process is still alive
  500 ms after the signal, proving the request is drained rather than dropped,
  and exits by the deadline plus a margin. Removing the `select!` bound makes the
  request end only at the 30 s request timeout.

**They cannot run on the machine that wrote them.** It is Windows, the file is
`#![cfg(unix)]`, and both attempts to obtain a Linux runtime failed — the Docker
daemon stopped mid-pull and a cross-compilation check could not link because
`x86_64-linux-gnu-gcc` is absent. So `KI-019` recorded the gap rather than letting
a test's presence imply a pass, and CI became the first execution.

It passed. Run
[`40011694171`](https://github.com/g-zenr/atom-api/actions/runs/40011694171)
reports `test result: ok. 2 passed; 0 failed` for `tests/shutdown.rs`, and the
bounded-drain test logs
`the shutdown grace elapsed with requests still in flight grace_seconds=2` — the
deadline being reached with a request still in flight, which is precisely the
behaviour that could not be observed on Windows and the exact path the review
found unverified. `KI-019` is closed.

#### `F-45`, `F-46` — reviewed, deliberately not changed

Measured with SQLx's own query logger in a throwaway test, not inferred:

- one authenticated request runs **exactly one** statement on the auth path — a
  primary-key lookup fetching seven columns, of which `id`, `email` and
  `is_active` are used (`F-46` confirmed);
- `GET /auth/me` runs that identical statement **twice** in one request (`F-45`
  confirmed).

They were left alone because **the two findings pull in opposite directions**: the
only way to make `/auth/me` a single query is to load the full row on every
request, which is precisely the over-fetch `F-46` reports. The remaining fix —
narrowing the projection to `id, email, is_active` — crosses the authentication
boundary for a three-column instead of seven-column primary-key fetch, and the
boundary review concluded that does not justify it today. The exact change is
recorded in [`../audit/remediation-backlog.md`](../audit/remediation-backlog.md)
under `ATOM-TASK-20261022-020` so nobody re-derives it.

The elapsed times the logger emitted were **not** used as evidence: debug build,
cold statement, local PostgreSQL.

#### The checker now reads the second table

The project keeps two operator reference tables, one under `docs/architecture/`
and one in `config/`, and only the first was machine-checked. Both are now
compared with the keys the four configuration files define, in both directions.
The check was proved to bite: deleting the `shutdown_grace_seconds` row from
`config/README.md` makes it fail with
`does not document the setting app.limits.shutdown_grace_seconds` (exit 1), and
restoring the row returns it to green. Values are still not compared; that half
stays deferred, because the reference writes `1 MiB` where the file writes
`1048576` and a false failure in the gate is worse than a narrower check.

#### Compatibility impact

None for a client. The OpenAPI document and every endpoint contract are
unchanged; `/api/v1/docs` is not an API endpoint in the document. The release
build command changed, and the container image uses it.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Passed.** |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** |
| `cargo check --no-default-features --all-targets` | **Passed.** |
| `cargo test --all-targets` | **Passed.** 181 on CI's `ubuntu`, in run [`40011694171`](https://github.com/g-zenr/atom-api/actions/runs/40011694171): 111 unit, 29 auth, 2 explorer, 12 notes, 25 projects and tasks, 2 shutdown. 179 on Windows, where the two shutdown tests are `#[cfg(unix)]`. |
| `container.yml`, run [`40011694239`](https://github.com/g-zenr/atom-api/actions/runs/40011694239) | **Passed.** The image builds and the verification steps confirm `Config.User=10001:10001`, `uid=10001(atom)`, no `cargo`/`rustc`, no missing shared library, and a non-zero exit with `DATABASE_URL` named. |
| `cargo run --bin atom-docs` | **Passed.** `OK (0 warning(s))`. |
| `cargo run --bin atom-docs -- --strict` | **Passed.** `OK (0 warning(s))`. |
| Two release builds and both binaries exercised over HTTP | **Passed**, figures above. |

**Verified by CI, on the first run that could exercise them:** the container image
build and its verification steps, and the two shutdown tests on `ubuntu`. Neither
could be run on the development machine, and both were recorded as unverified
until a run proved otherwise rather than assumed green.

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../architecture/deployment.md`](../architecture/deployment.md) §5.1a | The image and the build command. |
| [`../testing/strategy.md`](../testing/strategy.md) §1.4 | The process layer, and why it is unix-only. |
| `F-44`, `F-45`, `F-46`, `KI-006`, `KI-019` | The findings and the two remaining gaps. |
| [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md) | Why no configuration file is copied into the image. |

---

### ATOM-CHANGE-20261023-001 - The default OpenCode model, and a validator hint that pointed at nothing

**Date:** 2026-10-23 (UTC)
**Type:** Process and repository configuration
**Author:** The ATOM engineering team - the orchestrator with the `devops` and
`docs` subagents
**Status:** Complete

#### Summary

The committed default model in `opencode.jsonc` is now
`opencode-go/deepseek-v4.1-flash`, because the previous default,
`opencode-go/minimax-m3.1-flash-free`, is no longer listed by `opencode models`.
The ten agents carry no `model:` of their own, so all of them inherit the new
default, and `.opencode/opencode.jsonc.example` follows it. A dead documentation
reference in the PowerShell validator's failure hint was corrected at the same
time. No application behaviour, API contract, schema or migration changed; the
one consequence a contributor feels is that running the agent team now costs
money.

#### Reason

The repository shipped a configuration that failed its own validator.

`opencode models` no longer lists `opencode-go/minimax-m3.1-flash-free`; the
closest entries are `opencode-go/minimax-m3.1-flash` and
`opencode-go/minimax-m3`. The committed configuration therefore failed the
validator's model-availability check, so every contributor who ran
`& ./scripts/validate-opencode.ps1` saw a red gate that the repository caused
and their own change did not. A gate that is red for everyone is a gate that
teaches people to ignore it, which is the failure mode
`ATOM-CHANGE-20261022-008` recorded for the CI job before its
`continue-on-error` flag was removed.

CI was not affected, because the workflow runs the validator with
`--skip-model-check --skip-agent-discovery`: a hosted runner holds no provider
credentials, so those two checks cannot succeed there and are printed as
warnings rather than as a pass.

#### What changed

| Area | Change |
| ---- | ------ |
| `opencode.jsonc` | The default `model` is `opencode-go/deepseek-v4.1-flash`. The comment block above the setting now records that the model is metered ($0.15 per million input tokens and $0.60 per million output tokens, per the catalogue), that the previous default had dropped out of the catalogue, and exactly what was and was not observed about tool calling and delegation. |
| The agent team | Unchanged, and deliberately so: none of the ten agents carries a `model:` of its own, so all of them inherit the default and none of them needs an edit when it changes. |
| `.opencode/opencode.jsonc.example` | Follows the new default, so the override a contributor copies cannot start from the stale model. |
| `scripts/validate-opencode.ps1` | The model-check failure hint named `docs/contributing.md`, which does not exist. It now prints `docs/contributing-opencode.md`, matching the `.sh` sibling, so the advice it gives can actually be followed. |
| `docs/contributing-opencode.md`, `CONTRIBUTING.md` | Both state the new default, the metered pricing, the warning that the catalogue changes, the past-tense note that the old default disappeared from it, and the precise scope of what has been observed. |

#### Affected features and files

| Feature | Area | Files |
| ------- | ---- | ----- |
| Foundation | The delivery toolchain | `opencode.jsonc`, `.opencode/opencode.jsonc.example`, `scripts/validate-opencode.ps1` |
| Foundation | The contributor documentation | [`../contributing-opencode.md`](../contributing-opencode.md), [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) |
| Foundation | The engineering record | [`change-log.md`](change-log.md) |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../../AGENTS.md`](../../AGENTS.md) §7 | Never silently fall back to a different model; report an unavailable or rate-limited model instead of routing around it. |
| [`../testing/verification-policy.md`](../testing/verification-policy.md) §4 | A check that did not run is reported as not run, never as passing. |
| [`../contributing-opencode.md`](../contributing-opencode.md) | The defaults, the override ladder and the validator, as a contributor meets them. |
| [`../decisions/README.md`](../decisions/README.md) §2.1 | A tuned default is recorded in the change log rather than in an ADR; this entry is that record. The same row sends an *application* default to [`../architecture/deployment.md`](../architecture/deployment.md) §1.3, which covers the settings in `config/` — a surface this change does not touch. |
| [`../../opencode.jsonc`](../../opencode.jsonc) | The committed setting this entry changes. |

`../project-truth.md` says nothing about the OpenCode agent configuration or the
default model: no section and no requirement identifier names a model or a
provider. Its §13 governs the quality gates and the evidence discipline, and
those are cited through the verification policy above. This entry therefore
satisfies no product requirement; it repairs the delivery toolchain.

#### Architectural impact

None. No module, boundary, invariant, request lifecycle or schema is touched,
which is why the change is confined to two configuration files, one shell
script and two documents: none of them can reach the application.

#### Compatibility impact

None for an API client and none for the database. One contributor-facing
default changed: a clone now runs the agent team on a metered model rather than
a free one, so a session costs money where it previously cost nothing. That is a
change to what the development tooling costs, not to what the product does.

#### Security implications

None. No credential, endpoint or permission rule was added, removed or
weakened, and the validator's secret scan passes over the committed files
unchanged. The committed configuration still names no provider credential, so
nothing in the repository can spend money by itself; the spend falls on the
account a contributor is signed in to.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `opencode models` (before) | **The previous default was absent.** `opencode-go/minimax-m3.1-flash-free` is not in the catalogue; the closest entries are `opencode-go/minimax-m3.1-flash` and `opencode-go/minimax-m3`. |
| `opencode models` (after) | `opencode-go/deepseek-v4.1-flash` is present. |
| `& ./scripts/validate-opencode.ps1` (before) | **Failed.** `FAIL the configured model is not in the catalog: opencode-go/minimax-m3.1-flash-free`; `FAILED: 1 check(s) failed.` Every other check passed. |
| `& ./scripts/validate-opencode.ps1` (after) | **Passed.** `PASSED: all checks completed (0 warning(s)).` — agent discovery for all ten agents, the read-only enforcement of `code-reviewer` and `quality-gate`, `OK   model offered by the catalog: opencode-go/deepseek-v4.1-flash`, the configuration parse and the secret scan. |
| `opencode debug config` (after) | Resolves `"providerID": "opencode-go"` and `"model": "deepseek-v4.1-flash"`. |
| `cargo run --quiet --bin atom-docs` (after) | **Passed.** `atom-docs: OK (0 warning(s))`. |
| `cargo fmt --all -- --check` | **Passed.** Exit code 0, no output. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** Exit code 0, no warnings and no errors; the profile finished in 7.26 s. |
| `$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"; cargo test --all-targets` | **Passed.** Exit code 0. 177 passed, 0 failed, 0 ignored: `src/lib.rs` 111, `src/main.rs` 0, `src/bin/atom-docs.rs` 0, `tests/auth_api.rs` 29, `tests/notes_api.rs` 12, `tests/projects_tasks_api.rs` 25. |

The three code gates ran because the policy requires them on every change, not
because this change is in their reach: none of them can observe a committed
configuration file or a document, and no Rust source changed.

`pwsh` is not installed on the reference machine, so the validator was invoked
from Windows PowerShell 5.1 as `& ./scripts/validate-opencode.ps1`. It is the
same script file either way; only the interpreter differs.

#### Manual verification

The orchestration session that made the change ran on the new default, under
OpenCode v2.0.27. In that session the orchestrator called tools and launched two
subagents, a `devops` specialist and a `docs` specialist; both completed and
returned results. That observation is what the two contributor documents now
say, and it is a session-level observation rather than a test.

Two things were **not** observed, and are not claimed: the model that the
subagent sessions resolved for themselves, and a full feature delivery run
end-to-end on the new default. A failed subagent launch is still to be reported,
never routed around.

#### Review findings

No independent review has been performed on this change set. It was checked by
the configuration validator and the documentation checker, which is not a
review. See [`review-log.md`](review-log.md).

#### Documentation

[`../contributing-opencode.md`](../contributing-opencode.md) §2, §3 and §4 and
[`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) §6 now describe the new
default, its price, the catalogue warning and the evidence for it. §4 of the
first document also stopped implying that metered use was something a
contributor opts into, which the new default made untrue. The default and the
override ladder are otherwise unchanged.

#### Limitations

- **No application behaviour, API contract, schema or migration changed**, so
  the suite that just passed exercises an application this change did not
  alter; no check above exercises the change itself.
- **The new default is metered.** Running the agent team now costs money where
  the previous default was free. The trade was deliberate: the free model is no
  longer offered by the catalogue.
- **The subagent-side model resolution was not recorded.** Only the
  orchestrator session's tool calling and delegation were observed; a subagent
  could in principle resolve a different model, and nothing here demonstrates
  otherwise.
- **No end-to-end feature delivery has been run on the new default**, so the
  team's behaviour across a full workflow is unproven.
- **The pricing is a catalogue listing, not a contractual figure.** It can
  change without a repository change, and nothing in the repository notices
  when it does.
- **[`../project-truth.md`](../project-truth.md) §8 requires that the build not
  depend on a hosted or paid service.** That still holds: `cargo build` and the
  three gates remain local. The agent team is a development tool rather than
  part of the build, and it now requires a paid provider account for whoever
  runs it.

#### Links

- [`../contributing-opencode.md`](../contributing-opencode.md) - the defaults,
  the override ladder and the validator.
- [`../../opencode.jsonc`](../../opencode.jsonc) - the setting, and the comment
  block that explains it.
- [`../testing/verification-policy.md`](../testing/verification-policy.md) §4 -
  what a results table may claim, and what it may not.
- `ATOM-CHANGE-20261022-008` above - the CI job that was green for the same
  reason: a gate that could not fail.

---

### ATOM-CHANGE-20261022-008 - The Git workflow, the pull-request standard and the release path

**Date:** 2026-10-22 (UTC)
**Type:** Process and repository configuration
**Author:** The ATOM engineering team
**Status:** Complete

#### Summary

The repository is now published to `https://github.com/g-zenr/atom-api`, and the
workflow around it is written down: a two-branch model (`develop` for
integration, `master` for production), a branch and commit standard, a
pull-request template, issue templates, and the DEV to UAT to PROD promotion
process. The existing CI workflow was corrected in three ways that mattered.

No application behaviour changed. No migration, no schema change, no API
contract change.

#### Reason

Three things were wrong or missing, and each had a concrete consequence:

1. **The CI trigger named a branch that does not exist.** `.github/workflows/ci.yml`
   ran on `push: branches: [main]`, while the repository's branch is `master`.
   A push could never have started it. The gates were therefore enforced only by
   whoever remembered to run them, which is how the remediation for this very
   release reached a review with a blocking defect in it.
2. **`KI-005` was recorded as closed** on the strength of a workflow file that
   had never executed. That is the failure mode the project's own verification
   policy exists to prevent, so it is corrected here rather than quietly left.
3. **There was no written workflow at all.** Branch naming appeared only as
   `feature/<description>` in `CONTRIBUTING.md`, which contradicted the
   conventional-commit types the log already used, and nothing stated which
   branch a pull request targets.

#### What changed

| Area | Change |
| ---- | ------ |
| CI | The workflow triggers on `master` and `develop`, not the non-existent `main`. The strict documentation check was added: only the non-strict `atom-docs` ran, so a broken cross-reference passed. `continue-on-error` was removed from the OpenCode job. A `concurrency` group cancels superseded runs. |
| Script modes | All five files under `scripts/` were committed with mode `100644`, so `./scripts/validate-opencode.sh` failed with `Permission denied` (exit 126) on Linux. The three shell scripts are now `100755`, and the workflow names the interpreter (`bash ./scripts/…`) so a checkout that loses the mode cannot disable the check again. |
| Pull requests | `.github/pull_request_template.md` added, with the six real gates, an explicit "not verified" field, a migration-impact block and a reviewer checklist. |
| Issues | `.github/ISSUE_TEMPLATE/bug_report.md` and `feature_request.md` added. The feature template states that a roadmap item is not agreed scope. |
| `CONTRIBUTING.md` | The branch rule now points at `develop` and at the full type list; `feature/` became the `feat/` family. The pull-request step names its base branch. The commit conventions gained `perf:` and `security:` and the rule that a commit must compile. |
| New documents | [`git-workflow.md`](git-workflow.md) - the branch model, naming, gates, review and merge strategy. [`release-and-promotion.md`](release-and-promotion.md) - the DEV/UAT/PROD gates, what is verified and recorded at each, and an explicit table of the infrastructure that does not exist. |
| Index | Both documents added to [`../README.md`](../README.md) §3. |
| `KI-005` | Corrected: the gate exists but has never been observed running, so it is `Implemented`, not `Verified`, and the entry says so. |
| `implementation-status.md`, `system-overview.md` | The same correction, and the branch name. |

#### What the first CI run found

The first pull request merged nothing, but it did run for the first time, and it
found a defect that had been invisible since the workflow was written:

**The OpenCode validation job had never executed its script.** It invoked
`./scripts/validate-opencode.sh` as a program, but every file under `scripts/`
was committed with mode `100644`, so the runner answered `Permission denied` and
the step exited `126`. With `continue-on-error: true`, the job reported success
anyway. Removing that flag is what surfaced it.

This is the concrete argument for the change: a gate that cannot fail is not a
gate, and the project had believed for its whole life that this one was checking
something.

The quality-gate job itself passed on the first run, in 4m15s, on the
remediation pull request — so the formatting, lint, test, release-build and
documentation steps are now observed working on a runner rather than assumed.

**A second defect followed.** With the script executable, it ran and reported
every agent as missing. The agents carry no `model:` of their own; they inherit
the default from `opencode.jsonc`, which names a provider a hosted runner has no
credentials for. Agent discovery therefore cannot succeed there, and neither can
the model check.

The validator gained `--skip-agent-discovery`, so a runner can skip exactly the
checks that need a provider while still enforcing the rest. The skip is printed
as a **warning**, never as a pass, so the output cannot be misread as "the
agents were checked". CI now runs:

```text
bash ./scripts/validate-opencode.sh --skip-model-check --skip-agent-discovery
```

which in CI still enforces that the configuration parses, that the expected
sources are loaded, and that the committed configuration contains no credential.
The agent wiring remains a **local** gate, which holds a provider by definition.

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §13.6 | The quality gates that the pipeline enforces. |
| [`../testing/verification-policy.md`](../testing/verification-policy.md) | A check that did not run is not reported as passing. |
| [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md) | Why the environments differ only in secrets and the overlay. |
| [`git-workflow.md`](git-workflow.md) | The process this entry establishes. |
| [`release-and-promotion.md`](release-and-promotion.md) | The promotion path this entry establishes. |

#### Compatibility impact

None for a client and none for the database. The change is repository
configuration and documentation.

#### Security implications

Two positive ones. The strict documentation check and the enforcing OpenCode job
are now real gates rather than advisory ones, and the production branch is
documented as receiving release merges only, which is what makes a protected
`master` meaningful when protection is configured. The repository contains no
secret: the CI signing secret is a fixed, obviously-fake placeholder used only
against a disposable database inside the runner.

#### What this entry does **not** claim

- **No deployment infrastructure exists.** There is no container image, no
  orchestrator manifest, no UAT or PROD instance. The promotion stages are
  described as gates that are applied by hand. `KI-006` and
  [`implementation-status.md`](implementation-status.md) record this, and
  [`release-and-promotion.md`](release-and-promotion.md) §6 repeats it in a
  table so that the document cannot be mistaken for a description of working
  automation.
- **Branch protection is not configured.** The two-branch model is a convention
  the pipeline enforces; it is not a GitHub rule that prevents a direct push to
  `master`. Configuring that is a repository-settings change and was not made.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Passed.** Exit code 0. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** Exit code 0. |
| `cargo test --all-targets` | **Passed.** 177 tests, 0 failed. |
| `cargo run --bin atom-docs` | **Passed.** `OK (0 warning(s))`. |
| `cargo run --bin atom-docs -- --strict` | **Passed.** `OK (0 warning(s))`. |
| `scripts/validate-opencode.ps1` | **Passed.** 0 warning(s). |
| GitHub Actions, `pull_request` | **Passed.** Run [`39761383251`](https://github.com/g-zenr/atom-api/actions/runs/39761383251) on the remediation branch, 4m15s; run [`39765107169`](https://github.com/g-zenr/atom-api/actions/runs/39765107169) on this branch, 1m25s. Both jobs green. |

---

### ATOM-CHANGE-20261022-007 - The remaining code findings, and the backlog that accounts for all of them

**Date:** 2026-10-22 (UTC)
**Type:** Fix
**Author:** The ATOM engineering team - the orchestrator with the `general`
(efficiency) subagent
**Status:** Complete

#### Summary

The audit findings that the earlier passes had not actioned are now closed:
the twenty-one code findings of `ATOM-TASK-20261022-014`, the two diagnostic
leaks of `ATOM-TASK-20261022-015`, the repository layout of
`ATOM-TASK-20261022-016`, and the five efficiency findings of
`ATOM-TASK-20261022-017` that a measurement supported. One setting was added,
because a hard-coded value turned out to be a deployment knob. The backlog was
rewritten so that every one of the eighty-three findings carries a
disposition.

#### Reason

The backlog was the weakest part of the earlier work: thirty-nine of the
eighty-three findings had **no task at all**, so their disposition existed
only in the audit report. The work was largely done, but it was not
*recorded*, and an unrecorded disposition is not a disposition.

#### Measured efficiency

The audit stated its efficiency findings as **hypotheses**. Each was measured
before it was accepted, on PostgreSQL 18 against a disposable database holding
20 000 notes for one owner, with `ANALYZE` run and
`plan_cache_mode = force_generic_plan` so that the plan shown is the one a
prepared statement actually uses.

| Finding | Before | After | Change |
| ------- | ------ | ----- | ------ |
| `F-47` the note-search page | 10.80 ms, 819 buffers, 19 000 rows discarded by a filter | 0.85 ms, 21 buffers, `BitmapAnd` of the owner index and the GIN index | **12.7× faster** |
| `F-83` the note-search count | 9.93 ms, 812 buffers | 1.33 ms, 278 buffers | **7.5× faster** |
| `F-48` the listing without a search term | 0.066 ms, 8 buffers | 0.032 ms, 5 buffers | small, and reported as small |
| `F-51` the rate-limiter sweep | one visit per tracked client per request | at most one sweep per window; an entry lives at most two windows | bounded, and now amortised |
| `F-50` an empty task list | three round trips | two | the `404`-versus-empty-page distinction preserved |

The search change is what the split statements buy: with the clause guarded by
`($3::text IS NULL OR …)` the planner under a generic plan could not use the
GIN index as a qual and scanned the owner's whole active set instead. Three
tests pin the rate limiter's boundedness, and the empty-task-list distinction
has its own test.

**One audit proposal was rejected on measurement.** `F-44` suggested dropping
the `vendored` feature from `utoipa-swagger-ui` to remove 10.73 MiB of Swagger
assets. The build was tried both ways: without `vendored` the build script
downloads the same bundle at compile time and `rust-embed` embeds it anyway, so
the embedded dist is 10.73 MiB either way. The change only cost reproducible,
offline builds. It was reverted and the finding is deferred with the reason.

#### What changed

| Area | Change |
| ---- | ------ |
| Diagnostics | The start-up path reports the error kind and the SQLSTATE and never the driver message, the connection string or a row. The test harness reports the database **name** in its refusal, never the configured URL; the separate "variable is unset" panic still shows the documented local sample URL, which is a placeholder and not a credential. |
| Configuration | `app.limits.shutdown_grace_seconds` was added: the drain was a hard-coded `sleep(100 ms)` inside the shutdown future, which meant the listener kept accepting for 100 ms and then drained **unbounded**. It is now a ceiling of 10 s on the drain, applied after the listener stops. It follows the established procedure - file, typed field, environment override, validation rule, template, reference table, and tests for the default, the override and the rejected value. |
| The request path | The timeout middleware requires the request context, which the router always installs, so the duplicated timeout string and the fallback timeout constant are gone. |
| Observability | The log format follows the **environment** rather than the build profile, and `telemetry::init` returns an error instead of falling back to `info`, which is what the configuration contract promises. |
| The token | The subject claim is a `Uuid` rather than a `String`. The wire format is unchanged, and a malformed subject is now a type error at the trust boundary. |
| The configuration module | `validate_settings` is split into one function per settings group, in a fixed order, so that a configuration that breaks several rules is always reported by the same one. The loader normalises an environment value in one place. |
| The surface | `auth_service()` is gone: the services are reached through their field, as the other three already were. `AppState::sessions()`, `AppState::tokens`, `AppState::passwords`, `ConfigLoader::directory()`, `RateLimiter::reset()` and `Messages::from_toml`'s public visibility are gone. |
| ADR-0002 | The decision is unchanged. An **Amendment** section records the dependency changes made since it was accepted, because the crate list is a consequence of the decision and not the decision itself. |
| The backlog | [`../audit/remediation-backlog.md`](../audit/remediation-backlog.md) now gives every finding a task, a status, an owner and a verification evidence, and records the eleven retained items. |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The absolute refresh lifetime, which the rotation preserves. |
| [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md) | The ownership predicates, which were checked and deliberately left as they are. |
| [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md) | The configuration layers and the rule that a setting has exactly one home. |
| [`../project-truth.md`](../project-truth.md) §13 | The password policy, whose twelve-character minimum is a floor. |

#### Compatibility impact

None for a client. The `search` bound, the page maximum, the body limit and
the configuration floors were introduced by the earlier entry in this series
and are unchanged here. The access-token wire format is unchanged: the subject
claim serialises as the same hyphenated string it always was.

**One operational behaviour changed deliberately.** Shutdown used to sleep
100 ms *before* the graceful drain even began, so the listener accepted new
connections during that window and the drain that followed had no bound at
all. The listener now stops on the signal and `app.limits.shutdown_grace_seconds`
(10 s by default) bounds the drain. A deployment that wants a shorter ceiling
sets it; a deployment that relied on the old timing never had a drain, only a
delay.

#### Security implications

Two diagnostic paths that could print a connection string or a driver message
now cannot. The start-up failure reports the error kind and the SQLSTATE, the
same rule the request path already applied in `src/error.rs`, and the test
harness reports a database name instead of a URL. No control was removed or
weakened.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Passed.** Exit code 0. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** Exit code 0, no warning. |
| `cargo test --all-targets` | **Passed.** 176 tests: 110 unit, 29 authentication, 12 notes, 25 projects and tasks. 0 failed. |
| `cargo run --bin atom-docs` | **Passed.** `atom-docs: OK (0 warning(s))`. |
| `cargo run --bin atom-docs -- --strict` | **Passed.** `atom-docs: OK (0 warning(s))`. |
| `scripts/validate-opencode.ps1` | **Passed.** `PASSED: all checks completed (0 warning(s)).` |

#### Documentation

The documents changed by this entry are listed under *Affected features and
files*. The backlog was rewritten and is the record of the dispositions.

---

### ATOM-CHANGE-20261022-006 - The remaining documentation findings, resolved

**Date:** 2026-10-22 (UTC)
**Type:** Documentation
**Author:** The ATOM engineering team - the orchestrator with the `docs`
subagent
**Status:** Complete

#### Summary

The last eight confirmed documentation defects of the audit were closed, and a
sweep of the documentation tree against the code found and corrected eleven
further inaccuracies. Four of the findings that the earlier pass had been
reported as resolved turned out to be **still open** in the tree, and are
closed here: the `F-54` family, `F-56`, `F-59` and `F-71`. The change is
documentation only: no code, no schema, no configuration and no public
contract changed.

#### Reason

A document that states something the code does not do is worse than no
document: it sends the next contributor looking for a behaviour that is not
there. The audit findings were confirmed against the source before they were
fixed, and every correction was made in the document rather than in the code.

#### The eight closed findings

| Finding | Defect | Correction |
| ------- | ------ | ---------- |
| `F-57` | Q-10 said the manifest declares `license = "MIT"`. | The manifest declares **no** licence — its comment reads "The licence has not been chosen yet." — and only `publish = false`. `README.md` and `CONTRIBUTING.md` agree with it. The decision is the choice itself, not "add the `LICENSE` file". |
| `F-58` | `module-boundaries.md` had no section for the `utils` module. | A new §7.9 with the same nine-row shape as §7.8, describing `char_len` and `is_blank`. |
| `F-61` | `system-overview.md` listed `subtle` as a direct dependency. | The row now names only the crates the manifest declares, and the note says which crates are dev-dependencies and which are not dependencies of the crate at all. |
| `F-62` | The start-up table hid the connect-info wrapper. | The real call is shown: `axum::serve(listener, api::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>())`, with a note on what the wrapper decides. |
| `F-63` | The notes specification pointed at `schema.md` §5 for the search design. | §7, which is the search design; §5 is `projects`. |
| `F-64` | `project-truth.md` §9.11 described a trusted-proxy mechanism that does not exist. | The requirement now says the API never reads the header and that there is no trusted-proxy declaration, and cross-references `Q-07` and `KI-003`. |
| `F-66` | The change log recorded a fix to the feature status files that had not been applied. | The three rows now name the covering tests and no longer say the opposite, so the record is true of the tree. |
| `F-69` | `testing/strategy.md` named `test_config()` for the test-database guard. | `test_database_url()`, which holds the assertion. |

#### What the sweep found

| Defect | Correction |
| ------ | ---------- |
| `F-54` family: the projects and tasks **specifications** still said `Implemented` and claimed that the search, the pagination, the read, the delete and the manual ordering had no HTTP-level test, while their own `status.md` said `Verified` and the tests exist. | Both specifications now say `Verified`, name the covering tests, and no longer carry the false limitations. |
| `F-56`: Q-07's context was premised on the pre-`KI-010` defect — "the server also does not install the connection information". | The context now describes the current state: the server installs the connection information, the key is the peer address, and what is missing is the trusted-proxy declaration. |
| `F-59`: `api/conventions.md` said the correlation id "appears in the structured log lines of the request". | The sentence now says what is true: the id is carried by the header and the tracing span, and is not an explicit field on every log line — as `request-lifecycle.md` §8 and `deployment.md` §6.2 already said. |
| `F-71`: `data-architecture.md` said "Two gaps are worth naming" and listed three. | The count is now correct. |
| `security/README.md` gave the unit-test counts of `src/auth/password.rs` and `src/auth/token.rs` the wrong way round (7 and 6, instead of 6 and 7). | Both counts now match the tree. |
| `security/README.md` and `secure-development.md` (SD-14) said that the CORS refusals had no automated test, and `testing/strategy.md` §6 item 1 listed the refusal of a `*` origin as uncovered. | The two configuration tests are named, and the remaining gap is narrowed to the HTTP-level behaviour: a disallowed `Origin` is answered without the CORS headers. |
| `secure-development.md` (SD-4) said the redaction of the two configuration types had no test in the tree. | `the_secret_is_never_rendered` in `src/config/tests.rs` is named. |
| `database/README.md` still said "Multi-step writes use an explicit transaction", which is the requirement `F-18` amended. | The rule now carries the exception that `ADR-0003` records. |
| `ADR-0003` still said the rate-limit key "is currently shared by every caller", which was the `KI-010` defect and is Closed. | The consequence now describes the peer-address key. The decision itself is unchanged. |
| `system-overview.md` also named `hex` and the `sqlx/json` feature, and `tower` as a runtime dependency. | The table now lists only the declared direct dependencies, and names the dev-dependencies separately. |
| `deployment.md` presented §5.2 before §5.1, and one table cell used `-` where the others use `—`. | The two subsections are in order, and the cell is consistent. |

#### Affected features and files

| Feature | Area | Files |
| ------- | ---- | ----- |
| Foundation | The engineering record | [`open-questions.md`](open-questions.md), [`known-issues.md`](known-issues.md), [`change-log.md`](change-log.md) |
| Foundation | The architecture | [`system-overview.md`](../architecture/system-overview.md), [`module-boundaries.md`](../architecture/module-boundaries.md), [`data-architecture.md`](../architecture/data-architecture.md), [`deployment.md`](../architecture/deployment.md) |
| Foundation | The public contract | [`api/conventions.md`](../api/conventions.md) |
| Foundation | The persistence documentation | [`database/README.md`](../database/README.md) |
| Foundation | The security documentation | [`security/README.md`](../security/README.md), [`secure-development.md`](../security/secure-development.md) |
| Foundation | The testing documentation | [`testing/strategy.md`](../testing/strategy.md) |
| Foundation | The requirements | [`project-truth.md`](../project-truth.md) |
| Notes, projects and tasks | The feature documents | the six `specification.md`, `status.md`, `architecture.md` and `testing.md` files of the three features |
| Foundation | The decisions | [`ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) — a stale consequence sentence, not the decision |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §9.11 | The security expectation about the client address, reworded to match the code. |
| [`../project-truth.md`](../project-truth.md) §12 | The transaction rule, with the exception `ADR-0003` records. |
| [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The token design; the rotation that preserves the absolute deadline, and the rate-limit key. |
| [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md) | The ownership predicates, which were not touched. |
| [`../audit/audit-report.md`](../audit/audit-report.md) | The findings this entry closes, and the eleven duplications that were verified as deliberate. |

#### Architectural impact

None. The modules, the boundaries, the request lifecycle and the schema are
unchanged. The only architectural document that changed is
`module-boundaries.md`, which gained the missing section for the `utils`
module, and `ADR-0003`, whose stale consequence sentence was corrected.

#### Compatibility impact

None for a client. No endpoint, status code, error code, response shape or
configuration key changed. The published pagination convention, the `search`
bound, the body limit and the token lifetimes are described exactly as they
were before.

#### Security implications

None. No control was added, removed or weakened, and no code changed. The
security documentation now says what the code does: the API never reads
`X-Forwarded-For` and has no trusted-proxy declaration, the rate-limit key is
the peer address of the connection, and the redaction of the two
configuration types is covered by a test.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `cargo run --bin atom-docs` | **Passed.** `atom-docs: OK (0 warning(s))`, exit code 0. |
| `cargo run --bin atom-docs -- --strict` | **Passed.** `atom-docs: OK (0 warning(s))`, exit code 0. |
| `cargo fmt --all -- --check` | **Not run.** This entry changed no Rust file, and the build cache is shared with the agents that are changing the code. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Not run.** For the same reason. |
| `cargo test --all-targets` | **Not run.** For the same reason. The suite was last run on 2026-10-22 against the tree of `ATOM-CHANGE-20261022-003`: 162 tests, 0 failed. |

#### Manual verification

Every test count quoted in a document that this entry touches was recounted
against the tree: 106 unit tests in 12 modules, of which 48 are in
`src/config/tests.rs`, and 56 integration tests in three files — 28 in
`tests/auth_api.rs`, 9 in `tests/notes_api.rs` and 19 in
`tests/projects_tasks_api.rs`. The 27 keys in the four files in `config/`
were compared by hand with both reference tables: the one in
`../architecture/deployment.md` §1.3, which the checker compares
mechanically, and the one in [`../../config/README.md`](../../config/README.md)
§3, which it does not. The two tables agree with the files and with each
other.

The documentation checker was run over the tree after the change.

#### Review findings

No independent review has been performed on this change set. The findings
this entry resolves were produced by the audits recorded in
[`../audit/audit-report.md`](../audit/audit-report.md) and confirmed against
the source before they were fixed. See [`review-log.md`](review-log.md).

#### Documentation

This entry is itself part of the documentation it describes. The documents
changed are listed under *Affected features and files* above.

#### Links

- [`../audit/audit-report.md`](../audit/audit-report.md) — the findings.
- `KI-003`, `KI-010` and `KI-018` in [`known-issues.md`](known-issues.md).
- `Q-07` and `Q-10` in [`open-questions.md`](open-questions.md).
- [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md).

---

### ATOM-CHANGE-20261022-003 - The audit remediation

**Date:** 2026-10-22 (UTC)
**Type:** Fix
**Author:** The ATOM engineering team - the orchestrator with the `general`
and `docs` subagents
**Status:** Complete

#### Summary

Five independent audits were run over the whole repository: duplication,
standards and architecture, documentation, efficiency, and configuration and
reusability. Eighty-three findings were confirmed, eleven deliberate
duplications were verified as correct and recorded as such, and the highest
value findings were remediated. Two of them were real behavioural defects:
the configured pagination bounds were validated and published but never
applied, and a refresh-token rotation silently extended the absolute lifetime
that ADR-0003 requires.

The findings, their evidence and the retained duplications are in
[`../audit/audit-report.md`](../audit/audit-report.md). The task backlog is
[`../audit/remediation-backlog.md`](../audit/remediation-backlog.md).

#### Reason

A setting an operator can change that changes nothing is worse than no
setting: the operator believes the system behaves as configured. The same
applies to a documented absolute lifetime that the rotation does not keep.
Both were found by more than one audit, independently.

#### The two behavioural defects

| Finding | Defect | Resolution |
| ------- | ------ | ---------- |
| `F-01` | `api.pagination.default_limit` and `max_limit` were read into the typed settings, validated at start-up and published in the operator reference, but the list endpoints clamped against compile-time constants. A setting an operator could change had no effect. | `PaginationQuery::effective_limit` now takes the configured bounds, and the three services hold them. The constants `DEFAULT_LIMIT` and `MAX_LIMIT` are gone, so the published convention has exactly one home. |
| `F-02` | The refresh rotation wrote the replacement session with `expires_at = now + ttl`, which converts the absolute lifetime of ADR-0003 into a sliding one: a client that keeps rotating holds a session for ever. | The replacement session inherits the deadline of the session it replaces. An integration test asserts that the rotated row carries the same `expires_at`. |

#### The rest of the remediation

| Area | What changed |
| ---- | ------------ |
| Configuration | The `app.name` key and the `messages.general.body_mismatch` key were removed: nothing read them. The password minimum, the clock-skew allowance, the prune grace, the two token lifetimes, the page maximum and the body limit gained the floors and bounds that keep a deployment from configuring away an accepted requirement or a safety ceiling. |
| Validation | The fifteen rules that had no test now have one each: twenty-two new configuration tests. Two `Default` implementations that returned zero — `PaginationSettings` and `RateLimitSettings` — now match the serde defaults. |
| Error handling | The start-up failure is printed with its `Display` form, so the operator sees `error: required environment variable DATABASE_URL is missing or empty` instead of `Error: Missing("DATABASE_URL")`. |
| Duplication | `field_error` is one public helper instead of two private copies. The catalogue has one `too_long` template instead of three byte-identical ones. The session-creation sequence is one helper used by both the first session and a rotation. `AuthService` reaches the user through the `UserService` it already holds instead of a second repository over the same pool. |
| Dead code | `AppState::sessions()`, `AppState::tokens`, `AppState::passwords`, `ConfigLoader::directory()`, `RateLimiter::reset()`, `RequestSettings` and the `x-request-id` literal were removed or consolidated. |
| Tooling | The documentation checker compares the keys in `config/*.toml` with the keys in the operator reference, in both directions, and it no longer filters `ATOM_CONFIG_DIR` out of the environment-variable comparison. It found two wrong key names and one missing key on the first run. |
| Dependencies | The `axum/macros`, `tokio/sync` and `sqlx/json` features and the `hex` crate were removed. The refresh-token hash is now formatted with `format!("{hasher:x}")`, and a literal assertion proves the encoding is unchanged. |
| Build | The development profile keeps line tables only. The build artifacts fell from 4.4 GB to 1.45 GB, a measured 67% reduction. |
| Review | The change was reviewed independently by the read-only `code-reviewer` agent. Its seventeen findings are recorded in [`review-log.md`](review-log.md) as `ATOM-REVIEW-20261022-002`; the seven it required are resolved here. |

#### Affected features and files

| Feature | Area | Files |
| ------- | ---- | ----- |
| Foundation | The pagination convention | [`../../src/pagination.rs`](../../src/pagination.rs), [`../../src/notes/service.rs`](../../src/notes/service.rs), [`../../src/projects/service.rs`](../../src/projects/service.rs), [`../../src/tasks/service.rs`](../../src/tasks/service.rs), [`../../src/state.rs`](../../src/state.rs) |
| Foundation | The configuration | [`../../src/config/`](../../src/config/), [`../../config/`](../../config/), [`.env.example`](../../.env.example) |
| Foundation | The request lifecycle | [`../../src/api/`](../../src/api/), [`../../src/helpers/`](../../src/helpers/), [`../../src/utils/`](../../src/utils/) |
| Authentication | The token lifecycle | [`../../src/auth/`](../../src/auth/), [`../../tests/auth_api.rs`](../../tests/auth_api.rs) |
| Foundation | The start-up | [`../../src/main.rs`](../../src/main.rs) |
| Foundation | The repository checker | [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs) |
| Foundation | The build | [`../../Cargo.toml`](../../Cargo.toml) |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The absolute refresh lifetime, which the rotation now preserves. |
| [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md) | The ownership predicates, which were checked and deliberately left as they are. |
| [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md) | The configuration layers, the precedence and the rule that a setting has exactly one home. |
| [`../project-truth.md`](../project-truth.md) §13 | The password policy, whose twelve-character minimum is now a floor. |

#### Architectural impact

None of the module boundaries changed. The configuration module gains two
exported floors, and the HTTP layer reads the body-reader ceiling from the
configuration module instead of declaring its own copy, so that the ceiling
has one home.

#### Compatibility impact

None for a client. The pagination bounds keep their committed values of 20 and
100, so the published convention is unchanged; only the *source* of the bound
changed. The `search` parameter of the notes and projects listings gains a
bound of 200 characters, which is a `422` naming the `search` field. A
configuration that sets the body limit above 4 MiB, the password minimum below
twelve characters, the clock skew above five minutes or the prune grace below
one hour is now refused at start-up, with a message that names the bound. A
configuration that sets the pagination bounds differently from 20 and 100 now
takes effect — that is the point of the fix, and the setting still has no
environment variable, so it cannot change by accident.

#### Security implications

Two properties are now enforced rather than documented: the absolute lifetime
of a refresh session survives a rotation, and the configuration cannot lower
the password minimum, widen the clock-skew allowance or shorten the prune
grace below its floor. No control was removed or weakened. The secrets are
untouched: both remain environment-only and are still redacted in every
`Debug` output.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Passed.** Exit code 0, no diff. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** Exit code 0, no warning. |
| `cargo test --all-targets` | **Passed.** 162 tests: 106 unit, 28 authentication, 9 notes, 19 projects and tasks. 0 failed. |
| `cargo run --bin atom-docs` | **Passed.** `atom-docs: OK (0 warning(s))`, exit code 0. |
| `cargo run --bin atom-docs -- --strict` | **Passed.** `atom-docs: OK (0 warning(s))`, exit code 0. |
| `scripts/validate-opencode.ps1` | **Passed.** `PASSED: all checks completed (0 warning(s)).` |
| `cargo build --release` | **Passed.** Exit code 0. The binary is 19.35 MB. |
| The release binary, started against the disposable test database | **Passed.** `GET /health/live` answered `200`. `GET /api/v1/openapi.json` answered `200`, carries the title `ATOM API` and the version `0.1.0`, and declares the `422` on `GET /api/v1/notes`. `GET /api/v1/notes` without a bearer token answered `401` in the standard envelope. |

#### Manual verification

Every finding that was fixed was read out of the source before it was fixed,
and the fixed behaviour was read back out of the source afterwards. The
configuration keys were compared with the files in `config/` and with the
operator reference by the checker itself, in both directions. The pagination
bounds were confirmed to be applied by the unit test that a configured maximum
is honoured, and the rotation deadline by the integration test that asserts
the inherited `expires_at`.

#### Reviewer findings

The findings that this entry resolves were produced by five independent
audits and confirmed against the source. The full list, with the evidence and
the eleven duplications that were verified as deliberate, is in
[`../audit/audit-report.md`](../audit/audit-report.md).

The change was then reviewed independently by the read-only `code-reviewer`
agent, recorded in [`review-log.md`](review-log.md) as
`ATOM-REVIEW-20261022-002`. It returned **ACCEPT WITH REQUIRED FOLLOW-UP**
with seventeen findings; the seven it required are resolved in the same
change, and the re-verification is recorded with the review.

#### Documentation

The documents changed by the parallel documentation pass are listed in
`ATOM-CHANGE-20261022-002` above. The audit report and the backlog are new
documents under [`../audit/`](../audit/).

#### Links

- [`../audit/audit-report.md`](../audit/audit-report.md) — the five audits,
  the eighty-three findings and the retained duplications.
- [`../audit/remediation-backlog.md`](../audit/remediation-backlog.md) — the
  task backlog, with the status of every task.
- `KI-015`, `KI-016`, `KI-017` and `KI-018` in
  [`known-issues.md`](known-issues.md).

---

### ATOM-CHANGE-20261022-002 - The documentation findings, resolved

**Date:** 2026-10-22 (UTC)
**Type:** Documentation
**Author:** The ATOM engineering team - the orchestrator with the `docs`
subagent
**Status:** Complete

#### Summary

Fourteen confirmed documentation defects were remediated, together with the
two behaviour changes that were landing in parallel. The body-limit contract,
the CORS layer, the configuration reference, the pagination convention, the
refresh-rotation lifetime policy and the engineering record were each
brought back into agreement with the code. Three new known issues were opened
and closed, a fourth was opened to record a discrepancy between a finding and
the code, one open question was annotated, and the requirement about
transactions was amended to name the exception that ADR-0003 already records.

#### Reason

The documentation is part of the product, and a document that states
something the code does not do is worse than no document: it sends the next
contributor looking for a behaviour that is not there. The findings were
confirmed against the source before they were fixed, and each fix was made in
the document rather than in the code.

#### Affected features and files

| Feature | Area | Files |
| ------- | ---- | ----- |
| Foundation | The API contract | [`../api/conventions.md`](../api/conventions.md) |
| Foundation | The operator-facing configuration reference | [`../architecture/deployment.md`](../architecture/deployment.md) |
| Foundation | The request lifecycle | [`../architecture/request-lifecycle.md`](../architecture/request-lifecycle.md) |
| Foundation | The module interfaces | [`../architecture/module-boundaries.md`](../architecture/module-boundaries.md) |
| Foundation | The persistence model | [`../architecture/data-architecture.md`](../architecture/data-architecture.md), [`../database/schema.md`](../database/schema.md) |
| Foundation | The security architecture | [`../architecture/security-architecture.md`](../architecture/security-architecture.md) |
| Authentication | The token lifecycle | [`../features/authentication/specification.md`](../features/authentication/specification.md), [`../features/authentication/architecture.md`](../features/authentication/architecture.md), [`../features/authentication/testing.md`](../features/authentication/testing.md), [`../features/authentication/status.md`](../features/authentication/status.md), [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) |
| Notes and projects | The list contract | [`../features/notes/api.md`](../features/notes/api.md), [`../features/notes/specification.md`](../features/notes/specification.md), [`../features/notes/architecture.md`](../features/notes/architecture.md), [`../features/projects/api.md`](../features/projects/api.md), [`../features/projects/specification.md`](../features/projects/specification.md), [`../features/projects/architecture.md`](../features/projects/architecture.md), [`../features/tasks/api.md`](../features/tasks/api.md) |
| Security | The rules a change must follow | [`../security/secure-development.md`](../security/secure-development.md) |
| Quality | The testing strategy | [`../testing/strategy.md`](../testing/strategy.md) |
| Product | The requirements | [`../project-truth.md`](../project-truth.md) |
| Engineering | The record | [`known-issues.md`](known-issues.md), [`open-questions.md`](open-questions.md), [`implementation-status.md`](implementation-status.md) |

#### The findings, and what each one became

The identifiers in the table below are the **per-audit** ones, as they were
recorded when the finding was made. The consolidated identifiers used by
[`ATOM-CHANGE-20261022-003`](../audit/audit-report.md) and by the backlog are
different; the mapping is in the *status* line of each finding in
[`../audit/audit-report.md`](../audit/audit-report.md).

| Finding | Defect | Resolution |
| ------- | ------ | ---------- |
| `F-03`, `CFG-10` | The API conventions described a second, independent 4 MiB body cap, which the finding said the code did not have. | The code changed under this pass: the extractor now passes a named `MAX_BODY_BYTES_BACKSTOP` of 4 MiB, so the two-limit description is accurate again. §9 was rewritten to describe **both** limits, and `KI-018` records the discrepancy between the finding and the code for the architect to settle. |
| `F-04` | Three passages described a `panic!` in the CORS layer and called it an open finding. | The layer falls back to an empty header value. The request-lifecycle and the secure-development rules now describe that behaviour, and the "open finding" wording is gone. |
| `F-05` | The requirements demanded an explicit transaction for every multi-step write, which the refresh rotation does not use. | The requirement now reads "unless an accepted ADR records that the ordering fails safe" and cites ADR-0003. The meaning is otherwise unchanged. |
| `F-06` | The header comment of the applied migration `0001` claimed the `projects → tasks` relation is handled in the service layer. | The comment was **not** edited: the migration is applied, and editing it would change its SQLx checksum. The correction is recorded in [`../database/schema.md`](../database/schema.md) §1.1 and here. The relation is a database cascade, declared in `0003`. |
| `F-07` | The operator-facing start-up failure was not described. | A new §1.5 in [`../architecture/deployment.md`](../architecture/deployment.md) shows the readable message the operator sees, in the four shapes `ConfigError` renders. |
| `F-09` | The `AppState` public interface listed `sessions()`, `tokens` and `passwords`. | §7.3 of the module boundaries now lists what remains, and the security boundary row follows the services that hold the signing secret and the Argon2 parameters. |
| `F-11` | The implementation status said the configuration module had no unit test at all, while the table above it said `Verified`. | The closing paragraph was rewritten: the module has its own unit tests and the rows are `Verified`. The test counts it quoted were stale and were corrected at the same time. |
| `F-12` | The security architecture said the `429` path was not covered by the suite. | The row now records `the_authentication_endpoints_are_rate_limited` and `KI-002` as Closed. |
| `CFG-09` | The reference table named `auth.jwt_issuer` and `auth.jwt_audience`; the keys in `config/auth.toml` are `auth.issuer` and `auth.audience`. | The rows were corrected, and every other key in the table was checked against the files in `config/`. The `app.name` key, which the parallel configuration work removed, was dropped from the table at the same time. |
| `CFG-15` | The production checklist said to leave `APP_CORS_ALLOWED_ORIGINS` empty, but the template shipped a value that overrides the committed default. | The precedence is now stated explicitly in §1.1 and in a new §5.2, and the checklist says the variable must name the real origins or stay empty. The template now ships an empty value, so the committed defaults and the template agree. |
| `F-14` | The `search` parameter had no documented length bound. | The bound of 200 characters is documented in the conventions, in the notes and projects contracts, in the two specifications and in the testing gaps, together with the shared validator `validate_search` that enforces it and the catalogue message that words it. |
| `KI-015` | The refresh rotation recomputed the session expiry, extending the absolute lifetime. | Opened and closed: the replacement session inherits the original `expires_at`. The property is recorded in ADR-0003, in the data architecture, in the schema and in the authentication documents. |
| `KI-016`, `KI-017` | The configuration accepted a password minimum below the policy floor, an unbounded clock skew and a prune grace shorter than the expired-token window needs. | Opened and closed: the floors are enforced in `src/config/validation.rs` and recorded in the constraint column of the reference table. |
| `KI-018` | The finding said the extractor had no second body cap; the code has an independent 4 MiB backstop. | Opened: the documentation describes the code, and the discrepancy is recorded for the architect to settle. |

Two further inconsistencies were found while the findings were being fixed,
and are corrected here as well: the authentication specification still
described the feature as `Implemented` because of the rate-limit gap that
`KI-002` closed, and the testing strategy still said that the configuration
module had no unit test while listing twenty-seven of them.

The unit-test inventory in [`../testing/strategy.md`](../testing/strategy.md)
§1.1 was corrected as well: the table double-counted the `auth` module, and
the counts were stale. The table now lists 106 unit tests in 12 modules,
counted on 2026-10-22, of which the configuration module contributes 46.

**Correction (2026-10-22).** The configuration module contributes **48** unit
tests, not 46. The audit remediation recorded in `ATOM-CHANGE-20261022-003`
added the tests for the validation rules that had none, and the current
figure is 48 in [`../../src/config/tests.rs`](../../src/config/tests.rs). The
totals are unchanged: 106 unit tests in 12 modules, and 162 tests in the
suite as a whole.

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §12 | The persistence requirements, including the amended transaction rule. |
| [`../project-truth.md`](../project-truth.md) §16 Q3 | The refresh-token lifetime question, of which the absolute-lifetime half is now settled by ADR-0003. |
| [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The token design: the absolute lifetime, the rotation that preserves it, and the session-pruning grace. |
| [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md) | The configuration layers and the precedence the reference table describes. |
| [`../database/migration-policy.md`](../database/migration-policy.md) | The rule that an applied migration is never edited. |

#### Architectural impact

None. No code, schema, contract or configuration changed by this entry: it is
the documentation pass that accompanies the behaviour changes landing in
parallel. Three of the documented behaviours are new or newly enforced — the
pagination bounds are read from the configuration, the refresh rotation
preserves the absolute deadline, and the configuration refuses a password
minimum, a clock skew, a session prune grace and the two token lifetimes
outside their bounds — and the documents now describe them as the truth.

#### Compatibility impact

None for a client. The pagination bounds keep their committed values of 20
and 100, and the documents now say plainly that changing either of them is a
breaking change for a client. The `search` parameter gains a documented
length bound of 200 characters; a longer term is a `422` validation failure
that names the `search` field. The body limit keeps both of its caps: the
configured `app.limits.max_body_bytes` and the independent 4 MiB backstop in
the extractor, which is recorded as `KI-018`.

#### Security implications

None of the findings was a security defect in the code, and no control was
added, removed or weakened by this entry. Two security properties are now
described accurately rather than falsely: the absolute lifetime of a refresh
session is preserved by the rotation, and the configuration floors cannot be
configured away.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `cargo run --bin atom-docs` | **Passed.** `atom-docs: OK (0 warning(s))`, exit code 0. |
| `cargo fmt --all -- --check` | **Not run.** This entry changed no Rust file, and the build cache is shared with the agents that are changing the code. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Not run.** For the same reason. |
| `cargo test --all-targets` | **Not run.** For the same reason. |

#### Manual verification

Every configuration key in the reference table was read out of the files in
[`../../config/`](../../config/) and compared with the table, and every
environment variable was compared with the override in
[`../../src/config/loader.rs`](../../src/config/loader.rs). The two keys that
did not match — `auth.jwt_issuer` and `auth.jwt_audience` — were corrected,
and the key the parallel configuration work removed — `app.name` — was
dropped.

The `unwrap`, `expect` and `panic!` inventory was run over
[`../../src/`](../../src/), and the remaining hits are in test modules, in
[`../../src/telemetry.rs`](../../src/telemetry.rs) and in the default-value
helpers of [`../../src/config/settings.rs`](../../src/config/settings.rs).
The CORS layer contains no `panic!`.

#### Reviewer findings

The findings that this entry resolves were produced by the review of the
documentation tree and confirmed against the source. No independent review
has been performed on this change set; see [`review-log.md`](review-log.md).

#### Documentation

This entry is itself part of the documentation it describes. The documents
changed are listed under *Affected features and files* above.

#### Links

- [`../database/schema.md`](../database/schema.md) §1.1 — the correction to the
  stale migration comment.
- `KI-015`, `KI-016`, `KI-017` and `KI-018` in
  [`known-issues.md`](known-issues.md).
- `Q-03` in [`open-questions.md`](open-questions.md).
- [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md).

---

### ATOM-CHANGE-20261022-001 - The configuration centralisation

**Date:** 2026-10-22 (UTC)
**Type:** Refactor
**Author:** The ATOM engineering team - the orchestrator with the `rust-backend`
and `docs` subagents
**Status:** Complete

#### Summary

The application configuration was replaced. It used to be a single structure
read from the process environment by one `Config::from_env` in `src/config.rs`.
It is now three layers, resolved once at start-up by the module
`src/config/`: the committed files in `config/`, the environment overlay
`config/environments/<APP_ENV>.toml`, and the environment variables. Later
layers win, key by key. The two secrets have no file layer at all: they are
the `DATABASE_URL` and `APP_JWT_SECRET` environment variables, and no file in
`config/` may hold a secret. The strings that reach a client through the error
envelope moved into `config/messages.toml` — except the two that must never be
replaced, which stay in the error type — the stable error codes into a `code`
module in `src/error.rs`, three feature modules now share two text validators,
and the dead `limit` and `offset` fields were removed from the three
list-query DTOs.

#### Reason

A configuration that lives only in environment variables has no source of truth
in the repository: the operator-facing reference is prose, the values cannot be
reviewed, and a typo in a variable name is discovered when the affected
request runs rather than when the process starts. The client-facing messages
had the same problem in a smaller form — they were scattered over the source,
so a wording change meant a search across the feature modules.

Centralising the configuration makes it reviewable and makes a
misconfiguration a start-up error instead of a runtime surprise. It also gives
one place to enforce the rules that were previously spread across the code:
the rate limit may not be zero, the pool size may not be zero, the timeouts
may not be zero, the Argon2 cost may not be zero, the issuer and the audience
may not be empty, the CORS allowlist may not contain a wildcard, the
pagination bounds must be consistent, and the log filter must parse.

#### Affected features and files

| Feature | Area | Files |
| ------- | ---- | ----- |
| Foundation | The configuration module and the committed files | [`../../src/config/`](../../src/config/) — `mod.rs`, `settings.rs`, `loader.rs`, `validation.rs`, `messages.rs` — and [`../../config/`](../../config/) |
| Foundation | The environment-variable template | [`../../.env.example`](../../.env.example) |
| Foundation | The error contract and the message catalogue | [`../../src/error.rs`](../../src/error.rs), [`../../config/messages.toml`](../../config/messages.toml), [`../../src/config/messages.rs`](../../src/config/messages.rs) |
| Notes, projects and tasks | The shared validators and the list-query DTOs | [`../../src/helpers/validation.rs`](../../src/helpers/validation.rs), [`../../src/notes/dto.rs`](../../src/notes/dto.rs), [`../../src/projects/dto.rs`](../../src/projects/dto.rs), [`../../src/tasks/dto.rs`](../../src/tasks/dto.rs) |
| Authentication | The timing-equalisation hash | [`../../src/auth/password.rs`](../../src/auth/password.rs) |
| Tooling | The harness starts from the real configuration | [`../../tests/common/mod.rs`](../../tests/common/mod.rs) |
| Documentation | The security, testing and engineering record | The documents listed under *Documentation* below |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §4.1 | Typed, validated configuration read once at start-up. |
| [`../project-truth.md`](../project-truth.md) §4.1 | Centralized error contract with a stable error code. |
| [`../project-truth.md`](../project-truth.md) §9 | The security expectations the new start-up validation enforces. |
| [`../project-truth.md`](../project-truth.md) §13 | The testing requirements and the quality gates. |
| [`../decisions/ADR-0002-rust-axum-postgresql.md`](../decisions/ADR-0002-rust-axum-postgresql.md) | The technology stack. |
| [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The token lifetimes and the session-pruning grace, which is a security window and not a tuning knob. |
| [`../decisions/ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product. |

#### Architectural impact

The configuration module became a directory with a fixed internal layout: the
typed structures, the loader, the validation rules and the message catalogue.
The settings structures use `deny_unknown_fields`, so a typo in a
configuration file is a start-up error rather than a silently ignored setting.
The `Config` value is still resolved once and passed down explicitly through
`AppState`; there is still no global mutable state and no `env::var` call
outside the loader.

The feature modules keep their layering. The shared validators in
`src/helpers/validation.rs` are a new shared module, and the trimming
behaviour is deliberately not uniform: a title, a name and a display name are
trimmed, while a body and a description are kept verbatim, because the
database `CHECK` constraints measure the stored value.

#### Compatibility impact

- **The OpenAPI document no longer publishes the pagination parameters
  twice.** The three list-query DTOs declared `limit` and `offset` next to
  the shared `PaginationQuery`, which duplicated them in the generated
  document. The feature-specific query parameters are now the filters only:
  `search` and `archived` for notes, `search` for projects, `status` and
  `priority` for tasks.
- **The `limit` and `offset` query parameters are unchanged.** They are still
  accepted on every list endpoint, with the same defaults and the same
  clamping, and they now come from the shared pagination parameters.
- **The error envelope is unchanged.** The shape is still
  `{ "error": { "code", "message", "details" } }`, and the status codes are
  the same.
- **The error `code` values are unchanged.** They were moved into a `code`
  module in `src/error.rs` so that each code has exactly one definition; the
  strings themselves did not change.
- **The environment variables are preserved, and new ones were added.** Every
  variable the previous configuration read is still read, with the same name
  and the same meaning. `APP_ENV`, which the process already read and which
  enables the production placeholder-secret check, is now documented in
  `.env.example`. The variables the loader reads today are
  `APP_ACCESS_TOKEN_CLOCK_SKEW_SECONDS`, `APP_ACCESS_TOKEN_TTL_SECONDS`,
  `APP_API_DOCS_ENABLED`, `APP_ARGON2_ITERATIONS`, `APP_ARGON2_MEMORY_KIB`,
  `APP_ARGON2_PARALLELISM`, `APP_AUTH_RATE_LIMIT_MAX_REQUESTS`,
  `APP_AUTH_RATE_LIMIT_WINDOW_SECONDS`, `APP_BIND_ADDR`,
  `APP_CORS_ALLOWED_ORIGINS`, `APP_ENV`, `APP_JWT_AUDIENCE`, `APP_JWT_ISSUER`,
  `APP_JWT_SECRET`, `APP_LOG_FILTER`, `APP_MAX_BODY_BYTES`,
  `APP_READINESS_PROBE_TIMEOUT_SECONDS`, `APP_REFRESH_TOKEN_TTL_SECONDS`,
  `APP_REQUEST_TIMEOUT_SECONDS`, `APP_SESSION_PRUNE_GRACE_SECONDS`,
  `APP_SESSION_PRUNE_INTERVAL_SECONDS`, `DATABASE_ACQUIRE_TIMEOUT_SECS`,
  `DATABASE_CONNECT_TIMEOUT_SECS`, `DATABASE_MAX_CONNECTIONS` and
  `DATABASE_URL`.
- **The message text is not part of the contract.** The client-facing strings
  now come from `config/messages.toml`; a client must switch on the `code`.

#### Security implications

- The signing secret and the database URL have **no file layer**. The
  configuration directory is committed and embedded in the binary, so a secret
  in a file there would be a secret in the published source. The rule and the
  check that enforces it are in
  [`../security/secure-development.md`](../security/secure-development.md)
  SD-4.
- A misconfiguration now stops the process before the socket is bound. A
  wildcard or an invalid origin in the CORS allowlist is a start-up error
  rather than a panic in the router; the rate limit, the pool size, the
  timeouts, the Argon2 cost, the issuer, the audience, the pagination bounds
  and the log filter are all checked at start-up.
- The timing-equalisation hash is computed from the configured Argon2 cost
  parameters, so verifying against it costs exactly what verifying against a
  stored password costs. The `DUMMY_HASH` constant is removed.
- The types that carry a secret are `AuthSettings` and `DatabaseSettings`;
  they still print `[REDACTED]` in their `Debug` output.
- The two messages that must never be replaced — the one for a database failure
  and the one for an unexpected failure — stay in the error type. The code
  does not read them from the catalogue, so a change to
  `config/messages.toml` cannot make them reveal an internal detail.

#### Checks and results

The commands were run on 2026-10-22 against the tree that contains this
entry, at 02:45 UTC.

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Passed.** No formatting differences. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** No lint finding and no suppressed lint. |
| `cargo test --all-targets` | **Passed.** 133 tests: 80 unit, 26 authentication, 8 notes, 19 projects and tasks. No failure. |
| `cargo build --release` | **Passed.** |
| `cargo run --bin atom-docs` | **Passed.** `atom-docs: OK (0 warning(s))`. |
| `scripts/validate-opencode.ps1` | **Passed.** No warning. |

**The configuration module has 27 unit tests of its own**, in
[`../../src/config/tests.rs`](../../src/config/tests.rs). They cover the
documented defaults, the precedence between the three layers - a file on disk
over the embedded copy, the environment overlay over the base, and an
environment variable over the overlay - the two required secrets, every
validation rule and the message catalogue. The environment is injected into the
loader rather than read from the process, so the tests neither mutate the
environment of the test process nor depend on each other.

**A running server was exercised.** The API was started against the local
development database and answered `/health/live`, `/health/ready`, a
registration, a note creation, a note listing, the OpenAPI document and the
API explorer.

#### Manual verification

The configuration reference was read out of the source before it was written:
every setting in [`../../config/README.md`](../../config/README.md) was checked
against the key in the file and against the environment-variable override in
[`../../src/config/loader.rs`](../../src/config/loader.rs), and every
validation rule was checked against
[`../../src/config/validation.rs`](../../src/config/validation.rs).

The command that proves that no committed configuration file holds a
secret-shaped value was run against the tree:

```text
Get-ChildItem -Recurse -File -Path config -Filter *.toml |
  Select-String -Pattern '(jwt_secret|database_url|password|secret|token|api_key)\s*=\s*"[^"]{8,}"' -CaseSensitive:$false
```

It reported no match.

The documentation checker was run as well and reported no error and no
warning, which means that every relative link in the documents this entry
wrote resolves and that `.env.example` and the loader agree about the
environment variables.

#### Reviewer findings

**No independent review has been performed on this change set.** The
`code-reviewer` and the `quality-gate` agents are read-only and had not been
run against this change when the entry was written, so this section is empty
by fact and not by omission. See [`review-log.md`](review-log.md).

#### Documentation

| Document | Change |
| -------- | ------ |
| [`../../config/README.md`](../../config/README.md) | The configuration reference: the layers, the precedence, the settings, the secrets, the message catalogue and the validation rules. |
| [`../../.env.example`](../../.env.example) | Every environment variable, including `APP_ENV`. |
| [`../architecture/deployment.md`](../architecture/deployment.md) | The operator-facing configuration reference. |
| [`../security/threat-model.md`](../security/threat-model.md) | The assets, the trust boundary and the threats that the configuration touches. |
| [`../security/secure-development.md`](../security/secure-development.md) | SD-4, SD-5, SD-12 and SD-14, and the check that no committed configuration file holds a secret. |
| [`../security/README.md`](../security/README.md) | The control table: the paths, and the tests that were missing from it. |
| [`../testing/strategy.md`](../testing/strategy.md) | The unit-test inventory, the harness configuration, the configuration-loader coverage and the list of what is not covered. |
| [`../testing/verification-policy.md`](../testing/verification-policy.md) | The configuration gate. |
| [`../engineering/known-issues.md`](../engineering/known-issues.md) | `KI-014`, the remaining configuration limitations. |
| [`../engineering/open-questions.md`](../engineering/open-questions.md) | `Q-12`, the reload question. |
| [`../engineering/implementation-status.md`](../engineering/implementation-status.md) | The configuration, the message catalogue and the shared validators. |
| [`../../AGENTS.md`](../../AGENTS.md) | The layout, the configuration rule, the documentation intake and the secrets list. |
| [`../../README.md`](../../README.md), [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) | The setup, the stack and the repository layout. |

#### Links

- The configuration reference: [`../../config/README.md`](../../config/README.md).
- The operator-facing reference:
  [`../architecture/deployment.md`](../architecture/deployment.md).
- `KI-014` in [`known-issues.md`](known-issues.md).
- `Q-12` in [`open-questions.md`](open-questions.md).

---

### ATOM-CHANGE-20261022-005 - The quality-gate findings, resolved

**Date:** 2026-10-22 (UTC)
**Type:** Defect fixes
**Author:** The ATOM engineering team - the orchestrator with the `quality-gate`
subagent
**Status:** Complete

#### Summary

The read-only `quality-gate` agent judged the delivery and returned **FAIL**,
with one blocking finding, seven required findings and four advisories. All of
them are addressed. The blocking finding was a false claim in the engineering
record: the OpenAPI document did not declare `401` on the two endpoints that
return it, while four documents asserted that it did.

#### Reason

[`../testing/verification-policy.md`](../testing/verification-policy.md)
requires a quality gate that judges the evidence rather than the summary of the
agent that produced it. The gate found exactly the class of defect the role
exists to catch: a record that claimed a verification that had not held.

#### Affected features and files

| Finding | Severity | Resolution |
| ------- | -------- | ---------- |
| The OpenAPI document omitted `401` on `POST /auth/login` and `POST /auth/refresh`, and no endpoint declared `408` | Blocking | Both endpoints answer `401` when the credentials or the refresh token are wrong, and the request timeout can produce a `408` on any endpoint. The annotations now declare both. |
| The CI pipeline was recorded as non-existent in four documents | Required | [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) exists; the status is now `Implemented` in the README, in [`../project-truth.md`](../project-truth.md) §17 and in [`implementation-status.md`](implementation-status.md). `KI-005` is Closed. |
| `implementation-status.md` said the change set had never been reviewed | Required | It now points at `ATOM-REVIEW-20261021-001`. |
| `testing/strategy.md` misstated the size of the suite and listed closed gaps | Required | 53 integration tests, not 30; the five closed gaps are removed and the remaining list is renumbered. |
| Two feature status files understated their own evidence | Required | The `PROJ-R02`, `TASK-R01` and `TASK-R04` rows now name the covering tests. |
| `security/README.md` claimed two controls had no test | Required | Both rows name the covering tests. |
| **The CI workflow would not have created the test database it points at** | Required | The official `postgres` image ignores `POSTGRES_MULTIPLE_DATABASES`. The workflow now creates `atom_test` with `psql` before the test step, and the compose file no longer relies on the non-existent variable. |
| Two records still described the rate limiter as sharing one bucket | Required | `Q-07` and `KI-012` now describe the post-`KI-010` behaviour. |
| Four advisory items | Advisory | A wrong document path, a mislabelled link, a stale sentence about the session revocation and a stale description of the rate-limiter row. |

**Correction (2026-10-22).** The row "Two feature status files understated
their own evidence" was written before the change had actually landed: the
three rows still carried the sentences that understated the evidence, even
though the tests they name existed in the tree. The sentences have now been
removed, so the row is true of the tree: the `PROJ-R02`, `TASK-R01` and
`TASK-R04` rows in
[`../features/projects/status.md`](../features/projects/status.md) and
[`../features/tasks/status.md`](../features/tasks/status.md) name the
covering tests and no longer say the opposite.

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §11.3 | `401` for invalid credentials. |
| [`../project-truth.md`](../project-truth.md) §7.8 | The OpenAPI document describes the API that actually exists. |
| [`../api/error-contract.md`](../api/error-contract.md) | The stable error codes and their status codes. |

#### Architectural impact

None. The route table, the public contracts and the schema are unchanged; the
OpenAPI document now describes them accurately.

#### Compatibility impact

The generated OpenAPI document now declares `401` on the login and refresh
operations, and `408` on every operation. A client generated from the
document will handle the `401` of a failed login and the `408` of a slow
request.

#### Security implications

None of the findings was a security defect. The rate limiter, the ownership
predicates and the secret handling were confirmed by the gate to be as
documented.

#### Tests and checks actually executed

| Check | Result |
| ----- | ------ |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed. |
| `cargo test --all-targets` | Passed: 39 unit, 26 authentication, 8 notes, 19 projects and tasks. 92 in total. |
| `cargo run --bin atom-docs` | Passed, no error and no warning. |
| `scripts/validate-opencode.ps1` | Passed, no warnings. |
| The generated OpenAPI document | 22 operations; `201` on the creates, `204` with no body on the deletes, `401` on `login` and `refresh`, `408` on every operation, and the bearer security scheme registered. |

#### Reviewer findings

Recorded in [`review-log.md`](review-log.md). The quality gate's own
limitations - it could not run `cargo run --bin atom-docs` or the OpenCode
validation script, because the read-only shell allow-list refuses them - are
recorded with its verdict.

---

### ATOM-CHANGE-20261021-004 - The independent review findings, resolved

**Date:** 2026-10-21 (UTC)
**Type:** Defect fixes
**Author:** The ATOM engineering team - the orchestrator with the `code-reviewer`
subagent
**Status:** Complete

#### Summary

The Phase 1 implementation was reviewed by the read-only `code-reviewer`
agent. The review returned **APPROVE WITH COMMENTS** and eleven findings. The
three Medium findings and the eight Low findings were all addressed. No
authorization bypass, no SQL injection, no secret leak, no `unwrap` in a
request path, no lock held across an `.await` and no PATCH-semantics defect
were found.

#### Reason

[`../testing/verification-policy.md`](../testing/verification-policy.md)
requires an independent review of a substantial change, and a recorded
resolution of every finding. The findings were real: the refresh-token reuse
detection was only half-implemented, and the request timeout answered with an
empty body while the published error contract promised the standard envelope.

#### Affected features and files

| Finding | Severity | Resolution |
| ------- | -------- | ---------- |
| Argon2id hashing ran on the Tokio worker thread | Medium | [`../../src/auth/service.rs`](../../src/auth/service.rs) moves hashing and verification to `tokio::task::spawn_blocking`. ADR-0002 calls the "no blocking work in a request path" rule load-bearing; it is now true. |
| Two concurrent refreshes of the same token both succeeded | Medium | [`../../src/auth/repository.rs`](../../src/auth/repository.rs) gains `claim`, an `UPDATE ... WHERE revoked_at IS NULL` that only one caller can win. The loser is treated as a reuse and every session of that user is revoked. |
| The 30-second timeout answered with an empty body | Medium | [`../../src/api/mod.rs`](../../src/api/mod.rs) replaces the `tower-http` timeout layer with a middleware that reports the timeout through the standard envelope. The timeout is now `APP_REQUEST_TIMEOUT_SECONDS`. |
| The OpenAPI document omitted the status codes the handlers return | Medium | Every `#[utoipa::path]` now declares `400`, `401`, `403`, `404`, `408`, `409`, `413`, `422`, `429` and `500` where they can occur. Two further passes were needed. The re-review found that the first pass keyed the response lists on the path alone, so the three `create` endpoints still declared `200` with a page body and the three `delete` endpoints declared `200` with a resource body; the lists are now keyed on the method and the path together. The quality gate then found that `login` and `refresh` still omitted `401`, which they do return when the credentials or the refresh token are wrong, and that no endpoint declared `408`, which the request timeout can produce anywhere. Both are now declared. |
| `revoke` was not scoped by the owner | Low | [`../../src/auth/repository.rs`](../../src/auth/repository.rs) adds `AND user_id = $2` to the predicate, and ADR-0005 now records the lookup exception explicitly. |
| The middleware-order comment was reversed | Low | [`../../src/api/mod.rs`](../../src/api/mod.rs) now states the effective order and points at the request-lifecycle document. |
| `hyper`, `http` and `subtle` were unused; `tower` and `http-body-util` were duplicated | Low | [`../../Cargo.toml`](../../Cargo.toml) no longer declares them. `tower` and `http-body-util` remain as dev-dependencies, which is where the test suite uses them. |
| A `CHECK` violation could log the failing row, including the password hash | Low | [`../../src/error.rs`](../../src/error.rs) logs only the SQLSTATE and the error kind for a database failure, and [`../../src/users/repository.rs`](../../src/users/repository.rs) maps a check violation to a `422` before it can reach the log. |
| The dummy-hash fallback was not a valid Argon2 hash | Low | [`../../src/auth/password.rs`](../../src/auth/password.rs) holds a valid `DUMMY_HASH` constant, so the timing equalisation always costs a real verification. |
| Unreferenced public helpers, and expired sessions were never pruned | Low | The unused helpers are removed. [`../../src/main.rs`](../../src/main.rs) runs a background task that prunes expired refresh sessions every hour. |
| `APP_MAX_BODY_BYTES` constrained nothing | Low | [`../../src/api/extract.rs`](../../src/api/extract.rs) reads the limit the router installs, so the configured value is the effective ceiling. |
| The dummy hash used cheaper parameters than a real hash, so the timing equalisation was only about a fifth effective | Medium | [`../../src/auth/password.rs`](../../src/auth/password.rs) derives the dummy hash from the configured cost parameters, so verifying against it costs exactly what verifying against a stored password costs. |
| The `SetRequestIdLayer` and `PropagateRequestIdLayer` were in the wrong order, so a *generated* correlation id was never echoed on the response | Medium | The setter is now the outer of the two, so it puts the id on the request before the propagator reads it. Covered by `a_generated_request_id_is_propagated_to_the_response`. |
| A timed-out response carried no correlation id | Low | The timeout is now inner to the request-id pair, so the `408` still carries the id. Covered by `a_slow_request_is_abandoned_with_the_error_envelope`. |
| The session-pruning task logged the raw driver message | Low | [`../../src/main.rs`](../../src/main.rs) logs only the SQLSTATE, like [`../../src/error.rs`](../../src/error.rs). |
| `claim` mutated a session without an owner predicate | Low | The predicate now carries `AND user_id = $2`, and ADR-0005 names it. |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §9 | The security expectations the fixes restore. |
| [`../decisions/ADR-0002-rust-axum-postgresql.md`](../decisions/ADR-0002-rust-axum-postgresql.md) | The rule that a request path performs no blocking work. |
| [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The refresh-token rotation and reuse-detection design. |
| [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md) | The ownership predicate rule and its two documented exceptions. |
| [`../api/error-contract.md`](../api/error-contract.md) | The single error shape, including the `timeout` code. |

#### Architectural impact

None. The layering, the route table and the public contracts are unchanged.
The timeout became configurable, which adds one environment variable.

#### Compatibility impact

- The refresh-token rotation now fails closed: a second, concurrent
  exchange of the same token is refused and revokes the user's sessions. A
  client that exchanged the same refresh token twice in parallel now sees one
  success and one `401`, instead of two successes.
- The request timeout now answers with the standard envelope instead of an
  empty body. A client that parsed the body as JSON will now succeed where it
  previously failed to parse.
- `APP_MAX_BODY_BYTES` now bounds the request body. A client that sent more
  than one mebibyte used to be accepted up to four mebibytes and now receives
  `413`.

#### Security implications

Three of the fixes close security gaps: the blocking Argon2 work (a
denial-of-service amplifier), the refresh-token reuse race (a stolen token
could be raced against the legitimate client), and the database error log
(a `CHECK` violation message can contain the failing row, which for the
`users` table includes the password hash). The generic login failure is
unchanged.

#### Tests and checks actually executed

| Check | Result |
| ----- | ------ |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed. |
| `cargo test --all-targets` | Passed: 39 unit, 26 authentication, 8 notes, 19 projects and tasks. 92 in total. |
| `cargo run --bin atom-docs` | Passed, no errors and no warnings. |
| A running server | `/health/live`, `/health/ready`, registration, note creation, note listing, the OpenAPI document and the Swagger UI all answered. |

The new tests close the gaps the review identified: the concurrent refresh
race, the HTTP-level rate limit, the disabled account, the expired refresh
token, the expired access token, the per-user session revocation, the malformed
JSON body, the unknown query parameter value, the oversized body, the unmatched
route, the request-id echo, the generated request-id echo, the request timeout
and a task created as `done`.

#### Reviewer findings

The findings and their resolutions are recorded in
[`review-log.md`](review-log.md) under `ATOM-REVIEW-20261021-001`.

---

### ATOM-CHANGE-20261021-003 - The OpenCode engineering system, and the fixes it exposed

**Date:** 2026-10-21 (UTC)
**Type:** Engineering system, with source fixes
**Author:** The ATOM engineering team - the orchestrator with the specialist
agents defined in [`.opencode/agents/`](../../.opencode/agents/)
**Status:** Complete

#### Summary

The repository was given a complete OpenCode engineering system: a project
configuration, an orchestrator and nine specialists, three commands, three
skills, two validation scripts and a contributor guide. Building and verifying
it exposed four defects in the Phase 1 code and in the documentation, all of
which are fixed by this change.

#### Reason

[`../project-truth.md`](../project-truth.md) A13 requires the quality gates to
be real, and [`../testing/verification-policy.md`](../testing/verification-policy.md)
requires that a result is only claimed when it was observed. The same
standard applies to the agent system: a configuration that parses is not a
configuration that works. Every permission rule, every agent and every
documented behaviour was therefore verified against the installed OpenCode
CLI rather than against the schema alone.

#### Affected features and files

| Area | Change |
| ---- | ------ |
| [`../../opencode.jsonc`](../../opencode.jsonc) | The project configuration: the default model, the ordered permission rules, the policies, the ambient instructions, the watcher, compaction and the experimental settings. |
| [`../../AGENTS.md`](../../AGENTS.md) | The instructions every agent reads, including the mandatory documentation intake protocol and the status vocabulary. |
| [`../../.opencode/agents/`](../../.opencode/agents/) | Ten agents: the orchestrator, the architect, the Rust backend, authentication and security, the database, the test engineer, the code reviewer, the quality gate, the documentation specialist and the DevOps engineer. The reviewer and the quality gate are read-only. |
| [`../../.opencode/commands/`](../../.opencode/commands/) | `/feature`, `/review` and `/verify`. |
| [`../../.opencode/skills/`](../../.opencode/skills/) | The `atom-delivery`, `atom-docs` and `atom-specialist` workflows. |
| [`../../scripts/`](../../scripts/) | `validate-opencode.ps1`, `validate-opencode.sh`, `check-docs.ps1`, `check-docs.sh`. |
| [`../contributing-opencode.md`](../contributing-opencode.md) | The contributor guide: the model override ladder, the permission model, and the verified behaviour of the installed OpenCode version. |
| [`../../src/main.rs`](../../src/main.rs) | **Defect fix.** The server now installs the connection information, so the rate limiter is keyed by the client address. Closes `KI-010`. |
| [`../../tests/common/mod.rs`](../../tests/common/mod.rs), [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | **Defect fix.** An HTTP-level test of the rate limiter. Closes `KI-002`. |
| [`../../.env.example`](../../.env.example) | **Defect fix.** The description of the test harness now matches what it does. Closes `KI-011`. |
| [`../project-truth.md`](../project-truth.md), [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) | **Defect fix.** Corrected links and the missing contributor guide. Closes `KI-013`. |
| [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs) | The documentation checker: required documents, relative links, feature status vocabulary, ADR identifiers and status, change identifiers, feature directories. |
| [`../../.github/workflows/ci.yml`](../../.github/workflows/ci.yml) | The pipeline: formatting, linting, tests, a release build, the documentation check and the OpenCode configuration check. |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) A13 | The quality gates and the rule that a result is only claimed when observed. |
| [`../project-truth.md`](../project-truth.md) A9 | The security expectations that the permission model implements. |
| [`../decisions/ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product. |

#### Architectural impact

None. The application architecture is unchanged. The agent system is
development infrastructure and ships no runtime code.

#### Compatibility impact

None for the API. The change adds a CI workflow that does not exist yet and
corrects the description of the test database in `.env.example`.

#### Security implications

The permission model is the enforcement point for the "routine local
development is autonomous, consequential actions are controlled" rule. Two
things were verified by running the CLI rather than by reading the schema:

1. The **top-level** `policies` array is ignored by OpenCode v2.0.27. The
   same statements under `experimental.policies` do block the command. The
   committed configuration therefore keeps its policies under
   `experimental.policies`, and
   [`../contributing-opencode.md`](../contributing-opencode.md) records the
   finding.
2. An **unquoted** `color: #3b82f6` in an agent's frontmatter is parsed as a
   value followed by a YAML comment. The colour becomes empty, the agent
   fails validation, and OpenCode silently drops it. Every agent colour is
   now quoted.

#### Tests and checks actually executed

| Check | Result |
| ----- | ------ |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed. |
| `cargo test --all-targets` | Passed: 39 unit, 14 authentication, 8 notes, 9 projects and tasks. 70 in total. |
| `cargo run --bin atom-docs` | The documentation checker. Its result is reported in the section below. |
| `opencode debug config` | The project configuration is loaded and merged. `experimental.policies` is present in the resolved configuration. |
| `opencode debug agents` | All ten project agents are discovered, with the expected modes and permissions. |
| `scripts/validate-opencode.ps1` | Passed, no warnings. |
| `scripts/validate-opencode.sh` | Passed, no warnings. |
| A running server | `/health/live`, `/health/ready`, registration, note creation, note listing, the OpenAPI document and the Swagger UI all answered. Twelve login attempts produced ten `401` responses and then `429` with a `retry-after` header. |
| A policy probe | A `tool.use` deny statement blocked the command under `experimental.policies` and did nothing at the top level. |

#### Reviewer findings

No independent review has been performed on this change set yet;
[`review-log.md`](review-log.md) records that explicitly.

---

### ATOM-CHANGE-20261021-002 — Phase 1 security, testing and engineering record

**Date:** 2026-10-21 (UTC)
**Type:** Documentation
**Author:** The ATOM engineering team — the orchestrator with the specialist
agents defined in [`.opencode/agents/`](../../.opencode/agents/)
**Status:** Complete

#### Summary

The security, testing and engineering-record part of the knowledge base was
written: the security index, the threat model and the secure-development
rules; the testing strategy and the verification policy; the change log, the
implementation status, the review log, the known issues and the open
questions; the roadmap; and the five record templates.

#### Reason

`AGENTS.md` §5 makes documentation part of the delivery, and
[`ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md)
makes the documentation tree part of the product. A change set that is not
described cannot be reviewed, and a control that is not written down cannot be
checked by the next contributor.

#### Affected features and files

No behaviour changed. This entry changes documentation only.

| Document | Subject |
| -------- | ------- |
| [`../roadmap.md`](../roadmap.md) | The future features, their status, their dependencies and the Phase 1 boundary. |
| [`../security/README.md`](../security/README.md), [`../security/threat-model.md`](../security/threat-model.md), [`../security/secure-development.md`](../security/secure-development.md) | The security posture, the threat model and the rules a change must follow. |
| [`../testing/strategy.md`](../testing/strategy.md), [`../testing/verification-policy.md`](../testing/verification-policy.md) | The test layers and what must pass before a change is complete. |
| [`../engineering/`](../engineering/) | This record: changes, status, reviews, known issues, open questions. |
| [`../templates/`](../templates/) | The fill-in templates for a feature specification, an ADR, a change record, a review record and a verification record. |

The architecture, decision, feature, API and database documents were written in
the same effort by the other specialists and are not itemised here.

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §5 | The roadmap items and their dependencies. |
| [`../project-truth.md`](../project-truth.md) §6 | The explicit Phase 1 exclusions. |
| [`../project-truth.md`](../project-truth.md) §9 | The security and privacy expectations. |
| [`../project-truth.md`](../project-truth.md) §13 | The testing and quality requirements. |
| [`../project-truth.md`](../project-truth.md) §16 | The decisions awaiting confirmation. |
| [`ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product. |

#### Architectural impact

None. No code, schema, contract or configuration changed.

#### Compatibility impact

None. No public contract changed.

#### Security implications

None. No control was added, removed or altered. The security documents
describe the controls that already existed and record their limits; the
limits themselves are unchanged and are listed in
[`known-issues.md`](known-issues.md).

#### Checks and results

All four commands were run on 2026-10-21 against the tree that contains this
entry, with
`$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"`.

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Failed.** Six formatting differences, all in [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs), which was edited concurrently by another agent and is not part of this entry. This entry changed no source file. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Failed.** One error: `clippy::collapsible_if` in [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs), the same concurrent edit. This entry changed no source file. |
| `cargo test --all-targets` | **Passed.** 69 tests: 39 unit tests, 13 authentication integration tests, 8 notes integration tests, 9 projects/tasks integration tests. |
| `cargo run --bin atom-docs` | **Failed with 76 errors.** Every error is a broken link to a document that is required by [`../README.md`](../README.md) but was not yet in the tree when the check ran: `docs/architecture/…` and `docs/decisions/…`. Two further errors are pre-existing and are recorded as `KI-013` in [`known-issues.md`](known-issues.md): a wrong path prefix in [`../project-truth.md`](../project-truth.md) and a missing target in [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md). No error points at a document written by this entry other than at the two pending directories named above. |

The two failing code gates are caused by a file outside this entry's scope.
They are reported here rather than worked around, and the fix belongs to the
owner of that file.

#### Manual verification

Every control, test name, configuration key and command in the documents
written by this entry was read out of the source before it was written, and
every test name quoted in the documents was matched against the test files.
The test counts were confirmed by running the suite.

#### Reviewer findings

**No independent review has been performed on this change set.** See
[`review-log.md`](review-log.md).

#### Documentation

This entry is itself part of the documentation it describes.
### ATOM-CHANGE-20261021-001 — Phase 1 backend foundation

**Date:** 2026-10-21 (UTC)
**Type:** Foundation
**Author:** The ATOM engineering team — the orchestrator with the specialist
agents defined in [`.opencode/agents/`](../../.opencode/agents/)
**Status:** Complete

#### Summary

The Phase 1 backend was built: a Rust/Axum modular monolith on PostgreSQL
through SQLx, with a typed configuration read once at start-up, a central error
contract, a complete authentication and authorization boundary, notes, projects
and tasks, a versioned REST API under `/api/v1` with a generated OpenAPI
document and a local API explorer, health endpoints, structured logging with
request correlation, and a documentation tree.

#### Reason

ATOM needs a backend foundation before any client exists. The requirements are
the accepted items in [`../project-truth.md`](../project-truth.md) §4, and the
decision to build it as a modular monolith rather than a set of services is
recorded in
[`ADR-0001-modular-monolith.md`](../decisions/ADR-0001-modular-monolith.md).

#### Affected features and files

| Feature | Area | Files |
| ------- | ---- | ----- |
| Foundation | Configuration, error contract, pagination, state, telemetry, health | [`../../src/config/`](../../src/config/), [`../../src/error.rs`](../../src/error.rs), [`../../src/pagination.rs`](../../src/pagination.rs), [`../../src/state.rs`](../../src/state.rs), [`../../src/telemetry.rs`](../../src/telemetry.rs), [`../../src/health.rs`](../../src/health.rs), [`../../src/main.rs`](../../src/main.rs), [`../../src/lib.rs`](../../src/lib.rs) |
| Foundation | HTTP layer | [`../../src/api/mod.rs`](../../src/api/mod.rs), [`../../src/api/routes.rs`](../../src/api/routes.rs), [`../../src/api/extract.rs`](../../src/api/extract.rs), [`../../src/api/middleware.rs`](../../src/api/middleware.rs) |
| Authentication | Registration, login, refresh, logout, `/auth/me` | [`../../src/auth/`](../../src/auth/), [`../../src/users/`](../../src/users/) |
| Notes | CRUD, archive, search, pagination | [`../../src/notes/`](../../src/notes/) |
| Projects and tasks | CRUD, status, priority, filtering, ordering, cascade | [`../../src/projects/`](../../src/projects/), [`../../src/tasks/`](../../src/tasks/) |
| Security | Rate limiting | [`../../src/ratelimit.rs`](../../src/ratelimit.rs) |
| Database | Schema | [`../../migrations/0001_users_and_sessions.sql`](../../migrations/0001_users_and_sessions.sql), [`../../migrations/0002_notes.sql`](../../migrations/0002_notes.sql), [`../../migrations/0003_projects_and_tasks.sql`](../../migrations/0003_projects_and_tasks.sql) |
| Tests | Unit and integration suites | [`../../src/`](../../src/), [`../../tests/common/mod.rs`](../../tests/common/mod.rs), [`../../tests/auth_api.rs`](../../tests/auth_api.rs), [`../../tests/notes_api.rs`](../../tests/notes_api.rs), [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) |
| Tooling | Documentation checker, scripts, agent configuration | [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs), [`../../scripts/`](../../scripts/), [`../../opencode.jsonc`](../../opencode.jsonc) |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §4.1 | Foundation: modular monolith, SQLx persistence, typed configuration, the error contract, structured logging, health endpoints, the versioned API, the OpenAPI document, the documentation. |
| [`../project-truth.md`](../project-truth.md) §4.2 | Authentication and authorization: registration, Argon2id, the password policy, access tokens, refresh tokens, reuse detection, logout, ownership checks, rate limiting, the safe user representation. |
| [`../project-truth.md`](../project-truth.md) §4.3 | Notes: CRUD, partial update, archiving, search, pagination, isolation. |
| [`../project-truth.md`](../project-truth.md) §4.4 | Projects and tasks: CRUD, filtering, ordering, completion timestamps, the project delete behaviour. |
| [`../project-truth.md`](../project-truth.md) §4.5 | Quality: unit tests, integration tests, contract tests, security regression tests, the three gates, the disposable test database. |
| [`ADR-0001-modular-monolith.md`](../decisions/ADR-0001-modular-monolith.md) | ATOM is a modular monolith. |
| [`ADR-0002-rust-axum-postgresql.md`](../decisions/ADR-0002-rust-axum-postgresql.md) | The technology stack. |
| [`ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The access and refresh token design. |
| [`ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product. |
| [`ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md) | Authorization is enforced in SQL, not in handlers. |

#### Architectural impact

This is the change that establishes the architecture, so the impact is
foundational rather than incremental.

- A modular monolith with the fixed layering handlers → services →
  repositories → PostgreSQL. Every feature module keeps the same five
  layers.
- A typed `Config` is read once at start-up and passed down explicitly.
  `AppState` holds it and every service, and there is no global mutable
  state.
- One error type, `AppError`, with one public envelope and a stable error
  code. Handlers never build a response by hand.
- The request pipeline is explicit and ordered: panic catch, request
  correlation, timeout, CORS, tracing, body limit, route.
- The schema is owned by append-only migrations that are embedded in the
  binary, so a deployed instance never depends on files shipped next to it.
- The OpenAPI document is generated from the handlers, so it cannot describe a
  route that does not exist.

#### Compatibility impact

None. This is the first implementation; there is no earlier contract to keep
compatible with. The only externally visible surface is the `/api/v1` prefix
chosen here, which is versioned from the start precisely so that a future
change can be introduced without breaking an existing client.

#### Security implications

This change introduces the whole security boundary:

- Argon2id password hashing with a 16-byte random salt and configurable cost
  parameters; a 12–256 character password policy.
- Short-lived HS256 access tokens validated for signature, expiry, issuer and
  audience; a signing secret of at least 32 bytes with the documented
  placeholder refused in production; the secret and the database URL redacted
  from every `Debug` rendering.
- Opaque refresh tokens of which only the SHA-256 hash is stored, rotated on
  use, with reuse of an already-rotated token revoking every session of that
  account.
- A generic authentication failure, with a dummy Argon2 verification so that
  an unknown address does not answer faster than a wrong password.
- Ownership enforced in SQL on every query that touches user data, with `404`
  rather than `403` for a resource the caller does not own.
- A fixed error envelope that never carries database or internal detail.
- A per-process rate limit on registration, login and refresh, with a `429`
  and a `retry-after` header.
- An explicit CORS allowlist, a body-size limit, a request timeout and a
  panic handler.

The limitations of this boundary — the per-process limiter, the shared
rate-limit key, the ignored `X-Forwarded-For`, the run-time SQLx check — are
recorded in [`known-issues.md`](known-issues.md), not hidden.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Passed.** No formatting differences. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** No lint findings, no suppressed lint. |
| `cargo test --all-targets` | **Passed.** 69 tests: 39 unit tests, 13 authentication integration tests, 8 notes integration tests, 9 projects/tasks integration tests. The integration tests ran against a disposable PostgreSQL database. |

**Status note (added 2026-10-21, later the same day).** The tree changed
after this entry was written: another agent edited
[`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs) during the same
working session. At the last run, `cargo fmt --all -- --check` and
`cargo clippy --all-targets --all-features -- -D warnings` both fail on that
one file, while `cargo test --all-targets` still passes with 69 tests. The
results above remain the record of the state at the time of the foundation
work; the current state is recorded in `ATOM-CHANGE-20261021-002`.

#### Manual verification

The server was started and the following were exercised successfully:
`/health/live`, `/health/ready`, user registration, note creation, note
listing, the OpenAPI document at `/api/v1/openapi.json`, and the API explorer
at `/api/v1/docs/`.

#### Reviewer findings

**No independent review has been performed on this change set.** The
`code-reviewer` and the `quality-gate` agents are read-only and had not been
run against the foundation when this entry was written, so this section is
empty by fact and not by omission. The empty record and the format for a
future review are in [`review-log.md`](review-log.md).

#### Documentation

The knowledge base was created with the implementation: the requirements in
[`../project-truth.md`](../project-truth.md), the architecture and decision
documents under [`../architecture/`](../architecture/) and
[`../decisions/`](../decisions/), the feature documents under
[`../features/`](../features/), the API contract under [`../api/`](../api/),
the schema under [`../database/`](../database/), the security model under
[`../security/`](../security/), the test approach under
[`../testing/`](../testing/), and this engineering record.

**Correction (2026-10-22).** The file `src/config.rs` was replaced by the
module `src/config/` in `ATOM-CHANGE-20261022-001`. The link in the table
above now points at the directory, so that it resolves; the file list it
describes is otherwise unchanged.


# Phase 2 — plan

> **Status:** Proposed (planning — becomes `Accepted` only through the Phase 1 intake)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the orchestrator

The plan for turning the Phase 1 backend into the personal workspace: notes,
tasks, reimbursements, job-application tracking and local machine learning, with
a complete database seeder so the frontend can be built against realistic data
from the first phase.

Nothing here is agreed product scope until Phase 1 writes it into
[the product requirements](../../../../project-truth.md) with acceptance criteria.
The roadmap items remain `Proposed` until then.

---

## 1. Capabilities

| Capability | State | Phase 2 work |
| ---------- | ----- | ------------ |
| Notes | `Verified` (Phase 1) | Semantic search, seeder coverage |
| Task tracker | `Verified` (Phase 1) | Links from claims and applications; seeder coverage |
| Reimbursement tracking | New | Claims, line items, money, lifecycle |
| Job application helper | New | Companies, applications, stages, events |
| Local machine learning | New | Embeddings, hybrid search, match ranking |
| Database seeder | New | One command, deterministic, complete, guarded |

## 2. Scope boundary

In scope: the six capabilities above, and the hardening of the existing API.

**The application stack stays Rust**: Axum, Tokio, SQLx, PostgreSQL. No second
language runtime is added for any feature, including machine learning. Model
weights are local files read by the Rust process, not a service. PowerShell
scripts, `gh`, Docker and the OpenCode agent tooling are developer tooling, not
application stack, and are unaffected by this rule.

Out of scope for Phase 2, and not to be pre-wired:

- Text generation of any kind (cover letters, summaries). It requires an
  outbound model, which the constraints exclude.
- Outbound calls to a hosted or paid service, including AI providers.
- Team membership, sharing, multi-tenancy. The product is single-owner.
- **n8n automation.** It is a later integration, not part of the API runtime: a
  separate Node service that calls the API, never the reverse, and never inside
  the data path of a private record. It is `Proposed` in
  [the roadmap](../../../../roadmap.md), not Phase 2 scope.
- Everything in [the roadmap](../../../../roadmap.md) that is still `Proposed`.
- Compile-time SQLx verification (`KI-004`) and account lockout (`KI-012`).

## 3. Assumptions to confirm

These are product decisions. Phase 1 records the answer and its source; anything
unanswered becomes an open question and the behaviour is excluded from the
specifications.

| # | Assumption | Drives |
| - | ---------- | ------ |
| A1 | A reimbursement is an expense **claim** with line items, one currency per claim, lifecycle `draft → submitted → approved/rejected → paid`, no FX, no second approver | Claims schema |
| A2 | Receipts are a URL/reference in Phase 2, not uploaded files | Claim fields, ADR-0009 |
| A3 | The job helper tracks applications and events; it does not generate text | Applications schema |
| A4 | ML runs locally only and never generates text | ADR-0008, `src/ml/` |
| A5 | Still single owner; no teams or sharing | Every ownership predicate |
| A6 | Money is `NUMERIC(19,4)` in the database and a JSON **string** | Conventions, DTOs |

## 4. Quality bar

Every feature meets all of the following, or it is not done:

- **Endpoints**: tests for the happy path, `401`, `404` for another owner,
  `400` malformed id/body, `422` validation, and each documented error code.
- **Rules**: every state transition has an allowed and a forbidden test; every
  money rule has a precision test.
- **Lists**: pagination bounds, filter behaviour and ordering are asserted.
- **Contract**: every path and method is in `/api/v1/openapi.json`, and a
  contract test fails when a route and the document disagree.
- **Schema**: the migration applies from empty in CI; `schema.md` matches.
- **Docs**: five documents per feature, plus schema, API, conventions, change log.
- **Seeder**: every table the feature adds is seeded in the same chain.
- **Gates**: `fmt`, `clippy -D warnings`, `test`, `check --no-default-features`,
  `atom-docs`, `atom-docs --strict`.
- **Volume**: at least 150 new tests. Measured base: **179** on 2026-10-24 before
  the close-out, **191** after chain `D0` (12 new). The target is therefore
  roughly **340** at the end of Phase 2; the number is a floor for planning, not
  an acceptance criterion in itself.
- **Review**: `code-reviewer` on the diff and `quality-gate` on the evidence
  before anything is marked `Verified`.

## 5. Data model sketch

The architect finalises this on Phase 1; the shapes below are the working sketch.

### 5.1 Reimbursements

`expense_claims`: `id`, `user_id`, `title`, `description`, `currency` (ISO 4217,
three letters), `total_amount NUMERIC(19,4)`, `status` (`draft`, `submitted`,
`approved`, `rejected`, `paid`), `project_id` (nullable, `ON DELETE SET NULL`),
`submitted_at`, `decided_at`, `paid_at`, `created_at`, `updated_at`.

`expense_items`: `id`, `user_id`, `claim_id` (`ON DELETE CASCADE`), description,
`amount NUMERIC(19,4)` (`>= 0`), `incurred_on`, `category`, `receipt_url`
(nullable; ADR-0009 puts the receipt on the line item, not the claim),
`created_at`, `updated_at`.

Invariants:

- `claim.total_amount` equals the sum of its items, enforced in one transaction.
- Every item belongs to the claim's owner; the ownership predicate is in SQL.
- One currency per claim; items carry no currency.
- Submitting requires at least one item and a total above zero.
- `draft → submitted → approved | rejected`; `rejected → draft`; `approved →
  paid`; `paid` is terminal.
- JSON money is a string with four decimal places; no floating point anywhere.

### 5.2 Job applications

`companies`: `id`, `user_id`, `name`, `website`, `notes`, unique
`(user_id, lower(name))`.

`job_applications`: `id`, `user_id`, `company_id`, `role_title`, `source`,
`status` (`wishlist`, `applied`, `screening`, `interview`, `offer`, `rejected`,
`withdrawn`), `applied_on`, `salary_min`, `salary_max`, `salary_currency`,
`job_url`, `description`, `next_action_task_id` (nullable, `ON DELETE SET NULL`),
`created_at`, `updated_at`.

`application_events`: `id`, `user_id`, `application_id` (`ON DELETE CASCADE`),
`kind` (`note`, `email`, `call`, `interview`, `offer`, `rejection`),
`occurred_at`, `summary`, `details`, `created_at`.

Stage machine: `wishlist → applied → screening → interview → offer`;
`rejected` and `withdrawn` are reachable from any non-terminal stage; `offer`,
`rejected` and `withdrawn` are terminal. A company and an application always
share an owner; a foreign key that crosses owners is rejected by the service and
the tests. The accepted statement of this machine is
[`project-truth.md`](../../../../project-truth.md) §18.2 `JOB-R04`; where this sketch
and §18.2 differ, §18.2 wins.

### 5.3 Embeddings

`embeddings`: `id`, `user_id`, `source_type` (`note`, `application`), `source_id`,
`model_id`, `dim`, `vector REAL[]`, unique
`(user_id, source_type, source_id, model_id)`.

`source_id` is polymorphic, so it carries no foreign key. Deleting a note or an
application must delete its embeddings in the same transaction, and a reindex
job cleans up any orphan. That trade-off is recorded in ADR-0008.

## 6. Local machine learning

### The constraint that decided the design

`vector` 0.8.1 is available on the server, but the `atom` role is **not** a
superuser: `CREATE EXTENSION vector` is denied locally, while CI's `atom` is a
superuser. A migration containing that statement would pass CI and fail on the
development machine. That is the worst possible divergence, so Phase 2 does not
depend on the extension.

### The design

1. An `Embedder` trait with three implementations: `Burn` (the owner's chosen
   pure-Rust framework, v0.22, **`flex` backend** — pure Rust on the CPU — with
   weights loaded from a local SafeTensors file), `Deterministic` (hash-based,
   used by every test, no network), `Disabled`. **No second language runtime**:
   no Python, no Node, no Go, and no external model server such as Ollama.
2. Embeddings stored as `REAL[]`.
3. Search is **hybrid**: the existing lexical search narrows the owner's rows to
   a bounded candidate set, then cosine similarity re-ranks them in Rust. No
   extension, bounded memory, honest about being a reranker.
4. Configuration: `ml.enabled` (default `false`), `ml.backend`, `ml.model_path`,
   `ml.dimension`, `ml.timeout_seconds`, `ml.candidate_limit` — all seven places
   in one change, per `AGENTS.md` §4.11.
5. Degradation is tested: ML off, or the model file missing, returns lexical
   results with `200`, never `500`.
6. Evaluation: 15 fixture queries with expected notes, run with the
   deterministic embedder (proves the plumbing); one recorded manual quality run
   with the real model.
7. Burn is the framework decision (owner, 2026-10-24). Phase 4 opens with a spike
   that loads a small sentence-embedding model, measures latency and memory on
   this machine, and records the numbers **before** any endpoint depends on it.
   If the spike fails, ML ships as `Disabled` and lexical search is the
   behaviour. It is not replaced with a service, and it is not replaced with
   another language.

Endpoints: `POST /notes/search` (lexical or semantic), `POST
/job-applications/{id}/match`, `POST /ml/reindex` (idempotent, owner-scoped).

## 7. Delivery waves

| Phase | Wave | Output | Seeder addition |
| --- | ---- | ------ | --------------- |
| 0 | Close Phase 1 | Remediation leftovers verified and merged | — |
| 1 | Product intake | Accepted requirements, three feature specs, ADR-0007/0008/0009, schema, conventions, test matrix, seeder specification | Specified in `docs/database/seeding.md` |
| 2 | Seeder foundation | `atom-seed` for users, notes, projects, tasks; guard; determinism; tests; docs | S1 |
| 2 | Reimbursements | Migration, module, endpoints, tests, docs | Claims (S2) |
| 3 | Job applications | Migration, module, endpoints, tests, docs | Companies and applications (S3) |
| 4 | Local ML | ADR, embeddings, hybrid search, evaluation | Embeddings with `--with-embeddings` (S4) |
| 5 | Hardening | Contract tests, ownership sweep, performance, frontend handover | CI smoke run (S5) |

A phase is one run of [prompt.md](prompt.md). A **chain** is one queue item inside
a phase. The seeder is a first-class chain, not an afterthought: a feature that
adds a table without adding it to the seeder is not complete.

### Phase 0 — the close-out checklist

Phase 0 is the only phase that finishes existing work instead of new work. Read
[ledger.md](ledger.md) first: part of it may already be done by the session that
was writing when this plan was created.

1. **Reconcile the tree.** `cargo fmt --all`. Fix `config/README.md`
   (`shutdown_grace_seconds` says `100`; `config/app.toml` says `10`). Decide the
   fate of the untracked `docs/audit/handoff.md` — update and commit it, or
   delete it. Reconcile the OpenAPI explorer status: `docs/project-truth.md`
   says `Verified`, `docs/engineering/implementation-status.md` says
   `Implemented`.
2. **F-44 / `ATOM-TASK-20261022-022`.** Prove the `docs` feature: with and
   without default features, and measure the release binary both ways. Record
   the sizes, update the backlog and the change log, document the build flag.
3. **The shutdown test.** Format `tests/shutdown.rs`, prove it fails against the
   old defect and passes after, and get the Linux CI run that executes it.
4. **`ATOM-TASK-20261022-027`.** Compare setting values, not just names; prove
   it catches the `100`/`10` drift; no false failures.
5. **`ATOM-TASK-20261022-021`.** Make the OpenAPI document and the route table
   agree by construction, or add a contract test that fails when they differ.
6. **`ATOM-TASK-20261022-023`.** Parse the list query string once without
   changing the OpenAPI parameters.
7. **`ATOM-TASK-20261022-020`.** Serve the profile from the row the auth
   extractor already loaded; keep the per-request account-status check.
8. **`KI-006`.** The container image, built and probed in CI.
9. **Close.** Six gates green, push, tag `phase-1-complete`, and record the
   evidence in the ledger.

## 8. The seeder

### Goal

One command gives a complete, deterministic, realistic dataset covering every
feature, so the frontend can be built and the API demonstrated without
hand-crafting data.

```bash
cargo run --bin atom-seed -- --profile dev --reset
```

### Profiles

| Profile | Users | Notes | Projects | Tasks | Claims | Applications |
| ------- | ----- | ----- | -------- | ----- | ------ | ------------ |
| `minimal` | 2 | 5 | 1 | 3 | 1 | 1 |
| `dev` (default) | 2 | 200 | 8 | 60 | 25 | 20 |
| `demo` | 2 | 500 | 12 | 120 | 50 | 40 |

Two users, always: the second account exists so ownership isolation can be
demonstrated and tested. Notes and tasks use realistic, searchable text;
claims cover every status; applications cover every stage.

### Safety guard

The seeder refuses to run unless all of these hold:

1. `APP_ENV` is unset, or one of `development`, `dev`, `local`. Every other value is refused (`A-C-S1-5`).
2. The database name is `atom_dev`, `atom_test`, `atom_local` or `atom_demo`, or ends in `_dev`, `_test` or `_local`, read through the driver's own URL parser (`A-C-S1-6`).
3. The database contains no user outside the seeded id set, unless
   `--allow-non-empty` is passed. This stops a real development database from
   being wiped by a typo.
4. It never prints `DATABASE_URL`, a password, or a password hash. Errors name
   the reason, not the connection string.

`--reset` deletes only rows owned by the two seeded users, in foreign-key order,
then inserts. It never truncates a table it does not own.

### Determinism and idempotency

- Every id is derived from a fixed namespace and a counter. No `rand`, no clock,
  no `uuid::new_v4()`. Re-running `--reset` reproduces the same ids and counts.
- Timestamps are derived from a fixed base date, not `now()`, so a re-run is
  identical in ids and timestamps, so screenshots are stable. It is not literally byte-identical: the Argon2 salt of `users.password_hash` is fresh on every run, which is the one documented exception.
- Without `--reset`, inserts use `ON CONFLICT (id) DO NOTHING` and the run
  reports how many rows were inserted and how many were skipped.

### Post-insert verification

The seeder checks its own output and fails loudly if any of these is false:

- every claim total equals the sum of its items;
- every task's project, every claim's project and every application's company
  belong to the same owner;
- status and timestamp columns are consistent with the transitions;
- the row counts match the profile.

### Credentials

`owner@atom.local` and `other@atom.local`, password
`atom-dev-password-1234` — documented, fake, development-only. The Argon2 hash
is computed once and reused, because hashing per user would make the seeder
unnecessarily slow.

### Tests

`tests/seeder.rs`: the production guard (`A-C-S1-5`), the database-name guard
(`A-C-S1-6`), the two-phase foreign-rows guard (`A-C-S1-7`), the redaction tests
(`A-C-S1-8`) and the ten tests locked in `C-S1` — twelve integration tests in
total, listed in
[`../../database/seeding.md`](../../../../database/seeding.md) §9.

### Documentation

`docs/database/seeding.md` (new), a quickstart line in the repository README,
and an entry in `docs/database/README.md`. The seeder is development-only; the
deployment document says so.

## 9. Sequencing rules

- Days run in order; a phase starts only when the previous one's checkpoint is
  `ADVANCE`.
- Within a phase, chains run in queue order. The seeder foundation runs first on
  Phase 2, because it depends only on the Phase 1 schema; every feature chain then
  extends it in the same chain.
- Two agents never edit the same file in one wave. `Cargo.toml` is edited once,
  by the orchestrator, before the fan-out that adds a binary.
- Migrations are append-only and numbered sequentially: one per feature.

## 10. Decisions that block

- A1–A6 above, before Phase 1 writes acceptance criteria.
- [Open questions](../../../open-questions.md) `Q-01` … `Q-12`. None blocks the
  engineering work in this plan; each blocks the product behaviour it describes.
- pgvector versus `REAL[]`: settled for Phase 2 by the non-superuser finding,
  unless the owner prefers to make `CREATE EXTENSION` a deployment prerequisite.

## 11. Related documents

- [The run procedure](prompt.md)
- [The live ledger](ledger.md)
- [The run folder](README.md)
- [The product requirements](../../../../project-truth.md)
- [The roadmap](../../../../roadmap.md)
- [The schema](../../../../database/schema.md)
- [The API conventions](../../../../api/conventions.md)
- [The test strategy](../../../../testing/strategy.md)
- [Known issues](../../../known-issues.md)
- [Open questions](../../../open-questions.md)

### Reference standards

- [The Rust Style Guide](https://doc.rust-lang.org/style-guide/)
- [Google's Rust Style Guide](https://google.github.io/styleguide/rustguide.html)
- [rust-skills](https://github.com/leonardomso/rust-skills)
- [Burn 0.22 documentation](https://burn.dev/docs/burn/)

They are references, not overrides: `AGENTS.md` §4.12 wins, and
`rustfmt`/`clippy` decide formatting and lints. Google's `[chrome]`,
`[fuchsia]` and `google3` rules are Google-internal and do not apply here.

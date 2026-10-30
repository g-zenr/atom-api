# ATOM Repository Audit — Existing Feature & Codebase Audit

> **Status:** Point-in-time audit snapshot (this file is a session deliverable, **not** the maintained
> [`audit-report.md`](audit-report.md) record; nothing in that record was modified)
> **Audit date:** 2026-10-30 (session ran 2026-10-29 → 2026-10-30)
> **Tree:** `develop` @ `96c37b2` + **5 uncommitted modified files** (see §B)
> **Mode:** read-only. No project file was created, modified, or deleted. No database was connected to,
> no migration was run, no test suite was executed (rationale in §K).

Read-only checks actually executed during the session: `cargo fmt --all -- --check` (**pass**),
`cargo clippy --all-targets --all-features -- -D warnings` (**pass, exit 0**), plus read-only
`git status/diff/log` and source/doc inspection.

---

## A. Executive Summary

ATOM is materially **further along than its own `AGENTS.md`/`README.md` describe**. The repository contains a
complete, layered Rust/Axum + PostgreSQL backend with **54 HTTP operations** across 8 features (auth, api-keys,
notes, projects, tasks, reimbursements, job-applications, local ML), 7 append-only migrations, ~298 committed
integration tests + 229 unit tests (recorded **536 passing / six gates green** on 2026-10-29,
`ATOM-REVIEW-20261029-002`), contract tests that pin the OpenAPI document to the router in both directions, and a
real six-layer Burn/flex encoder with a hand-written WordPiece tokenizer — **not a scaffold**.

**Strongest implemented areas:** authorization discipline (every user-data query scoped by `user_id` in SQL per
ADR-0005, 404-never-403 proven by whole-surface sweeps), money handling (`NUMERIC(19,4)`, string-on-the-wire,
DB-only arithmetic), configuration/secret hygiene (secrets env-only, redacted in every debug/error path), and
verification culture (recorded gate runs with identifiers, honestly declared gaps).

**Major gaps:** no frontend of any kind; no n8n/webhook/external-integration code (currently *excluded* by
`project-truth.md` §6.1, not merely unbuilt); no prompt-history, conversation storage, generation, notifications,
or price monitoring; container image built in CI but no registry/release path.

**Important risks:** (1) **471 lines of uncommitted work-in-progress** (5 new tests + 3 doc edits) that the
records don't know about; (2) the two most authoritative documents (`project-truth.md` §3/§18, `roadmap.md` §1)
contradict the 2026-10-29 Verified promotion recorded elsewhere in themselves; (3) design-level security
residuals (no Argon2 pepper, unrevocable 15-min access tokens, registration enumeration, deployment-dependent
IP-keyed rate limiter); (4) no dependency-audit tooling in CI.

**No confirmed exploitable vulnerability was found.** Confirmed issues are documentation drift and process gaps;
security items below are labeled potential vs confirmed.

---

## B. Repository Overview

- **Detected stack:** Rust (edition 2024, `rust-version 1.95`), Axum 0.8 / Tokio, SQLx 0.8 (Postgres, no
  compile-time macros — DB not needed to build), Argon2id + jsonwebtoken (HS256), `toml` layered config
  (`config/`), utoipa 5 + Swagger UI (feature `docs`, vendored), Burn 0.22 `flex` (feature `ml`, optional),
  tracing JSON/pretty. `Cargo.toml:68-152`.
- **Structure:** 19 modules under `src/` (`api`, `api_keys`, `auth`, `users`, `notes`, `projects`, `tasks`,
  `reimbursements`, `job_applications`, `ml`, `config`, `error`, `health`, `helpers`, `pagination`, `ratelimit`,
  `state`, `telemetry`, `utils`) + 3 dev binaries (`atom-docs`, `atom-seed`, `atom-ml-bench`/`-spike`, ML-gated).
  Every feature keeps the handlers→service→repository→models→dto layering; verified, not assumed.
- **Tests:** 17 integration test files + harness (`tests/common/mod.rs`, 418 lines), tower `oneshot` transport
  (no socket), per-test fresh pool + embedded migrations, `TEST_DATABASE_URL` name-guarded to contain `test`.
- **Tooling:** CI (`.github/workflows/ci.yml` — fmt/clippy/test on real `postgres:18-alpine`, seeder gate,
  `--no-default-features` check, release build, docs gate, opencode-config validation), `container.yml`
  (digest-pinned build + runtime verification), `Makefile`, `scripts/gate.ps1` (six gates),
  `scripts/validate-opencode.*`, doc-tree checker binary.
- **Git state (unexpected, reported not fixed):** branch `develop`, HEAD `96c37b2`, with **uncommitted
  modifications** to `docs/engineering/implementation-status.md`, `docs/features/ml/status.md`,
  `docs/testing/strategy.md` (ML benchmark re-run record) and `tests/claims_api.rs` (+280 lines / 4 tests),
  `tests/claims_flow.rs` (+136 lines / 1 test). Local branches `fix/audit-remediation-phase-1`, `run/day-1/2`,
  etc. exist; no unpushed-HEAD anomaly found on `develop`.
- **Test count correction:** the audit brief said "20 test files"; 18 exist (17 test binaries +
  `tests/common/mod.rs`). Committed tree: **298 test functions** (296 on Windows; 2 shutdown tests are
  unix-only). Working tree adds 5 → 303 (never run).

---

## C. Feature Inventory

Status vocabulary per the brief. "Recorded" = passing runs cited with identifiers in `docs/` (gate verdicts
`ATOM-REVIEW-20261027-001`, `ATOM-REVIEW-20261029-001/002`, CI run IDs); **this audit did not re-execute those
runs.**

| Feature | Status | Evidence | Tests / Verification | Missing Pieces |
|---|---|---|---|---|
| Authentication (register/login/refresh/logout/me) | Implemented and tested | `src/auth/{handlers,service,token,password,repository}.rs`; routes in `src/api/routes.rs` | `tests/auth_api.rs` (29): rotation, reuse-detection mass revocation, absolute lifetime, concurrent-refresh single-winner, timing-parity, rate limit, 401/400/413 envelopes | Argon2 pepper (design gap, P-1); access-token revocation unused (`jti` reserved, `src/auth/token.rs:43`); session-prune task untested (self-declared) |
| Token/session storage | Implemented and tested | `refresh_sessions` (`migrations/0001`), SHA-256-at-rest, rotation link `replaced_by_session_id` | Schema assertions in `tests/auth_api.rs:258-604`; hourly prune task `src/main.rs:176-210` — **no test covers the prune task** | — |
| Scoped API keys (machine clients) | Implemented and tested | `src/api_keys/` (incl. `secret.rs`); `migrations/0007`; ADR-0011 | `tests/api_keys_api.rs` (11): single reveal, revocation→401, disabled account→403, foreign key→404, hourly `last_used_at` throttle | No `tests/api_keys_schema.rs` (0007 CHECKs proven by recorded hand-run, accepted gap); API keys carry **full user authority** incl. self-management (documented, deliberate) |
| Notes (CRUD, archive, search, pagination) | Implemented and tested | `src/notes/`; generated `tsvector` (`migrations/0002`) | `tests/notes_api.rs` (12) + `tests/list_query.rs` (4) | — |
| Projects & tasks (lifecycle, ordering, filters, cascade) | Implemented and tested | `src/projects/`, `src/tasks/`; fractional `position` ordering; `tasks_completion_consistency_chk` | `tests/projects_tasks_api.rs` (25) | — |
| Reimbursements (claims, items, money, lifecycle state machine) | Implemented and tested (recorded) | `src/reimbursements/` (12 ops); `migrations/0004`; composite FK `(claim_id,user_id)`; ADR-0007 money | `tests/claims_api.rs` (56 @ HEAD), `claims_flow.rs` (3), `claims_schema.rs` (10, SQLSTATE+constraint-name pinned) | **5 new uncommitted tests** close 4 of 5 recorded gaps; until committed/run/recorded, tree is ahead of every count; one stale record: `strategy.md` §7.2.1 still calls REIMB-R11 "in any status" unasserted although `a_claim_can_be_deleted_in_any_status` is in HEAD |
| Job applications (companies, applications, stage machine, events) | Implemented and tested (recorded) | `src/job_applications/` (14 ops); `migrations/0005`; 7-stage transition table; NO-ACTION company FK rationale | `tests/job_applications_api.rs` (51), `jobs_schema.rs` (11), `job_applications_flow.rs` (2) | Broken sentence in `status.md` §1.2 (cosmetic) |
| Local ML (embeddings, hybrid search, match ranking) | Implemented and tested (recorded), with recorded limitation | `src/ml/` 11 files: `burn_embedder.rs` (BERT-mini 6×384, masked mean pool, L2, Safetensors), `tokenizer.rs` (uncased WordPiece), `repository.rs` (upsert/orphan-sweep), cosine in Rust; `migrations/0006` (`REAL[]`, dim CHECK, unique upsert target) | `tests/ml_api.rs` (41) with **deterministic embedder** — no CI case loads a real model; 15-query quality eval is a manual `atom-ml-bench` run (doc-recorded 2026-10-29, uncommitted edit) | Default `ml.enabled=false` → shipped default degrades to lexical (200 + `degraded_reason`, never 408/409/503 — pinned by tests); sequential scan bounded by `candidate_limit` (ADR-0010, accepted) |
| OpenAPI document + explorer | Implemented and tested | `utoipa` annotations ×54; `tests/openapi_contract.rs` (11) pins router⇄document both ways, 54-op surface, parameter sets, money-as-string | `tests/docs_route.rs` (2, feature-gated) | Documented statuses not asserted reachable (strategy §6) |
| Health probes | Implemented and tested | `src/health.rs` (live public, ready→DB `SELECT 1`, 503 degraded) | Covered in `contract_sweep.rs` | Degraded-path (DB down) readiness untested |
| Rate limiting | Implemented and tested | `src/ratelimit.rs` — fixed-window per IP, in-process, bounded map, **fails open on poisoned lock** (`:67-72`) | 6 unit tests + HTTP auth-endpoint test | Only 3 auth endpoints limited; oneshot transport means client-IP keying never exercised end-to-end (KI-003) |
| Background session pruning | Implemented, runtime unverified | `src/main.rs:176-210`, hourly, 1-day grace | **No test** (self-declared in `implementation-status.md`) | The one background job in the system |
| Config system + message catalogue | Implemented and tested | `src/config/` (loader, settings, validation, `messages.toml`), layered TOML, env-only secrets | `src/config/tests.rs` (58) | — |
| Seeder (`atom-seed`) | Implemented and tested | `src/bin/atom-seed.rs` (~75 KB), profiles, `--reset`, advisory lock, prod refusal | `tests/seeder.rs` (21) + CI seeder gate (count pinned `22002520`) | Embeddings purge under `--reset` not specifically asserted |
| Graceful shutdown | Implemented and tested | `src/main.rs` drain; real SIGTERM | `tests/shutdown.rs` (2) — **unix-only**; nothing proven on Windows | — |
| Container image | Implemented, runtime unverified | `Dockerfile` (digest-pinned, non-root UID 10001, `--no-default-features`), `container.yml` verifies no baked secret, binary refuses start without `DATABASE_URL` | CI build job only; **no registry push, no release workflow** | README still says "container image is not built yet" (stale) |
| n8n integration / webhooks | Missing (excluded by design) | No HTTP client dep; grep `n8n\|webhook` hits only doc comments (`src/api_keys/dto.rs:38,133`) | — | Roadmap `Proposed`; owner decision O7 required first |
| Scheduled automation / external integrations | Missing | Only background job = session prune; no `reqwest` in `Cargo.toml` | — | — |
| LLM/AI-provider integration, RAG generation | Missing (excluded by project-truth §6.1) | No client, no prompt assembly | — | Retrieval half exists (search/match); generation absent |
| Prompt/conversation history | Missing | No table, model, endpoint | — | Roadmap `Proposed` |
| Notifications / approval workflows | Missing (only claims `approve` state transition exists) | No tables/endpoints | — | — |
| Laptop price monitoring & price history | Missing | Targeted grep for `price\|laptop\|monitor` across `*.rs\|md\|toml\|sql` returns only unrelated prose | — | No code, schema, or roadmap row found in this audit |

---

## D. API Endpoint Inventory

54 operations, pinned by `CONTRACT_OPERATIONS = 54` (`tests/openapi_contract.rs:42`); router
`src/api/mod.rs:76-87`, routes `src/api/routes.rs:326-446`. Auth via the `AuthUser` extractor
(`src/api/middleware.rs:43-119`) — JWT or `atom_` API key decided by token prefix. "RL" = in-handler rate limit.
No route has a sort parameter anywhere; ordering is fixed server-side per list.

| Method & Path | Handler (file) | Auth | Behavior / notes |
|---|---|---|---|
| POST `/api/v1/auth/register` | `register` (`src/auth/handlers.rs:51`) | Public + RL | Argon2id, token pair |
| POST `/api/v1/auth/login` | `login` (`:85`) | Public + RL | Timing-equalised vs dummy hash |
| POST `/api/v1/auth/refresh` | `refresh` (`:119`) | Public + RL | Rotation + reuse detection |
| POST `/api/v1/auth/logout` | `logout` (`:154`) | extractor | Cross-account logout → 403 (documented exception) |
| GET `/api/v1/auth/me` | `me` (`:184`) | extractor | — |
| POST `/api/v1/api-keys` | `create_api_key` (`src/api_keys/handlers.rs:48`) | extractor | Secret returned exactly once |
| GET `/api/v1/api-keys` | `list_api_keys` (`:78`) | extractor | Paginated; no secret/hash on wire |
| DELETE `/api/v1/api-keys/{key_id}` | `revoke_api_key` (`:109`) | extractor | Idempotent 204 |
| GET/POST `/api/v1/notes` | `list_notes`/`create_note` (`src/notes/handlers.rs:37/66`) | extractor | Filters `search`, `archived` |
| POST `/api/v1/notes/search` | `search_notes` (`src/ml/handlers.rs:49`) | extractor | ML modes; scored mode **degrades to lexical 200** with `degraded_reason` |
| GET/PATCH/DELETE `/api/v1/notes/{note_id}` | `src/notes/handlers.rs:93/123/151` | extractor | Partial PATCH; owner-scoped |
| GET/POST `/api/v1/projects` · GET/PATCH/DELETE `/api/v1/projects/{id}` | `src/projects/handlers.rs:35-152` | extractor | Delete cascades tasks; claims' `project_id` → NULL |
| GET/POST `/api/v1/projects/{id}/tasks` · GET/PATCH/DELETE `/api/v1/tasks/{id}` | `src/tasks/handlers.rs:34-158` | extractor | Filters `status`, `priority`; fractional ordering |
| GET/POST `/api/v1/claims` · GET/PATCH/DELETE `/api/v1/claims/{id}` | `src/reimbursements/handlers.rs:48-173` | extractor | Filters status/currency/search/date-range |
| POST/PATCH/DELETE `/api/v1/claims/{id}/items[/{item_id}]` | `:205-280` | extractor | Draft-only; transactional total recompute with `FOR UPDATE` |
| POST `/api/v1/claims/{id}/submit·approve·reject·pay` | `:314-410` | extractor | State machine; 409 on illegal move; timestamp matrix DB-enforced |
| GET/POST `/api/v1/companies` · GET/PATCH/DELETE `/api/v1/companies/{id}` | `src/job_applications/handlers.rs:54-188` | extractor | Case-insensitive per-owner unique name; delete refused while applications exist (409) |
| GET/POST `/api/v1/job-applications` · GET/PATCH/DELETE `/api/v1/job-applications/{id}` | `:222-356` | extractor | Filters status/company/search/date-range; create-only `wishlist` |
| GET/POST `/api/v1/job-applications/{id}/events[/{event_id}]` PATCH/DELETE | `:391-501` | extractor | Six-kind enum; no `updated_at` by design (A-D1-1) |
| POST `/api/v1/job-applications/{id}/match` | `match_application` (`src/ml/handlers.rs:90`) | extractor | Ownership checked **before** ML state → foreign app is 404, never 409 |
| POST `/api/v1/ml/reindex` | `reindex` (`:122`) | extractor | Owner-scoped, idempotent; disabled → 409 |
| GET `/health/live`, `/health/ready` | `src/health.rs:48/67` | Public | Only handlers building responses by hand (deliberate; ready→503 degraded) |
| GET `/api/v1/docs`(+assets), `/api/v1/openapi.json` | Swagger UI (`routes.rs:462-469`) | Public | Only when feature `docs` **and** `api.docs_enabled=true` |

**Findings:** no unregistered documented routes; no unreachable handlers; no duplicates; no placeholder/hardcoded
data; 405-vs-400 on `GET /notes/search` is intentional and test-pinned, but the comment at
`src/api/routes.rs:252-255` still claims 400 (stale). Error handling: all 52 feature handlers return `AppResult`;
single `AppError` envelope; 11 stable codes; `Database`/`Internal` bodies redacted to a fixed sentence
(`src/error.rs:237-242`); 408 timeout middleware; 429 + `retry-after`; panic guard returns generic 500 without
logging payload.

---

## E. Database Inventory

7 migrations (`migrations/0001…0007`), forward-only, **no down files**; chain proven to apply from empty by
scratch-DB tests. All 7 tables, 5 enum types, and named indexes match `docs/database/schema.md` and the Rust
models column-for-column (cross-checked).

| Migration | Tables / types | Integrity highlights |
|---|---|---|
| 0001 users & sessions | `users`, `refresh_sessions` | Email normalisation CHECK; unique token hash; partial active-session index; `replaced_by_session_id` rotation link; FK CASCADE to users |
| 0002 notes | `notes` | Generated weighted `tsvector` + GIN index; length CHECKs |
| 0003 projects & tasks | `projects`, `tasks`, `task_status`, `task_priority` | `tasks` inherits ownership via `projects` (no `user_id` column); completion-consistency CHECK; FK CASCADE |
| 0004 expense claims | `expense_claims`, `expense_items`, status enum | `NUMERIC(19,4)` + `>= 0`; lifecycle-timestamps matrix CHECK; **composite FK `(claim_id,user_id)`** makes cross-owner items unrepresentable; project `SET NULL` + partial index |
| 0005 job applications | `companies`, `job_applications`, `application_events`, 2 enums | Functional unique `(user_id, LOWER(name))`; composite FKs; company NO-ACTION chosen so account cascade still works (proven `tests/jobs_schema.rs:648-675`); salary all-or-none CHECK |
| 0006 embeddings | `embeddings` | `REAL[]` (no pgvector — extension unavailable to app role); `dim = cardinality(vector)`; unique `(user_id, source_type, source_id, model_id)`; polymorphic `source_id` deliberately FK-less, compensated by same-statement CTE purges + orphan sweep |
| 0007 api keys | `api_keys` | SHA-256 hex CHECK + UNIQUE; prefix grammar CHECK; no `updated_at` by design |

**Query audit:** 114 `sqlx::query*` sites in `src/` — all parameter-bound; no string-built SQL in any request
path (dynamic behavior is whole-statement selection, never fragment concatenation; the only `format!`-SQL is the
dev seeder's constant table list, unit-tested, and test-only DB creation). **94 `user_id` scoping predicates**
across repositories; tasks re-scope through project joins on every operation; candidate id-lists re-scoped by
owner before use. The only id/token-only lookups are documented ADR-0005 credential exceptions — plus
`UserRepository::find_by_id` (safe: id = token subject, but **absent from ADR-0005's exception table** — doc nit).

**Transactions:** money path exemplary (`LOCK … FOR UPDATE` → write → recompute → guarded rollback,
`src/reimbursements/repository.rs:718-840`; detail read under `REPEATABLE READ, READ ONLY`); note/application
deletes purge embeddings via single-statement CTEs; company delete guards the insert-race in one statement.
**One multi-write sequence without a transaction:** refresh-token rotation (insert replacement, then guarded
claim; compensated by reuse-detection mass revocation + inherited absolute expiry — bounded worst case, but it
is the outlier).

**Cannot verify without a live database:** applied-migration history of any real environment, planner/index
behavior, runtime FK behavior beyond what the (unexecuted) schema tests assert. Assessment is from migration
files + source only.

---

## F. Authentication and Security Findings

**Confirmed (code-verified):**

1. Argon2id, per-hash CSPRNG salt, 19 MiB / t=2 / p=1 defaults with start-up floors; 12-char policy with
   all-violations-reported; hashing on the blocking pool; unknown-email timing equalised via same-cost dummy
   hash (`src/auth/password.rs`, `src/auth/service.rs:111-119`).
2. HS256 access JWT (15 min default) pinned to one algorithm; iss/aud/exp/sub enforced; account existence +
   active re-checked per request so disabled accounts block immediately (`src/auth/service.rs:384-409`).
3. Refresh: 256-bit opaque, SHA-256-at-rest, atomic guarded rotation, reuse detection revokes all sessions,
   non-extendable absolute lifetime (`src/auth/service.rs:218-257`, `tests/auth_api.rs:252,574,695`).
4. API keys: 256-bit CSPRNG, shape gate before hashing, SHA-256 + UNIQUE, revocation idempotent,
   hourly-throttled `last_used_at`, owner-scoped management; revoked→401 / disabled→403 identical to JWT path.
5. Secrets: `APP_JWT_SECRET`/`DATABASE_URL` env-only, redacted `Debug`, placeholder refused in production,
   errors render rejected secrets by length only.
6. Error/log redaction: internal bodies fixed-sentence; SQLSTATE+kind to logs, never driver messages; message
   catalogue has no credential interpolation; panic guard drops payload.
7. CORS allowlist with wildcard refused at start-up; empty default = CORS disabled; no credentials mode.
8. Ownership: 404-never-403 across the surface, enforced in SQL, proven by `tests/contract_sweep.rs` H1/H2
   sweeping every route unauthenticated (401 except 3 public) and every id-taking op cross-account (404).
9. Deployment: digest-pinned non-root image; dockerignore excludes secrets; CI `contents: read`; no baked-in
   secret (verified by container.yml's refusal test).

**Confirmed gaps (not exploitable vulnerabilities, but real):** no `cargo-audit`/`cargo-deny`/Dependabot/CodeQL
anywhere; `ci.yml` uses `actions/checkout@v4` unpinned (container.yml pins by SHA).

**Potential risks (marked potential — none confirmed exploitable):**

- **P-1** No Argon2 pepper — offline cracking bounded by cost only, if DB leaks.
- **P-2** Access tokens unrevocable until expiry (15 min); `jti` reserved but unused.
- **P-3** Registration-based account enumeration (409 + distinct message; login is equalised, register is not).
- **P-4** Permissive email grammar (`a@b@c.com` accepted).
- **P-5** No control-character filtering in shared validators.
- **P-6** Serde error text echoed in 422 details (request-derived only).
- **P-7** Rate limiter keyed by TCP peer — behind a proxy, one shared bucket; per-replica counters dilute
  limits (documented in-repo).
- **P-8** Only 3 auth endpoints rate-limited; `POST /api-keys` (no per-user quota), writes, and
  `POST /ml/reindex` are unthrottled for valid credentials.
- **P-9** No security headers (minimal cookie-free exposure; low).
- **P-10** Single HS256 secret, no `kid`/dual-key rotation path.
- **P-11** `refresh_sessions.user_agent`/`ip_address` stored, never returned (future privacy surface).
- **P-12** `docker-compose` postgres pinned by tag only (dev); checkout pinning inconsistent across workflows.

**Unverified (needs runtime):** timing-equalisation effectiveness under load; concurrent-refresh race in
execution; trace-field output; TLS (assumed at reverse proxy, out of repo); effective env-var-dependent
CORS/limits in a real deployment; DB role least-privilege; downstream log redaction.

---

## G. Code Quality Findings

**Strengths (verified, not assumed):** uniform 5-layer module structure with explicit "no handler holds SQL"
contracts in module docs; consistent partial-PATCH `COALESCE` semantics with tri-state `Option<Option<T>>` where
null must be expressible; money never touches floating point (ADR-0007 honored end-to-end); config validation at
start-up with a message catalogue; strong unit-test presence inside `src/` (229 tests, e.g. `config/tests.rs` 58,
`ml` 64).

**Concrete findings:**

1. **Stale contract comment** — `src/api/routes.rs:252-255` says `GET /notes/search` yields 400; reality is 405
   (static route wins), test-pinned at `tests/ml_api.rs:2150-2168` with an in-code note that the doc needs
   correcting. Fix is a comment + doc edit, not behavior.
2. **`UserRepository::find_by_id` missing from ADR-0005's exception table** (`src/users/repository.rs:90-103`) —
   safe (id = authenticated subject) but the documented invariant map is incomplete.
3. **Refresh rotation is the only multi-statement mutation without a transaction**
   (`src/auth/service.rs:230-257`) — correct-by-compensation today; a transaction (or single CTE) would remove
   the reasoning burden.
4. **Rate limiter fails open on poisoned mutex** (`src/ratelimit.rs:67-72`) and holds all counters in one
   process — acceptable single-replica; a real limitation at scale, acknowledged in module docs.
5. **`cors_layer` silently maps an unparsable origin to an empty header** (`src/api/mod.rs:181-188`) — safe only
   because validation rejects it earlier; failing loudly would be clearer.
6. **Unescaped `%`/`_` in ILIKE search terms** (e.g. `src/projects/repository.rs:37-38`) — parameter-bound (no
   injection), but user-typed wildcards change match semantics; FTS terms are defused via `plainto_tsquery`.
   Inconsistency between search paths.
7. **Committed absolute path with a personal name** — `src/bin/atom-ml-bench.rs:51`,
   `src/bin/atom-ml-spike.rs:35` embed a user-specific absolute Hugging Face
   cache path (`%USERPROFILE%\.cache\huggingface\hub\...\model.safetensors`),
   violating the repo's own review checklist ("no absolute path or personal
   name in the diff").
8. **No `sort` parameter on any list** — fixed server-side ordering; consistent, but a product limitation worth
   recording if the frontend ever needs sorting.
9. **Untested units:** `src/api/extract.rs`, `middleware.rs`, `telemetry.rs`, `health.rs`, `state.rs`,
   `main.rs`, ML bench/spike binaries; the session-prune background task.
10. **No circular coupling detected**; cross-module reads go through explicit SQL joins in repositories;
    `state.rs` wiring is central and flat. No refactor recommendation beyond the items above — nothing here
    justifies structural churn.

---

## H. Documentation Accuracy

The documentation is **unusually accurate on facts and unusually honest on gaps** — every endpoint count, DTO
shape, error code, migration detail, module boundary, and test count checked against code matched at HEAD. The
problems are **status-lag and internal contradiction**, not fabrication:

1. **`docs/project-truth.md` contradicts itself:** §3 "Phase 2 is partly delivered" and §18's "not `Verified`"
   intro (stale, header says updated 2026-10-26) vs §17 recording all three Phase 2 features `Verified`
   2026-10-29 (`ATOM-REVIEW-20261029-002`). In a repo whose authority order makes this file #1, this is the
   highest-impact drift.
2. **`docs/roadmap.md` contradicts itself:** §1 note "nothing in Phase 2 is `Verified`" directly beneath §2
   "Phase 2 … `Complete` (2026-10-29)".
3. **`docs/database/schema.md` §11** header still "Proposed" while its own intro says no §11 table is proposed
   any more.
4. **`docs/README.md`** decision index lists ADR-0001…0009 only; ADR-0010 (semantic-search scan bound) and
   ADR-0011 (api-keys) exist and are indexed in `docs/decisions/README.md`.
5. **`docs/architecture/module-boundaries.md`** uses three different status words for `ml` in one file (table
   "Implemented", prose "Verified", heading "planned").
6. **Root `README.md` is stale:** "Phase 1 is in progress", "container image is not built yet", status table
   omits reimbursements, job applications, ML, and API keys entirely — while `container.yml` builds and verifies
   the image and four features shipped. This is the first file any contributor reads.
7. **Live drift (uncommitted):** 5 new tests close 4 of the 5 reimbursement gaps that
   `docs/features/reimbursements/status.md` and `docs/testing/strategy.md` §7.2.1 still list as open; counts
   (56/3, 536) are stale against the working tree (60/4, 543-if-passing). Conversely one recorded claim is
   stale in the other direction: REIMB-R11 "in any status" is called unasserted although the committed test
   exists.
8. **Implemented-but-undocumented / documented-but-missing:** none found in either direction beyond the items
   above — a notable result.

---

## I. Planned Architecture Gap Analysis

| Intended direction | Classification | Evidence & gap-closing work |
|---|---|---|
| Rust + Axum backend | **Already supported** | Implemented, recorded-verified; fmt+clippy pass this session |
| PostgreSQL persistent store | **Already supported** | 7 migrations, SQLx, schema tests; live-DB posture unverified (see §K) |
| API-first architecture | **Already supported** | OpenAPI 3.1 generated from annotations, contract-pinned to router both directions; explorer |
| Next.js web frontend | **Not yet supported** | No frontend code anywhere in the workspace; the 54-op contract is the only consumer-ready artifact |
| React Native + Expo mobile | **Not yet supported** | Same as above |
| Shared API contracts for web & mobile | **Already supported (contract only, no consumer yet)** | `openapi.json` + conventions + error contract; a frontend would be the first real consumer |
| Modular personal workspace (tasks & notes) | **Already supported** | Both features implemented and recorded-verified |
| n8n-driven workflow automation | **Not yet supported — currently excluded by product truth** | No HTTP client dependency exists at all (structural); `project-truth.md` §6.1 exclusion + owner decision O7; gap-closing starts with a product decision, then webhook endpoints + execution history |
| Laptop price monitoring & price history | **Missing** | No code, schema, roadmap row, or doc found in a targeted search; everything (storage of snapshots, fetch jobs, scheduling) would be new |
| Future local AI / prompt history / RAG | **Partially supported** | Local inference: real (Burn flex encoder + WordPiece, bench-measured, ADR-0008). Embeddings/vector storage: real (`embeddings`, cosine re-rank, ADR-0010 bound). Retrieval: real (lexical tsvector + semantic + hybrid + match ranking). **Missing entirely:** generation, prompt/conversation history, memory beyond the embedding index. Honest framing: this is retrieval-augmented *search*, not RAG in the full sense |
| Future notification & approval workflows | **Not yet supported** | Only the claims lifecycle state machine exists; no notification tables, channels, or workflow engine |

---

## J. Prioritized Recommendations

*(Nothing was implemented; these are recommendations only.)*

### P0 — Critical

| # | Problem | Evidence | Impact | Proposed solution | Deps | Size |
|---|---|---|---|---|---|---|
| P0-1 | 471 lines of uncommitted WIP (5 tests + 3 doc edits) exist outside version control and outside every recorded count | `git status`/`git diff` during the session | Work loss risk; verification records silently diverge from the tree | Commit to a task branch per `git-workflow.md` (or deliberately discard), run the six gates, then update reimbursement card + strategy + implementation-status counts | Human decision on intent of the WIP | S |

No confirmed security vulnerability or data-loss defect in the shipped code warranted P0.

### P1 — High

| # | Problem | Evidence | Impact | Proposed solution | Deps | Size |
|---|---|---|---|---|---|---|
| P1-1 | `project-truth.md` §3/§18 and `roadmap.md` §1 contradict the recorded 2026-10-29 Verified promotion | §H.1–2 | Authority-order corruption: an agent following the #1 source will treat delivered features as unverified | Reconcile §3/§18 and roadmap §1 with §17 / §2-3; bump headers | None | S |
| P1-2 | Root `README.md` omits Phase 2 features and misstates container status | §H.6 | Contributor onboarding misleads; first impression wrong | Rewrite status table (Phase 2 areas, container "CI-built, not published") | None | S |
| P1-3 | No dependency-vulnerability tooling in CI | `ci.yml`, `container.yml` (§F) | Silent exposure to crate CVEs (Burn/SQLx/Axum graphs are large) | Add `cargo-deny` (or `cargo-audit`) job + Dependabot; pin `actions/*` by SHA in `ci.yml` | None | M |
| P1-4 | Rate limiting posture is deployment-fragile (TCP-peer keying, in-process counters, auth-only coverage, fails open) | `src/ratelimit.rs:67-72`, `middleware.rs:121-156`; P-7/P-8 | Brute-force protection weak or shared-bucket DoS behind a proxy | Decide deployment topology; either document the required edge limiter or add shared-store limiting + coverage decision for `/api-keys` and `/ml/reindex` | Deployment decision | M |

### P2 — Medium

| # | Problem | Evidence | Impact | Solution | Size |
|---|---|---|---|---|---|
| P2-1 | Recorded test gaps: no schema tests for 0006/0007; session-prune task untested; no CORS HTTP-level test; no reachable-status-code assertions; no ML model-loading CI case | `strategy.md` §6; review finding 3 | Regressions in exactly the "proven by hand-run" areas would go uncaught | Add `api_keys_schema.rs`, `embeddings` schema case, a prune-task test, a CORS header test; optional model-load integration case behind a env-gated fixture | M |
| P2-2 | Access tokens unrevocable; `jti` unused (P-2) | `token.rs:43` | 15-min window for stolen access tokens | Decide: accept (short TTL) or implement a revocation list keyed by `jti` | M |
| P2-3 | Registration enumeration + permissive email grammar (P-3/P-4) | `users/repository.rs:66-72`, `users/service.rs` | Data quality + a standard trade-off to make explicitly | Product decision: equalise register response or accept; tighten email grammar | S |
| P2-4 | Argon2 pepper absent (P-1) | §F | Hardening depth against DB leak | ADR + implementation if accepted | M |
| P2-5 | ILIKE wildcard semantics inconsistency | `projects/repository.rs:37-38` et al. | Confusing search behavior across features | Escape or document per-endpoint | S |
| P2-6 | Refresh rotation without a transaction | `auth/service.rs:230-257` | Reasoning burden; worst case bounded but non-atomic | Wrap in one transaction or CTE | S |

### P3 — Low

- P3-1 Remove the hard-coded user path from the ML bins (`atom-ml-bench.rs:51`, `atom-ml-spike.rs:35`);
  document the expected model location. (S)
- P3-2 Fix stale route comment `routes.rs:252-255`; complete ADR-0005's exception table with `find_by_id`;
  repair `module-boundaries.md` status words; `schema.md` §11 header; broken sentence in
  `job-applications/status.md`; add ADR-0010/0011 to `docs/README.md`. (S each)
- P3-3 Security headers (`X-Content-Type-Options`, `Cache-Control: no-store` on token responses). (S)
- P3-4 Embeddings write-only columns (`id`, `created_at`, `updated_at` never read) — document or drop in a
  future migration. (S)

---

## K. Verification Limitations

1. **The test suite was not executed.** Running it requires a database and the harness applies embedded
   migrations on connect; the brief prohibits running migrations and any state-changing command. Therefore every
   "recorded passing" claim (536/538 tests, six gates, CI run IDs, review verdicts, benchmark latencies,
   "15/15 at rank 1") is a **documentation claim this audit did not re-execute** — the audit verified the cited
   tests, binaries and counts exist and match the committed tree.
2. **What was executed:** `cargo fmt --check` (pass), `cargo clippy -D warnings` all-targets/all-features
   (**exit 0**, re-run cleanly after disambiguating a PowerShell stderr artifact), read-only git inspection,
   targeted greps. **Compilation of the working tree is therefore verified; runtime behavior is not.**
3. **No database was connected to.** Live schema, applied-migration history, index/planner behavior:
   unverified — assessed from `migrations/` and source only.
4. **ML inference path unexercised:** no model file exists in the repo; the real encoder's behavior rests on
   unit tests (not run) and the manual benchmark record (doc claim).
5. **`tests/shutdown.rs` is unix-only** — nothing about graceful shutdown can be proven on this Windows host.
6. **TLS, reverse proxy, edge rate limiting, DB role privileges, downstream log redaction:** outside this
   repository; unverifiable from code.
7. **The review-log's "live n8n integration" mention** is recorded there as an external claim "not reproducible
   from this repository" — it remains unverifiable.
8. **Subagent model identities were not reported by the harness** and are therefore not asserted anywhere in
   this report.

---

## L. Suggested Next Development Steps

1. **Resolve the working tree first (P0-1):** commit or consciously discard the 5 claims tests + doc edits on a
   task branch, run `scripts/gate.ps1`, and record the new counts. Everything else builds on a tree whose
   records match its content.
2. **Reconcile the authority documents (P1-1/P1-2):** one documentation-only change aligning
   `project-truth.md` §3/§18, `roadmap.md` §1, `README.md`, `module-boundaries.md`, `schema.md` §11 and the
   `docs/README.md` ADR index with the 2026-10-29 promotion. Cheap, and it unblocks correct agent behavior.
3. **Add supply-chain gates (P1-3):** `cargo-deny` + Dependabot + SHA-pinned actions in CI. Independent of
   features; reduces risk while the codebase is still small.
4. **Close the self-declared test gaps (P2-1):** schema tests for migrations 0006/0007 and a test for the
   session-prune task — the two areas currently proven only by recorded hand-runs.
5. **Then the first real product fork:** either (a) a minimal Next.js consumer against `openapi.json` (the
   contract is ready and contract-pinned — the fastest way to convert "API-first" from claim to proof), or (b) a
   product decision on n8n/automation scope (owner decision O7), since every automation capability is blocked on
   that decision rather than on code. Price monitoring has no prerequisite artifact at all and should not be
   started before (b) is decided.

---

## Completion Ledger (session policy)

- **Role → model:** Orchestrator/Coordinator, all synthesis: `nvidia/z-ai/glm-5.3-flash` (Nvidia free-tier
  route via OpenCode; no metered calls made, no spend authorized or incurred). Five read-only `explore`
  subagents were delegated for API, database, security, tests/CI, and docs/AI inventories — their session IDs:
  `ses_e7974e788ffeQOsIbjA5DHXzWk`, `ses_e7974e750ffe8gyQ0cqlxZJVZP`, `ses_e7974e74effeXVuHfsxUnysBj1`,
  `ses_e7974e74cffea2eHbPsYr0zBxe`, `ses_e7974e737ffekLHwYtFHsHD7EK`; the harness did not report which models
  they resolved, so none is claimed. No Jev advisor/router used; no escalation triggered; no fallback models
  needed.
- **Verification actually performed this session:** `cargo fmt --all -- --check` → pass;
  `cargo clippy --all-targets --all-features -- -D warnings` → **exit 0**; read-only git inspection; targeted
  greps. **Test suite: NOT run (prohibited — would apply migrations to a database).**
- **Source state:** `develop` @ `96c37b2` + the 5-file uncommitted diff reported in §B, preserved untouched.
- **Unresolved MUSTs:** the recorded verification chain (gate runs, review verdicts) remains accepted-by-record,
  not re-proven; live-database posture unverified (§K).
- **Next action:** Human review of this report; the P0 item (working-tree disposition) needs an explicit Human
  decision before any further work. No files were changed during the audit; the only write performed is this
  report file, at explicit Human request.

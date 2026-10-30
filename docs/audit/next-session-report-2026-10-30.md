# Next Execution Session Report — Integration Preparation & Owner Decisions

> **Status:** Point-in-time session deliverable (companion to
> [`codebase-audit-2026-10-30.md`](codebase-audit-2026-10-30.md) and
> [`stabilization-report-2026-10-30.md`](stabilization-report-2026-10-30.md); durable
> records: [`../engineering/change-log.md`](../engineering/change-log.md)
> `ATOM-CHANGE-20261030-001/002`,
> [`../engineering/review-log.md`](../engineering/review-log.md)
> `ATOM-REVIEW-20261030-001`)
> **Date:** 2026-10-30
> **Constraint honored:** all user work preserved; no commits, pushes, stashes or resets;
> no security or automation behavior implemented before approval.

---

## 1. Current Repository State

- **Branch:** `develop`, in sync with `origin/develop`. **HEAD:** `96c37b2be33435f085651eb84544087db8204d60` — unchanged; nothing was committed, pushed, stashed or reset this session.
- **Reinspection confirmed** the state matches the 2026-10-30 stabilization report (change-log `ATOM-CHANGE-20261030-001/002`, review-log `ATOM-REVIEW-20261030-001`, project-truth §6.2, roadmap, ADR-0012 `Proposed`, Q-15…Q-19 `Open`).
- **User-owned pre-existing changes (identified separately, preserved):** 4 new tests in `tests/claims_api.rs`, 1 new test + `seeder_lock()` helper in `tests/claims_flow.rs`, the ML-benchmark-reproduction edits in `docs/features/ml/status.md`, `docs/testing/strategy.md` and `docs/engineering/implementation-status.md`.
- **Stabilization change set:** 19 further modified files + 10 untracked files (full ledger in the stabilization report §M).
- **Frontend:** `atom-web/` exists beside `atom/api`; **no `.git`** (verified).

## 2. Resolved Issues (this session)

| Issue | Resolution | Evidence |
|---|---|---|
| **WIP test seeding race** (`the_seeded_claims_are_readable_through_the_api` → 500 when the uncommitted minimal-profile case resets the shared DB between its existence check and login) | The seeded half of the committed case now takes the `ATOMSEED` advisory lock (`seeder_lock()`) **before its existence check** and holds it to the end — exactly the fix the independent review prescribed. No assertion weakened, no test deleted, the WIP case untouched (it already held the lock for its whole run). | `cargo test --test claims_flow` **8/8 consecutive green runs** (was 2/2 failing in-suite); full suite **563/563, 0 failed** |
| Stale records claiming the race was open | Change-log `ATOM-CHANGE-20261030-002` records the fix with a marked Correction to `ATOM-CHANGE-20261030-001`; `strategy.md` race paragraph updated | `atom-docs --strict` OK, 0 warnings |

## 3. Remaining Blockers (all awaiting owner)

1. **Commit authorization** for the proposed sequence (§4).
2. **Q-15…Q-19** security decisions (§5).
3. **O7** — accept/amend/reject `ADR-0012` (§6).
4. **`atom-web` git initialization** (§7).

## 4. Proposed Commit Sequence (no action taken)

Convention: Conventional-Commit subjects, task branch off `develop`, PR back to `develop` (`git-workflow.md`). **Files spanning several logical changes need hunk staging (`git add -p`)** — marked ⚗.

| # | Commit | Files | Depends on |
|---|---|---|---|
| 1 | `docs: record the 2026-10-30 repository audit` | `docs/audit/codebase-audit-2026-10-30.md` | — (must precede its citers: open-questions, deny.toml comment, change-log) |
| 2 | `test(claims): close the recorded reimbursement gaps` *(the owner's WIP)* | ⚗ `tests/claims_api.rs` (their 4 tests), ⚗ `tests/claims_flow.rs` (their WIP test + helper), `docs/features/ml/status.md` | — |
| 3 | `fix(test): serialize the seeded claims read with the seeder lock` | ⚗ `tests/claims_flow.rs` (lock hunks), ⚗ `docs/testing/strategy.md` (race-resolution sentence), `docs/engineering/change-log.md` (entry 002) | 2 (same file; the lock helper ships with the WIP) |
| 4 | `docs: reconcile the status documents with the 2026-10-29 promotion` | `project-truth.md`, `roadmap.md`, `README.md`, `module-boundaries.md`, `schema.md`, `ADR-0005` amendment, feature cards ×2, `AGENTS.md`, `docs/architecture/README.md`, ⚗ `docs/README.md`, ⚗ `docs/engineering/implementation-status.md`, ⚗ `docs/testing/strategy.md`, `docs/audit/stabilization-report-2026-10-30.md` | — |
| 5 | `build(ci): add the dependency-security gate and pin the workflow actions` | `deny.toml`, `.github/dependabot.yml`, `.github/workflows/ci.yml` | — (first execution is CI) |
| 6 | `test: add the schema, cors and session-prune coverage` | `tests/api_keys_schema.rs`, `tests/embeddings_schema.rs`, `tests/cors.rs`, `tests/session_prune.rs` | — |
| 7 | `chore(src): correct the stale search contract comment and the bench model default` | `src/api/routes.rs`, `src/bin/atom-ml-bench.rs` | — |
| 8 | `docs(decisions): propose the automation scope and record the open security questions` | `docs/decisions/ADR-0012-*.md`, ⚗ `docs/decisions/README.md`, ⚗ `docs/README.md`, `docs/engineering/open-questions.md`, `docs/architecture/mobile-preparation.md`, `docs/architecture/ai-roadmap.md` | 1 (cites the audit doc) |
| 9 | `docs(engineering): record the stabilization run and the independent review` | ⚗ `docs/engineering/change-log.md` (entry 001), ⚗ `docs/engineering/review-log.md`, `tests/ml_api.rs` (docstring correction) | 3–8 (it records them) |

Rationale: user work lands first and separately (their intent, reviewable on its own); the race fix immediately follows because it builds on the WIP helper; the engineering records land last because they cite everything. After the suite of commits: `cargo test --all-targets --locked` must be re-run on the committed tree (expected 563/563) before opening the PR. **`atom-web` is a separate repository** — suggested first commands (only on your authorization): `git init && git add -A && git commit -m "feat: Next.js shell, BFF auth and notes MVP"`.

## 5. Q-15…Q-19 Security Decision Table

| ID | Question | Current behavior | Security implication | **Recommendation** | Compatibility / migration | Ops requirements |
|---|---|---|---|---|---|---|
| **Q-15** | Server-side Argon2 pepper? | Argon2id 19 MiB/t=2/p=1 + per-hash salt, no pepper | DB disclosure allows offline cracking bounded by cost + 12-char policy | **Keep current (no pepper).** For a single-user personal workspace the added key-management surface outweighs the marginal cracking bound; revisit if a hosted/multi-user deployment ever exists | None; no migration | None. (Adopting later = third env secret + transparent re-hash-on-login path) |
| **Q-16** | Revocable access tokens (`jti` list)? | Stateless HS256, 15-min TTL; refresh revocable, disabled accounts blocked per-request | Stolen access token usable ≤15 min | **Accept the window now.** The TTL bounds exposure; `jti` stays reserved. Revisit if session-listing/instant-kill UX is ever wanted | None | None. (jti list = 1 table + 1 indexed read per request + eviction) |
| **Q-17** | Equalise registration response? | Duplicate email → `409` + distinct message; login already equalised | Registration is a live "is this email registered" oracle | **Accept now; force the decision when password-reset/email is built.** For a personal workspace the oracle's value is low, and honest 409s are better UX; equalisation without email infra is half a mitigation | Accepting = none. Equalising = new mail flow + UX change | Equalising needs an outbound mail channel (hosted dependency → owner decision) |
| **Q-18** | Tighten email grammar? | `validate_email` accepts e.g. `a@b@c.com`; DB enforces lower-case+length only | Data quality; matters only when email is consumed | **Tighten now, deliberately** — the cheapest moment is before any consumer exists (`atom-web` is new, zero stored addresses depend on it). Single `@`, non-empty labels, no consecutive dots; update validator tests + DB CHECK stays as-is | Contract-adjacent: some previously-accepted forms become `422` — acceptable pre-consumer; document in `api.md` | None |
| **Q-19** | Refresh rotation as one transaction? | Two statements (insert replacement → atomic claim), recorded in ADR-0003 as fail-safe; race compensated by reuse-detection mass revocation | Worst case: one bounded-lifetime unclaimed session row (pruned hourly); reasoning burden on every future reader | **Adopt the transaction + amend ADR-0003.** Strictly safer, removes the compensated-ordering exception in `project-truth.md` §12; interruption then leaves the old session valid (caller retries) | No wire change; needs a concurrency regression test + ADR amendment + §12/data-architecture §7 wording | None |

*Nothing is implemented until you approve; each option's full text is in `open-questions.md`.*

## 6. O7 Decision Summary (from `ADR-0012`, `Proposed`)

- **Architecture:** n8n stays an external service (scheduling, outbound collection, notifications); ATOM owns data and rules; PostgreSQL remains the source of truth; n8n writes only through authenticated ATOM endpoints; n8n is never embedded in the API process nor deployed by this repository.
- **Credential handling:** a dedicated ATOM API key (`ADR-0011`: 256-bit CSPRNG, SHA-256-at-rest, revocable) held in n8n's own secret store; `last_used_at` + revocation = instant off-switch + audit trail; no database credential ever leaves ATOM.
- **Authentication & idempotency:** webhooks are ordinary authenticated requests through the existing extractors/validators/ownership predicates; every retryable write carries a fingerprint idempotency identity (UNIQUE constraints make duplicates unrepresentable — e.g. `price_observations (user_id, listing_id, fingerprint)`, `price_alerts (rule_id, observation_id)`).
- **Retry & failure handling:** failed collection attempts are recorded as `automation_runs` outcomes, stale listings are marked (never re-reported as current), suspicious/missing prices are failures — never silently absorbed.
- **Execution history:** `automation_runs` (user-scoped, outcome CHECK, newest-first index) records the ATOM-side outcome; n8n's internal history stays in n8n (bounded scope, stated).
- **First use case — laptop price monitoring:** 6-table minimal schema fully sketched (`tracked_products`, `product_listings`, `price_observations`, `monitoring_rules`, `automation_runs`, `price_alerts`), `NUMERIC(19,4)` money per ADR-0007, composite-FK ownership per the `0004`/`0005` pattern, forward-only migration 0008; deterministic matching first (model + config hash); official APIs/permitted feeds only; no CAPTCHA/anti-bot bypass.
- **Operational costs / deployment:** a second runtime (n8n) enters the deployment (owner-operated); per-source collection policy is standing maintenance; the public surface grows by the collection endpoints (each needs credential + idempotency tests like the contract sweep); push/notifications remain separate.
- **Decision asked:** accept (start with migration 0008 + the collection endpoints), amend (e.g. different credential or history model), or reject (the §6 exclusion stands and price monitoring stops).

## 7. Frontend Readiness Report (`atom-web` — no scope expansion this session)

- **Implementation status:** Next.js 16 App Router + TypeScript BFF; shell + login/register/logout + notes (list/search/create/edit/delete/archive, pagination envelope). 49 files; 3 runtime deps (`next`, `react`, `react-dom`, `server-only`) + 7 dev deps, each with a recorded reason. No `git` repo (verified).
- **API contract compatibility:** `openapi.json` captured live from the running API (54 operations, 118,567 bytes); `schema.d.ts` generated via `openapi-typescript`; error-envelope and pagination helpers unit-tested against the documented contract. Compatible with the current backend surface as-is.
- **Authentication flow risks:** reviewer spot-check clean — tokens only in `session.ts`/`cookies.ts`/route handlers; `server-only` build-time boundary; HttpOnly + SameSite=Lax + Secure-in-prod; no `NEXT_PUBLIC_*` secrets. One hardening applied this session (redirect guard normalises `\` before the `//` check). Residual risks, stated: **no live browser E2E has ever run** (the auth flow's first-run correctness rests on contract docs, types and unit tests — `NOT_PROVEN` per policy); cookies + BFF trade-offs documented in its README.
- **Missing tests:** browser E2E (login → notes round-trip), component tests; none blocking the scaffold.
- **Recommended order for the remaining modules:** (1) live E2E auth smoke against a running API; (2) projects/tasks (reuses the list/pagination patterns); (3) reimbursements (money-as-string + lifecycle state machine UI); (4) job applications (stage machine); (5) price-monitoring section — only after O7 approval.

## 8. Exact Commands and Verification Results (this session)

| Command | Result |
|---|---|
| `git status --porcelain=v1 --branch`; `git log -1` | branch state + HEAD confirmed unchanged; user files separated |
| `cargo fmt --all -- --check` | **exit 0** |
| `cargo test --locked --test claims_flow` × 8 | **8 × exit 0** (4 tests each; binary failed 2/2 before the fix) |
| `cargo test --all-targets --locked --no-fail-fast` (with `TEST_DATABASE_URL` = disposable `atom_test`) | **27 result lines: 563 executed, 563 passed, 0 failed** (245 unit-target + 318 integration) — first zero-failure full-suite record for this tree |
| `cargo run --bin atom-docs -- --strict` | **OK, 0 warnings** (exit 0, after the record updates) |
| `atom-web`: `npm run lint`; `npx tsc --noEmit`; `npm test` | **exit 0 / exit 0 / 27 passed (27)** — re-executed this session |
| Not executed | anything that writes outside the disposable test DB; any git mutation; `cargo deny` (not installed; first run = CI) |

### Completion Ledger

- **Roles → models:** Orchestrator + fix author: `nvidia/z-ai/glm-5.3-flash` (Nvidia free-tier via OpenCode; no metered calls). No delegation was needed this session (one test-file fix + reports); no independent reviewer session — the fix is the one `ATOM-REVIEW-20261030-001` itself prescribed, and it is verified by 8 repeated runs + the full suite.
- **Source state:** `96c37b2` + the change set, all uncommitted; `tests/claims_flow.rs` now carries the owner's WIP test **plus** the owner-authorized lock fix (both preserved intents).
- **Outstanding MUSTs:** owner approvals in §3 (commits, Q-15…Q-19, O7, atom-web git init).
- **Next action:** your approval on §4's sequence (or an amended one); I execute it commit-by-commit on your word.
